//! Shared, allocation-light Markdown syntax recognition.
//!
//! This module intentionally recognizes the syntax Ekphos already supports; it
//! is not a complete CommonMark parser. Consumers keep ownership of rendering,
//! navigation, validation, and styling policy while sharing byte ranges and
//! syntax boundaries.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heading<'a> {
    pub level: usize,
    pub text: &'a str,
}

/// Recognize an ATX heading with one to six `#` markers.
pub fn heading(line: &str) -> Option<Heading<'_>> {
    let level = line.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(Heading { level, text: rest.trim_start().trim_end_matches(|ch: char| ch == '#' || ch.is_whitespace()) })
}

/// Return the zero-based line containing the closing frontmatter delimiter.
pub fn frontmatter_end(content: &str) -> Option<usize> {
    frontmatter_end_in_lines(content.lines())
}

pub fn frontmatter_end_in_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let mut lines = lines.into_iter();
    if lines.next()?.trim() != "---" {
        return None;
    }
    lines.enumerate().find_map(|(index, line)| (line.trim() == "---").then_some(index + 1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceMarker {
    Backtick,
    Tilde,
}

/// Recognize the fence forms already supported by Ekphos.
pub fn fence_marker(line: &str) -> Option<FenceMarker> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        Some(FenceMarker::Backtick)
    } else if trimmed.starts_with("~~~") {
        Some(FenceMarker::Tilde)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink<'a> {
    pub range: Range<usize>,
    pub raw: &'a str,
    pub target: &'a str,
    pub heading: Option<&'a str>,
    pub alias: Option<&'a str>,
}

impl<'a> WikiLink<'a> {
    pub fn display_text(&self) -> &'a str {
        self.alias.unwrap_or(self.raw)
    }

    pub fn char_range(&self, source: &str) -> Range<usize> {
        let start = source[..self.range.start].chars().count();
        start..start + source[self.range.clone()].chars().count()
    }
}

/// Parse a wiki link beginning exactly at `start`, using byte offsets.
pub fn wiki_link_at(source: &str, start: usize) -> Option<WikiLink<'_>> {
    let rest = source.get(start..)?;
    let body = rest.strip_prefix("[[")?;
    let close = body.find("]]")?;
    let raw = &body[..close];
    if raw.is_empty() || raw.contains(['[', ']']) {
        return None;
    }
    let (destination, alias) = raw.split_once('|').map(|(destination, alias)| (destination, Some(alias))).unwrap_or((raw, None));
    let (target, heading) = destination.split_once('#').map(|(target, heading)| (target, Some(heading))).unwrap_or((destination, None));
    let end = start + 2 + close + 2;
    Some(WikiLink { range: start..end, raw, target, heading, alias })
}

/// Visit valid wiki links on one source line, excluding inline-code spans.
pub fn visit_wiki_links<'a>(source: &'a str, mut visit: impl FnMut(WikiLink<'a>)) {
    let mut cursor = 0;
    while cursor < source.len() {
        let remaining = &source[cursor..];
        let next_wiki = remaining.find("[[");
        let next_tick = remaining.find('`');
        if let Some(tick) = next_tick {
            if next_wiki.is_none() || tick < next_wiki.unwrap() {
                let opening = cursor + tick;
                let Some(closing) = source[opening + 1..].find('`') else {
                    break;
                };
                cursor = opening + 1 + closing + 1;
                continue;
            }
        }
        let Some(relative_start) = next_wiki else {
            break;
        };
        let start = cursor + relative_start;
        if let Some(link) = wiki_link_at(source, start) {
            cursor = link.range.end;
            visit(link);
        } else if let Some(close) = source[start + 2..].find("]]") {
            cursor = start + 2 + close + 2;
        } else {
            break;
        }
    }
}

/// Find valid wiki links on one source line, excluding inline-code spans.
pub fn wiki_links(source: &str) -> Vec<WikiLink<'_>> {
    let mut links = Vec::new();
    visit_wiki_links(source, |link| links.push(link));
    links
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedWikiLink<'a> {
    pub row: usize,
    pub source: &'a str,
    pub link: WikiLink<'a>,
}

pub fn document_wiki_links_with_tilde_fences(content: &str, skip_through_row: Option<usize>, recognize_tilde_fences: bool) -> Vec<LocatedWikiLink<'_>> {
    let mut links = Vec::new();
    visit_document_wiki_links_with_tilde_fences(content, skip_through_row, recognize_tilde_fences, |link| links.push(link));
    links
}

/// Visit document links without retaining an intermediate collection.
pub fn visit_document_wiki_links_with_tilde_fences<'a>(content: &'a str, skip_through_row: Option<usize>, recognize_tilde_fences: bool, mut visit: impl FnMut(LocatedWikiLink<'a>)) {
    let mut fence = None;
    for (row, line) in content.lines().enumerate() {
        if skip_through_row.is_some_and(|end| row <= end) {
            continue;
        }
        if let Some(marker) = fence_marker(line) {
            if marker == FenceMarker::Tilde && !recognize_tilde_fences {
                visit_wiki_links(line, |link| visit(LocatedWikiLink { row, source: line, link }));
                continue;
            }
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        visit_wiki_links(line, |link| visit(LocatedWikiLink { row, source: line, link }));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownLinkKind {
    Link,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownLink<'a> {
    pub range: Range<usize>,
    pub label: &'a str,
    pub destination: &'a str,
    pub kind: MarkdownLinkKind,
}

/// Parse `[label](destination)` or `![alt](destination)` at a byte offset.
pub fn markdown_link_at(source: &str, start: usize) -> Option<MarkdownLink<'_>> {
    let rest = source.get(start..)?;
    let (kind, prefix_len) = if rest.starts_with("![") {
        (MarkdownLinkKind::Image, 2)
    } else if rest.starts_with('[') && !rest.starts_with("[[") {
        (MarkdownLinkKind::Link, 1)
    } else {
        return None;
    };
    let label_end = rest[prefix_len..].find("](")? + prefix_len;
    let destination_start = label_end + 2;
    let destination_end = rest[destination_start..].find(')')? + destination_start;
    let destination = &rest[destination_start..destination_end];
    Some(MarkdownLink { range: start..start + destination_end + 1, label: &rest[prefix_len..label_end], destination, kind })
}

/// Return the byte length of a bare HTTP(S) URL at `start`.
pub fn bare_url_len(source: &str, start: usize) -> Option<usize> {
    let rest = source.get(start..)?;
    let scheme_len = if rest.starts_with("https://") {
        8
    } else if rest.starts_with("http://") {
        7
    } else {
        return None;
    };
    let mut end = rest.len();
    for (index, ch) in rest[scheme_len..].char_indices() {
        if ch.is_whitespace() || matches!(ch, ')' | ']' | '>' | '<' | '"' | '\'' | '|') {
            end = scheme_len + index;
            break;
        }
    }
    while end > scheme_len {
        let last = rest[..end].chars().next_back()?;
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    (end > scheme_len).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_match_supported_atx_boundaries() {
        assert_eq!(heading("# Title").unwrap(), Heading { level: 1, text: "Title" });
        assert_eq!(heading("###### Deep ###").unwrap(), Heading { level: 6, text: "Deep" });
        assert!(heading("#hashtag").is_none());
        assert!(heading("####### too deep").is_none());
    }

    #[test]
    fn frontmatter_requires_opening_and_closing_delimiters() {
        assert_eq!(frontmatter_end("---\ntitle: Note\n---\nBody"), Some(2));
        assert_eq!(frontmatter_end("---\ntitle: Note\nBody"), None);
        assert_eq!(frontmatter_end("Body\n---"), None);
    }

    #[test]
    fn wiki_link_parts_and_unicode_columns_are_stable() {
        let source = "前 [[folder/笔记#标题|别名]] 后";
        let link = wiki_links(source).pop().unwrap();
        assert_eq!(link.target, "folder/笔记");
        assert_eq!(link.heading, Some("标题"));
        assert_eq!(link.alias, Some("别名"));
        assert_eq!(link.display_text(), "别名");
        assert_eq!(link.char_range(source).start, 2);
    }

    #[test]
    fn wiki_scanner_rejects_empty_nested_and_inline_code_links() {
        let links = wiki_links("[[]] [[[nested]]] `[[code]]` [[real]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "real");
    }

    #[test]
    fn document_scanner_skips_frontmatter_and_both_fence_styles() {
        let content = "---\n[[meta]]\n---\n[[one]]\n```md\n[[code]]\n```\n~~~\n[[tilde]]\n~~~\n[[two]]";
        let links = document_wiki_links_with_tilde_fences(content, frontmatter_end(content), true);
        assert_eq!(links.iter().map(|item| item.link.target).collect::<Vec<_>>(), vec!["one", "two"]);
        assert_eq!(links.iter().map(|item| item.row).collect::<Vec<_>>(), vec![3, 10]);
    }

    #[test]
    fn streaming_and_collecting_document_scans_match() {
        let content = "[[one]] and [[two#heading|alias]]\n```\n[[code]]\n```\n[[three]]";
        let collected = document_wiki_links_with_tilde_fences(content, None, true);
        let mut streamed = Vec::new();
        visit_document_wiki_links_with_tilde_fences(content, None, true, |link| {
            streamed.push((link.row, link.link.range, link.link.raw));
        });
        let collected = collected.iter().map(|link| (link.row, link.link.range.clone(), link.link.raw)).collect::<Vec<_>>();
        assert_eq!(streamed, collected);
    }

    #[test]
    fn markdown_links_and_images_preserve_byte_ranges() {
        let source = "[label](https://example.test) ![alt](image.png)";
        let link = markdown_link_at(source, 0).unwrap();
        assert_eq!(link.label, "label");
        assert_eq!(link.destination, "https://example.test");
        assert_eq!(link.kind, MarkdownLinkKind::Link);
        let image_start = source.find("![").unwrap();
        assert_eq!(markdown_link_at(source, image_start).unwrap().kind, MarkdownLinkKind::Image);
        assert!(markdown_link_at("[[wiki]]", 0).is_none());
        assert_eq!(markdown_link_at("[empty]()", 0).unwrap().destination, "");
    }

    #[test]
    fn bare_urls_stop_at_delimiters_and_sentence_punctuation() {
        assert_eq!(bare_url_len("https://example.test.", 0), Some(20));
        assert_eq!(bare_url_len("(https://example.test)", 1), Some(20));
        assert_eq!(bare_url_len("ftp://example.test", 0), None);
    }
}
