//! Clipboard utilities with HTML-to-Markdown conversion support.

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

/// Reuse one context: X11 starts an ownership thread for each context and a
/// per-operation context both leaks threads and corrupts the TUI on handoff.
#[cfg(not(target_os = "android"))]
fn clipboard_context() -> Option<&'static Mutex<ClipboardContext>> {
    static CTX: OnceLock<Option<Mutex<ClipboardContext>>> = OnceLock::new();
    CTX.get_or_init(|| native_clipboard_available().then(|| ClipboardContext::new().ok().map(Mutex::new)).flatten()).as_ref()
}

#[cfg(target_os = "macos")]
fn native_clipboard_available() -> bool {
    // clipboard-rs assumes `generalPasteboard` is non-null and panics in
    // headless sessions, so probe the same service before entering the crate.
    std::process::Command::new("/usr/bin/pbpaste").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|status| status.success())
}

#[cfg(all(not(target_os = "android"), not(target_os = "macos")))]
const fn native_clipboard_available() -> bool {
    true
}

#[cfg(not(target_os = "android"))]
fn with_clipboard<T>(f: impl FnOnce(&ClipboardContext) -> T) -> Option<T> {
    let guard = clipboard_context()?.lock().ok()?;
    Some(f(&guard))
}

#[cfg(not(target_os = "android"))]
fn set_system_text_result(text: &str) -> ClipboardResult<()> {
    with_clipboard(|ctx| ctx.set_text(text.to_string()).map_err(|error| ClipboardError::ReadError(error.to_string()))).unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn set_system_text_result(_text: &str) -> ClipboardResult<()> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
pub fn has_html() -> bool {
    with_clipboard(|ctx| ctx.has(ContentFormat::Html)).unwrap_or(false)
}

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
    with_clipboard(|ctx| ctx.get_text().map(Some).map_err(|e| ClipboardError::ReadError(e.to_string()))).unwrap_or_else(|| Err(ClipboardError::ContextCreation("clipboard unavailable".to_string())))
}

#[cfg(target_os = "android")]
fn get_system_text_result() -> ClipboardResult<Option<String>> {
    Ok(None)
}
fn create_converter() -> HtmlToMarkdown {
    let options = Options { bullet_list_marker: BulletListMarker::Dash, ..Options::default() };
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

pub fn get_content_plain() -> ClipboardResult<ClipboardContent> {
    get_content_plain_from(&SystemClipboard)
}

pub fn get_content_plain_from(clipboard: &dyn Clipboard) -> ClipboardResult<ClipboardContent> {
    match clipboard.get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_clipboard_content_is_converted_before_plain_text() {
        let clipboard = MemoryClipboard::with_text("fallback");
        clipboard.set_html("<p>Hello <strong>world</strong></p>");
        let ClipboardContent::Markdown(markdown) = get_content_as_markdown_from(&clipboard).unwrap() else { panic!("expected Markdown") };
        assert_eq!(markdown, "Hello **world**");
    }

    #[test]
    fn plain_and_empty_clipboards_keep_their_behavior() {
        let clipboard = MemoryClipboard::with_text("plain");
        assert!(matches!(get_content_as_markdown_from(&clipboard).unwrap(), ClipboardContent::PlainText(text) if text == "plain"));
        assert!(matches!(get_content_plain_from(&MemoryClipboard::default()).unwrap(), ClipboardContent::Empty));
    }
}
