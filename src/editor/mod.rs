mod buffer;
mod cursor;
mod history;
mod input;
mod wrap;

pub use cursor::{CursorMove, Position};
pub use input::{process_key, InputAction};

use buffer::TextBuffer;
use cursor::Cursor;
use history::{EditOperation, History};
use wrap::WrapCache;

use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Widget},
};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightType {
    WikiLink,
    Header,
    Bold,
    Italic,
    InlineCode,
    CodeBlock,
    Link,
    Blockquote,
    ListMarker,
    HorizontalRule,
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
    pub fn new(
        row: usize,
        start_col: usize,
        end_col: usize,
        style: Style,
        highlight_type: HighlightType,
    ) -> Self {
        Self {
            row,
            start_col,
            end_col,
            style,
            highlight_type,
            priority: 0,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.row == row && col >= self.start_col && col < self.end_col
    }
}

#[derive(Debug, Clone)]
pub struct WikiLinkRange {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_valid: bool,
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
    line_wrap_enabled: bool,
    block: Option<Block<'static>>,
    cursor_line_style: Style,
    selection_style: Style,
    clipboard: Option<String>,
    // General highlighting system
    highlights: Vec<HighlightRange>,
    // Wiki link highlighting (legacy, kept for compatibility)
    wiki_link_ranges: Vec<WikiLinkRange>,
    wiki_link_valid_style: Style,
    wiki_link_invalid_style: Style,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(vec![String::new()])
    }
}

impl Editor {
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            buffer: TextBuffer::from_lines(lines),
            cursor: Cursor::new(),
            history: History::new(),
            wrap_cache: WrapCache::new(),
            scroll_offset: 0,
            h_scroll_offset: 0,
            view_height: 0,
            view_width: 0,
            line_wrap_enabled: true,
            block: None,
            cursor_line_style: Style::default(),
            selection_style: Style::default().bg(ratatui::style::Color::DarkGray),
            clipboard: None,
            highlights: Vec::new(),
            wiki_link_ranges: Vec::new(),
            wiki_link_valid_style: Style::default().fg(Color::Cyan),
            wiki_link_invalid_style: Style::default().fg(Color::Red),
        }
    }

    pub fn from_str(text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        if lines.is_empty() {
            Self::default()
        } else {
            Self::new(lines)
        }
    }

    // Line wrap
    pub fn set_line_wrap(&mut self, enabled: bool) {
        self.line_wrap_enabled = enabled;
        if enabled {
            self.h_scroll_offset = 0;
        }
    }

    pub fn line_wrap_enabled(&self) -> bool {
        self.line_wrap_enabled
    }

    // Styling
    pub fn set_block(&mut self, block: Block<'static>) {
        self.block = Some(block);
    }

    pub fn set_cursor_line_style(&mut self, style: Style) {
        self.cursor_line_style = style;
    }

    pub fn set_selection_style(&mut self, style: Style) {
        self.selection_style = style;
    }

    pub fn set_wiki_link_styles(&mut self, valid_style: Style, invalid_style: Style) {
        self.wiki_link_valid_style = valid_style;
        self.wiki_link_invalid_style = invalid_style;
    }

    pub fn update_wiki_links<F>(&mut self, validator: F)
    where
        F: Fn(&str) -> bool,
    {
        self.wiki_link_ranges.clear();

        for (row, line) in self.buffer.lines().iter().enumerate() {
            let mut search_start = 0;

            while search_start < line.len() {
                let remaining = &line[search_start..];

                if let Some(start_pos) = remaining.find("[[") {
                    let abs_start = search_start + start_pos;
                    let after_brackets = &line[abs_start + 2..];

                    if let Some(end_pos) = after_brackets.find("]]") {
                        let target = &after_brackets[..end_pos];

                        if !target.is_empty() && !target.contains('[') && !target.contains(']') {
                            let is_valid = validator(target);

                            let start_col = line[..abs_start].chars().count();
                            let end_col = start_col + 2 + target.chars().count() + 2; // [[target]]

                            self.wiki_link_ranges.push(WikiLinkRange {
                                row,
                                start_col,
                                end_col,
                                is_valid,
                            });
                        }

                        search_start = abs_start + 2 + end_pos + 2;
                        continue;
                    }
                }
                break;
            }
        }
    }

    fn wiki_link_style_at(&self, row: usize, col: usize) -> Option<Style> {
        for range in &self.wiki_link_ranges {
            if range.row == row && col >= range.start_col && col < range.end_col {
                return if range.is_valid {
                    Some(self.wiki_link_valid_style)
                } else {
                    Some(self.wiki_link_invalid_style)
                };
            }
        }
        None
    }

    // ==================== Highlight Management ====================

    pub fn add_highlight(&mut self, highlight: HighlightRange) {
        self.highlights.push(highlight);
    }

    pub fn add_highlights(&mut self, highlights: impl IntoIterator<Item = HighlightRange>) {
        self.highlights.extend(highlights);
    }

    pub fn clear_highlights(&mut self) {
        self.highlights.clear();
    }
    pub fn clear_highlights_of_type(&mut self, highlight_type: HighlightType) {
        self.highlights.retain(|h| h.highlight_type != highlight_type);
    }

    pub fn clear_highlights_for_row(&mut self, row: usize) {
        self.highlights.retain(|h| h.row != row);
    }

    pub fn clear_highlights_for_row_and_type(&mut self, row: usize, highlight_type: HighlightType) {
        self.highlights.retain(|h| h.row != row || h.highlight_type != highlight_type);
    }

    fn highlight_style_at(&self, row: usize, col: usize) -> Option<Style> {
        let mut best_match: Option<&HighlightRange> = None;

        for highlight in &self.highlights {
            if highlight.contains(row, col) {
                match best_match {
                    None => best_match = Some(highlight),
                    Some(current) if highlight.priority > current.priority => {
                        best_match = Some(highlight);
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|h| h.style)
    }

    #[allow(dead_code)]
    pub fn highlights_for_row(&self, row: usize) -> Vec<&HighlightRange> {
        self.highlights.iter().filter(|h| h.row == row).collect()
    }

    #[allow(dead_code)]
    pub fn highlights_of_type(&self, highlight_type: HighlightType) -> Vec<&HighlightRange> {
        self.highlights.iter().filter(|h| h.highlight_type == highlight_type).collect()
    }

    #[allow(dead_code)]
    pub fn has_highlights(&self) -> bool {
        !self.highlights.is_empty()
    }

    #[allow(dead_code)]
    pub fn highlight_count(&self) -> usize {
        self.highlights.len()
    }

    // ==================== Markdown Syntax Highlighting ====================

    pub fn update_markdown_highlights(&mut self) {
        self.highlights.retain(|h| h.highlight_type == HighlightType::WikiLink);

        let line_count = self.buffer.line_count();
        let mut in_code_block = false;

        for row in 0..line_count {
            let line = self.buffer.line(row).unwrap_or("").to_string();

            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                let start = line.find("```").unwrap_or(0);
                self.highlights.push(HighlightRange::new(
                    row,
                    start,
                    line.chars().count(),
                    Style::default().fg(Color::Green),
                    HighlightType::CodeBlock,
                ));
                continue;
            }

            if in_code_block {
                self.highlights.push(HighlightRange::new(
                    row,
                    0,
                    line.chars().count(),
                    Style::default().fg(Color::Green),
                    HighlightType::CodeBlock,
                ));
                continue;
            }

            self.highlight_line_markdown(row, &line);
        }
    }

    fn highlight_line_markdown(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let line_len = chars.len();

        if line_len == 0 {
            return;
        }

        if let Some(header_end) = self.detect_header(line) {
            let level = line.chars().take_while(|&c| c == '#').count();
            let color = match level {
                1 => Color::Blue,
                2 => Color::Green,
                3 => Color::Yellow,
                4 => Color::Magenta,
                5 => Color::Cyan,
                _ => Color::Gray,
            };
            self.highlights.push(HighlightRange::new(
                row,
                0,
                header_end.min(line_len),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
                HighlightType::Header,
            ));
            return; 
        }

        if line.trim_start().starts_with('>') {
            let start = line.find('>').unwrap_or(0);
            self.highlights.push(HighlightRange::new(
                row,
                start,
                start + 1,
                Style::default().fg(Color::Cyan),
                HighlightType::Blockquote,
            ));
        }

        self.highlight_list_marker(row, line);

        self.highlight_inline_code(row, line);
        self.highlight_links(row, line);
        self.highlight_bold(row, line);
        self.highlight_italic(row, line);
    }

    fn detect_header(&self, line: &str) -> Option<usize> {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 6 {
                let after_hashes = &trimmed[hash_count..];
                if after_hashes.is_empty() || after_hashes.starts_with(' ') {
                    return Some(line.chars().count());
                }
            }
        }
        None
    }

    fn highlight_list_marker(&mut self, row: usize, line: &str) {
        let trimmed = line.trim_start();
        let indent_chars = line.chars().take_while(|c| c.is_whitespace()).count();

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            self.highlights.push(HighlightRange::new(
                row,
                indent_chars,
                indent_chars + 1,
                Style::default().fg(Color::Yellow),
                HighlightType::ListMarker,
            ));

            if trimmed.len() >= 5 {
                let after_marker = &trimmed[2..];
                if after_marker.starts_with("[ ] ") || after_marker.starts_with("[x] ") || after_marker.starts_with("[X] ") {
                    self.highlights.push(HighlightRange::new(
                        row,
                        indent_chars + 2,
                        indent_chars + 5,
                        Style::default().fg(Color::Cyan),
                        HighlightType::ListMarker,
                    ));
                }
            }
        }
        else if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
                self.highlights.push(HighlightRange::new(
                    row,
                    indent_chars,
                    indent_chars + dot_pos + 1,
                    Style::default().fg(Color::Yellow),
                    HighlightType::ListMarker,
                ));
            }
        }
    }

    fn highlight_inline_code(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '`' && (i + 1 >= chars.len() || chars[i + 1] != '`') {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                    let end_pos = i + 1 + end;
                    self.highlights.push(HighlightRange::new(
                        row,
                        i,
                        end_pos + 1,
                        Style::default().fg(Color::Green),
                        HighlightType::InlineCode,
                    ).with_priority(2)); 
                    i = end_pos + 1;
                    continue;
                }
            }
            i += 1;
        }
    }

    fn highlight_links(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '[' {
                if let Some(bracket_end) = chars[i + 1..].iter().position(|&c| c == ']') {
                    let bracket_end_pos = i + 1 + bracket_end;
                    if bracket_end_pos + 1 < chars.len() && chars[bracket_end_pos + 1] == '(' {
                        if let Some(paren_end) = chars[bracket_end_pos + 2..].iter().position(|&c| c == ')') {
                            let paren_end_pos = bracket_end_pos + 2 + paren_end;
                            self.highlights.push(HighlightRange::new(
                                row,
                                i,
                                paren_end_pos + 1,
                                Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
                                HighlightType::Link,
                            ).with_priority(1));
                            i = paren_end_pos + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn highlight_bold(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len().saturating_sub(1) {
            if (chars[i] == '*' && chars[i + 1] == '*') || (chars[i] == '_' && chars[i + 1] == '_') {
                let marker = chars[i];
                let mut j = i + 2;
                while j < chars.len().saturating_sub(1) {
                    if chars[j] == marker && chars[j + 1] == marker {
                        if !self.is_position_highlighted(row, i) {
                            self.highlights.push(HighlightRange::new(
                                row,
                                i,
                                j + 2,
                                Style::default().add_modifier(Modifier::BOLD),
                                HighlightType::Bold,
                            ));
                        }
                        i = j + 2;
                        break;
                    }
                    j += 1;
                }
                if j >= chars.len().saturating_sub(1) {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }

    fn highlight_italic(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '*' || chars[i] == '_' {
                let marker = chars[i];
                if i + 1 < chars.len() && chars[i + 1] == marker {
                    i += 2;
                    continue;
                }
                if i > 0 && chars[i - 1] == marker {
                    i += 1;
                    continue;
                }

                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == marker {
                        if j + 1 < chars.len() && chars[j + 1] == marker {
                            j += 2;
                            continue;
                        }
                        if !self.is_position_highlighted(row, i) {
                            self.highlights.push(HighlightRange::new(
                                row,
                                i,
                                j + 1,
                                Style::default().add_modifier(Modifier::ITALIC),
                                HighlightType::Italic,
                            ));
                        }
                        i = j + 1;
                        break;
                    }
                    j += 1;
                }
                if j >= chars.len() {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }

    fn is_position_highlighted(&self, row: usize, col: usize) -> bool {
        self.highlights.iter().any(|h| {
            h.row == row && col >= h.start_col && col < h.end_col &&
            (h.highlight_type == HighlightType::InlineCode || h.highlight_type == HighlightType::Link)
        })
    }

    // Cursor
    pub fn cursor(&self) -> (usize, usize) {
        let pos = self.cursor.pos();
        (pos.row, pos.col)
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        let line_count = self.buffer.line_count();
        let safe_row = row.min(line_count.saturating_sub(1));
        let line_len = self.buffer.line_len(safe_row);
        let safe_col = col.min(line_len);
        self.cursor.move_to(safe_row, safe_col);
        self.ensure_cursor_visible();
    }

    pub fn move_cursor(&mut self, movement: CursorMove) {
        let pos = self.cursor.pos();
        let line_count = self.buffer.line_count();

        match movement {
            CursorMove::Forward => {
                let line_len = self.buffer.line_len(pos.row);
                if pos.col < line_len {
                    self.cursor.move_to(pos.row, pos.col + 1);
                } else if pos.row + 1 < line_count {
                    self.cursor.move_to(pos.row + 1, 0);
                }
            }
            CursorMove::Back => {
                if pos.col > 0 {
                    self.cursor.move_to(pos.row, pos.col - 1);
                } else if pos.row > 0 {
                    let prev_len = self.buffer.line_len(pos.row - 1);
                    self.cursor.move_to(pos.row - 1, prev_len);
                }
            }
            CursorMove::Up => {
                if pos.row > 0 {
                    let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                    let prev_len = self.buffer.line_len(pos.row - 1);
                    self.cursor.set_pos(Position::new(pos.row - 1, preferred.min(prev_len)), false);
                }
            }
            CursorMove::Down => {
                if pos.row + 1 < line_count {
                    let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                    let next_len = self.buffer.line_len(pos.row + 1);
                    self.cursor.set_pos(Position::new(pos.row + 1, preferred.min(next_len)), false);
                }
            }
            CursorMove::Head => self.cursor.move_to(pos.row, 0),
            CursorMove::End => self.cursor.move_to(pos.row, self.buffer.line_len(pos.row)),
            CursorMove::Top => self.cursor.move_to(0, 0),
            CursorMove::Bottom => {
                let last_row = line_count.saturating_sub(1);
                self.cursor.move_to(last_row, self.buffer.line_len(last_row));
            }
            CursorMove::WordForward => self.move_word_forward(),
            CursorMove::WordBack => self.move_word_back(),
        }
        self.ensure_cursor_visible();
    }

    fn move_word_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else { return };

        let new_col = cursor::find_word_forward(line, pos.col);
        let line_len = line.chars().count();

        if new_col >= line_len && pos.row + 1 < self.buffer.line_count() {
            self.cursor.move_to(pos.row + 1, 0);
            if let Some(next_line) = self.buffer.line(pos.row + 1) {
                let skip = next_line.chars().take_while(|c| c.is_whitespace()).count();
                self.cursor.move_to(pos.row + 1, skip);
            }
        } else {
            self.cursor.move_to(pos.row, new_col.min(line_len));
        }
    }

    fn move_word_back(&mut self) {
        let pos = self.cursor.pos();

        if pos.col == 0 && pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            self.cursor.move_to(pos.row - 1, prev_len);
            return;
        }

        if let Some(line) = self.buffer.line(pos.row) {
            self.cursor.move_to(pos.row, cursor::find_word_back(line, pos.col));
        }
    }

    // Selection
    pub fn start_selection(&mut self) {
        self.cursor.start_selection();
    }

    pub fn cancel_selection(&mut self) {
        self.cursor.cancel_selection();
    }

    pub fn has_selection(&self) -> bool {
        self.cursor.has_selection()
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.cursor.selection_range()?;
        Some(self.buffer.get_text_range(start.row, start.col, end.row, end.col))
    }

    // Clipboard
    pub fn copy(&mut self) {
        if let Some(text) = self.selected_text() {
            self.clipboard = Some(text.clone());
            // Sorryyy forgot the damn system clipboard 
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&text);
            }
        }
    }

    pub fn cut(&mut self) {
        if let Some((start, end)) = self.cursor.selection_range() {
            let cursor_before = self.cursor.pos();
            let deleted = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
            self.clipboard = Some(deleted.clone());
            // This one too copy to system clipboard
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&deleted);
            }
            self.wrap_cache.invalidate_from(start.row);

            self.history.record(
                EditOperation::Delete { start, end, deleted_text: deleted },
                cursor_before,
                start,
            );

            self.cursor.move_to(start.row, start.col);
            self.cursor.cancel_selection();
        }
    }

    pub fn paste(&mut self) {
        if let Some(text) = self.clipboard.clone() {
            self.insert_str(&text);
        }
    }

    // Text manipulation
    pub fn insert_char(&mut self, c: char) {
        let cursor_before = self.cursor.pos();

        if self.cursor.has_selection() {
            self.delete_selection_internal();
        }

        let pos = self.cursor.pos();
        self.buffer.insert_char(pos.row, pos.col, c);
        self.wrap_cache.invalidate_line(pos.row);

        self.history.record(
            EditOperation::Insert { pos, text: c.to_string() },
            cursor_before,
            Position::new(pos.row, pos.col + 1),
        );

        self.cursor.move_to(pos.row, pos.col + 1);
        self.ensure_cursor_visible();
    }

    /// Insert string at cursor, handling multi-line text and selection replacement
    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }

        let cursor_before = self.cursor.pos();

        // Delete selection first, record for undo
        let deleted_selection = if self.cursor.has_selection() {
            if let Some((start, end)) = self.cursor.selection_range() {
                let deleted = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
                self.wrap_cache.invalidate_from(start.row);
                self.cursor.move_to(start.row, start.col);
                self.cursor.cancel_selection();
                Some((start, end, deleted))
            } else {
                None
            }
        } else {
            None
        };

        let pos = self.cursor.pos();
        let parts: Vec<&str> = s.split('\n').collect();
        let newline_count = parts.len().saturating_sub(1);

        if newline_count == 0 {
            self.buffer.insert_str(pos.row, pos.col, s);
            self.wrap_cache.invalidate_line(pos.row);
            self.cursor.move_to(pos.row, pos.col + s.chars().count());
        } else {
            if !parts[0].is_empty() {
                self.buffer.insert_str(pos.row, pos.col, parts[0]);
            }

            let split_col = pos.col + parts[0].chars().count();
            self.buffer.split_line(pos.row, split_col);
            self.wrap_cache.insert_line(pos.row + 1);

            for (i, part) in parts[1..parts.len() - 1].iter().enumerate() {
                self.buffer.insert_line(pos.row + 1 + i, part.to_string());
                self.wrap_cache.insert_line(pos.row + 1 + i);
            }

            let last_idx = pos.row + newline_count;
            let last_part = parts[parts.len() - 1];
            if !last_part.is_empty() {
                self.buffer.insert_str(last_idx, 0, last_part);
            }

            self.wrap_cache.invalidate_from(pos.row);
            self.cursor.move_to(last_idx, last_part.chars().count());
        }

        // Record undo operations
        let had_selection = deleted_selection.is_some();
        if let Some((start, end, deleted_text)) = deleted_selection {
            self.history.record(
                EditOperation::Delete { start, end, deleted_text },
                cursor_before,
                pos,
            );
        }

        self.history.record(
            EditOperation::Insert { pos, text: s.to_string() },
            if had_selection { pos } else { cursor_before },
            self.cursor.pos(),
        );

        self.ensure_cursor_visible();
    }

    pub fn insert_newline(&mut self) {
        let cursor_before = self.cursor.pos();

        if self.cursor.has_selection() {
            self.delete_selection_internal();
        }

        let pos = self.cursor.pos();
        self.buffer.split_line(pos.row, pos.col);
        self.wrap_cache.insert_line(pos.row + 1);
        self.wrap_cache.invalidate_line(pos.row);

        self.history.record(
            EditOperation::SplitLine { pos },
            cursor_before,
            Position::new(pos.row + 1, 0),
        );

        self.cursor.move_to(pos.row + 1, 0);
        self.ensure_cursor_visible();
    }

    pub fn delete_char(&mut self) {
        let pos = self.cursor.pos();
        let line_len = self.buffer.line_len(pos.row);

        if pos.col < line_len {
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col) {
                self.wrap_cache.invalidate_line(pos.row);
                self.history.record(
                    EditOperation::Delete {
                        start: pos,
                        end: Position::new(pos.row, pos.col + 1),
                        deleted_text: c.to_string(),
                    },
                    pos,
                    pos,
                );
            }
        } else if pos.row + 1 < self.buffer.line_count() {
            self.buffer.join_with_previous(pos.row + 1);
            self.wrap_cache.remove_line(pos.row + 1);
            self.wrap_cache.invalidate_line(pos.row);
            self.history.record(
                EditOperation::JoinLine { row: pos.row + 1, col: line_len },
                pos,
                pos,
            );
        }
    }

    pub fn delete_newline(&mut self) {
        let pos = self.cursor.pos();

        if pos.col > 0 {
            let cursor_before = pos;
            self.cursor.move_to(pos.row, pos.col - 1);
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col - 1) {
                self.wrap_cache.invalidate_line(pos.row);
                self.history.record(
                    EditOperation::Delete {
                        start: Position::new(pos.row, pos.col - 1),
                        end: pos,
                        deleted_text: c.to_string(),
                    },
                    cursor_before,
                    self.cursor.pos(),
                );
            }
        } else if pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            let cursor_before = pos;

            self.buffer.join_with_previous(pos.row);
            self.wrap_cache.remove_line(pos.row);
            self.wrap_cache.invalidate_line(pos.row - 1);

            self.history.record(
                EditOperation::JoinLine { row: pos.row, col: prev_len },
                cursor_before,
                Position::new(pos.row - 1, prev_len),
            );

            self.cursor.move_to(pos.row - 1, prev_len);
        }

        self.ensure_cursor_visible();
    }

    fn delete_selection_internal(&mut self) {
        if let Some((start, end)) = self.cursor.selection_range() {
            self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
            self.wrap_cache.invalidate_from(start.row);
            self.cursor.move_to(start.row, start.col);
            self.cursor.cancel_selection();
        }
    }

    // Undo/Redo
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.history.pop_undo() {
            for op in entry.operations.iter().rev() {
                self.apply_operation(&op.inverse());
            }
            self.cursor.move_to(entry.cursor_before.row, entry.cursor_before.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.history.pop_redo() {
            for op in &entry.operations {
                self.apply_operation(op);
            }
            self.cursor.move_to(entry.cursor_after.row, entry.cursor_after.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { pos, text } => {
                if text.contains('\n') {
                    let lines: Vec<&str> = text.lines().collect();
                    self.buffer.insert_str(pos.row, pos.col, lines[0]);
                    if lines.len() > 1 {
                        let split_col = pos.col + lines[0].chars().count();
                        self.buffer.split_line(pos.row, split_col);
                        for (i, line) in lines[1..].iter().enumerate() {
                            if i < lines.len() - 2 {
                                self.buffer.insert_line(pos.row + 1 + i, line.to_string());
                            } else {
                                self.buffer.insert_str(pos.row + 1 + i, 0, line);
                            }
                        }
                    }
                } else {
                    self.buffer.insert_str(pos.row, pos.col, text);
                }
                self.wrap_cache.invalidate_from(pos.row);
            }
            EditOperation::Delete { start, end, .. } => {
                self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
                self.wrap_cache.invalidate_from(start.row);
            }
            EditOperation::SplitLine { pos } => {
                self.buffer.split_line(pos.row, pos.col);
                self.wrap_cache.insert_line(pos.row + 1);
                self.wrap_cache.invalidate_line(pos.row);
            }
            EditOperation::JoinLine { row, .. } => {
                self.buffer.join_with_previous(*row);
                self.wrap_cache.remove_line(*row);
                self.wrap_cache.invalidate_line(row - 1);
            }
        }
    }

    // Input processing
    pub fn input(&mut self, key: KeyEvent) {
        match process_key(key) {
            InputAction::InsertChar(c) => self.insert_char(c),
            InputAction::InsertNewline => self.insert_newline(),
            InputAction::DeleteChar => self.delete_char(),
            InputAction::DeleteCharBefore => self.delete_newline(),
            InputAction::Move(movement) => self.move_cursor(movement),
            InputAction::None => {}
        }
    }

    // Query
    pub fn lines(&self) -> Vec<&str> {
        self.buffer.lines()
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    // Scrolling
    pub fn update_scroll(&mut self, view_height: usize) {
        self.view_height = view_height;
        if view_height == 0 {
            return;
        }

        let (cursor_row, cursor_col) = self.cursor();
        let line_count = self.buffer.line_count();

        if cursor_row < self.scroll_offset {
            self.scroll_offset = cursor_row;
        }

        if self.line_wrap_enabled && self.view_width > 0 {
            while self.scroll_offset < cursor_row {
                let visual_lines = self.visual_lines_in_range(self.scroll_offset, cursor_row);
                if visual_lines <= view_height {
                    break;
                }
                self.scroll_offset += 1;
            }
        } else {
            if cursor_row >= self.scroll_offset + view_height {
                self.scroll_offset = cursor_row.saturating_sub(view_height.saturating_sub(1));
            }
        }

        // Clamp to valid range
        let max_scroll = line_count.saturating_sub(1);
        self.scroll_offset = self.scroll_offset.min(max_scroll);

        if self.view_width > 0 {
            let effective_width = self.view_width.saturating_sub(1);
            if cursor_col < self.h_scroll_offset {
                self.h_scroll_offset = cursor_col;
            } else if cursor_col >= self.h_scroll_offset + effective_width {
                self.h_scroll_offset = cursor_col.saturating_sub(effective_width) + 1;
            }
        }
    }

    fn visual_lines_in_range(&self, start_row: usize, end_row: usize) -> usize {
        let width = self.view_width.max(1);
        let mut visual_lines = 0;

        for row in start_row..=end_row.min(self.buffer.line_count().saturating_sub(1)) {
            let line_len = self.buffer.line_len(row);
            if line_len == 0 {
                visual_lines += 1;
            } else {
                visual_lines += (line_len + width - 1) / width;
            }
        }

        visual_lines
    }

    fn ensure_cursor_visible(&mut self) {
        if self.view_height > 0 {
            self.update_scroll(self.view_height);
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn h_scroll_offset(&self) -> usize {
        self.h_scroll_offset
    }

    pub fn set_view_size(&mut self, width: usize, height: usize) {
        self.view_width = width;
        self.view_height = height;
        self.ensure_cursor_visible();
    }

    pub fn get_overflow_info(&self) -> (bool, bool) {
        let (cursor_row, _) = self.cursor();
        let line_len = self.buffer.line_len(cursor_row);
        (self.h_scroll_offset > 0, line_len > self.h_scroll_offset + self.view_width)
    }
}

// Widget implementation
impl Widget for &Editor {
    fn render(self, area: Rect, buf: &mut RatatuiBuffer) {
        let inner_area = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        if self.line_wrap_enabled {
            self.render_wrapped(inner_area, buf);
        } else {
            self.render_no_wrap(inner_area, buf);
        }
    }
}

impl Editor {
    fn render_wrapped(&self, area: Rect, buf: &mut RatatuiBuffer) {
        let width = area.width as usize;
        if width == 0 {
            return;
        }

        let cursor_pos = self.cursor.pos();
        let selection = self.cursor.selection_range();
        let line_count = self.buffer.line_count();

        // Use row-based scrolling (consistent with update_scroll)
        // scroll_offset is the first visible ROW, not visual line
        let start_row = self.scroll_offset.min(line_count);
        let mut screen_y = area.y;

        for row in start_row..line_count {
            if screen_y >= area.y + area.height {
                break;
            }

            let line = self.buffer.line(row).unwrap_or("");
            let is_cursor_line = row == cursor_pos.row;
            let chars: Vec<char> = line.chars().collect();

            if chars.is_empty() {
                if is_cursor_line {
                    if let Some(cell) = buf.cell_mut((area.x, screen_y)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                    }
                }
                screen_y += 1;
                continue;
            }

            // Render line with wrapping
            let mut col = 0;
            while col < chars.len() {
                if screen_y >= area.y + area.height {
                    return;
                }

                let segment_end = (col + width).min(chars.len());
                let mut x = area.x;

                for c in col..segment_end {
                    let ch = chars[c];
                    let mut style = self.get_char_style(row, c, selection);
                    if is_cursor_line && c == cursor_pos.col {
                        style = style.add_modifier(Modifier::REVERSED);
                    }

                    if let Some(cell) = buf.cell_mut((x, screen_y)) {
                        cell.set_char(ch);
                        cell.set_style(style);
                    }
                    x += ch.width().unwrap_or(1) as u16;
                }

                // Render cursor at end of line if cursor is past last char
                if is_cursor_line && cursor_pos.col >= chars.len() && segment_end == chars.len() {
                    if x < area.x + area.width {
                        if let Some(cell) = buf.cell_mut((x, screen_y)) {
                            cell.set_char(' ');
                            cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                        }
                    }
                }

                screen_y += 1;
                col += width;
            }
        }

        if self.buffer.is_empty() {
            if let Some(cell) = buf.cell_mut((area.x, area.y)) {
                cell.set_char(' ');
                cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
            }
        }
    }

    fn render_no_wrap(&self, area: Rect, buf: &mut RatatuiBuffer) {
        let cursor_pos = self.cursor.pos();
        let selection = self.cursor.selection_range();
        let h_scroll = self.h_scroll_offset;

        let mut y = area.y;
        let end_row = (self.scroll_offset + area.height as usize).min(self.buffer.line_count());

        for row in self.scroll_offset..end_row {
            if y >= area.y + area.height {
                break;
            }

            let line = self.buffer.line(row).unwrap_or("");
            let is_cursor_line = row == cursor_pos.row;
            let chars: Vec<char> = line.chars().collect();
            let line_h_scroll = if is_cursor_line { h_scroll } else { 0 };

            let mut x = area.x;
            for col in line_h_scroll..chars.len() {
                if x >= area.x + area.width {
                    break;
                }

                let ch = chars[col];
                let mut style = self.get_char_style(row, col, selection);
                if is_cursor_line && col == cursor_pos.col {
                    style = style.add_modifier(Modifier::REVERSED);
                }

                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(ch);
                    cell.set_style(style);
                }
                x += ch.width().unwrap_or(1) as u16;
            }

            if is_cursor_line && cursor_pos.col >= chars.len() {
                let cursor_x = area.x + (cursor_pos.col.saturating_sub(line_h_scroll)) as u16;
                if cursor_x < area.x + area.width {
                    if let Some(cell) = buf.cell_mut((cursor_x, y)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                    }
                }
            }

            y += 1;
        }

        if self.buffer.line_count() <= self.scroll_offset {
            if let Some(cell) = buf.cell_mut((area.x, area.y)) {
                cell.set_char(' ');
                cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
            }
        }
    }

    fn get_char_style(&self, row: usize, col: usize, selection: Option<(Position, Position)>) -> Style {
        // Selection takes priority (highest)
        if let Some((start, end)) = selection {
            let in_selection = if start.row == end.row {
                row == start.row && col >= start.col && col < end.col
            } else if row == start.row {
                col >= start.col
            } else if row == end.row {
                col < end.col
            } else {
                row > start.row && row < end.row
            };

            if in_selection {
                return self.selection_style;
            }
        }

        if let Some(highlight_style) = self.highlight_style_at(row, col) {
            return highlight_style;
        }
        if let Some(wiki_style) = self.wiki_link_style_at(row, col) {
            return wiki_style;
        }

        Style::default()
    }
}
