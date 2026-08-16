//! Clipboard utilities with HTML-to-Markdown conversion support

#[cfg(not(target_os = "android"))]
use clipboard_rs::{Clipboard as ClipboardTrait, ClipboardContext, ContentFormat};
use htmd::{
    element_handler::Handlers,
    options::{BulletListMarker, Options},
    Element, HtmlToMarkdown,
};
#[cfg(not(target_os = "android"))]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};

pub use ekphos_editor::{Clipboard, ClipboardError, ClipboardResult, MemoryClipboard};

pub enum ClipboardContent {
    Markdown(String),
    PlainText(String),
    Empty,
}

#[derive(Debug, Default)]
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn set_text(&self, text: &str) -> ClipboardResult<()> {
        set_system_text_result(text)
    }

    fn get_text(&self) -> ClipboardResult<Option<String>> {
        get_system_text_result()
    }

    fn get_html(&self) -> ClipboardResult<Option<String>> {
        get_system_html_result()
    }
}

pub fn default_clipboard() -> Arc<dyn Clipboard> {
    #[cfg(test)]
    {
        Arc::new(MemoryClipboard::default())
    }
    #[cfg(not(test))]
    {
        Arc::new(SystemClipboard)
    }
}

/// The one, lazily-created, process-wide clipboard context.
///
/// `clipboard-rs`'s X11 backend spawns a background thread per context that
/// holds the CLIPBOARD selection so its contents survive. Creating a fresh
/// context for every copy/paste therefore leaked a thread each time and, worse,
/// each new context stole selection ownership from the previous one — which
/// made the now-orphaned thread print "Somebody else owns the clipboard now"
/// to stdout and corrupt the TUI. Reusing one long-lived context keeps a single
/// owner, so re-copying never triggers a self-inflicted `SelectionClear`.
///
/// Returns `None` if a backend could not be created (e.g. no display server).
/// Kept non-generic so there is exactly one `CTX` regardless of caller.
#[cfg(not(target_os = "android"))]
fn clipboard_context() -> Option<&'static Mutex<ClipboardContext>> {
    static CTX: OnceLock<Option<Mutex<ClipboardContext>>> = OnceLock::new();
    CTX.get_or_init(|| ClipboardContext::new().ok().map(Mutex::new)).as_ref()
}

/// Run `f` against the shared clipboard context, or return `None` if no
/// clipboard backend is available or the lock is poisoned.
#[cfg(not(target_os = "android"))]
fn with_clipboard<T>(f: impl FnOnce(&ClipboardContext) -> T) -> Option<T> {
    let guard = clipboard_context()?.lock().ok()?;
    Some(f(&guard))
}

/// Write plain text to the system clipboard.
///
/// No-op on platforms without a system clipboard backend (e.g. Android/Termux),
/// where the editor relies on its internal clipboard instead. Failures are
/// swallowed silently — never logged to stdout/stderr, which would corrupt the
/// TUI.
#[cfg(not(target_os = "android"))]
fn set_system_text_result(text: &str) -> ClipboardResult<()> {
    with_clipboard(|ctx| ctx.set_text(text.to_string()).map_err(|error| ClipboardError::ReadError(error.to_string())))
        .unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn set_system_text_result(_text: &str) -> ClipboardResult<()> {
    Ok(())
}

/// Read plain text from the system clipboard, or `None` if unavailable.
#[allow(dead_code)]
#[cfg(not(target_os = "android"))]
pub fn has_html() -> bool {
    with_clipboard(|ctx| ctx.has(ContentFormat::Html)).unwrap_or(false)
}

#[allow(dead_code)]
#[cfg(target_os = "android")]
pub fn has_html() -> bool {
    false
}

#[cfg(not(target_os = "android"))]
fn get_system_html_result() -> ClipboardResult<Option<String>> {
    with_clipboard(|ctx| {
        if !ctx.has(ContentFormat::Html) {
            return Ok(None);
        }
        ctx.get_html().map(Some).map_err(|e| ClipboardError::ReadError(e.to_string()))
    })
    .unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn get_system_html_result() -> ClipboardResult<Option<String>> {
    Ok(None)
}

#[cfg(not(target_os = "android"))]
fn get_system_text_result() -> ClipboardResult<Option<String>> {
    with_clipboard(|ctx| ctx.get_text().map(Some).map_err(|e| ClipboardError::ReadError(e.to_string())))
        .unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn get_system_text_result() -> ClipboardResult<Option<String>> {
    Ok(None)
}

fn create_converter() -> HtmlToMarkdown {
    let options = Options {
        bullet_list_marker: BulletListMarker::Dash,
        ..Options::default()
    };

    HtmlToMarkdown::builder()
        .options(options)
        .add_handler(vec!["a"], |handlers: &dyn Handlers, element: Element| {
            let mut href: Option<String> = None;
            for attr in element.attrs.iter() {
                if &*attr.name.local == "href" {
                    href = Some(attr.value.to_string());
                    break;
                }
            }

            let href = match href {
                Some(h) if !h.is_empty() => h,
                _ => return Some(handlers.walk_children(element.node)),
            };

            if href.starts_with('#') {
                return Some(handlers.walk_children(element.node));
            }

            let content = handlers.walk_children(element.node).content;
            let text = content.trim();

            if text.is_empty() {
                return None;
            }

            // Escape parentheses in URL
            let href = href.replace('(', "\\(").replace(')', "\\)");

            Some(format!("[{}]({})", text, href).into())
        })
        .build()
}

/// Convert HTML to Markdown using htmd with custom link handling
pub fn html_to_markdown(html: &str) -> ClipboardResult<String> {
    let converter = create_converter();
    converter.convert(html).map_err(|e| ClipboardError::ConversionError(e.to_string()))
}

/// Get clipboard content, converting HTML to Markdown if available
///
/// Priority:
/// 1. If HTML is available, convert to Markdown
/// 2. Fall back to plain text
/// 3. Return Empty if nothing available
pub fn get_content_as_markdown() -> ClipboardResult<ClipboardContent> {
    get_content_as_markdown_from(&SystemClipboard)
}

pub fn get_content_as_markdown_from(clipboard: &dyn Clipboard) -> ClipboardResult<ClipboardContent> {
    if let Ok(Some(html)) = clipboard.get_html() {
        if !html.trim().is_empty() {
            if let Ok(md) = html_to_markdown(&html) {
                let trimmed = md.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(ClipboardContent::Markdown(trimmed));
                }
            }
        }
    }

    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}

#[allow(dead_code)]
pub fn get_content_plain() -> ClipboardResult<ClipboardContent> {
    get_content_plain_from(&SystemClipboard)
}

#[allow(dead_code)]
pub fn get_content_plain_from(clipboard: &dyn Clipboard) -> ClipboardResult<ClipboardContent> {
    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}
