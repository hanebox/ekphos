use std::sync::Mutex;

pub type ClipboardResult<T> = Result<T, ClipboardError>;

#[derive(Debug)]
pub enum ClipboardError {
    ContextCreation(String),
    ReadError(String),
    ConversionError(String),
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContextCreation(error) => write!(formatter, "Failed to create clipboard context: {error}"),
            Self::ReadError(error) => write!(formatter, "Failed to read clipboard: {error}"),
            Self::ConversionError(error) => write!(formatter, "Failed to convert HTML: {error}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// Text/HTML clipboard operations required by the editor and its host.
pub trait Clipboard: Send + Sync {
    fn set_text(&self, text: &str) -> ClipboardResult<()>;
    fn get_text(&self) -> ClipboardResult<Option<String>>;
    fn get_html(&self) -> ClipboardResult<Option<String>>;
}

/// Process-local clipboard for tests and headless integrations.
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    text: Mutex<Option<String>>,
    html: Mutex<Option<String>>,
}

impl MemoryClipboard {
    pub fn with_text(text: impl Into<String>) -> Self {
        Self {
            text: Mutex::new(Some(text.into())),
            html: Mutex::new(None),
        }
    }

    pub fn set_html(&self, html: impl Into<String>) {
        if let Ok(mut value) = self.html.lock() {
            *value = Some(html.into());
        }
    }
}

impl Clipboard for MemoryClipboard {
    fn set_text(&self, text: &str) -> ClipboardResult<()> {
        let mut value = self
            .text
            .lock()
            .map_err(|_| ClipboardError::ReadError("in-memory clipboard lock poisoned".to_string()))?;
        *value = Some(text.to_string());
        Ok(())
    }

    fn get_text(&self) -> ClipboardResult<Option<String>> {
        self.text
            .lock()
            .map(|value| value.clone())
            .map_err(|_| ClipboardError::ReadError("in-memory clipboard lock poisoned".to_string()))
    }

    fn get_html(&self) -> ClipboardResult<Option<String>> {
        self.html
            .lock()
            .map(|value| value.clone())
            .map_err(|_| ClipboardError::ReadError("in-memory clipboard lock poisoned".to_string()))
    }
}
