use crate::highlight::Highlighter;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxServiceStatus {
    Unloaded,
    Loading,
    Ready,
    Failed,
}

enum SyntaxServiceState {
    Unloaded,
    Loading,
    Ready(Box<Highlighter>),
    Failed(String),
}

pub struct SyntaxService {
    state: SyntaxServiceState,
    theme_name: String,
    receiver: Option<Receiver<Result<Highlighter, String>>>,
    worker: Option<JoinHandle<()>>,
}

impl SyntaxService {
    pub fn new(theme_name: String) -> Self {
        Self {
            state: SyntaxServiceState::Unloaded,
            theme_name,
            receiver: None,
            worker: None,
        }
    }

    pub fn status(&self) -> SyntaxServiceStatus {
        match self.state {
            SyntaxServiceState::Unloaded => SyntaxServiceStatus::Unloaded,
            SyntaxServiceState::Loading => SyntaxServiceStatus::Loading,
            SyntaxServiceState::Ready(_) => SyntaxServiceStatus::Ready,
            SyntaxServiceState::Failed(_) => SyntaxServiceStatus::Failed,
        }
    }

    pub fn ensure_loaded(&mut self) {
        if !matches!(self.state, SyntaxServiceState::Unloaded) {
            return;
        }

        let (sender, receiver) = mpsc::sync_channel(1);
        let theme_name = self.theme_name.clone();
        match std::thread::Builder::new().name("syntax-loader".into()).spawn(move || {
            let loaded = std::panic::catch_unwind(|| Highlighter::new(&theme_name)).map_err(|_| "syntax definition loader panicked".to_string());
            let _ = sender.send(loaded);
        }) {
            Ok(worker) => {
                self.receiver = Some(receiver);
                self.worker = Some(worker);
                self.state = SyntaxServiceState::Loading;
            }
            Err(error) => {
                self.state = SyntaxServiceState::Failed(format!("could not start syntax loader: {error}"));
            }
        }
    }

    /// Poll the managed loader. Returns true when the externally visible state
    /// changes and the UI should render again.
    pub fn poll(&mut self) -> bool {
        if !matches!(self.state, SyntaxServiceState::Loading) {
            return false;
        }

        let received = match self.receiver.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Disconnected)) => Some(Err("syntax loader disconnected".to_string())),
            Some(Err(TryRecvError::Empty)) | None => None,
        };
        let Some(result) = received else {
            return false;
        };

        self.receiver = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.state = match result {
            Ok(mut highlighter) => {
                highlighter.set_theme(&self.theme_name);
                SyntaxServiceState::Ready(Box::new(highlighter))
            }
            Err(error) => SyntaxServiceState::Failed(error),
        };
        true
    }

    pub fn highlighter(&self) -> Option<&Highlighter> {
        match &self.state {
            SyntaxServiceState::Ready(highlighter) => Some(highlighter.as_ref()),
            _ => None,
        }
    }

    pub fn configure_theme(&mut self, theme_name: &str) {
        if self.theme_name == theme_name {
            return;
        }
        self.theme_name.clear();
        self.theme_name.push_str(theme_name);
        if let SyntaxServiceState::Ready(highlighter) = &mut self.state {
            highlighter.as_mut().set_theme(theme_name);
        }
    }

    pub fn clear_results(&self) {
        if let Some(highlighter) = self.highlighter() {
            highlighter.clear_cache();
        }
    }

    pub fn retry(&mut self) {
        if matches!(self.state, SyntaxServiceState::Failed(_)) {
            self.state = SyntaxServiceState::Unloaded;
        }
    }

    pub fn failure(&self) -> Option<&str> {
        match &self.state {
            SyntaxServiceState::Failed(error) => Some(error),
            _ => None,
        }
    }

    pub fn definition_bytes(&self) -> usize {
        self.highlighter().map_or(0, Highlighter::definition_bytes)
    }

    pub fn result_cache_bytes(&self) -> usize {
        self.highlighter().map_or(0, Highlighter::retained_cache_bytes)
    }

    pub fn live_workers(&self) -> usize {
        usize::from(self.worker.is_some())
    }
}

impl Drop for SyntaxService {
    fn drop(&mut self) {
        // Syntax loading is finite and owns no application references. Joining
        // here prevents a detached loader from surviving application shutdown.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn state_machine_is_lazy_and_reaches_ready() {
        let mut service = SyntaxService::new("base16-ocean.dark".to_string());
        assert_eq!(service.status(), SyntaxServiceStatus::Unloaded);
        assert_eq!(service.definition_bytes(), 0);

        service.ensure_loaded();
        assert_eq!(service.status(), SyntaxServiceStatus::Loading);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !service.poll() && Instant::now() < deadline {
            std::thread::yield_now();
        }

        assert_eq!(service.status(), SyntaxServiceStatus::Ready, "{:?}", service.failure());
        assert!(service.definition_bytes() > 0);
        assert_eq!(service.live_workers(), 0);
    }

    #[test]
    fn document_eviction_clears_results_but_keeps_definitions() {
        let mut service = SyntaxService::new("base16-ocean.dark".to_string());
        service.state = SyntaxServiceState::Ready(Box::default());
        service.highlighter().unwrap().highlight_block("let value = 1;", "rust");
        assert!(service.result_cache_bytes() > 0);
        let definitions = service.definition_bytes();

        service.clear_results();
        assert_eq!(service.result_cache_bytes(), 0);
        assert_eq!(service.definition_bytes(), definitions);
    }
}
