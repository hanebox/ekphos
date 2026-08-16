use super::*;

impl Editor {
    // Scrolling
    pub fn update_scroll(&mut self, view_height: usize) {
        self.view_height = view_height;
        if view_height == 0 {
            return;
        }

        let (cursor_row, cursor_col) = self.cursor();
        let line_count = self.buffer.line_count();

        let effective_scrolloff = self.scrolloff.min(view_height / 2);

        if cursor_row < self.scroll_offset + effective_scrolloff {
            self.scroll_offset = cursor_row.saturating_sub(effective_scrolloff);
        }

        if self.line_wrap_enabled && self.view_width > 0 {
            let (cursor_visual_offset, _) = self.cursor_wrapped_position();
            while self.scroll_offset < cursor_row {
                let lines_before = self.visual_lines_in_range(self.scroll_offset, cursor_row - 1);
                let total_lines = lines_before + cursor_visual_offset + 1;
                if total_lines + effective_scrolloff <= view_height {
                    break;
                }
                self.scroll_offset += 1;
            }
        } else if cursor_row + effective_scrolloff >= self.scroll_offset + view_height {
            self.scroll_offset = cursor_row.saturating_add(effective_scrolloff).saturating_sub(view_height.saturating_sub(1));
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

    pub(super) fn visual_lines_for_row(&self, row: usize, content_width: usize) -> usize {
        let line = match self.buffer.line(row) {
            Some(l) => l,
            None => return 1,
        };
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return 1;
        }

        let mut col = 0;
        let mut visual_lines = 1;

        while col < chars.len() {
            let mut x: usize = 0;
            let is_wrapped = visual_lines > 1;
            if is_wrapped && col < chars.len() && chars[col] == ' ' {
                col += 1;
                if col >= chars.len() {
                    break;
                }
            }

            while col < chars.len() && x < content_width {
                let ch = chars[col];
                let ch_width = char_display_width(ch, self.tab_width) as usize;
                x += ch_width;
                col += 1;
            }

            if col < chars.len() {
                visual_lines += 1;
            }
        }

        visual_lines
    }

    pub(super) fn visual_lines_in_range(&self, start_row: usize, end_row: usize) -> usize {
        let content_x_offset = self.content_x_offset() as usize;
        let content_width = self
            .view_width
            .saturating_sub(content_x_offset)
            .saturating_sub(self.right_padding as usize)
            .max(1);

        let mut visual_lines = 0;
        for row in start_row..=end_row.min(self.buffer.line_count().saturating_sub(1)) {
            visual_lines += self.visual_lines_for_row(row, content_width);
        }

        visual_lines
    }

    pub(super) fn ensure_cursor_visible(&mut self) {
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

    /// Returns the horizontal scroll offset in display units (accounting for Unicode widths).
    /// This calculates the display width of characters from 0 to h_scroll_offset on the cursor line.
    pub fn h_scroll_display_offset(&self) -> usize {
        if self.h_scroll_offset == 0 {
            return 0;
        }

        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        let mut display_offset: usize = 0;
        for (i, ch) in chars.iter().enumerate() {
            if i >= self.h_scroll_offset {
                break;
            }
            display_offset += char_display_width(*ch, self.tab_width) as usize;
        }
        display_offset
    }

    pub fn line_number_gutter_width(&self) -> u16 {
        if self.line_number_mode != LineNumberMode::None {
            self.line_number_width
        } else {
            0
        }
    }
    pub fn content_left_offset(&self) -> u16 {
        self.left_padding + self.line_number_gutter_width()
    }

    /// Returns the cursor's display column position, accounting for Unicode character widths and tabs.
    pub fn cursor_display_col(&self) -> usize {
        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        let mut display_col: usize = 0;
        for (i, ch) in chars.iter().enumerate() {
            if i >= pos.col {
                break;
            }
            display_col += char_display_width(*ch, self.tab_width) as usize;
        }
        display_col
    }

    /// Returns the cursor screen position info for native cursor positioning.
    pub fn cursor_screen_info(&self) -> (usize, bool, usize) {
        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        let mut display_col: usize = 0;
        let mut line_display_width: usize = 0;

        for (i, ch) in chars.iter().enumerate() {
            let ch_width = char_display_width(*ch, self.tab_width) as usize;
            if i < pos.col {
                display_col += ch_width;
            }
            line_display_width += ch_width;
        }

        let is_at_line_end = pos.col >= chars.len();
        (display_col, is_at_line_end, line_display_width)
    }

    /// The wrap width used for line wrapping — identical to what the renderer
    /// and `cursor_wrapped_position` use (content area minus gutter/padding).
    pub(super) fn wrap_content_width(&self) -> usize {
        let content_x_offset = self.content_x_offset() as usize;
        self.view_width.saturating_sub(content_x_offset).saturating_sub(self.right_padding as usize)
    }

    /// Inverse of `cursor_wrapped_position`: given a target visual line within a
    /// row and a target display column, return the char column whose cell covers
    /// (or is nearest to) that display column. Mirrors the renderer's wrap walk,
    /// including the leading-space skip on continuation lines, so vertical
    /// navigation stays display-width-correct for wide chars and tabs.
    pub(super) fn col_at_visual_pos(&self, row: usize, target_visual_line: usize, target_x: usize, content_width: usize) -> usize {
        let line = self.buffer.line(row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() || content_width == 0 {
            return 0;
        }

        let mut col = 0;
        let mut visual_line = 0;
        let mut is_wrapped_continuation = false;

        while col < chars.len() {
            if is_wrapped_continuation && chars[col] == ' ' {
                col += 1;
                if col >= chars.len() {
                    break;
                }
            }

            if visual_line == target_visual_line {
                let mut x = 0;
                while col < chars.len() && x < content_width {
                    let w = char_display_width(chars[col], self.tab_width) as usize;
                    if x + w > target_x {
                        return col;
                    }
                    x += w;
                    col += 1;
                }
                if col < chars.len() {
                    return col.saturating_sub(1);
                }
                return col;
            }

            let mut x = 0;
            while col < chars.len() && x < content_width {
                x += char_display_width(chars[col], self.tab_width) as usize;
                col += 1;
            }
            is_wrapped_continuation = true;
            visual_line += 1;
        }

        chars.len()
    }

    /// Returns the cursor's screen position accounting for line wrapping.
    pub fn cursor_wrapped_position(&self) -> (usize, usize) {
        if !self.line_wrap_enabled {
            return (0, self.cursor_display_col());
        }
        let content_x_offset = self.content_x_offset() as usize;
        let content_width = self.view_width.saturating_sub(content_x_offset).saturating_sub(self.right_padding as usize);

        if content_width == 0 {
            return (0, 0);
        }

        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return (0, 0);
        }

        let mut col = 0;
        let mut visual_line: usize = 0;
        let mut is_wrapped_continuation = false;

        while col < chars.len() {
            let mut x: usize = 0;
            if is_wrapped_continuation && col < chars.len() && chars[col] == ' ' {
                let is_cursor_on_space = col == pos.col;
                if !is_cursor_on_space {
                    col += 1;
                    if col >= chars.len() {
                        if pos.col >= chars.len() {
                            return (visual_line, 0);
                        }
                        visual_line += 1;
                        continue;
                    }
                }
            }

            while col < chars.len() && x < content_width {
                let ch = chars[col];
                let ch_width = char_display_width(ch, self.tab_width) as usize;

                if col == pos.col {
                    return (visual_line, x);
                }

                x += ch_width;
                col += 1;
            }

            if pos.col >= chars.len() && col == chars.len() {
                return (visual_line, x);
            }

            is_wrapped_continuation = true;
            visual_line += 1;
        }

        (visual_line.saturating_sub(1), 0)
    }
    pub fn line_wrapped_height(&self, row: usize) -> usize {
        let content_x_offset = self.content_x_offset() as usize;
        let content_width = self.view_width.saturating_sub(content_x_offset).saturating_sub(self.right_padding as usize);

        if content_width == 0 {
            return 1;
        }

        let line = self.buffer.line(row).unwrap_or("");
        if line.is_empty() {
            return 1;
        }

        let mut display_width: usize = 0;
        for ch in line.chars() {
            display_width += char_display_width(ch, self.tab_width) as usize;
        }
        display_width.div_ceil(content_width).max(1)
    }

    /// Center the cursor line on screen (zz command)
    pub fn center_cursor(&mut self) {
        let (cursor_row, _) = self.cursor();
        let half_height = self.view_height / 2;
        self.scroll_offset = cursor_row.saturating_sub(half_height);
    }

    /// Scroll so cursor line is at top of screen (zt command)
    pub fn scroll_cursor_to_top(&mut self) {
        let (cursor_row, _) = self.cursor();
        self.scroll_offset = cursor_row;
    }

    /// Scroll so cursor line is at bottom of screen (zb command)
    pub fn scroll_cursor_to_bottom(&mut self) {
        let (cursor_row, _) = self.cursor();
        self.scroll_offset = cursor_row.saturating_sub(self.view_height.saturating_sub(1));
    }

    pub fn set_view_size(&mut self, width: usize, height: usize) {
        self.view_width = width;
        self.view_height = height;
    }

    pub fn get_overflow_info(&self) -> (bool, bool) {
        let (cursor_row, _) = self.cursor();
        let line_len = self.buffer.line_len(cursor_row);
        (self.h_scroll_offset > 0, line_len > self.h_scroll_offset + self.view_width)
    }

    pub fn content_x_offset(&self) -> u16 {
        let gutter_width = if self.line_number_mode != LineNumberMode::None {
            self.line_number_width
        } else {
            0
        };
        self.left_padding + gutter_width
    }

    pub fn visual_to_logical_coords(&self, visual_y: usize, visual_x: usize) -> (usize, usize) {
        if !self.line_wrap_enabled || self.view_width == 0 {
            let row = visual_y + self.scroll_offset;
            let col = visual_x + self.h_scroll_offset;
            return (row, col);
        }

        let content_x_offset = self.content_x_offset() as usize;
        let content_width = self.view_width.saturating_sub(content_x_offset).saturating_sub(self.right_padding as usize);
        if content_width == 0 {
            return (self.scroll_offset, 0);
        }

        let line_count = self.buffer.line_count();
        let mut visual_lines_consumed = 0;
        let mut row = self.scroll_offset;

        while row < line_count {
            let line = self.buffer.line(row).unwrap_or("");
            let chars: Vec<char> = line.chars().collect();

            if chars.is_empty() {
                if visual_lines_consumed == visual_y {
                    return (row, 0);
                }
                visual_lines_consumed += 1;
                row += 1;
                continue;
            }

            let mut col_idx = 0;
            let mut visual_line_of_row = 0;

            while col_idx < chars.len() {
                let mut visual_line_start = col_idx;
                let mut x: usize = 0;
                let is_wrapped_continuation = visual_line_of_row > 0;
                if is_wrapped_continuation && col_idx < chars.len() && chars[col_idx] == ' ' {
                    col_idx += 1;
                    visual_line_start = col_idx;
                    if col_idx >= chars.len() {
                        if visual_lines_consumed + visual_line_of_row == visual_y {
                            return (row, col_idx);
                        }
                        visual_line_of_row += 1;
                        continue;
                    }
                }

                while col_idx < chars.len() && x < content_width {
                    let ch = chars[col_idx];
                    let ch_width = char_display_width(ch, self.tab_width) as usize;
                    x += ch_width;
                    col_idx += 1;
                }

                if visual_lines_consumed + visual_line_of_row == visual_y {
                    let mut target_x: usize = 0;
                    for i in visual_line_start..col_idx {
                        let ch = chars[i];
                        let ch_width = char_display_width(ch, self.tab_width) as usize;
                        if target_x + ch_width > visual_x {
                            return (row, i);
                        }
                        target_x += ch_width;
                    }
                    return (row, col_idx);
                }

                visual_line_of_row += 1;
            }

            visual_lines_consumed += visual_line_of_row.max(1);
            row += 1;
        }

        if line_count > 0 {
            let last_row = line_count - 1;
            let last_col = self.buffer.line_len(last_row);
            (last_row, last_col)
        } else {
            (0, 0)
        }
    }
}
