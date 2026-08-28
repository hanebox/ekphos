use super::*;

impl Editor {
    pub fn delete_current_line(&mut self) {
        let pos = self.cursor.pos();
        let row = pos.row;
        let line_count = self.buffer.line_count();
        let line_text = self.buffer.line(row).unwrap_or("").to_string();
        let deleted_text = format!("{}\n", line_text);
        self.clipboard = Some(deleted_text.clone());
        let _ = self.clipboard_port.set_text(&deleted_text);
        self.buffer.delete_line(row);
        self.wrap_cache.invalidate_from(row);
        let cursor_after = Position { row: row.min(self.buffer.line_count().saturating_sub(1)), col: 0 };
        if line_count == 1 {
            self.history.record(EditOperation::Delete { start: Position { row: 0, col: 0 }, end: Position { row: 0, col: line_text.chars().count() }, deleted_text: line_text }, pos, cursor_after);
        } else {
            self.history.record(EditOperation::LineDelete { row, lines: vec![line_text] }, pos, cursor_after);
        }
        let new_row = if line_count == 1 {
            0
        } else if row >= self.buffer.line_count() {
            self.buffer.line_count().saturating_sub(1)
        } else {
            row
        };
        self.cursor.move_to(new_row, 0);
        self.cursor.cancel_selection();
        self.ensure_cursor_visible();
    }

    pub fn paste(&mut self) {
        let text = self.clipboard.clone().or_else(|| self.clipboard_port.get_text().ok().flatten());
        if let Some(text) = text {
            self.insert_str(&text);
        }
    }

    /// Paste after cursor (vim 'p' command)
    /// For line-wise content: paste below current line
    /// For character-wise content: paste after cursor
    pub fn paste_after(&mut self) {
        let (text, linewise) = if let Some(text) = self.clipboard.clone() {
            (text, self.clipboard_linewise)
        } else if let Some(text) = self.clipboard_port.get_text().ok().flatten() {
            let linewise = text.ends_with('\n');
            (text, linewise)
        } else {
            return;
        };
        if linewise {
            let (row, col) = self.cursor();
            let cursor_before = Position { row, col };
            let new_row = row + 1;
            let text_to_insert = text.trim_end_matches('\n');
            let lines: Vec<String> = text_to_insert.split('\n').map(|s| s.to_string()).collect();
            for (i, line) in lines.iter().enumerate() {
                self.buffer.insert_line(new_row + i, line.clone());
                self.wrap_cache.insert_line(new_row + i);
            }
            self.cursor.move_to(new_row, 0);
            self.history.record(EditOperation::LineInsert { row: new_row, lines }, cursor_before, Position { row: new_row, col: 0 });
        } else {
            let (row, col) = self.cursor();
            let line_len = self.buffer.line(row).map(|l| l.chars().count()).unwrap_or(0);
            let new_col = (col + 1).min(line_len);
            self.cursor.move_to(row, new_col);
            self.insert_str(&text);
        }
        self.ensure_cursor_visible();
    }

    /// Paste before cursor (vim 'P' command)
    /// For line-wise content: paste above current line
    /// For character-wise content: paste before cursor
    pub fn paste_before(&mut self) {
        let (text, linewise) = if let Some(text) = self.clipboard.clone() {
            (text, self.clipboard_linewise)
        } else if let Some(text) = self.clipboard_port.get_text().ok().flatten() {
            let linewise = text.ends_with('\n');
            (text, linewise)
        } else {
            return;
        };
        if linewise {
            let (row, col) = self.cursor();
            let cursor_before = Position { row, col };
            let text_to_insert = text.trim_end_matches('\n');
            let lines: Vec<String> = text_to_insert.split('\n').map(|s| s.to_string()).collect();
            for (i, line) in lines.iter().enumerate() {
                self.buffer.insert_line(row + i, line.clone());
                self.wrap_cache.insert_line(row + i);
            }
            self.cursor.move_to(row, 0);
            self.history.record(EditOperation::LineInsert { row, lines }, cursor_before, Position { row, col: 0 });
        } else {
            self.insert_str(&text);
        }
        self.ensure_cursor_visible();
    }
    pub fn insert_char(&mut self, c: char) {
        let cursor_before = self.cursor.pos();
        if self.cursor.has_selection() {
            self.delete_selection_internal();
        }
        let pos = self.cursor.pos();
        self.buffer.insert_char(pos.row, pos.col, c);
        self.wrap_cache.invalidate_line(pos.row);
        self.update_row_highlights(pos.row);
        self.history.record(EditOperation::Insert { pos, text: c.to_string() }, cursor_before, Position::new(pos.row, pos.col + 1));
        self.cursor.move_to(pos.row, pos.col + 1);
        self.ensure_cursor_visible();
    }

    /// Insert string at cursor, handling multi-line text and selection replacement
    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let cursor_before = self.cursor.pos();
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
        let newline_count = s.bytes().filter(|byte| *byte == b'\n').count();
        if newline_count == 0 {
            self.buffer.insert_str(pos.row, pos.col, s);
            self.wrap_cache.invalidate_line(pos.row);
            self.update_row_highlights(pos.row);
            self.cursor.move_to(pos.row, pos.col + s.chars().count());
        } else {
            let mut parts = s.split('\n');
            let first_part = parts.next().unwrap_or_default();
            if !first_part.is_empty() {
                self.buffer.insert_str(pos.row, pos.col, first_part);
            }
            let split_col = pos.col + first_part.chars().count();
            self.buffer.split_line(pos.row, split_col);
            self.wrap_cache.insert_line(pos.row + 1);
            let mut remaining = parts.peekable();
            let mut inserted_rows = 0;
            let mut last_part = "";
            while let Some(part) = remaining.next() {
                if remaining.peek().is_none() {
                    last_part = part;
                } else {
                    self.buffer.insert_line(pos.row + 1 + inserted_rows, part.to_string());
                    self.wrap_cache.insert_line(pos.row + 1 + inserted_rows);
                    inserted_rows += 1;
                }
            }
            let last_idx = pos.row + newline_count;
            if !last_part.is_empty() {
                self.buffer.insert_str(last_idx, 0, last_part);
            }
            self.wrap_cache.invalidate_from(pos.row);
            self.highlight_index.shift_rows_after(pos.row + 1, newline_count as isize);
            self.row_style_cache.borrow_mut().shift_rows_after(pos.row + 1, newline_count as isize);
            self.recalc_code_blocks_from(pos.row);
            self.cursor.move_to(last_idx, last_part.chars().count());
        }
        let had_selection = deleted_selection.is_some();
        if let Some((start, end, deleted_text)) = deleted_selection {
            self.history.record(EditOperation::Delete { start, end, deleted_text }, cursor_before, pos);
        }
        self.history.record(EditOperation::Insert { pos, text: s.to_string() }, if had_selection { pos } else { cursor_before }, self.cursor.pos());
        self.ensure_cursor_visible();
    }

    pub fn insert_newline(&mut self) {
        let cursor_before = self.cursor.pos();
        if self.cursor.has_selection() {
            self.delete_selection_internal();
        }
        let pos = self.cursor.pos();
        let list_prefix = self.buffer.line(pos.row).and_then(|line| {
            let prefix = ListPrefix::detect(line)?;
            let prefix_len = prefix.prefix_len(line);
            let line_char_count = line.chars().count();
            let is_empty_item = line_char_count <= prefix_len;
            Some((prefix, prefix_len, is_empty_item))
        });
        if let Some((_, prefix_len, true)) = &list_prefix {
            let deleted = self.buffer.delete_range(pos.row, 0, *prefix_len);
            self.wrap_cache.invalidate_line(pos.row);
            self.update_row_highlights(pos.row);
            self.history.record(EditOperation::Delete { start: Position::new(pos.row, 0), end: Position::new(pos.row, *prefix_len), deleted_text: deleted }, cursor_before, Position::new(pos.row, 0));
            self.cursor.move_to(pos.row, 0);
            self.ensure_cursor_visible();
            return;
        }
        self.buffer.split_line(pos.row, pos.col);
        self.wrap_cache.insert_line(pos.row + 1);
        self.wrap_cache.invalidate_line(pos.row);
        self.highlight_index.shift_rows_after(pos.row + 1, 1);
        self.row_style_cache.borrow_mut().shift_rows_after(pos.row + 1, 1);
        self.history.record(EditOperation::SplitLine { pos }, cursor_before, Position::new(pos.row + 1, 0));
        if let Some((prefix, _, false)) = list_prefix {
            let next_prefix = prefix.next_prefix();
            let prefix_char_count = next_prefix.chars().count();
            self.buffer.insert_str(pos.row + 1, 0, &next_prefix);
            self.wrap_cache.invalidate_line(pos.row + 1);
            self.history.record(EditOperation::Insert { pos: Position::new(pos.row + 1, 0), text: next_prefix }, Position::new(pos.row + 1, 0), Position::new(pos.row + 1, prefix_char_count));
            self.cursor.move_to(pos.row + 1, prefix_char_count);
        } else {
            self.cursor.move_to(pos.row + 1, 0);
        }
        self.update_row_highlights(pos.row);
        self.update_row_highlights(pos.row + 1);
        self.ensure_cursor_visible();
    }

    pub fn open_line_above(&mut self) {
        let pos = self.cursor.pos();
        let cursor_before = pos;
        let indent: String = self.buffer.line(pos.row).map(|line| line.chars().take_while(|c| c.is_whitespace()).collect()).unwrap_or_default();
        let indent_len = indent.chars().count();
        self.buffer.insert_line(pos.row, indent.clone());
        self.wrap_cache.insert_line(pos.row);
        self.highlight_index.shift_rows_after(pos.row, 1);
        self.row_style_cache.borrow_mut().shift_rows_after(pos.row, 1);
        self.update_row_highlights(pos.row);
        self.history.record(EditOperation::LineInsert { row: pos.row, lines: vec![indent] }, cursor_before, Position::new(pos.row, indent_len));
        self.cursor.move_to(pos.row, indent_len);
        self.ensure_cursor_visible();
    }

    pub fn delete_char(&mut self) {
        let pos = self.cursor.pos();
        let line_len = self.buffer.line_len(pos.row);
        if pos.col < line_len {
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col) {
                self.wrap_cache.invalidate_line(pos.row);
                self.update_row_highlights(pos.row);
                self.history.record(EditOperation::Delete { start: pos, end: Position::new(pos.row, pos.col + 1), deleted_text: c.to_string() }, pos, pos);
            }
        } else if pos.row + 1 < self.buffer.line_count() {
            self.buffer.join_with_previous(pos.row + 1);
            self.wrap_cache.remove_line(pos.row + 1);
            self.wrap_cache.invalidate_line(pos.row);
            self.highlight_index.shift_rows_after(pos.row + 1, -1);
            self.row_style_cache.borrow_mut().shift_rows_after(pos.row + 1, -1);
            self.update_row_highlights(pos.row);
            self.history.record(EditOperation::JoinLine { row: pos.row + 1, col: line_len }, pos, pos);
        }
    }

    pub fn delete_newline(&mut self) {
        let pos = self.cursor.pos();
        if pos.col > 0 {
            let cursor_before = pos;
            self.cursor.move_to(pos.row, pos.col - 1);
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col - 1) {
                self.wrap_cache.invalidate_line(pos.row);
                self.update_row_highlights(pos.row);
                self.history.record(EditOperation::Delete { start: Position::new(pos.row, pos.col - 1), end: pos, deleted_text: c.to_string() }, cursor_before, self.cursor.pos());
            }
        } else if pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            let cursor_before = pos;
            self.buffer.join_with_previous(pos.row);
            self.wrap_cache.remove_line(pos.row);
            self.wrap_cache.invalidate_line(pos.row - 1);
            self.highlight_index.shift_rows_after(pos.row, -1);
            self.row_style_cache.borrow_mut().shift_rows_after(pos.row, -1);
            self.update_row_highlights(pos.row - 1);
            self.history.record(EditOperation::JoinLine { row: pos.row, col: prev_len }, cursor_before, Position::new(pos.row - 1, prev_len));
            self.cursor.move_to(pos.row - 1, prev_len);
        }
        self.ensure_cursor_visible();
    }

    pub fn undo(&mut self) -> bool {
        let mut history = std::mem::take(&mut self.history);
        let undone = if let Some(entry) = history.pop_undo() {
            for op in entry.operations.iter().rev() {
                self.apply_inverse_operation(op);
            }
            self.cursor.move_to(entry.cursor_before.row, entry.cursor_before.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
            true
        } else {
            false
        };
        self.history = history;
        undone
    }

    pub fn redo(&mut self) -> bool {
        let mut history = std::mem::take(&mut self.history);
        let redone = if let Some(entry) = history.pop_redo() {
            for op in &entry.operations {
                self.apply_operation(op);
            }
            self.cursor.move_to(entry.cursor_after.row, entry.cursor_after.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
            true
        } else {
            false
        };
        self.history = history;
        redone
    }
    fn apply_inverse_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { pos, text } => {
                let end = history::calculate_end_position(*pos, text);
                self.buffer.discard_text_range(pos.row, pos.col, end.row, end.col);
                self.wrap_cache.invalidate_from(pos.row);
            }
            EditOperation::Delete { start, deleted_text, .. } => self.apply_insert(*start, deleted_text),
            EditOperation::SplitLine { pos } => {
                self.buffer.join_with_previous(pos.row + 1);
                self.wrap_cache.remove_line(pos.row + 1);
                self.wrap_cache.invalidate_line(pos.row);
            }
            EditOperation::JoinLine { row, col } => {
                self.buffer.split_line(row - 1, *col);
                self.wrap_cache.insert_line(*row);
                self.wrap_cache.invalidate_line(row - 1);
            }
            EditOperation::BlockDelete { start_row, start_col, deleted_lines, .. } => {
                for (index, text) in deleted_lines.iter().enumerate() {
                    let row = start_row + index;
                    if row < self.buffer.line_count() {
                        self.buffer.insert_str(row, *start_col, text);
                    }
                }
                self.wrap_cache.invalidate_from(*start_row);
            }
            #[cfg(test)]
            EditOperation::BlockInsert { start_row, col, lines } => {
                let width = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
                if width > 0 {
                    self.delete_block(*start_row, start_row + lines.len().saturating_sub(1), *col, col + width - 1);
                }
            }
            EditOperation::LineInsert { row, lines } => {
                for _ in 0..lines.len() {
                    if *row < self.buffer.line_count() {
                        self.buffer.delete_line(*row);
                        self.wrap_cache.remove_line(*row);
                    }
                }
            }
            EditOperation::LineDelete { row, lines } => {
                for (index, line) in lines.iter().enumerate() {
                    self.buffer.insert_line(row + index, line.clone());
                    self.wrap_cache.insert_line(row + index);
                }
            }
        }
    }
    fn apply_insert(&mut self, pos: Position, text: &str) {
        if text.contains('\n') {
            let mut parts = text.split('\n');
            let first = parts.next().unwrap_or_default();
            self.buffer.insert_str(pos.row, pos.col, first);
            let mut current_row = pos.row;
            let mut split_col = pos.col + first.chars().count();
            for part in parts {
                self.buffer.split_line(current_row, split_col);
                current_row += 1;
                if !part.is_empty() {
                    self.buffer.insert_str(current_row, 0, part);
                }
                split_col = part.chars().count();
            }
        } else {
            self.buffer.insert_str(pos.row, pos.col, text);
        }
        self.wrap_cache.invalidate_from(pos.row);
    }
    fn delete_block(&mut self, start_row: usize, end_row: usize, start_col: usize, end_col: usize) {
        for row in (start_row..=end_row).rev() {
            self.buffer.delete_range(row, start_col, end_col.saturating_add(1));
        }
        self.wrap_cache.invalidate_from(start_row);
    }
    pub(super) fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { pos, text } => {
                self.apply_insert(*pos, text);
            }
            EditOperation::Delete { start, end, .. } => {
                self.buffer.discard_text_range(start.row, start.col, end.row, end.col);
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
            EditOperation::BlockDelete { start_row, end_row, start_col, end_col, .. } => {
                self.delete_block(*start_row, *end_row, *start_col, *end_col);
            }
            #[cfg(test)]
            EditOperation::BlockInsert { start_row, col, lines } => {
                for (i, text) in lines.iter().enumerate() {
                    let row = start_row + i;
                    if row < self.buffer.line_count() {
                        self.buffer.insert_str(row, *col, text);
                    }
                }
                self.wrap_cache.invalidate_from(*start_row);
            }
            EditOperation::LineInsert { row, lines } => {
                for (i, line) in lines.iter().enumerate() {
                    self.buffer.insert_line(row + i, line.clone());
                    self.wrap_cache.insert_line(row + i);
                }
            }
            EditOperation::LineDelete { row, lines } => {
                for _ in 0..lines.len() {
                    if *row < self.buffer.line_count() {
                        self.buffer.delete_line(*row);
                        self.wrap_cache.remove_line(*row);
                    }
                }
            }
        }
    }
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
    pub fn iter_lines(&self) -> impl Iterator<Item = &str> {
        self.buffer.iter_lines()
    }

    pub fn snapshot(&self) -> EditorSnapshot {
        self.buffer.snapshot()
    }

    pub fn text(&self) -> String {
        self.snapshot().to_text()
    }

    pub fn line(&self, row: usize) -> Option<&str> {
        self.buffer.line(row)
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}
