use super::*;

impl Editor {
    // Selection
    pub fn delete_current_line(&mut self) {
        let pos = self.cursor.pos();
        let row = pos.row;
        let line_count = self.buffer.line_count();

        // Get the line content for clipboard (with newline)
        let line_text = self.buffer.line(row).unwrap_or("").to_string();
        let deleted_text = format!("{}\n", line_text);

        // Copy to clipboard
        self.clipboard = Some(deleted_text.clone());
        let _ = self.clipboard_port.set_text(&deleted_text);

        // Delete the line
        self.buffer.delete_line(row);
        self.wrap_cache.invalidate_from(row);

        let cursor_after = Position {
            row: row.min(self.buffer.line_count().saturating_sub(1)),
            col: 0,
        };

        if line_count == 1 {
            // Single-line buffer: the line isn't removed, its content is cleared
            // in place (the buffer always keeps at least one line). Record a
            // character-range delete with no trailing newline so undo restores the
            // content without inserting a spurious empty line below it.
            self.history.record(
                EditOperation::Delete {
                    start: Position { row: 0, col: 0 },
                    end: Position {
                        row: 0,
                        col: line_text.chars().count(),
                    },
                    deleted_text: line_text,
                },
                pos,
                cursor_after,
            );
        } else {
            // Multi-line buffer: a whole line is removed. LineDelete's inverse
            // (LineInsert) re-creates the line correctly even when it was the last
            // line of the buffer, where a character-range Delete's inverse would
            // target a row that no longer exists and silently lose the text.
            self.history
                .record(EditOperation::LineDelete { row, lines: vec![line_text] }, pos, cursor_after);
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
        // Try internal clipboard first, then fall back to system clipboard
        let (text, linewise) = if let Some(text) = self.clipboard.clone() {
            (text, self.clipboard_linewise)
        } else if let Some(text) = self.clipboard_port.get_text().ok().flatten() {
            // For system clipboard, detect linewise by checking if ends with newline
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

            self.history.record(
                EditOperation::LineInsert { row: new_row, lines },
                cursor_before,
                Position { row: new_row, col: 0 },
            );
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
            // For system clipboard, detect linewise by checking if ends with newline
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

            self.history
                .record(EditOperation::LineInsert { row, lines }, cursor_before, Position { row, col: 0 });
        } else {
            self.insert_str(&text);
        }
        self.ensure_cursor_visible();
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

        self.update_row_highlights(pos.row);

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
            self.update_row_highlights(pos.row);
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

            self.highlight_index.shift_rows_after(pos.row + 1, newline_count as isize);
            self.row_style_cache.borrow_mut().shift_rows_after(pos.row + 1, newline_count as isize);
            self.recalc_code_blocks_from(pos.row);

            self.cursor.move_to(last_idx, last_part.chars().count());
        }

        // Record undo operations
        let had_selection = deleted_selection.is_some();
        if let Some((start, end, deleted_text)) = deleted_selection {
            self.history.record(EditOperation::Delete { start, end, deleted_text }, cursor_before, pos);
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
            self.history.record(
                EditOperation::Delete {
                    start: Position::new(pos.row, 0),
                    end: Position::new(pos.row, *prefix_len),
                    deleted_text: deleted,
                },
                cursor_before,
                Position::new(pos.row, 0),
            );
            self.cursor.move_to(pos.row, 0);
            self.ensure_cursor_visible();
            return;
        }

        self.buffer.split_line(pos.row, pos.col);
        self.wrap_cache.insert_line(pos.row + 1);
        self.wrap_cache.invalidate_line(pos.row);

        self.highlight_index.shift_rows_after(pos.row + 1, 1);
        self.row_style_cache.borrow_mut().shift_rows_after(pos.row + 1, 1);

        self.history
            .record(EditOperation::SplitLine { pos }, cursor_before, Position::new(pos.row + 1, 0));

        if let Some((prefix, _, false)) = list_prefix {
            let next_prefix = prefix.next_prefix();
            let prefix_char_count = next_prefix.chars().count();
            self.buffer.insert_str(pos.row + 1, 0, &next_prefix);
            self.wrap_cache.invalidate_line(pos.row + 1);
            self.history.record(
                EditOperation::Insert {
                    pos: Position::new(pos.row + 1, 0),
                    text: next_prefix,
                },
                Position::new(pos.row + 1, 0),
                Position::new(pos.row + 1, prefix_char_count),
            );
            self.cursor.move_to(pos.row + 1, prefix_char_count);
        } else {
            self.cursor.move_to(pos.row + 1, 0);
        }

        // Update highlights for both affected rows
        self.update_row_highlights(pos.row);
        self.update_row_highlights(pos.row + 1);

        self.ensure_cursor_visible();
    }

    pub fn open_line_above(&mut self) {
        let pos = self.cursor.pos();
        let cursor_before = pos;
        let indent: String = self
            .buffer
            .line(pos.row)
            .map(|line| line.chars().take_while(|c| c.is_whitespace()).collect())
            .unwrap_or_default();

        let indent_len = indent.chars().count();
        self.buffer.insert_line(pos.row, indent.clone());
        self.wrap_cache.insert_line(pos.row);

        // Shift highlights for inserted line
        self.highlight_index.shift_rows_after(pos.row, 1);
        self.row_style_cache.borrow_mut().shift_rows_after(pos.row, 1);
        self.update_row_highlights(pos.row);

        self.history.record(
            EditOperation::LineInsert {
                row: pos.row,
                lines: vec![indent],
            },
            cursor_before,
            Position::new(pos.row, indent_len),
        );

        self.cursor.move_to(pos.row, indent_len);
        self.ensure_cursor_visible();
    }

    pub fn delete_char(&mut self) {
        let pos = self.cursor.pos();
        let line_len = self.buffer.line_len(pos.row);

        if pos.col < line_len {
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col) {
                self.wrap_cache.invalidate_line(pos.row);
                // Reactive highlight update
                self.update_row_highlights(pos.row);
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
            // Line joined: shift highlights and update
            self.highlight_index.shift_rows_after(pos.row + 1, -1);
            self.row_style_cache.borrow_mut().shift_rows_after(pos.row + 1, -1);
            self.update_row_highlights(pos.row);
            self.history.record(
                EditOperation::JoinLine {
                    row: pos.row + 1,
                    col: line_len,
                },
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
                // Reactive highlight update
                self.update_row_highlights(pos.row);
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

            self.highlight_index.shift_rows_after(pos.row, -1);
            self.row_style_cache.borrow_mut().shift_rows_after(pos.row, -1);
            self.update_row_highlights(pos.row - 1);

            self.history.record(
                EditOperation::JoinLine { row: pos.row, col: prev_len },
                cursor_before,
                Position::new(pos.row - 1, prev_len),
            );

            self.cursor.move_to(pos.row - 1, prev_len);
        }

        self.ensure_cursor_visible();
    }

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

    pub(super) fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { pos, text } => {
                if text.contains('\n') {
                    // Use split instead of lines() to preserve trailing newlines
                    // e.g., "hello\n".lines() returns ["hello"] but split returns ["hello", ""]
                    let parts: Vec<&str> = text.split('\n').collect();
                    if parts.is_empty() {
                        return;
                    }

                    // Insert first part at position
                    self.buffer.insert_str(pos.row, pos.col, parts[0]);

                    // For each subsequent part, split line and insert
                    let mut current_row = pos.row;
                    let mut split_col = pos.col + parts[0].chars().count();

                    for part in &parts[1..] {
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
            EditOperation::BlockDelete {
                start_row,
                end_row,
                start_col,
                end_col,
                ..
            } => {
                for row in (*start_row..=*end_row).rev() {
                    if let Some(line) = self.buffer.line(row) {
                        let chars: Vec<char> = line.chars().collect();
                        let line_len = chars.len();
                        let actual_start = (*start_col).min(line_len);
                        let actual_end = (*end_col + 1).min(line_len);
                        if actual_start < actual_end {
                            let new_line: String = chars[..actual_start].iter().chain(chars[actual_end..].iter()).collect();
                            if let Some(line_ref) = self.buffer.line_mut(row) {
                                *line_ref = new_line;
                            }
                        }
                    }
                }
                self.wrap_cache.invalidate_from(*start_row);
            }
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
}
