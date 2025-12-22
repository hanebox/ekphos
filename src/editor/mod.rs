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
    style::{Modifier, Style},
    widgets::{Block, Widget},
};
use unicode_width::UnicodeWidthChar;

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

    // Cursor
    pub fn cursor(&self) -> (usize, usize) {
        let pos = self.cursor.pos();
        (pos.row, pos.col)
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
        let (cursor_row, cursor_col) = self.cursor();

        if cursor_row < self.scroll_offset {
            self.scroll_offset = cursor_row;
        } else if cursor_row >= self.scroll_offset + view_height {
            self.scroll_offset = cursor_row - view_height + 1;
        }

        if self.view_width > 0 {
            let effective_width = self.view_width.saturating_sub(1);
            if cursor_col < self.h_scroll_offset {
                self.h_scroll_offset = cursor_col;
            } else if cursor_col >= self.h_scroll_offset + effective_width {
                self.h_scroll_offset = cursor_col.saturating_sub(effective_width) + 1;
            }
        }
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
        let cursor_pos = self.cursor.pos();
        let selection = self.cursor.selection_range();

        let mut visual_y = 0usize;
        let mut screen_y = area.y;

        for row in 0..self.buffer.line_count() {
            let line = self.buffer.line(row).unwrap_or("");
            let is_cursor_line = row == cursor_pos.row;
            let chars: Vec<char> = line.chars().collect();

            if chars.is_empty() {
                if visual_y >= self.scroll_offset && screen_y < area.y + area.height {
                    if is_cursor_line {
                        if let Some(cell) = buf.cell_mut((area.x, screen_y)) {
                            cell.set_char(' ');
                            cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                        }
                    }
                    screen_y += 1;
                }
                visual_y += 1;
                continue;
            }

            let mut col = 0;
            while col < chars.len() {
                if visual_y >= self.scroll_offset {
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

                    if is_cursor_line && cursor_pos.col >= chars.len() && segment_end == chars.len() {
                        if x < area.x + area.width {
                            if let Some(cell) = buf.cell_mut((x, screen_y)) {
                                cell.set_char(' ');
                                cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
                            }
                        }
                    }

                    screen_y += 1;
                }

                visual_y += 1;
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
        Style::default()
    }
}
