mod buffer;
mod clipboard;
mod cursor;
mod history;
mod input;
mod wrap;

pub use clipboard::{Clipboard, ClipboardError, ClipboardResult, MemoryClipboard};
pub use cursor::{CursorMove, Position};
pub use input::{process_key, InputAction};

pub use buffer::EditorSnapshot;
use buffer::TextBuffer;
use cursor::Cursor;
pub use highlighting::MarkdownColors;
pub use history::HistoryStats;
use history::{EditOperation, History};
use wrap::WrapCache;

use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Widget},
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LineNumberMode {
    None,
    #[default]
    Absolute,
    Relative,
    Hybrid,
}

#[inline]
fn char_display_width(ch: char, tab_width: u16) -> u16 {
    if ch == '\t' {
        tab_width
    } else {
        ch.width().unwrap_or(1) as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightType {
    WikiLink,
    Header,
    Bold,
    Italic,
    InlineCode,
    Math,
    CodeBlock,
    Link,
    Blockquote,
    ListMarker,
    HorizontalRule,
    SearchMatch,
    SearchMatchCurrent,
    Frontmatter,
    Details,
    Custom(u8),
}

#[derive(Debug, Clone)]
pub struct HighlightRange {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub style: Style,
    pub highlight_type: HighlightType,
    pub priority: u8,
}

impl HighlightRange {
    pub fn new(row: usize, start_col: usize, end_col: usize, style: Style, highlight_type: HighlightType) -> Self {
        Self { row, start_col, end_col, style, highlight_type, priority: 0 }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.row == row && col >= self.start_col && col < self.end_col
    }
}

#[derive(Debug, Clone, Default)]
struct HighlightIndex {
    by_row: BTreeMap<usize, Vec<HighlightRange>>,
    recency: VecDeque<usize>,
    retained_bytes: usize,
}

impl HighlightIndex {
    const MAX_ROWS: usize = 512;
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    fn new() -> Self {
        Self { by_row: BTreeMap::new(), recency: VecDeque::new(), retained_bytes: 0 }
    }
    fn insert(&mut self, highlight: HighlightRange) {
        let row = highlight.row;
        if !self.by_row.contains_key(&row) {
            self.recency.push_back(row);
        }
        let ranges = self.by_row.entry(row).or_default();
        let old_bytes = ranges.capacity() * std::mem::size_of::<HighlightRange>();
        ranges.push(highlight);
        self.retained_bytes += ranges.capacity() * std::mem::size_of::<HighlightRange>() - old_bytes;
        self.enforce_limits();
    }
    fn get_row(&self, row: usize) -> &[HighlightRange] {
        self.by_row.get(&row).map(|v| v.as_slice()).unwrap_or(&[])
    }
    fn clear_row(&mut self, row: usize) {
        self.remove_row(row);
    }
    fn clear_row_of_type(&mut self, row: usize, highlight_type: HighlightType) {
        if let Some(highlights) = self.by_row.get_mut(&row) {
            highlights.retain(|h| h.highlight_type != highlight_type);
            if highlights.is_empty() {
                self.remove_row(row);
            }
        }
    }
    fn clear(&mut self) {
        self.by_row.clear();
        self.recency.clear();
        self.retained_bytes = 0;
    }
    fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&HighlightRange) -> bool,
    {
        for highlights in self.by_row.values_mut() {
            highlights.retain(|h| f(h));
        }
        self.by_row.retain(|_, v| !v.is_empty());
        self.recency.retain(|row| self.by_row.contains_key(row));
        self.recalculate_retained_bytes();
    }
    fn shift_rows_after(&mut self, row: usize, delta: isize) {
        if delta == 0 {
            return;
        }
        let rows_to_shift: Vec<usize> = if delta > 0 { self.by_row.range(row..).map(|(r, _)| *r).collect() } else { self.by_row.range(row..).map(|(r, _)| *r).collect() };
        let mut shifted: Vec<(usize, Vec<HighlightRange>)> = Vec::new();
        for old_row in rows_to_shift {
            if let Some(mut highlights) = self.by_row.remove(&old_row) {
                let new_row = if delta > 0 { old_row + delta as usize } else { old_row.saturating_sub((-delta) as usize) };
                for h in &mut highlights {
                    h.row = new_row;
                }
                shifted.push((new_row, highlights));
            }
        }
        for (new_row, highlights) in shifted {
            self.by_row.insert(new_row, highlights);
        }
        for cached_row in &mut self.recency {
            if *cached_row >= row {
                *cached_row = if delta > 0 { *cached_row + delta as usize } else { cached_row.saturating_sub((-delta) as usize) };
            }
        }
        let mut seen = HashSet::new();
        self.recency.retain(|cached_row| self.by_row.contains_key(cached_row) && seen.insert(*cached_row));
        self.recalculate_retained_bytes();
        self.enforce_limits();
    }
    fn iter(&self) -> impl Iterator<Item = &HighlightRange> {
        self.by_row.values().flat_map(|v| v.iter())
    }
    fn is_empty(&self) -> bool {
        self.by_row.is_empty()
    }
    fn len(&self) -> usize {
        self.by_row.values().map(|v| v.len()).sum()
    }
    fn remove_row(&mut self, row: usize) {
        if let Some(ranges) = self.by_row.remove(&row) {
            self.retained_bytes = self.retained_bytes.saturating_sub(ranges.capacity() * std::mem::size_of::<HighlightRange>());
        }
        self.recency.retain(|cached_row| *cached_row != row);
    }
    fn recalculate_retained_bytes(&mut self) {
        self.retained_bytes = self.by_row.values().map(|ranges| ranges.capacity() * std::mem::size_of::<HighlightRange>()).sum();
    }
    fn enforce_limits(&mut self) {
        while self.by_row.len() > 1 && (self.by_row.len() > Self::MAX_ROWS || self.retained_bytes > Self::MAX_BYTES) {
            if let Some(oldest) = self.recency.pop_front() {
                if let Some(ranges) = self.by_row.remove(&oldest) {
                    self.retained_bytes = self.retained_bytes.saturating_sub(ranges.capacity() * std::mem::size_of::<HighlightRange>());
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct RowStyleCache {
    rows: BTreeMap<usize, Vec<Style>>,
    recency: VecDeque<usize>,
    retained_bytes: usize,
}

impl RowStyleCache {
    const MAX_ROWS: usize = 256;
    const MAX_BYTES: usize = 512 * 1024;
    fn new() -> Self {
        Self { rows: BTreeMap::new(), recency: VecDeque::new(), retained_bytes: 0 }
    }
    fn invalidate_row(&mut self, row: usize) {
        self.remove(row);
    }
    fn invalidate_from(&mut self, row: usize) {
        let rows_to_remove: Vec<usize> = self.rows.range(row..).map(|(r, _)| *r).collect();
        for r in rows_to_remove {
            self.remove(r);
        }
    }
    fn invalidate_all(&mut self) {
        self.rows.clear();
        self.recency.clear();
        self.retained_bytes = 0;
    }
    fn set_row_styles(&mut self, row: usize, styles: Vec<Style>) {
        self.remove(row);
        self.retained_bytes += styles.capacity() * std::mem::size_of::<Style>();
        self.rows.insert(row, styles);
        self.recency.push_back(row);
        self.enforce_limits();
    }
    fn get_row_styles(&self, row: usize) -> Option<&[Style]> {
        self.rows.get(&row).map(|v| v.as_slice())
    }
    fn remove(&mut self, row: usize) {
        if let Some(styles) = self.rows.remove(&row) {
            self.retained_bytes = self.retained_bytes.saturating_sub(styles.capacity() * std::mem::size_of::<Style>());
        }
        self.recency.retain(|cached_row| *cached_row != row);
    }
    fn enforce_limits(&mut self) {
        while self.rows.len() > 1 && (self.rows.len() > Self::MAX_ROWS || self.retained_bytes > Self::MAX_BYTES) {
            if let Some(oldest) = self.recency.pop_front() {
                if let Some(styles) = self.rows.remove(&oldest) {
                    self.retained_bytes = self.retained_bytes.saturating_sub(styles.capacity() * std::mem::size_of::<Style>());
                }
            }
        }
    }
    fn shift_rows_after(&mut self, row: usize, delta: isize) {
        if delta == 0 {
            return;
        }
        let rows_to_shift: Vec<usize> = if delta > 0 { self.rows.range(row..).map(|(r, _)| *r).collect() } else { self.rows.range(row..).map(|(r, _)| *r).collect() };
        let mut shifted: Vec<(usize, Vec<Style>)> = Vec::new();
        for old_row in rows_to_shift {
            if let Some(styles) = self.rows.remove(&old_row) {
                let new_row = if delta > 0 { old_row + delta as usize } else { old_row.saturating_sub((-delta) as usize) };
                shifted.push((new_row, styles));
            }
        }
        for (new_row, styles) in shifted {
            self.rows.insert(new_row, styles);
        }
        for cached_row in &mut self.recency {
            if *cached_row >= row {
                *cached_row = if delta > 0 { *cached_row + delta as usize } else { cached_row.saturating_sub((-delta) as usize) };
            }
        }
        let mut seen = HashSet::new();
        self.recency.retain(|cached_row| self.rows.contains_key(cached_row) && seen.insert(*cached_row));
        self.retained_bytes = self.rows.values().map(|styles| styles.capacity() * std::mem::size_of::<Style>()).sum();
        self.enforce_limits();
    }
}

#[derive(Debug, Clone)]
pub struct WikiLinkRange {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_valid: bool,
}

#[derive(Debug, Clone)]
enum ListPrefix {
    Unordered { indent: String, marker: char },
    Task { indent: String, marker: char },
    Ordered { indent: String, number: usize },
}

impl ListPrefix {
    fn detect(line: &str) -> Option<Self> {
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            let marker = trimmed.chars().next().unwrap();
            if trimmed.len() >= 5 {
                let after_marker = &trimmed[2..];
                if after_marker.starts_with("[ ] ") || after_marker.starts_with("[x] ") || after_marker.starts_with("[X] ") {
                    return Some(ListPrefix::Task { indent, marker });
                }
            }
            return Some(ListPrefix::Unordered { indent, marker });
        }
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(number) = num_part.parse::<usize>() {
                    return Some(ListPrefix::Ordered { indent, number });
                }
            }
        }
        None
    }
    fn next_prefix(&self) -> String {
        match self {
            ListPrefix::Unordered { indent, marker } => {
                format!("{}{} ", indent, marker)
            }
            ListPrefix::Task { indent, marker } => {
                format!("{}{} [ ] ", indent, marker)
            }
            ListPrefix::Ordered { indent, number } => {
                format!("{}{}. ", indent, number + 1)
            }
        }
    }
    fn prefix_len(&self, line: &str) -> usize {
        let trimmed = line.trim_start();
        let indent_len = line.chars().count() - trimmed.chars().count();
        match self {
            ListPrefix::Unordered { .. } => indent_len + 2, // "- " or "* " or "+ "
            ListPrefix::Task { .. } => indent_len + 6,      // "- [ ] "
            ListPrefix::Ordered { number, .. } => {
                indent_len + number.to_string().len() + 2 // "N. "
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Bar,
    Underline,
}

pub struct Editor {
    buffer: TextBuffer,
    cursor: Cursor,
    history: History,
    wrap_cache: WrapCache,
    scroll_offset: usize,
    h_scroll_offset: usize,
    view_height: usize,
    view_width: usize,
    preferred_visual_x: Option<usize>,
    line_wrap_enabled: bool,
    tab_width: u16,
    left_padding: u16,
    right_padding: u16,
    block: Option<Block<'static>>,
    cursor_line_style: Style,
    selection_style: Style,
    clipboard: Option<String>,
    clipboard_linewise: bool,
    clipboard_port: Arc<dyn Clipboard>,
    highlight_index: HighlightIndex,
    row_style_cache: RefCell<RowStyleCache>,
    code_block_rows: HashSet<usize>,
    frontmatter_end: Option<usize>,
    wiki_link_ranges: Vec<WikiLinkRange>,
    wiki_link_valid_style: Style,
    wiki_link_invalid_style: Style,
    visual_line_selection: Option<(usize, usize)>,
    visual_block_selection: Option<(Position, Position)>,
    inclusive_selection: bool,
    heading_colors: [Color; 6],
    code_color: Color,
    link_color: Color,
    blockquote_color: Color,
    list_marker_color: Color,
    bold_color: Option<Color>,
    italic_color: Option<Color>,
    frontmatter_color: Color,
    line_number_mode: LineNumberMode,
    line_number_style: Style,
    line_number_width: u16,
    scrolloff: usize,
    cursor_shape: CursorShape,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(vec![String::new()])
    }
}

mod api;
mod commands;
mod coordinates;
mod highlighting;
mod navigation;
mod rendering;
mod selection;

#[cfg(test)]
mod tests {
    use super::*;
    fn editor_with(line: &str) -> Editor {
        Editor::new(vec![line.to_string()])
    }

    /// Character-wise Visual mode is Vim-inclusive: selecting "Open" (cursor on
    /// the final "n") must yank the whole word, not "Ope".
    #[test]
    fn visual_selection_is_inclusive_of_cursor_char() {
        let mut ed = editor_with("Open the door");
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_inclusive_selection(true);
        ed.set_cursor(0, 3); // cursor on the "n" of "Open"
        assert_eq!(ed.selected_text().as_deref(), Some("Open"));
    }

    /// A single-cell visual selection (just `v` then `y`) yanks that one char.
    #[test]
    fn visual_selection_single_char_is_inclusive() {
        let mut ed = editor_with("Open");
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_inclusive_selection(true);
        assert_eq!(ed.selected_text().as_deref(), Some("O"));
    }

    /// Selecting backwards (anchor after the cursor) is still inclusive of both
    /// ends, so dragging the cursor left across "Open" still yields "Open".
    #[test]
    fn visual_selection_inclusive_when_reversed() {
        let mut ed = editor_with("Open the door");
        ed.set_cursor(0, 3); // anchor on the "n"
        ed.start_selection();
        ed.set_inclusive_selection(true);
        ed.set_cursor(0, 0); // move cursor back to the "O"
        assert_eq!(ed.selected_text().as_deref(), Some("Open"));
    }

    /// Cutting an inclusive selection removes the whole word from the buffer.
    #[test]
    fn visual_cut_is_inclusive() {
        let mut ed = editor_with("Open the door");
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_inclusive_selection(true);
        ed.set_cursor(0, 3);
        ed.cut();
        assert_eq!(ed.line(0), Some(" the door"));
    }

    /// Without the inclusive flag the range stays exclusive — operator-pending
    /// motions (e.g. `dw`) rely on this, so it must not regress.
    #[test]
    fn exclusive_selection_unchanged_by_default() {
        let mut ed = editor_with("Open the door");
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_cursor(0, 5); // exclusive end at start of "the"
        assert_eq!(ed.selected_text().as_deref(), Some("Open "));
    }

    /// `cancel_selection` must clear the inclusive flag so a later exclusive
    /// (operator-pending) selection is not silently extended.
    #[test]
    fn cancel_selection_resets_inclusive_flag() {
        let mut ed = editor_with("Open the door");
        ed.set_inclusive_selection(true);
        ed.cancel_selection();
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_cursor(0, 5);
        assert_eq!(ed.selected_text().as_deref(), Some("Open "));
    }

    #[test]
    fn select_all_uses_an_exclusive_document_range() {
        let mut ed = Editor::new(vec!["first".to_string(), "second".to_string()]);
        ed.set_cursor(0, 2);
        ed.select_all();
        assert_eq!(ed.selected_text().as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn replacing_a_selection_is_one_undoable_edit() {
        let mut ed = editor_with("hello world");
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_cursor(0, 5);
        ed.insert_char('H');
        assert_eq!(ed.text(), "H world");
        assert!(ed.undo());
        assert_eq!(ed.text(), "hello world");
        assert!(ed.redo());
        assert_eq!(ed.text(), "H world");
    }

    #[test]
    fn newline_replacement_is_one_undoable_edit() {
        let mut ed = editor_with("hello world");
        ed.set_cursor(0, 5);
        ed.start_selection();
        ed.set_cursor(0, 6);
        ed.insert_newline();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec!["hello", "world"]);
        assert!(ed.undo());
        assert_eq!(ed.text(), "hello world");
    }

    #[test]
    fn delete_selection_does_not_replace_the_clipboard() {
        let mut ed = editor_with("copy delete");
        ed.set_cursor(0, 0);
        ed.start_selection();
        ed.set_cursor(0, 4);
        ed.copy();
        ed.cancel_selection();
        ed.set_cursor(0, 5);
        ed.start_selection();
        ed.set_cursor(0, 11);
        assert!(ed.delete_selection());
        ed.set_cursor(0, 5);
        ed.paste();
        assert_eq!(ed.text(), "copy copy");
    }

    /// Regression: deleting the LAST line with `dd` must be undoable. The recorded
    /// op used to target a row past the buffer end, so undo silently lost the text.
    #[test]
    fn dd_last_line_is_undoable() {
        let mut ed = Editor::new(vec!["first".to_string(), "second".to_string()]);
        ed.set_cursor(1, 0);
        ed.delete_current_line();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec!["first"]);
        ed.undo();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec!["first", "second"]);
    }

    /// Regression: `dd` on a single-line buffer clears the line in place; undo
    /// restores the content without leaving a spurious extra empty line.
    #[test]
    fn dd_single_line_undo_has_no_extra_line() {
        let mut ed = editor_with("only line");
        ed.set_cursor(0, 0);
        ed.delete_current_line();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec![""]);
        ed.undo();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec!["only line"]);
    }

    /// `dd` on a middle line stays undoable (the previously-working path).
    #[test]
    fn dd_middle_line_is_undoable() {
        let mut ed = Editor::new(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        ed.set_cursor(1, 0);
        ed.delete_current_line();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec!["a", "c"]);
        ed.undo();
        assert_eq!(ed.iter_lines().collect::<Vec<_>>(), vec!["a", "b", "c"]);
    }

    /// Regression: wrapped Up/Down must use display width, not char index. With a
    /// 4-cell content width, "一二三四五" (each CJK char is 2 cells wide) wraps to
    /// 二-per-line, so Down advances by two chars per visual line.
    #[test]
    fn wrapped_down_uses_display_width_for_wide_chars() {
        let mut ed = Editor::new(vec!["一二三四五".to_string()]);
        ed.set_line_wrap(true);
        ed.set_line_number_mode(LineNumberMode::None);
        ed.set_view_size(5, 10); // content_width = 5 - right_padding(1) = 4 cells
        ed.set_cursor(0, 0);
        ed.move_cursor(CursorMove::Down);
        assert_eq!(ed.cursor(), (0, 2)); // 3rd CJK char starts the 2nd visual line
        ed.move_cursor(CursorMove::Down);
        assert_eq!(ed.cursor(), (0, 4)); // 5th CJK char starts the 3rd visual line
    }

    /// Wrapped Up/Down keep the sticky display column for plain ASCII too.
    #[test]
    fn wrapped_navigation_ascii_roundtrip() {
        let mut ed = editor_with("abcdefgh");
        ed.set_line_wrap(true);
        ed.set_line_number_mode(LineNumberMode::None);
        ed.set_view_size(5, 10); // content_width 4 -> "abcd" / "efgh"
        ed.set_cursor(0, 1); // 'b' (visual line 0, column 1)
        ed.move_cursor(CursorMove::Down);
        assert_eq!(ed.cursor(), (0, 5)); // 'f' (visual line 1, column 1)
        ed.move_cursor(CursorMove::Up);
        assert_eq!(ed.cursor(), (0, 1)); // back to 'b'
    }

    #[test]
    fn large_paste_delete_undo_redo_stays_within_history_budget() {
        let mut editor = editor_with("");
        editor.set_history_limits(100, 64 * 1024);
        let large = "日本語-and-ascii ".repeat(64 * 1024);
        editor.insert_str(&large);
        assert_eq!(editor.text(), large);
        assert!(editor.history_stats().total_payload_bytes() > editor.history_stats().payload_limit_bytes);
        assert_eq!(editor.history_stats().total_entries(), 1);
        editor.set_cursor(0, 0);
        editor.start_selection();
        editor.set_cursor(0, large.chars().count());
        editor.cut();
        assert!(editor.is_empty());
        assert_eq!(editor.history_stats().total_entries(), 1);
        assert!(editor.undo());
        assert_eq!(editor.text(), large);
        assert!(editor.redo());
        assert!(editor.is_empty());
        assert_eq!(editor.history_stats().total_entries(), 1);
    }

    #[test]
    fn highlight_and_style_caches_are_row_and_byte_bounded() {
        let lines = (0..1_000).map(|row| format!("# highlighted row {row}")).collect();
        let mut editor = Editor::new(lines);
        editor.add_highlights((0..10_000).map(|row| HighlightRange::new(row, 0, 1, Style::default().fg(Color::Blue), HighlightType::Header)));
        assert!(editor.highlight_index.by_row.len() <= HighlightIndex::MAX_ROWS);
        assert!(editor.highlight_index.retained_bytes <= HighlightIndex::MAX_BYTES || editor.highlight_index.by_row.len() == 1);
        for row in 0..1_000 {
            editor.get_row_styles_cached(row);
        }
        let style_cache = editor.row_style_cache.borrow();
        assert!(style_cache.rows.len() <= RowStyleCache::MAX_ROWS);
        assert!(style_cache.retained_bytes <= RowStyleCache::MAX_BYTES || style_cache.rows.len() == 1);
    }

    #[test]
    fn document_highlight_state_is_scoped_to_the_active_rows() {
        let lines = (0..1_000).map(|row| format!("# row {row} [[target-{row}]]")).collect();
        let mut editor = Editor::new(lines);
        editor.set_view_size(100, 20);
        editor.set_scroll_offset(500);
        let active_rows = editor.active_highlight_rows();
        editor.update_markdown_highlights();
        editor.update_wiki_links(|_| true);
        assert!(editor.highlight_index.iter().all(|highlight| active_rows.contains(&highlight.row)));
        assert!(editor.wiki_link_ranges.iter().all(|link| active_rows.contains(&link.row)));
        let mut fenced = vec!["```markdown".to_owned()];
        fenced.extend((1..1_000).map(|row| format!("code row {row}")));
        let mut editor = Editor::new(fenced);
        editor.set_view_size(100, 20);
        editor.set_scroll_offset(500);
        let active_rows = editor.active_highlight_rows();
        editor.update_markdown_highlights();
        assert!(editor.code_block_rows.iter().all(|row| active_rows.contains(row)));
        assert!(editor.code_block_rows.len() <= active_rows.len());
    }
}
