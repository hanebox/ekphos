//! Background thread worker for markdown syntax highlighting.
//!
//! This module provides reactive, non-blocking syntax highlighting by running
//! computations in a dedicated background thread. The editor sends content changes
//! to the worker, which computes all highlights and sends results back.

use ratatui::style::{Color, Modifier, Style};
use std::panic;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

use crate::editor::{HighlightRange, HighlightType, WikiLinkRange};

#[derive(Debug, Clone)]
pub struct HighlightColors {
    pub heading_colors: [Color; 6],
    pub code_color: Color,
    pub link_color: Color,
    pub blockquote_color: Color,
    pub list_marker_color: Color,
    pub bold_color: Option<Color>,
    pub italic_color: Option<Color>,
    pub frontmatter_color: Color,
    pub details_color: Color,
    pub horizontal_rule_color: Color,
}

impl Default for HighlightColors {
    fn default() -> Self {
        Self {
            heading_colors: [
                Color::Blue,
                Color::Green,
                Color::Yellow,
                Color::Magenta,
                Color::Cyan,
                Color::Gray,
            ],
            code_color: Color::Green,
            link_color: Color::Cyan,
            blockquote_color: Color::Cyan,
            list_marker_color: Color::Yellow,
            bold_color: None,
            italic_color: None,
            frontmatter_color: Color::DarkGray,
            details_color: Color::Magenta,
            horizontal_rule_color: Color::DarkGray,
        }
    }
}

#[derive(Debug)]
pub struct HighlightRequest {
    pub content: String,
    pub version: u64,
    pub colors: HighlightColors,
}

#[derive(Debug)]
pub struct HighlightResult {
    pub version: u64,
    pub highlights: Vec<HighlightRange>,
    pub wiki_links: Vec<WikiLinkRange>,
}

/// Handle to the background highlight worker
pub struct HighlightWorker {
    request_sender: Sender<HighlightRequest>,
    result_receiver: Receiver<HighlightResult>,
    #[allow(dead_code)]
    thread_handle: JoinHandle<()>,
}

impl HighlightWorker {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<HighlightRequest>();
        let (result_tx, result_rx) = mpsc::channel::<HighlightResult>();

        let thread_handle = thread::Builder::new()
            .name("highlight-worker".into())
            .spawn(move || {
                worker_thread_loop(request_rx, result_tx);
            })
            .expect("Failed to spawn highlight worker thread");

        Self {
            request_sender: request_tx,
            result_receiver: result_rx,
            thread_handle,
        }
    }

    #[inline]
    pub fn request(&self, content: String, version: u64, colors: HighlightColors) {
        let request = HighlightRequest {
            content,
            version,
            colors,
        };
        let _ = self.request_sender.send(request);
    }

    #[inline]
    pub fn try_recv(&self) -> Option<HighlightResult> {
        match self.result_receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    #[inline]
    pub fn drain_results(&self) {
        while self.result_receiver.try_recv().is_ok() {}
    }
}

impl Default for HighlightWorker {
    fn default() -> Self {
        Self::new()
    }
}

/// Main loop for the worker thread
fn worker_thread_loop(receiver: Receiver<HighlightRequest>, sender: Sender<HighlightResult>) {
    while let Ok(request) = receiver.recv() {
        let mut latest_request = request;
        while let Ok(newer) = receiver.try_recv() {
            latest_request = newer;
        }

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let (highlights, frontmatter_end) =
                compute_all_highlights(&latest_request.content, &latest_request.colors);
            let wiki_links = compute_all_wiki_links(&latest_request.content, frontmatter_end);

            HighlightResult {
                version: latest_request.version,
                highlights,
                wiki_links,
            }
        }));

        match result {
            Ok(highlight_result) => {
                if sender.send(highlight_result).is_err() {
                    break;
                }
            }
            Err(_) => {
                let empty_result = HighlightResult {
                    version: latest_request.version,
                    highlights: Vec::new(),
                    wiki_links: Vec::new(),
                };
                if sender.send(empty_result).is_err() {
                    break;
                }
            }
        }
    }
}

fn compute_all_highlights(
    content: &str,
    colors: &HighlightColors,
) -> (Vec<HighlightRange>, Option<usize>) {
    let line_count = content.lines().count();
    let mut highlights = Vec::with_capacity(line_count * 2);
    let lines: Vec<&str> = content.lines().collect();
    let frontmatter_end = detect_frontmatter_end(&lines);

    let mut in_code_block = false;

    for (row, line) in lines.iter().enumerate() {
        if let Some(fm_end) = frontmatter_end {
            if row <= fm_end {
                let char_count = bytecount_chars(line);
                highlights.push(HighlightRange::new(
                    row,
                    0,
                    char_count,
                    Style::default().fg(colors.frontmatter_color),
                    HighlightType::Frontmatter,
                ));
                continue;
            }
        }

        let trimmed = line.trim_start();
        if trimmed.len() >= 3 && trimmed.as_bytes()[0] == b'`' && trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            let start = line.len() - trimmed.len();
            let char_start = bytecount_chars(&line[..start]);
            highlights.push(HighlightRange::new(
                row,
                char_start,
                char_start + bytecount_chars(trimmed),
                Style::default().fg(colors.code_color),
                HighlightType::CodeBlock,
            ));
            continue;
        }

        if in_code_block {
            highlights.push(HighlightRange::new(
                row,
                0,
                bytecount_chars(line),
                Style::default().fg(colors.code_color),
                HighlightType::CodeBlock,
            ));
            continue;
        }

        // Normal markdown highlighting
        highlight_markdown_line(row, line, colors, &mut highlights);
    }

    (highlights, frontmatter_end)
}

#[inline]
fn bytecount_chars(s: &str) -> usize {
    s.chars().count()
}

#[inline]
fn detect_frontmatter_end(lines: &[&str]) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    let first_line = lines[0].trim();
    if first_line != "---" {
        return None;
    }

    for (row, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            return Some(row);
        }
    }

    None
}

fn highlight_markdown_line(
    row: usize,
    line: &str,
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
) {
    if line.is_empty() {
        return;
    }

    let chars: Vec<char> = line.chars().collect();
    let line_len = chars.len();

    if let Some(header_end) = detect_header_fast(line, &chars) {
        let level = chars.iter().take_while(|&&c| c == '#').count();
        let color = colors.heading_colors[level.saturating_sub(1).min(5)];
        highlights.push(HighlightRange::new(
            row,
            0,
            header_end.min(line_len),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
            HighlightType::Header,
        ));
        return;
    }
    if is_horizontal_rule(line) {
        highlights.push(HighlightRange::new(
            row,
            0,
            line_len,
            Style::default().fg(colors.horizontal_rule_color),
            HighlightType::HorizontalRule,
        ));
        return;
    }

    let trimmed = line.trim_start();
    if !trimmed.is_empty() && trimmed.as_bytes()[0] == b'>' {
        let start = line.len() - trimmed.len();
        let char_start = line[..start].chars().count();
        highlights.push(HighlightRange::new(
            row,
            char_start,
            char_start + 1,
            Style::default().fg(colors.blockquote_color),
            HighlightType::Blockquote,
        ));
    }

    highlight_details_tags_fast(row, line, colors, highlights);
    highlight_list_marker_fast(row, line, trimmed, colors, highlights);
    highlight_inline_code_fast(row, &chars, colors, highlights);
    highlight_links_fast(row, &chars, colors, highlights);
    let highlight_start = highlights.len();
    highlight_bold_fast(row, &chars, colors, highlights, highlight_start);
    highlight_italic_fast(row, &chars, colors, highlights, highlight_start);
}

#[inline]
fn detect_header_fast(line: &str, chars: &[char]) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.as_bytes()[0] != b'#' {
        return None;
    }

    let hash_count = chars.iter().skip_while(|c| c.is_whitespace()).take_while(|&&c| c == '#').count();
    if hash_count == 0 || hash_count > 6 {
        return None;
    }

    let trimmed_chars: Vec<char> = trimmed.chars().collect();
    if trimmed_chars.len() == hash_count || trimmed_chars.get(hash_count) == Some(&' ') {
        return Some(chars.len());
    }

    None
}

#[inline]
fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed.len() < 3 {
        return false;
    }

    let bytes = trimmed.as_bytes();
    let first = bytes[0];

    if first != b'-' && first != b'*' && first != b'_' {
        return false;
    }

    let mut count = 0;
    for &b in bytes {
        if b == first {
            count += 1;
        } else if b != b' ' {
            return false;
        }
    }

    count >= 3
}

fn highlight_details_tags_fast(
    row: usize,
    line: &str,
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
) {
    let line_lower = line.to_ascii_lowercase();
    let bytes = line_lower.as_bytes();

    const TAGS: &[&[u8]] = &[
        b"<details>",
        b"</details>",
        b"<summary>",
        b"</summary>",
    ];
    const TAG_LENS: &[usize] = &[9, 10, 9, 10];

    for (tag, &tag_len) in TAGS.iter().zip(TAG_LENS.iter()) {
        let mut pos = 0;
        while pos + tag_len <= bytes.len() {
            if let Some(found) = bytes[pos..].windows(tag_len).position(|w| w == *tag) {
                let abs_pos = pos + found;
                let start_col = line[..abs_pos].chars().count();
                let end_col = start_col + line[abs_pos..abs_pos + tag_len].chars().count();

                highlights.push(HighlightRange::new(
                    row,
                    start_col,
                    end_col,
                    Style::default().fg(colors.details_color),
                    HighlightType::Details,
                ));

                pos = abs_pos + tag_len;
            } else {
                break;
            }
        }
    }
}

#[inline]
fn highlight_list_marker_fast(
    row: usize,
    line: &str,
    trimmed: &str,
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
) {
    if trimmed.is_empty() {
        return;
    }

    let indent_chars = line.len() - trimmed.len();
    let indent_char_count = line[..indent_chars].chars().count();
    let first_byte = trimmed.as_bytes()[0];

    if (first_byte == b'-' || first_byte == b'*' || first_byte == b'+')
        && trimmed.len() > 1
        && trimmed.as_bytes()[1] == b' '
    {
        highlights.push(HighlightRange::new(
            row,
            indent_char_count,
            indent_char_count + 1,
            Style::default().fg(colors.list_marker_color),
            HighlightType::ListMarker,
        ));

        if trimmed.len() >= 5 {
            let after = &trimmed[2..];
            if after.starts_with("[ ] ")
                || after.starts_with("[x] ")
                || after.starts_with("[X] ")
            {
                highlights.push(HighlightRange::new(
                    row,
                    indent_char_count + 2,
                    indent_char_count + 5,
                    Style::default().fg(colors.link_color),
                    HighlightType::ListMarker,
                ));
            }
        }
        return;
    }

    if first_byte.is_ascii_digit() {
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if num_part.bytes().all(|b| b.is_ascii_digit()) {
                highlights.push(HighlightRange::new(
                    row,
                    indent_char_count,
                    indent_char_count + dot_pos + 1,
                    Style::default().fg(colors.list_marker_color),
                    HighlightType::ListMarker,
                ));
            }
        }
    }
}

#[inline]
fn highlight_inline_code_fast(
    row: usize,
    chars: &[char],
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
) {
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '`' {
            if i + 1 < len && chars[i + 1] == '`' {
                i += 2;
                continue;
            }

            let mut j = i + 1;
            while j < len {
                if chars[j] == '`' {
                    highlights.push(
                        HighlightRange::new(
                            row,
                            i,
                            j + 1,
                            Style::default().fg(colors.code_color),
                            HighlightType::InlineCode,
                        )
                        .with_priority(2),
                    );
                    i = j + 1;
                    break;
                }
                j += 1;
            }
            if j >= len {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

#[inline]
fn highlight_links_fast(
    row: usize,
    chars: &[char],
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
) {
    let len = chars.len();
    let mut i = 0;

    let check_from = highlights.len();

    while i < len {
        if chars[i] == '[' {
            if is_position_highlighted_fast(highlights, row, i, check_from) {
                i += 1;
                continue;
            }

            let mut j = i + 1;
            while j < len && chars[j] != ']' {
                j += 1;
            }

            if j < len && j + 1 < len && chars[j + 1] == '(' {
                let mut k = j + 2;
                while k < len && chars[k] != ')' {
                    k += 1;
                }

                if k < len {
                    highlights.push(
                        HighlightRange::new(
                            row,
                            i,
                            k + 1,
                            Style::default()
                                .fg(colors.link_color)
                                .add_modifier(Modifier::UNDERLINED),
                            HighlightType::Link,
                        )
                        .with_priority(1),
                    );
                    i = k + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
}

#[inline]
fn is_position_highlighted_fast(
    highlights: &[HighlightRange],
    row: usize,
    col: usize,
    start_idx: usize,
) -> bool {
    highlights[..start_idx]
        .iter()
        .any(|h| h.row == row && col >= h.start_col && col < h.end_col)
}

fn highlight_bold_fast(
    row: usize,
    chars: &[char],
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
    check_from: usize,
) {
    let len = chars.len();
    if len < 4 {
        return;
    }

    let mut i = 0;
    while i + 3 < len {
        let c = chars[i];
        if (c == '*' || c == '_') && chars[i + 1] == c {
            let mut j = i + 2;
            while j + 1 < len {
                if chars[j] == c && chars[j + 1] == c {
                    if !is_position_highlighted_fast(highlights, row, i, check_from) {
                        let mut style = Style::default().add_modifier(Modifier::BOLD);
                        if let Some(color) = colors.bold_color {
                            style = style.fg(color);
                        }
                        highlights.push(HighlightRange::new(row, i, j + 2, style, HighlightType::Bold));
                    }
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= len {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

fn highlight_italic_fast(
    row: usize,
    chars: &[char],
    colors: &HighlightColors,
    highlights: &mut Vec<HighlightRange>,
    check_from: usize,
) {
    let len = chars.len();
    if len < 2 {
        return;
    }

    let mut i = 0;
    while i < len {
        let c = chars[i];
        if c == '*' || c == '_' {
            if i + 1 < len && chars[i + 1] == c {
                i += 2;
                continue;
            }
            if i > 0 && chars[i - 1] == c {
                i += 1;
                continue;
            }

            let mut j = i + 1;
            while j < len {
                if chars[j] == c {
                    if j + 1 < len && chars[j + 1] == c {
                        j += 2;
                        continue;
                    }
                    if !is_position_highlighted_fast(highlights, row, i, check_from) {
                        let mut style = Style::default().add_modifier(Modifier::ITALIC);
                        if let Some(color) = colors.italic_color {
                            style = style.fg(color);
                        }
                        highlights.push(HighlightRange::new(row, i, j + 1, style, HighlightType::Italic));
                    }
                    i = j + 1;
                    break;
                }
                j += 1;
            }
            if j >= len {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
}

fn compute_all_wiki_links(content: &str, frontmatter_end: Option<usize>) -> Vec<WikiLinkRange> {
    let line_count = content.lines().count();
    let mut wiki_links = Vec::with_capacity(line_count / 4); 
    let mut in_code_block = false;

    for (row, line) in content.lines().enumerate() {
        if let Some(fm_end) = frontmatter_end {
            if row <= fm_end {
                continue;
            }
        }

        let trimmed = line.trim_start();
        if trimmed.len() >= 3 && trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        if !line.contains("[[") {
            continue;
        }

        let mut search_start = 0;

        while search_start < line.len() {
            let remaining = &line[search_start..];

            if let Some(backtick_pos) = remaining.find('`') {
                if let Some(wiki_pos) = remaining.find("[[") {
                    if backtick_pos < wiki_pos {
                        let abs_backtick = search_start + backtick_pos;
                        if let Some(close_backtick) = line[abs_backtick + 1..].find('`') {
                            search_start = abs_backtick + 1 + close_backtick + 1;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            if let Some(start_pos) = remaining.find("[[") {
                let abs_start = search_start + start_pos;
                let after_brackets = &line[abs_start + 2..];

                if let Some(end_pos) = after_brackets.find("]]") {
                    let raw_content = &after_brackets[..end_pos];

                    if !raw_content.is_empty()
                        && !raw_content.as_bytes().contains(&b'[')
                        && !raw_content.as_bytes().contains(&b']')
                    {
                        let start_col = line[..abs_start].chars().count();
                        let end_col = start_col + 2 + raw_content.chars().count() + 2;

                        wiki_links.push(WikiLinkRange {
                            row,
                            start_col,
                            end_col,
                            is_valid: false,
                        });
                    }

                    search_start = abs_start + 2 + end_pos + 2;
                    continue;
                }
            }
            break;
        }
    }

    wiki_links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_frontmatter() {
        let lines = vec!["---", "title: test", "---", "# Content"];
        assert_eq!(detect_frontmatter_end(&lines), Some(2));

        let lines_no_fm = vec!["# No frontmatter", "Content"];
        assert_eq!(detect_frontmatter_end(&lines_no_fm), None);
    }

    #[test]
    fn test_detect_header() {
        let chars1: Vec<char> = "# Header 1".chars().collect();
        assert!(detect_header_fast("# Header 1", &chars1).is_some());

        let chars2: Vec<char> = "## Header 2".chars().collect();
        assert!(detect_header_fast("## Header 2", &chars2).is_some());

        let chars3: Vec<char> = "Not a header".chars().collect();
        assert!(detect_header_fast("Not a header", &chars3).is_none());

        let chars4: Vec<char> = "#NoSpace".chars().collect();
        assert!(detect_header_fast("#NoSpace", &chars4).is_none());
    }

    #[test]
    fn test_compute_wiki_links() {
        let content = "[[link1]] and [[link2]]";
        let links = compute_all_wiki_links(content, None);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].start_col, 0);
        assert_eq!(links[0].end_col, 9);
        assert_eq!(links[1].start_col, 14);
    }

    #[test]
    fn test_wiki_links_skip_code() {
        let content = "`[[not a link]]` and [[real link]]";
        let links = compute_all_wiki_links(content, None);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].start_col, 21);
    }

    #[test]
    fn test_details_tags_highlighting() {
        let colors = HighlightColors::default();
        let mut highlights = Vec::new();

        highlight_details_tags_fast(0, "<details>", &colors, &mut highlights);
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].start_col, 0);
        assert_eq!(highlights[0].end_col, 9);
        assert_eq!(highlights[0].highlight_type, HighlightType::Details);

        highlights.clear();
        highlight_details_tags_fast(0, "<summary>Click to expand</summary>", &colors, &mut highlights);
        assert_eq!(highlights.len(), 2); // <summary> and </summary>

        highlights.clear();
        highlight_details_tags_fast(0, "</details>", &colors, &mut highlights);
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].end_col, 10);
    }

    #[test]
    fn test_panic_safety() {
        // Test that worker handles edge cases without panicking
        let colors = HighlightColors::default();

        // Empty content
        let (highlights, _) = compute_all_highlights("", &colors);
        assert!(highlights.is_empty());

        // Only whitespace
        let (highlights, _) = compute_all_highlights("   \n\t\n", &colors);
        assert!(highlights.is_empty());

        // Very long line
        let long_line = "a".repeat(10000);
        let (highlights, _) = compute_all_highlights(&long_line, &colors);
        assert!(highlights.is_empty()); // No markdown syntax

        // Unicode content
        let unicode = "# 你好世界\n[[链接]] **粗体** *斜体*";
        let (highlights, _) = compute_all_highlights(unicode, &colors);
        assert!(!highlights.is_empty());
    }

    // ==================== FALSE POSITIVE TESTS ====================

    #[test]
    fn test_no_false_positive_headers() {
        let colors = HighlightColors::default();

        // #hashtag without space should NOT be a header
        let (highlights, _) = compute_all_highlights("#hashtag", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Header),
            "Hashtag without space should not be a header");

        // ####### (7 hashes) should NOT be a header
        let (highlights, _) = compute_all_highlights("####### too many", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Header),
            "7+ hashes should not be a header");

        // # in middle of line should NOT be a header
        let (highlights, _) = compute_all_highlights("text # not header", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Header),
            "Hash in middle of line should not be a header");
    }

    #[test]
    fn test_no_false_positive_bold() {
        let colors = HighlightColors::default();

        // Single * should NOT be bold
        let (highlights, _) = compute_all_highlights("single * star", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold),
            "Single * should not trigger bold");

        // Unclosed ** should NOT be bold
        let (highlights, _) = compute_all_highlights("**unclosed bold", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold),
            "Unclosed ** should not be bold");

        // snake_case should NOT be bold (mid-word underscores)
        let (highlights, _) = compute_all_highlights("snake_case_variable", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold),
            "snake_case should not trigger bold");
    }

    #[test]
    fn test_no_false_positive_italic() {
        let colors = HighlightColors::default();

        // Unclosed * should NOT be italic
        let (highlights, _) = compute_all_highlights("*unclosed italic", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Italic),
            "Unclosed * should not be italic");

        // file_name.txt should NOT trigger italic
        let (highlights, _) = compute_all_highlights("file_name.txt", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Italic),
            "Underscores in filenames should not trigger italic");
    }

    #[test]
    fn test_no_false_positive_links() {
        let colors = HighlightColors::default();

        // [text] without (url) should NOT be a link
        let (highlights, _) = compute_all_highlights("[just brackets]", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link),
            "[text] without url should not be a link");

        // (url) without [text] should NOT be a link
        let (highlights, _) = compute_all_highlights("(just parens)", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link),
            "(url) without text should not be a link");

        // [text] (url) with space should NOT be a link
        let (highlights, _) = compute_all_highlights("[text] (url)", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link),
            "[text] (url) with space should not be a link");
    }

    #[test]
    fn test_no_false_positive_inline_code() {
        let colors = HighlightColors::default();

        // Single backtick without closing should NOT be inline code
        let (highlights, _) = compute_all_highlights("text `unclosed", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::InlineCode),
            "Unclosed backtick should not be inline code");
    }

    #[test]
    fn test_no_false_positive_wiki_links() {
        // Nested brackets should NOT be wiki links
        let links = compute_all_wiki_links("[[[nested]]]", None);
        assert!(links.is_empty(), "Nested brackets should not be wiki links");

        // Single brackets should NOT be wiki links
        let links = compute_all_wiki_links("[single]", None);
        assert!(links.is_empty(), "Single brackets should not be wiki links");

        // Empty wiki link should NOT be matched
        let links = compute_all_wiki_links("[[]]", None);
        assert!(links.is_empty(), "Empty wiki link should not be matched");

        // Wiki link in code block should NOT be matched
        let content = "```\n[[in code block]]\n```";
        let links = compute_all_wiki_links(content, None);
        assert!(links.is_empty(), "Wiki link in code block should not be matched");
    }

    #[test]
    fn test_no_false_positive_list_markers() {
        let colors = HighlightColors::default();

        // Dash without space should NOT be a list marker
        let (highlights, _) = compute_all_highlights("-nospace", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::ListMarker),
            "Dash without space should not be list marker");

        // Number without dot+space should NOT be a list marker
        let (highlights, _) = compute_all_highlights("123", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::ListMarker),
            "Number alone should not be list marker");

        // Number with dot but no space should NOT be a list marker
        let (highlights, _) = compute_all_highlights("1.nospace", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::ListMarker),
            "Number.text should not be list marker");
    }

    #[test]
    fn test_horizontal_rule_highlighting() {
        let colors = HighlightColors::default();

        // Basic horizontal rules
        let (highlights, _) = compute_all_highlights("---", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "--- should be a horizontal rule");

        let (highlights, _) = compute_all_highlights("***", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "*** should be a horizontal rule");

        let (highlights, _) = compute_all_highlights("___", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "___ should be a horizontal rule");

        // With spaces
        let (highlights, _) = compute_all_highlights("- - -", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "- - - should be a horizontal rule");

        let (highlights, _) = compute_all_highlights("* * *", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "* * * should be a horizontal rule");

        // More than 3
        let (highlights, _) = compute_all_highlights("-----", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "----- should be a horizontal rule");

        // With leading/trailing whitespace
        let (highlights, _) = compute_all_highlights("  ---  ", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule),
            "  ---   should be a horizontal rule");
    }

    #[test]
    fn test_no_false_positive_horizontal_rules() {
        let colors = HighlightColors::default();

        // Only 2 dashes should NOT be a horizontal rule
        let (highlights, _) = compute_all_highlights("--", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule),
            "-- should not be a horizontal rule");

        // Mixed characters should NOT be a horizontal rule
        let (highlights, _) = compute_all_highlights("--*", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule),
            "--* should not be a horizontal rule");

        // Text after dashes should NOT be a horizontal rule
        let (highlights, _) = compute_all_highlights("--- text", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule),
            "--- text should not be a horizontal rule");

        // List marker should NOT be a horizontal rule
        let (highlights, _) = compute_all_highlights("- item", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule),
            "- item should not be a horizontal rule");
    }

    #[test]
    fn test_no_false_positive_details_tags() {
        let colors = HighlightColors::default();
        let mut highlights = Vec::new();

        // Partial tags should NOT be highlighted
        highlight_details_tags_fast(0, "<detail>", &colors, &mut highlights);
        assert!(highlights.is_empty(), "<detail> (missing s) should not match");

        highlights.clear();
        highlight_details_tags_fast(0, "<summar>", &colors, &mut highlights);
        assert!(highlights.is_empty(), "<summar> (missing y) should not match");

        highlights.clear();
        highlight_details_tags_fast(0, "details>", &colors, &mut highlights);
        assert!(highlights.is_empty(), "details> (missing <) should not match");
    }

    #[test]
    fn test_code_block_prevents_all_highlighting() {
        let colors = HighlightColors::default();

        // Everything inside code block should only be CodeBlock type
        let content = "```\n# Header\n**bold** *italic*\n- list\n[[link]]\n```";
        let (highlights, _) = compute_all_highlights(content, &colors);

        // All highlights should be CodeBlock type
        for h in &highlights {
            assert_eq!(h.highlight_type, HighlightType::CodeBlock,
                "Content inside code block should only have CodeBlock highlight type, got {:?}", h.highlight_type);
        }
    }

    #[test]
    fn test_inline_code_prevents_inner_highlighting() {
        let colors = HighlightColors::default();

        // Bold inside inline code should NOT be highlighted as bold
        let (highlights, _) = compute_all_highlights("`**not bold**`", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold),
            "Bold markers inside inline code should not be highlighted");

        // Link inside inline code should NOT be highlighted as link
        let (highlights, _) = compute_all_highlights("`[text](url)`", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link),
            "Link inside inline code should not be highlighted");
    }

    #[test]
    fn test_frontmatter_prevents_highlighting() {
        let colors = HighlightColors::default();

        // Content in frontmatter should only be Frontmatter type
        let content = "---\ntitle: # Not a header\ntags: **not bold**\n---\n# Real header";
        let (highlights, _) = compute_all_highlights(content, &colors);

        // First 4 lines (0-3) should be frontmatter
        for h in highlights.iter().filter(|h| h.row <= 3) {
            assert_eq!(h.highlight_type, HighlightType::Frontmatter,
                "Content in frontmatter should only be Frontmatter type at row {}", h.row);
        }

        // Line 4 should have a header
        assert!(highlights.iter().any(|h| h.row == 4 && h.highlight_type == HighlightType::Header),
            "Header after frontmatter should be highlighted");
    }

    #[test]
    fn test_correct_column_positions() {
        let colors = HighlightColors::default();

        // Test that highlight positions are correct
        let (highlights, _) = compute_all_highlights("  # Header", &colors);
        let header = highlights.iter().find(|h| h.highlight_type == HighlightType::Header);
        assert!(header.is_some(), "Should find header");
        assert_eq!(header.unwrap().start_col, 0, "Header should start at column 0");

        // Test list marker position with indent
        let (highlights, _) = compute_all_highlights("  - item", &colors);
        let marker = highlights.iter().find(|h| h.highlight_type == HighlightType::ListMarker);
        assert!(marker.is_some(), "Should find list marker");
        assert_eq!(marker.unwrap().start_col, 2, "List marker should start at column 2");

        // Test unicode positions
        let (highlights, _) = compute_all_highlights("你好 **bold**", &colors);
        let bold = highlights.iter().find(|h| h.highlight_type == HighlightType::Bold);
        assert!(bold.is_some(), "Should find bold");
        assert_eq!(bold.unwrap().start_col, 3, "Bold should start at column 3 (after '你好 ')");
    }
}
