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

    /// Selection range as used by copy/cut/rendering. When the selection is
    /// inclusive (character-wise Visual mode), the end is extended by one
    /// character so the cell under the cursor is part of the range.
    pub(super) fn effective_selection_range(&self) -> Option<(Position, Position)> {
        let (start, end) = self.cursor.selection_range()?;
        if self.inclusive_selection {
            let line_len = self.buffer.line(end.row).map(|l| l.chars().count()).unwrap_or(0);
            let end = Position {
                row: end.row,
                col: (end.col + 1).min(line_len),
            };
            Some((start, end))
        } else {
            Some((start, end))
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.effective_selection_range()?;
        Some(self.buffer.get_text_range(start.row, start.col, end.row, end.col))
    }

    // Clipboard
    pub fn copy(&mut self) {
        if let Some(text) = self.selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = false;
            let _ = self.clipboard_port.set_text(&text);
        }
    }

    pub fn cut(&mut self) {
        if let Some((start, end)) = self.effective_selection_range() {
            let cursor_before = self.cursor.pos();
            let deleted = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
            self.clipboard = Some(deleted.clone());
            self.clipboard_linewise = false;
            let _ = self.clipboard_port.set_text(&deleted);
            self.wrap_cache.invalidate_from(start.row);

            self.history.record(
                EditOperation::Delete {
                    start,
                    end,
                    deleted_text: deleted,
                },
                cursor_before,
                start,
            );

            self.cursor.move_to(start.row, start.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
        }
    }

    /// Delete the current line entirely (for dd command)
    pub(super) fn delete_selection_internal(&mut self) {
        if let Some((start, end)) = self.cursor.selection_range() {
            let lines_deleted = end.row - start.row;
            self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
            self.wrap_cache.invalidate_from(start.row);

            if lines_deleted > 0 {
                self.highlight_index.shift_rows_after(end.row + 1, -(lines_deleted as isize));
                self.row_style_cache.borrow_mut().shift_rows_after(end.row + 1, -(lines_deleted as isize));
            }
            self.update_row_highlights(start.row);

            self.cursor.move_to(start.row, start.col);
            self.cursor.cancel_selection();
        }
    }

    // Undo/Redo
}
