use super::*;

impl Editor {
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

    pub fn set_cursor_no_scroll(&mut self, row: usize, col: usize) {
        let line_count = self.buffer.line_count();
        let safe_row = row.min(line_count.saturating_sub(1));
        let line_len = self.buffer.line_len(safe_row);
        let safe_col = col.min(line_len);
        self.cursor.move_to(safe_row, safe_col);
    }

    pub fn move_cursor(&mut self, movement: CursorMove) {
        let pos = self.cursor.pos();
        let line_count = self.buffer.line_count();
        if !matches!(movement, CursorMove::Up | CursorMove::Down) {
            self.preferred_visual_x = None;
        }
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
                if self.line_wrap_enabled && self.view_width > 0 {
                    let content_width = self.wrap_content_width();
                    if content_width > 0 {
                        let (cur_visual_line, cur_x) = self.cursor_wrapped_position();
                        let preferred_x = self.preferred_visual_x.unwrap_or(cur_x);
                        self.preferred_visual_x = Some(preferred_x);
                        if cur_visual_line > 0 {
                            let new_col = self.col_at_visual_pos(pos.row, cur_visual_line - 1, preferred_x, content_width);
                            self.cursor.set_pos(Position::new(pos.row, new_col), false);
                        } else if pos.row > 0 {
                            let prev_visual_lines = self.visual_lines_for_row(pos.row - 1, content_width);
                            let new_col = self.col_at_visual_pos(pos.row - 1, prev_visual_lines.saturating_sub(1), preferred_x, content_width);
                            self.cursor.set_pos(Position::new(pos.row - 1, new_col), false);
                        }
                    } else if pos.row > 0 {
                        let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                        let prev_len = self.buffer.line_len(pos.row - 1);
                        self.cursor.set_pos(Position::new(pos.row - 1, preferred.min(prev_len)), false);
                    }
                } else if pos.row > 0 {
                    let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                    let prev_len = self.buffer.line_len(pos.row - 1);
                    self.cursor.set_pos(Position::new(pos.row - 1, preferred.min(prev_len)), false);
                }
            }
            CursorMove::Down => {
                if self.line_wrap_enabled && self.view_width > 0 {
                    let content_width = self.wrap_content_width();
                    if content_width > 0 {
                        let (cur_visual_line, cur_x) = self.cursor_wrapped_position();
                        let preferred_x = self.preferred_visual_x.unwrap_or(cur_x);
                        self.preferred_visual_x = Some(preferred_x);
                        let total_visual_lines = self.visual_lines_for_row(pos.row, content_width);
                        if cur_visual_line + 1 < total_visual_lines {
                            let new_col = self.col_at_visual_pos(pos.row, cur_visual_line + 1, preferred_x, content_width);
                            self.cursor.set_pos(Position::new(pos.row, new_col), false);
                        } else if pos.row + 1 < line_count {
                            let new_col = self.col_at_visual_pos(pos.row + 1, 0, preferred_x, content_width);
                            self.cursor.set_pos(Position::new(pos.row + 1, new_col), false);
                        }
                    } else if pos.row + 1 < line_count {
                        let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                        let next_len = self.buffer.line_len(pos.row + 1);
                        self.cursor.set_pos(Position::new(pos.row + 1, preferred.min(next_len)), false);
                    }
                } else if pos.row + 1 < line_count {
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
            CursorMove::FirstNonBlank => {
                if let Some(line) = self.buffer.line(pos.row) {
                    let col = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                    self.cursor.move_to(pos.row, col);
                }
            }
            CursorMove::WordEndForward => self.move_word_end_forward(),
            CursorMove::BigWordForward => self.move_big_word_forward(),
            CursorMove::BigWordBack => self.move_big_word_back(),
            CursorMove::BigWordEndForward => self.move_big_word_end_forward(),
            CursorMove::WordEndBackward => self.move_word_end_backward(),
            CursorMove::BigWordEndBackward => self.move_big_word_end_backward(),
            CursorMove::ParagraphForward => {
                let mut row = pos.row;
                while row < line_count && !self.buffer.line(row).is_none_or(|l| l.trim().is_empty()) {
                    row += 1;
                }
                while row < line_count && self.buffer.line(row).is_some_and(|l| l.trim().is_empty()) {
                    row += 1;
                }
                self.cursor.move_to(row.min(line_count.saturating_sub(1)), 0);
            }
            CursorMove::ParagraphBack => {
                let mut row = pos.row;
                row = row.saturating_sub(1);
                while row > 0 && self.buffer.line(row).is_some_and(|l| l.trim().is_empty()) {
                    row -= 1;
                }
                while row > 0 && !self.buffer.line(row - 1).is_none_or(|l| l.trim().is_empty()) {
                    row -= 1;
                }
                self.cursor.move_to(row, 0);
            }
            CursorMove::ScreenTop => {
                let row = self.scroll_offset;
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::ScreenMiddle => {
                let row = (self.scroll_offset + self.view_height / 2).min(line_count.saturating_sub(1));
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::ScreenBottom => {
                let row = (self.scroll_offset + self.view_height.saturating_sub(1)).min(line_count.saturating_sub(1));
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::HalfPageUp => {
                let half = self.view_height / 2;
                let new_row = pos.row.saturating_sub(half);
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                self.scroll_offset = self.scroll_offset.saturating_sub(half);
            }
            CursorMove::HalfPageDown => {
                let half = self.view_height / 2;
                let new_row = (pos.row + half).min(line_count.saturating_sub(1));
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                if self.scroll_offset + half < line_count.saturating_sub(self.view_height) {
                    self.scroll_offset += half;
                }
            }
            CursorMove::PageUp => {
                let page = self.view_height.saturating_sub(2);
                let new_row = pos.row.saturating_sub(page);
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                self.scroll_offset = self.scroll_offset.saturating_sub(page);
            }
            CursorMove::PageDown => {
                let page = self.view_height.saturating_sub(2);
                let new_row = (pos.row + page).min(line_count.saturating_sub(1));
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                let max_scroll = line_count.saturating_sub(self.view_height);
                self.scroll_offset = (self.scroll_offset + page).min(max_scroll);
            }
            CursorMove::MatchingBracket => {
                if let Some(new_pos) = self.find_matching_bracket() {
                    self.cursor.move_to(new_pos.row, new_pos.col);
                }
            }
            CursorMove::GoToLine(line) => {
                let row = line.saturating_sub(1).min(line_count.saturating_sub(1));
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::GoToColumn(col) => {
                let line_len = self.buffer.line_len(pos.row);
                self.cursor.move_to(pos.row, col.saturating_sub(1).min(line_len));
            }
        }
        self.ensure_cursor_visible();
    }
    pub(super) fn move_word_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
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
    pub(super) fn move_word_back(&mut self) {
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
    pub(super) fn move_word_end_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
        let len = line.chars().count();
        if len == 0 || pos.col >= len.saturating_sub(1) {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_word_end_forward();
            }
            return;
        }
        let mut chars = line.chars().enumerate().skip(pos.col + 1);
        let Some((mut col, first)) = chars.find(|(_, ch)| !ch.is_whitespace()) else {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_word_end_forward();
            }
            return;
        };
        let is_word = cursor::is_word_char(first);
        for (index, ch) in chars {
            if ch.is_whitespace() || cursor::is_word_char(ch) != is_word {
                break;
            }
            col = index;
        }
        self.cursor.move_to(pos.row, col);
    }
    pub(super) fn move_word_end_backward(&mut self) {
        let pos = self.cursor.pos();
        if pos.col == 0 {
            if pos.row > 0 {
                let prev_len = self.buffer.line_len(pos.row - 1);
                self.cursor.move_to(pos.row - 1, prev_len.saturating_sub(1));
            }
            return;
        }
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
        let mut run = None;
        let mut previous_run = None;
        for (index, ch) in line.chars().enumerate().take(pos.col) {
            if ch.is_whitespace() {
                previous_run = run.take().or(previous_run);
            } else {
                let class = cursor::is_word_char(ch);
                if run.is_none_or(|(_, current_class)| current_class != class) {
                    previous_run = run.take().or(previous_run);
                    run = Some((index, class));
                }
            }
        }
        self.cursor.move_to(pos.row, run.or(previous_run).map_or(0, |(start, _)| start));
    }
    pub(super) fn move_big_word_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
        let len = line.chars().count();
        let col = line.chars().enumerate().skip(pos.col).skip_while(|(_, ch)| !ch.is_whitespace()).find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index)).unwrap_or(len);
        if col >= len && pos.row + 1 < self.buffer.line_count() {
            self.cursor.move_to(pos.row + 1, 0);
            if let Some(next) = self.buffer.line(pos.row + 1) {
                let skip = next.chars().take_while(|c| c.is_whitespace()).count();
                self.cursor.move_to(pos.row + 1, skip);
            }
        } else {
            self.cursor.move_to(pos.row, col.min(len));
        }
    }
    pub(super) fn move_big_word_back(&mut self) {
        let pos = self.cursor.pos();
        if pos.col == 0 && pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            self.cursor.move_to(pos.row - 1, prev_len);
            self.move_big_word_back();
            return;
        }
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
        let mut run_start = None;
        let mut previous_run_start = None;
        for (index, ch) in line.chars().enumerate().take(pos.col) {
            if ch.is_whitespace() {
                previous_run_start = run_start.take().or(previous_run_start);
            } else if run_start.is_none() {
                run_start = Some(index);
            }
        }
        self.cursor.move_to(pos.row, run_start.or(previous_run_start).unwrap_or(0));
    }
    pub(super) fn move_big_word_end_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
        let len = line.chars().count();
        if len == 0 || pos.col >= len.saturating_sub(1) {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_big_word_end_forward();
            }
            return;
        }
        let mut chars = line.chars().enumerate().skip(pos.col + 1);
        let Some((mut col, _)) = chars.find(|(_, ch)| !ch.is_whitespace()) else {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_big_word_end_forward();
            }
            return;
        };
        for (index, ch) in chars {
            if ch.is_whitespace() {
                break;
            }
            col = index;
        }
        self.cursor.move_to(pos.row, col);
    }
    pub(super) fn move_big_word_end_backward(&mut self) {
        let pos = self.cursor.pos();
        if pos.col == 0 {
            if pos.row > 0 {
                let prev_len = self.buffer.line_len(pos.row - 1);
                self.cursor.move_to(pos.row - 1, prev_len.saturating_sub(1));
            }
            return;
        }
        let Some(line) = self.buffer.line(pos.row) else {
            return;
        };
        let mut run_start = None;
        let mut previous_run_start = None;
        for (index, ch) in line.chars().enumerate().take(pos.col) {
            if ch.is_whitespace() {
                previous_run_start = run_start.take().or(previous_run_start);
            } else if run_start.is_none() {
                run_start = Some(index);
            }
        }
        self.cursor.move_to(pos.row, run_start.or(previous_run_start).unwrap_or(0));
    }
    pub(super) fn find_matching_bracket(&self) -> Option<Position> {
        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row)?;
        let current = line.chars().nth(pos.col)?;
        let (open, close, forward) = match current {
            '(' => ('(', ')', true),
            ')' => ('(', ')', false),
            '[' => ('[', ']', true),
            ']' => ('[', ']', false),
            '{' => ('{', '}', true),
            '}' => ('{', '}', false),
            '<' => ('<', '>', true),
            '>' => ('<', '>', false),
            _ => return None,
        };
        let mut depth = 1;
        let mut row = pos.row;
        let mut col = pos.col;
        let line_count = self.buffer.line_count();
        if forward {
            let mut start_col = col + 1;
            loop {
                let l = self.buffer.line(row)?;
                for (char_col, ch) in l.chars().enumerate().skip(start_col) {
                    if ch == open {
                        depth += 1;
                    } else if ch == close {
                        depth -= 1;
                        if depth == 0 {
                            return Some(Position::new(row, char_col));
                        }
                    }
                }
                row += 1;
                start_col = 0;
                if row >= line_count {
                    return None;
                }
            }
        } else {
            if col == 0 {
                if row == 0 {
                    return None;
                }
                row -= 1;
                col = self.buffer.line_len(row).saturating_sub(1);
            } else {
                col -= 1;
            }
            loop {
                let l = self.buffer.line(row)?;
                let end_byte = buffer::char_to_byte_index(l, col.saturating_add(1));
                let mut char_col = l[..end_byte].chars().count();
                for (_, ch) in l[..end_byte].char_indices().rev() {
                    char_col -= 1;
                    if ch == close {
                        depth += 1;
                    } else if ch == open {
                        depth -= 1;
                        if depth == 0 {
                            return Some(Position::new(row, char_col));
                        }
                    }
                }
                if row == 0 {
                    return None;
                }
                row -= 1;
                col = self.buffer.line_len(row).saturating_sub(1);
            }
        }
    }
}
