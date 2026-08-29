//! Background thread worker for markdown syntax highlighting.
//!
//! This module provides reactive, non-blocking syntax highlighting by running
//! computations in a dedicated background thread. The editor sends content changes
//! to the worker, which computes all highlights and sends results back.

use ratatui::style::{Color, Modifier, Style};
use std::panic;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use ekphos_editor::{EditorSnapshot, HighlightRange, HighlightType, WikiLinkRange};

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
            heading_colors: [Color::Blue, Color::Green, Color::Yellow, Color::Magenta, Color::Cyan, Color::Gray],
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
    ticket: u64,
    pub snapshot: EditorSnapshot,
    pub version: u64,
    pub colors: HighlightColors,
    row_start: usize,
    row_end: usize,
}

#[derive(Debug)]
pub struct HighlightResult {
    pub version: u64,
    pub highlights: Vec<HighlightRange>,
    pub wiki_links: Vec<WikiLinkRange>,
}

#[derive(Default)]
struct RequestState {
    request: Option<HighlightRequest>,
    stop: bool,
}

struct Shared {
    request: Mutex<RequestState>,
    request_ready: Condvar,
    result: Mutex<Option<HighlightResult>>,
    current_ticket: AtomicU64,
    pending: AtomicBool,
    queued_snapshot_bytes: AtomicUsize,
    active_snapshot_bytes: AtomicUsize,
}

/// Handle to one managed background highlight worker. Both request and result
/// storage are single replaceable slots, so rapid edits cannot queue document
/// snapshots or completed highlight vectors without bound.
pub struct HighlightWorker {
    shared: Arc<Shared>,
    thread_handle: Option<JoinHandle<()>>,
}

impl HighlightWorker {
    pub fn new() -> Self {
        let shared = Arc::new(Shared { request: Mutex::new(RequestState::default()), request_ready: Condvar::new(), result: Mutex::new(None), current_ticket: AtomicU64::new(0), pending: AtomicBool::new(false), queued_snapshot_bytes: AtomicUsize::new(0), active_snapshot_bytes: AtomicUsize::new(0) });
        let worker_shared = Arc::clone(&shared);
        let thread_handle = thread::Builder::new().name("highlight-worker".into()).spawn(move || worker_thread_loop(worker_shared)).ok();
        Self { shared, thread_handle }
    }

    #[inline]
    pub fn request(&self, snapshot: EditorSnapshot, version: u64, colors: HighlightColors, rows: std::ops::Range<usize>) {
        if self.thread_handle.as_ref().is_none_or(JoinHandle::is_finished) {
            self.shared.pending.store(false, Ordering::Release);
            self.shared.queued_snapshot_bytes.store(0, Ordering::Release);
            self.shared.active_snapshot_bytes.store(0, Ordering::Release);
            if let Ok(mut result) = self.shared.result.lock() {
                *result = Some(HighlightResult { version, highlights: Vec::new(), wiki_links: Vec::new() });
            }
            return;
        }
        let ticket = self.shared.current_ticket.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        let snapshot_bytes = snapshot.reference_bytes();
        self.shared.pending.store(true, Ordering::Release);
        if let Ok(mut result) = self.shared.result.lock() {
            *result = None;
        }
        if let Ok(mut state) = self.shared.request.lock() {
            state.request = Some(HighlightRequest { ticket, snapshot, version, colors, row_start: rows.start, row_end: rows.end });
            self.shared.queued_snapshot_bytes.store(snapshot_bytes, Ordering::Release);
            self.shared.request_ready.notify_one();
        }
    }

    #[inline]
    pub fn try_recv(&self) -> Option<HighlightResult> {
        self.shared.result.lock().ok()?.take()
    }

    #[inline]
    pub fn drain_results(&self) {
        if let Ok(mut result) = self.shared.result.lock() {
            *result = None;
        }
    }

    pub fn cancel(&self) {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel);
        self.shared.pending.store(false, Ordering::Release);
        self.shared.queued_snapshot_bytes.store(0, Ordering::Release);
        if let Ok(mut state) = self.shared.request.lock() {
            state.request = None;
        }
        self.drain_results();
    }

    pub fn is_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }

    pub fn retained_bytes(&self) -> usize {
        let result_bytes = self.shared.result.lock().ok().and_then(|result| result.as_ref().map(|result| result.highlights.capacity() * std::mem::size_of::<HighlightRange>() + result.wiki_links.capacity() * std::mem::size_of::<WikiLinkRange>()));
        self.shared.queued_snapshot_bytes.load(Ordering::Acquire) + self.shared.active_snapshot_bytes.load(Ordering::Acquire) + result_bytes.unwrap_or(0)
    }
}

impl Default for HighlightWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HighlightWorker {
    fn drop(&mut self) {
        self.shared.current_ticket.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut state) = self.shared.request.lock() {
            state.stop = true;
            state.request = None;
            self.shared.request_ready.notify_one();
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// Main loop for the worker thread.
fn worker_thread_loop(shared: Arc<Shared>) {
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
            let request = state.request.take().expect("request checked above");
            shared.queued_snapshot_bytes.store(0, Ordering::Release);
            shared.active_snapshot_bytes.store(request.snapshot.reference_bytes(), Ordering::Release);
            request
        };
        let cancelled = || shared.current_ticket.load(Ordering::Acquire) != request.ticket;
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let rows = request.row_start..request.row_end;
            let (highlights, frontmatter_end) = compute_snapshot_highlights(&request.snapshot, &request.colors, rows.clone(), cancelled)?;
            let wiki_links = compute_snapshot_wiki_links(&request.snapshot, frontmatter_end, rows, cancelled)?;
            Some(HighlightResult { version: request.version, highlights, wiki_links })
        }));
        shared.active_snapshot_bytes.store(0, Ordering::Release);
        if cancelled() {
            continue;
        }
        let result = match result {
            Ok(result) => result,
            Err(_) => Some(HighlightResult { version: request.version, highlights: Vec::new(), wiki_links: Vec::new() }),
        };
        if let Some(result) = result {
            if let Ok(mut slot) = shared.result.lock() {
                *slot = Some(result);
            }
        }
        if !cancelled() {
            shared.pending.store(false, Ordering::Release);
        }
    }
}
fn compute_snapshot_highlights(snapshot: &EditorSnapshot, colors: &HighlightColors, rows: std::ops::Range<usize>, mut is_cancelled: impl FnMut() -> bool) -> Option<(Vec<HighlightRange>, Option<usize>)> {
    let row_end = rows.end.min(snapshot.line_count());
    let mut highlights = Vec::with_capacity(row_end.saturating_sub(rows.start).saturating_mul(2));
    let frontmatter_end = ekphos_core::markdown::frontmatter_end_in_lines(snapshot.iter_lines());
    let mut in_code_block = false;
    let mut in_math_block = false;
    for (row, line) in snapshot.iter_lines().take(row_end).enumerate() {
        if is_cancelled() {
            return None;
        }
        if let Some(fm_end) = frontmatter_end {
            if row <= fm_end {
                if rows.contains(&row) {
                    let char_count = bytecount_chars(line);
                    highlights.push(HighlightRange::new(row, 0, char_count, Style::default().fg(colors.frontmatter_color), HighlightType::Frontmatter));
                }
                continue;
            }
        }
        let trimmed = line.trim_start();
        if trimmed.len() >= 3 && trimmed.as_bytes()[0] == b'`' && trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            let start = line.len() - trimmed.len();
            let char_start = bytecount_chars(&line[..start]);
            if rows.contains(&row) {
                highlights.push(HighlightRange::new(row, char_start, char_start + bytecount_chars(trimmed), Style::default().fg(colors.code_color), HighlightType::CodeBlock));
            }
            continue;
        }
        if in_code_block {
            if rows.contains(&row) {
                highlights.push(HighlightRange::new(row, 0, bytecount_chars(line), Style::default().fg(colors.code_color), HighlightType::CodeBlock));
            }
            continue;
        }
        if ekphos_core::markdown::is_display_math_delimiter(line) {
            in_math_block = !in_math_block;
            if rows.contains(&row) {
                highlights.push(HighlightRange::new(row, 0, bytecount_chars(line), Style::default().fg(colors.link_color).add_modifier(Modifier::ITALIC), HighlightType::Math).with_priority(2));
            }
            continue;
        }
        if in_math_block {
            if rows.contains(&row) {
                highlights.push(HighlightRange::new(row, 0, bytecount_chars(line), Style::default().fg(colors.link_color).add_modifier(Modifier::ITALIC), HighlightType::Math).with_priority(2));
            }
            continue;
        }
        if rows.contains(&row) {
            highlight_markdown_line(row, line, colors, &mut highlights);
        }
    }
    Some((highlights, frontmatter_end))
}

#[inline]
fn bytecount_chars(s: &str) -> usize {
    s.chars().count()
}
fn highlight_markdown_line(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>) {
    if line.is_empty() {
        return;
    }
    let line_len = line.chars().count();
    if let Some(header_end) = detect_header_fast(line) {
        let level = line.chars().take_while(|&ch| ch == '#').count();
        let color = colors.heading_colors[level.saturating_sub(1).min(5)];
        highlights.push(HighlightRange::new(row, 0, header_end.min(line_len), Style::default().fg(color).add_modifier(Modifier::BOLD), HighlightType::Header));
        return;
    }
    if is_horizontal_rule(line) {
        highlights.push(HighlightRange::new(row, 0, line_len, Style::default().fg(colors.horizontal_rule_color), HighlightType::HorizontalRule));
        return;
    }
    let trimmed = line.trim_start();
    if !trimmed.is_empty() && trimmed.as_bytes()[0] == b'>' {
        let start = line.len() - trimmed.len();
        let char_start = line[..start].chars().count();
        highlights.push(HighlightRange::new(row, char_start, char_start + 1, Style::default().fg(colors.blockquote_color), HighlightType::Blockquote));
    }
    highlight_details_tags_fast(row, line, colors, highlights);
    highlight_list_marker_fast(row, line, trimmed, colors, highlights);
    highlight_inline_code_fast(row, line, colors, highlights);
    highlight_math_fast(row, line, colors, highlights);
    highlight_links_fast(row, line, colors, highlights);
    let highlight_start = highlights.len();
    highlight_bold_fast(row, line, colors, highlights, highlight_start);
    highlight_italic_fast(row, line, colors, highlights, highlight_start);
}

#[inline]
fn detect_header_fast(line: &str) -> Option<usize> {
    ekphos_core::markdown::heading(line.trim_start()).map(|_| line.chars().count())
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
fn highlight_details_tags_fast(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>) {
    let line_lower = line.to_ascii_lowercase();
    let bytes = line_lower.as_bytes();

    const TAGS: &[&[u8]] = &[b"<details>", b"</details>", b"<summary>", b"</summary>"];
    const TAG_LENS: &[usize] = &[9, 10, 9, 10];
    for (tag, &tag_len) in TAGS.iter().zip(TAG_LENS.iter()) {
        let mut pos = 0;
        while pos + tag_len <= bytes.len() {
            if let Some(found) = bytes[pos..].windows(tag_len).position(|w| w == *tag) {
                let abs_pos = pos + found;
                let start_col = line[..abs_pos].chars().count();
                let end_col = start_col + line[abs_pos..abs_pos + tag_len].chars().count();
                highlights.push(HighlightRange::new(row, start_col, end_col, Style::default().fg(colors.details_color), HighlightType::Details));
                pos = abs_pos + tag_len;
            } else {
                break;
            }
        }
    }
}

#[inline]
fn highlight_list_marker_fast(row: usize, line: &str, trimmed: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>) {
    if trimmed.is_empty() {
        return;
    }
    let indent_chars = line.len() - trimmed.len();
    let indent_char_count = line[..indent_chars].chars().count();
    let first_byte = trimmed.as_bytes()[0];
    if (first_byte == b'-' || first_byte == b'*' || first_byte == b'+') && trimmed.len() > 1 && trimmed.as_bytes()[1] == b' ' {
        highlights.push(HighlightRange::new(row, indent_char_count, indent_char_count + 1, Style::default().fg(colors.list_marker_color), HighlightType::ListMarker));
        if trimmed.len() >= 5 {
            let after = &trimmed[2..];
            if after.starts_with("[ ] ") || after.starts_with("[x] ") || after.starts_with("[X] ") {
                highlights.push(HighlightRange::new(row, indent_char_count + 2, indent_char_count + 5, Style::default().fg(colors.link_color), HighlightType::ListMarker));
            }
        }
        return;
    }
    if first_byte.is_ascii_digit() {
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if num_part.bytes().all(|b| b.is_ascii_digit()) {
                highlights.push(HighlightRange::new(row, indent_char_count, indent_char_count + dot_pos + 1, Style::default().fg(colors.list_marker_color), HighlightType::ListMarker));
            }
        }
    }
}

#[inline]
fn highlight_inline_code_fast(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>) {
    let mut chars = line.chars().enumerate().peekable();
    let mut open = None;
    while let Some((col, ch)) = chars.next() {
        if ch != '`' {
            continue;
        }
        if open.is_none() && chars.peek().is_some_and(|(_, next)| *next == '`') {
            chars.next();
            continue;
        }
        if let Some(start) = open.take() {
            highlights.push(HighlightRange::new(row, start, col + 1, Style::default().fg(colors.code_color), HighlightType::InlineCode).with_priority(2));
        } else {
            open = Some(col);
        }
    }
}

#[inline]
fn highlight_math_fast(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>) {
    if ekphos_core::markdown::display_math_body(line).is_some() {
        highlights.push(HighlightRange::new(row, 0, line.chars().count(), Style::default().fg(colors.link_color).add_modifier(Modifier::ITALIC), HighlightType::Math).with_priority(2));
        return;
    }
    ekphos_core::markdown::visit_inline_math(line, |expression| {
        let start = line[..expression.range.start].chars().count();
        let end = start + line[expression.range].chars().count();
        highlights.push(HighlightRange::new(row, start, end, Style::default().fg(colors.link_color).add_modifier(Modifier::ITALIC), HighlightType::Math).with_priority(2));
    });
}

#[inline]
fn highlight_links_fast(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>) {
    let check_from = highlights.len();
    let mut cursor = 0;
    while let Some(relative_start) = line[cursor..].find('[') {
        let start = cursor + relative_start;
        let Some(link) = ekphos_core::markdown::markdown_link_at(line, start) else {
            cursor = start + 1;
            continue;
        };
        let columns = link.range.start..link.range.end;
        let start_col = line[..columns.start].chars().count();
        let end_col = start_col + line[columns.clone()].chars().count();
        if !is_position_highlighted_fast(highlights, row, start_col, check_from) {
            highlights.push(HighlightRange::new(row, start_col, end_col, Style::default().fg(colors.link_color).add_modifier(Modifier::UNDERLINED), HighlightType::Link).with_priority(1));
        }
        cursor = link.range.end;
    }
}

#[inline]
fn is_position_highlighted_fast(highlights: &[HighlightRange], row: usize, col: usize, start_idx: usize) -> bool {
    highlights[..start_idx].iter().any(|h| h.row == row && col >= h.start_col && col < h.end_col)
}
fn highlight_bold_fast(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>, check_from: usize) {
    let mut chars = line.chars().enumerate().peekable();
    let mut star_open = None;
    let mut underscore_open = None;
    while let Some((col, marker)) = chars.next() {
        if !matches!(marker, '*' | '_') || chars.peek().is_none_or(|(_, next)| *next != marker) {
            continue;
        }
        chars.next();
        let open = if marker == '*' { &mut star_open } else { &mut underscore_open };
        if let Some(start) = open.take() {
            if !is_position_highlighted_fast(highlights, row, start, check_from) {
                let mut style = Style::default().add_modifier(Modifier::BOLD);
                if let Some(color) = colors.bold_color {
                    style = style.fg(color);
                }
                highlights.push(HighlightRange::new(row, start, col + 2, style, HighlightType::Bold));
            }
        } else {
            *open = Some(col);
        }
    }
}
fn highlight_italic_fast(row: usize, line: &str, colors: &HighlightColors, highlights: &mut Vec<HighlightRange>, check_from: usize) {
    let mut chars = line.chars().enumerate().peekable();
    let mut star_open = None;
    let mut underscore_open = None;
    let mut previous = None;
    while let Some((col, marker)) = chars.next() {
        if !matches!(marker, '*' | '_') {
            previous = Some(marker);
            continue;
        }
        if chars.peek().is_some_and(|(_, next)| *next == marker) {
            chars.next();
            previous = Some(marker);
            continue;
        }
        if previous == Some(marker) {
            previous = Some(marker);
            continue;
        }
        let open = if marker == '*' { &mut star_open } else { &mut underscore_open };
        if let Some(start) = open.take() {
            if !is_position_highlighted_fast(highlights, row, start, check_from) {
                let mut style = Style::default().add_modifier(Modifier::ITALIC);
                if let Some(color) = colors.italic_color {
                    style = style.fg(color);
                }
                highlights.push(HighlightRange::new(row, start, col + 1, style, HighlightType::Italic));
            }
        } else {
            *open = Some(col);
        }
        previous = Some(marker);
    }
}
fn compute_snapshot_wiki_links(snapshot: &EditorSnapshot, frontmatter_end: Option<usize>, rows: std::ops::Range<usize>, mut is_cancelled: impl FnMut() -> bool) -> Option<Vec<WikiLinkRange>> {
    let mut links = Vec::new();
    let mut in_code_block = false;
    for (row, line) in snapshot.iter_lines().take(rows.end).enumerate() {
        if is_cancelled() {
            return None;
        }
        if frontmatter_end.is_some_and(|end| row <= end) {
            continue;
        }
        match ekphos_core::markdown::fence_marker(line) {
            Some(ekphos_core::markdown::FenceMarker::Backtick) => {
                in_code_block = !in_code_block;
                continue;
            }
            Some(ekphos_core::markdown::FenceMarker::Tilde) => {}
            None if in_code_block => continue,
            None => {}
        }
        if rows.contains(&row) {
            ekphos_core::markdown::visit_wiki_links(line, |link| {
                let columns = link.char_range(line);
                links.push(WikiLinkRange { row, start_col: columns.start, end_col: columns.end, is_valid: false });
            });
        }
    }
    Some(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ekphos_editor::Editor;
    use std::time::{Duration, Instant};
    fn snapshot(content: &str) -> EditorSnapshot {
        Editor::from_text(content).snapshot()
    }
    fn compute_all_highlights(content: &str, colors: &HighlightColors) -> (Vec<HighlightRange>, Option<usize>) {
        let snapshot = snapshot(content);
        compute_snapshot_highlights(&snapshot, colors, 0..snapshot.line_count(), || false).expect("test computation is not cancelled")
    }
    fn compute_all_wiki_links(content: &str, frontmatter_end: Option<usize>) -> Vec<WikiLinkRange> {
        let snapshot = snapshot(content);
        compute_snapshot_wiki_links(&snapshot, frontmatter_end, 0..snapshot.line_count(), || false).expect("test computation is not cancelled")
    }

    #[test]
    fn test_detect_frontmatter() {
        let lines = vec!["---", "title: test", "---", "# Content"];
        assert_eq!(ekphos_core::markdown::frontmatter_end_in_lines(lines), Some(2));
        let lines_no_fm = vec!["# No frontmatter", "Content"];
        assert_eq!(ekphos_core::markdown::frontmatter_end_in_lines(lines_no_fm), None);
    }

    #[test]
    fn test_detect_header() {
        assert!(detect_header_fast("# Header 1").is_some());
        assert!(detect_header_fast("## Header 2").is_some());
        assert!(detect_header_fast("Not a header").is_none());
        assert!(detect_header_fast("#NoSpace").is_none());
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
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("", &colors);
        assert!(highlights.is_empty());
        let (highlights, _) = compute_all_highlights("   \n\t\n", &colors);
        assert!(highlights.is_empty());
        let long_line = "a".repeat(10000);
        let (highlights, _) = compute_all_highlights(&long_line, &colors);
        assert!(highlights.is_empty()); // No markdown syntax
        let unicode = "# 你好世界\n[[链接]] **粗体** *斜体*";
        let (highlights, _) = compute_all_highlights(unicode, &colors);
        assert!(!highlights.is_empty());
    }

    #[test]
    fn test_no_false_positive_headers() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("#hashtag", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Header), "Hashtag without space should not be a header");
        let (highlights, _) = compute_all_highlights("####### too many", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Header), "7+ hashes should not be a header");
        let (highlights, _) = compute_all_highlights("text # not header", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Header), "Hash in middle of line should not be a header");
    }

    #[test]
    fn test_no_false_positive_bold() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("single * star", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold), "Single * should not trigger bold");
        let (highlights, _) = compute_all_highlights("**unclosed bold", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold), "Unclosed ** should not be bold");
        let (highlights, _) = compute_all_highlights("snake_case_variable", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold), "snake_case should not trigger bold");
    }

    #[test]
    fn test_no_false_positive_italic() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("*unclosed italic", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Italic), "Unclosed * should not be italic");
        let (highlights, _) = compute_all_highlights("file_name.txt", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Italic), "Underscores in filenames should not trigger italic");
    }

    #[test]
    fn test_no_false_positive_links() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("[just brackets]", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link), "[text] without url should not be a link");
        let (highlights, _) = compute_all_highlights("(just parens)", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link), "(url) without text should not be a link");
        let (highlights, _) = compute_all_highlights("[text] (url)", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link), "[text] (url) with space should not be a link");
    }

    #[test]
    fn test_no_false_positive_inline_code() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("text `unclosed", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::InlineCode), "Unclosed backtick should not be inline code");
    }

    #[test]
    fn test_no_false_positive_wiki_links() {
        let links = compute_all_wiki_links("[[[nested]]]", None);
        assert!(links.is_empty(), "Nested brackets should not be wiki links");
        let links = compute_all_wiki_links("[single]", None);
        assert!(links.is_empty(), "Single brackets should not be wiki links");
        let links = compute_all_wiki_links("[[]]", None);
        assert!(links.is_empty(), "Empty wiki link should not be matched");
        let content = "```\n[[in code block]]\n```";
        let links = compute_all_wiki_links(content, None);
        assert!(links.is_empty(), "Wiki link in code block should not be matched");
    }

    #[test]
    fn test_no_false_positive_list_markers() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("-nospace", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::ListMarker), "Dash without space should not be list marker");
        let (highlights, _) = compute_all_highlights("123", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::ListMarker), "Number alone should not be list marker");
        let (highlights, _) = compute_all_highlights("1.nospace", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::ListMarker), "Number.text should not be list marker");
    }

    #[test]
    fn test_horizontal_rule_highlighting() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("---", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "--- should be a horizontal rule");
        let (highlights, _) = compute_all_highlights("***", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "*** should be a horizontal rule");
        let (highlights, _) = compute_all_highlights("___", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "___ should be a horizontal rule");
        let (highlights, _) = compute_all_highlights("- - -", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "- - - should be a horizontal rule");
        let (highlights, _) = compute_all_highlights("* * *", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "* * * should be a horizontal rule");
        let (highlights, _) = compute_all_highlights("-----", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "----- should be a horizontal rule");
        let (highlights, _) = compute_all_highlights("  ---  ", &colors);
        assert!(highlights.iter().any(|h| h.highlight_type == HighlightType::HorizontalRule), "  ---   should be a horizontal rule");
    }

    #[test]
    fn test_no_false_positive_horizontal_rules() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("--", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule), "-- should not be a horizontal rule");
        let (highlights, _) = compute_all_highlights("--*", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule), "--* should not be a horizontal rule");
        let (highlights, _) = compute_all_highlights("--- text", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule), "--- text should not be a horizontal rule");
        let (highlights, _) = compute_all_highlights("- item", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::HorizontalRule), "- item should not be a horizontal rule");
    }

    #[test]
    fn test_no_false_positive_details_tags() {
        let colors = HighlightColors::default();
        let mut highlights = Vec::new();
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
        let content = "```\n# Header\n**bold** *italic*\n- list\n[[link]]\n```";
        let (highlights, _) = compute_all_highlights(content, &colors);
        for h in &highlights {
            assert_eq!(h.highlight_type, HighlightType::CodeBlock, "Content inside code block should only have CodeBlock highlight type, got {:?}", h.highlight_type);
        }
    }

    #[test]
    fn test_inline_code_prevents_inner_highlighting() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("`**not bold**`", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Bold), "Bold markers inside inline code should not be highlighted");
        let (highlights, _) = compute_all_highlights("`[text](url)`", &colors);
        assert!(highlights.iter().all(|h| h.highlight_type != HighlightType::Link), "Link inside inline code should not be highlighted");
    }

    #[test]
    fn math_highlighting_covers_inline_and_display_source_but_not_code() {
        let colors = HighlightColors::default();
        let content = "Inline $x_1 + \\alpha$\n$$\n\\frac{1}{2}\n$$\n```md\n$not_math$\n```";
        let (highlights, _) = compute_all_highlights(content, &colors);
        assert!(highlights.iter().any(|highlight| highlight.row == 0 && highlight.highlight_type == HighlightType::Math));
        assert!(highlights.iter().any(|highlight| highlight.row == 2 && highlight.highlight_type == HighlightType::Math));
        assert!(highlights.iter().filter(|highlight| highlight.row == 5).all(|highlight| highlight.highlight_type == HighlightType::CodeBlock));
        assert!(highlights.iter().all(|highlight| !(highlight.row == 0 && highlight.highlight_type == HighlightType::Italic)));
    }

    #[test]
    fn test_frontmatter_prevents_highlighting() {
        let colors = HighlightColors::default();
        let content = "---\ntitle: # Not a header\ntags: **not bold**\n---\n# Real header";
        let (highlights, _) = compute_all_highlights(content, &colors);
        for h in highlights.iter().filter(|h| h.row <= 3) {
            assert_eq!(h.highlight_type, HighlightType::Frontmatter, "Content in frontmatter should only be Frontmatter type at row {}", h.row);
        }
        assert!(highlights.iter().any(|h| h.row == 4 && h.highlight_type == HighlightType::Header), "Header after frontmatter should be highlighted");
    }

    #[test]
    fn test_correct_column_positions() {
        let colors = HighlightColors::default();
        let (highlights, _) = compute_all_highlights("  # Header", &colors);
        let header = highlights.iter().find(|h| h.highlight_type == HighlightType::Header);
        assert!(header.is_some(), "Should find header");
        assert_eq!(header.unwrap().start_col, 0, "Header should start at column 0");
        let (highlights, _) = compute_all_highlights("  - item", &colors);
        let marker = highlights.iter().find(|h| h.highlight_type == HighlightType::ListMarker);
        assert!(marker.is_some(), "Should find list marker");
        assert_eq!(marker.unwrap().start_col, 2, "List marker should start at column 2");
        let (highlights, _) = compute_all_highlights("你好 **bold**", &colors);
        let bold = highlights.iter().find(|h| h.highlight_type == HighlightType::Bold);
        assert!(bold.is_some(), "Should find bold");
        assert_eq!(bold.unwrap().start_col, 3, "Bold should start at column 3 (after '你好 ')");
    }

    #[test]
    fn test_cjk_code_block_content() {
        let colors = HighlightColors::default();
        let content = "```python\nprint(\"你好世界\")\nx = \"测试\"\n```";
        let (highlights, _) = compute_all_highlights(content, &colors);
        let row1 = highlights.iter().find(|h| h.row == 1 && h.highlight_type == HighlightType::CodeBlock);
        assert!(row1.is_some(), "CJK content line should have CodeBlock highlight");
        assert_eq!(row1.unwrap().end_col, 13, "End col should count chars, not bytes");
        let row2 = highlights.iter().find(|h| h.row == 2 && h.highlight_type == HighlightType::CodeBlock);
        assert!(row2.is_some(), "Second CJK line should have CodeBlock highlight");
        assert_eq!(row2.unwrap().end_col, 8, "End col should count chars, not bytes");
    }

    #[test]
    fn test_cjk_blockquote() {
        let colors = HighlightColors::default();
        let content = "> 你好世界";
        let (highlights, _) = compute_all_highlights(content, &colors);
        let bq = highlights.iter().find(|h| h.highlight_type == HighlightType::Blockquote);
        assert!(bq.is_some(), "Should find blockquote highlight");
        assert_eq!(bq.unwrap().start_col, 0);
        assert_eq!(bq.unwrap().end_col, 1);
    }

    #[test]
    fn rapid_requests_keep_only_the_latest_snapshot_and_result() {
        let worker = HighlightWorker::new();
        let large_text = "# old revision\n".repeat(100_000);
        let large_snapshot = snapshot(&large_text);
        let small_snapshot = snapshot("# latest\n\n[[target]]");
        worker.request(large_snapshot.clone(), 1, HighlightColors::default(), 0..large_snapshot.line_count());
        for version in 2..=500 {
            worker.request(small_snapshot.clone(), version, HighlightColors::default(), 0..small_snapshot.line_count());
        }
        assert!(worker.retained_bytes() <= large_snapshot.reference_bytes() + small_snapshot.reference_bytes(), "the worker retained more than one active and one replaceable snapshot");
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            if let Some(result) = worker.try_recv() {
                break result;
            }
            assert!(Instant::now() < deadline, "latest highlight result timed out");
            std::thread::yield_now();
        };
        assert_eq!(result.version, 500);
        assert_eq!(result.wiki_links.len(), 1);
        assert!(!worker.is_pending());
        assert!(worker.try_recv().is_none());
    }

    #[test]
    fn highlight_results_are_scoped_to_the_active_row_window() {
        let content = (0..1_000).map(|row| format!("# heading {row} [[target-{row}]]")).collect::<Vec<_>>().join("\n");
        let snapshot = snapshot(&content);
        let (highlights, frontmatter) = compute_snapshot_highlights(&snapshot, &HighlightColors::default(), 400..420, || false).unwrap();
        let wiki_links = compute_snapshot_wiki_links(&snapshot, frontmatter, 400..420, || false).unwrap();
        assert!(!highlights.is_empty());
        assert!(highlights.iter().all(|highlight| (400..420).contains(&highlight.row)));
        assert_eq!(wiki_links.len(), 20);
        assert!(wiki_links.iter().all(|link| (400..420).contains(&link.row)));
    }

    #[test]
    fn worker_shutdown_cancels_large_stale_work_and_joins() {
        let worker = HighlightWorker::new();
        let snapshot = snapshot(&"# heading **bold** [[target]]\n".repeat(200_000));
        worker.request(snapshot.clone(), 1, HighlightColors::default(), 0..snapshot.line_count());
        let started = Instant::now();
        drop(worker);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
