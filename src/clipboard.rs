//! Clipboard utilities with HTML-to-Markdown conversion support

use clipboard_rs::{Clipboard as ClipboardTrait, ClipboardContext, ContentFormat};
use htmd::{Element, HtmlToMarkdown, element_handler::Handlers, options::{BulletListMarker, Options}};

pub type ClipboardResult<T> = Result<T, ClipboardError>;

#[derive(Debug)]
pub enum ClipboardError {
    ContextCreation(String),
    ReadError(String),
    ConversionError(String),
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ContextCreation(e) => write!(f, "Failed to create clipboard context: {}", e),
            Self::ReadError(e) => write!(f, "Failed to read clipboard: {}", e),
            Self::ConversionError(e) => write!(f, "Failed to convert HTML: {}", e),
        }
    }
}

pub enum ClipboardContent {
    Markdown(String),
    PlainText(String),
    Empty,
}

pub fn has_html() -> bool {
    ClipboardContext::new()
        .map(|ctx| ctx.has(ContentFormat::Html))
        .unwrap_or(false)
}

pub fn get_html() -> ClipboardResult<Option<String>> {
    let ctx = ClipboardContext::new()
        .map_err(|e| ClipboardError::ContextCreation(e.to_string()))?;

    if !ctx.has(ContentFormat::Html) {
        return Ok(None);
    }

    ctx.get_html()
        .map(Some)
        .map_err(|e| ClipboardError::ReadError(e.to_string()))
}

pub fn get_text() -> ClipboardResult<Option<String>> {
    let ctx = ClipboardContext::new()
        .map_err(|e| ClipboardError::ContextCreation(e.to_string()))?;

    ctx.get_text()
        .map(Some)
        .map_err(|e| ClipboardError::ReadError(e.to_string()))
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
    converter
        .convert(html)
        .map_err(|e| ClipboardError::ConversionError(e.to_string()))
}

/// Get clipboard content, converting HTML to Markdown if available
///
/// Priority:
/// 1. If HTML is available, convert to Markdown
/// 2. Fall back to plain text
/// 3. Return Empty if nothing available
pub fn get_content_as_markdown() -> ClipboardResult<ClipboardContent> {
    if let Ok(Some(html)) = get_html() {
        if !html.trim().is_empty() {
            match html_to_markdown(&html) {
                Ok(md) => {
                    let trimmed = md.trim().to_string();
                    if !trimmed.is_empty() {
                        return Ok(ClipboardContent::Markdown(trimmed));
                    }
                }
                Err(_) => {
                }
            }
        }
    }

    match get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}

pub fn get_content_plain() -> ClipboardResult<ClipboardContent> {
    match get_text() {
        Ok(Some(text)) if !text.is_empty() => Ok(ClipboardContent::PlainText(text)),
        Ok(_) => Ok(ClipboardContent::Empty),
        Err(e) => Err(e),
    }
}
