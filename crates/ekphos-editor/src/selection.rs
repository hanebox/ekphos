use super::*;

impl Editor {
    pub fn start_selection(&mut self) {
        self.cursor.start_selection();
    }

    pub fn cancel_selection(&mut self) {
        self.cursor.cancel_selection();
        self.inclusive_selection = false;
    }

    pub fn has_selection(&self) -> bool {
        self.cursor.has_selection()
    }

    pub fn set_inclusive_selection(&mut self, inclusive: bool) {
        self.inclusive_selection = inclusive;
    }

    pub fn selection_range(&self) -> Option<(Position, Position)> {
        self.cursor.selection_range()
    }

    pub fn select_all(&mut self) {
        self.cursor.move_to(0, 0);
        self.cursor.start_selection();
        let last_row = self.buffer.line_count().saturating_sub(1);
        self.cursor.move_to(last_row, self.buffer.line_len(last_row));
        self.inclusive_selection = false;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_with_selection(&mut self, movement: CursorMove, extend: bool) {
        if extend {
            if !self.cursor.has_selection() {
                self.cursor.start_selection();
                self.inclusive_selection = false;
            }
        } else {
            self.cancel_selection();
        }
        self.move_cursor(movement);
    }

    /// Selection range as used by copy/cut/rendering. When the selection is
    /// inclusive (character-wise Visual mode), the end is extended by one
    /// character so the cell under the cursor is part of the range.
    pub(super) fn effective_selection_range(&self) -> Option<(Position, Position)> {
        let (start, end) = self.cursor.selection_range()?;
        if self.inclusive_selection {
            let line_len = self.buffer.line(end.row).map(|l| l.chars().count()).unwrap_or(0);
            let end = Position { row: end.row, col: (end.col + 1).min(line_len) };
            Some((start, end))
        } else {
            Some((start, end))
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.effective_selection_range()?;
        Some(self.buffer.get_text_range(start.row, start.col, end.row, end.col))
    }
    pub fn copy(&mut self) {
        if let Some(text) = self.selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = false;
            let _ = self.clipboard_port.set_text(&text);
        }
    }

    pub fn cut(&mut self) {
        if let Some(deleted) = self.delete_selection_impl() {
            self.clipboard = Some(deleted.clone());
            self.clipboard_linewise = false;
            let _ = self.clipboard_port.set_text(&deleted);
        }
    }

    pub fn delete_selection(&mut self) -> bool {
        self.delete_selection_impl().is_some()
    }

    fn delete_selection_impl(&mut self) -> Option<String> {
        let (start, end) = self.effective_selection_range()?;
        if start == end {
            self.cancel_selection();
            return None;
        }
        let cursor_before = self.cursor.pos();
        let deleted = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
        self.wrap_cache.invalidate_from(start.row);
        let removed_rows = end.row.saturating_sub(start.row);
        if removed_rows > 0 {
            self.highlight_index.shift_rows_after(end.row + 1, -(removed_rows as isize));
            self.row_style_cache.borrow_mut().shift_rows_after(end.row + 1, -(removed_rows as isize));
        }
        self.update_row_highlights(start.row);
        self.history.record(EditOperation::Delete { start, end, deleted_text: deleted.clone() }, cursor_before, start);
        self.cursor.move_to(start.row, start.col);
        self.cancel_selection();
        self.ensure_cursor_visible();
        Some(deleted)
    }

    pub(super) fn take_selection_operation(&mut self) -> Option<EditOperation> {
        let (start, end) = self.effective_selection_range()?;
        if start == end {
            self.cancel_selection();
            return None;
        }
        let deleted_text = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
        self.wrap_cache.invalidate_from(start.row);
        let removed_rows = end.row.saturating_sub(start.row);
        if removed_rows > 0 {
            self.highlight_index.shift_rows_after(end.row + 1, -(removed_rows as isize));
            self.row_style_cache.borrow_mut().shift_rows_after(end.row + 1, -(removed_rows as isize));
        }
        self.update_row_highlights(start.row);
        self.cursor.move_to(start.row, start.col);
        self.cancel_selection();
        Some(EditOperation::Delete { start, end, deleted_text })
    }
}
