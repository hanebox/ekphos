use crate::{apply_global_layout_cancelable, load_layout_cache, save_layout_cache, GraphBuildOutcome, GraphIndex, GraphProjection, GraphSourceFile};
use ekphos_core::NoteId;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

enum GraphRequest {
    Build { ticket: u64, generation: u64, sources: Vec<GraphSourceFile>, cache_path: PathBuf },
    Layout { ticket: u64, generation: u64, index: Arc<GraphIndex>, projection: GraphProjection, cache_path: PathBuf },
}

impl GraphRequest {
    fn ticket(&self) -> u64 {
        match self {
            Self::Build { ticket, .. } | Self::Layout { ticket, .. } => *ticket,
        }
    }
    fn generation(&self) -> u64 {
        match self {
            Self::Build { generation, .. } | Self::Layout { generation, .. } => *generation,
        }
    }
}

#[derive(Debug)]
pub enum GraphResponse {
    Index { generation: u64, outcome: GraphBuildOutcome },
    Layout { generation: u64, fingerprint: u64, positions: Vec<(NoteId, f32, f32)> },
    Failed { generation: u64 },
}

#[derive(Default)]
struct RequestState {
    request: Option<GraphRequest>,
    stop: bool,
}

struct Shared {
    request: Mutex<RequestState>,
    request_ready: Condvar,
    result: Mutex<Option<GraphResponse>>,
    current_ticket: AtomicU64,
    pending: AtomicBool,
}

/// One replaceable request slot for graph extraction and global layout. A new
/// generation cancels work at the next file or layout-iteration boundary.
pub struct GraphWorker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl Default for GraphWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphWorker {
    pub fn new() -> Self {
        let shared = Arc::new(Shared { request: Mutex::new(RequestState::default()), request_ready: Condvar::new(), result: Mutex::new(None), current_ticket: AtomicU64::new(0), pending: AtomicBool::new(false) });
        let worker_shared = Arc::clone(&shared);
        let handle = thread::Builder::new().name("ekphos-graph".to_string()).spawn(move || run_worker(worker_shared)).ok();
        Self { shared, handle }
    }

    pub fn submit_build(&self, generation: u64, sources: Vec<GraphSourceFile>, cache_path: PathBuf) {
        let ticket = self.next_ticket();
        self.replace_request(GraphRequest::Build { ticket, generation, sources, cache_path });
    }

    pub fn submit_layout(&self, generation: u64, index: Arc<GraphIndex>, projection: GraphProjection, cache_path: PathBuf) {
        let ticket = self.next_ticket();
        self.replace_request(GraphRequest::Layout { ticket, generation, index, projection, cache_path });
    }

    pub fn try_take(&self) -> Option<GraphResponse> {
        self.shared.result.lock().ok()?.take()
    }

    pub fn cancel(&self) {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel);
        self.shared.pending.store(false, Ordering::Release);
        if let Ok(mut request) = self.shared.request.lock() {
            request.request = None;
        }
        if let Ok(mut result) = self.shared.result.lock() {
            *result = None;
        }
    }

    pub fn is_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }
    fn next_ticket(&self) -> u64 {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
    }
    fn replace_request(&self, request: GraphRequest) {
        if self.handle.as_ref().is_none_or(JoinHandle::is_finished) {
            self.shared.pending.store(false, Ordering::Release);
            if let Ok(mut result) = self.shared.result.lock() {
                *result = Some(GraphResponse::Failed { generation: request.generation() });
            }
            return;
        }
        self.shared.pending.store(true, Ordering::Release);
        if let Ok(mut result) = self.shared.result.lock() {
            *result = None;
        }
        if let Ok(mut state) = self.shared.request.lock() {
            state.request = Some(request);
            self.shared.request_ready.notify_one();
        }
    }
}

impl Drop for GraphWorker {
    fn drop(&mut self) {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut state) = self.shared.request.lock() {
            state.stop = true;
            state.request = None;
            self.shared.request_ready.notify_one();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
fn run_worker(shared: Arc<Shared>) {
    loop {
        let request = {
            let mut state = match shared.request.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            while state.request.is_none() && !state.stop {
                state = match shared.request_ready.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            if state.stop {
                return;
            }
            state.request.take().expect("request checked above")
        };
        let ticket = request.ticket();
        let generation = request.generation();
        let response = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(&shared, request))) {
            Ok(response) => response,
            Err(_) => Some(GraphResponse::Failed { generation }),
        };
        if shared.current_ticket.load(Ordering::Acquire) != ticket {
            continue;
        }
        shared.pending.store(false, Ordering::Release);
        if let Some(response) = response {
            if let Ok(mut result) = shared.result.lock() {
                *result = Some(response);
            }
        }
    }
}
fn execute(shared: &Shared, request: GraphRequest) -> Option<GraphResponse> {
    match request {
        GraphRequest::Build { ticket, generation, sources, cache_path } => {
            let outcome = GraphIndex::load_or_build(sources, &cache_path, || shared.current_ticket.load(Ordering::Acquire) != ticket)?;
            Some(GraphResponse::Index { generation, outcome })
        }
        GraphRequest::Layout { ticket, generation, index, mut projection, cache_path } => {
            let fingerprint = index.fingerprint;
            if let Some(positions) = load_layout_cache(&cache_path, fingerprint, &projection.nodes) {
                return Some(GraphResponse::Layout { generation, fingerprint, positions });
            }
            let completed = apply_global_layout_cancelable(&index, &mut projection.nodes, &projection.edges, || shared.current_ticket.load(Ordering::Acquire) != ticket);
            if !completed {
                return None;
            }
            save_layout_cache(&cache_path, fingerprint, &projection.nodes);
            let positions = projection.nodes.into_iter().map(|node| (node.note_id, node.x, node.y)).collect();
            Some(GraphResponse::Layout { generation, fingerprint, positions })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphFileFingerprint, GraphSourceMetadata};
    use std::time::{Duration, Instant};
    fn source(index: usize, root: &std::path::Path) -> GraphSourceFile {
        GraphSourceFile { metadata: GraphSourceMetadata { note_id: NoteId::from_index(index).unwrap(), title: format!("N{index}"), path: format!("N{index}"), tags: Vec::new() }, absolute_path: root.join(format!("N{index}.md")), fingerprint: GraphFileFingerprint { size: 0, modified_nanos: 1 } }
    }

    #[test]
    fn latest_build_replaces_in_flight_work_and_shutdown_is_bounded() {
        let root = std::env::temp_dir().join(format!("ekphos-graph-worker-{}", std::process::id()));
        let worker = GraphWorker::new();
        worker.submit_build(1, (0..5_000).map(|index| source(index, &root)).collect(), root.join("old.bin"));
        worker.submit_build(2, vec![source(0, &root)], root.join("new.bin"));
        let started = Instant::now();
        let response = loop {
            if let Some(response) = worker.try_take() {
                break response;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        };
        match response {
            GraphResponse::Index { generation, outcome } => {
                assert_eq!(generation, 2);
                assert_eq!(outcome.index.nodes.len(), 1);
            }
            response => panic!("unexpected graph response: {response:?}"),
        }
        assert!(!worker.is_pending());
        worker.cancel();
        assert!(!worker.is_pending());
        let _ = std::fs::remove_dir_all(root);
    }
}
