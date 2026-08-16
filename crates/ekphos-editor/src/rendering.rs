use super::*;

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
    /// Renders a cursor at the given position in the buffer
    fn render_cursor_at(&self, buf: &mut RatatuiBuffer, x: u16, y: u16, ch: char, base_style: Style) {
        if let Some(cell) = buf.cell_mut((x, y)) {
            match self.cursor_shape {
                CursorShape::Block => {
                    // Full reversed block for Normal mode
                    cell.set_char(ch);
                    cell.set_style(base_style.add_modifier(Modifier::REVERSED));
                }
                CursorShape::Bar => {
                    // For bar cursor, don't render custom cursor - use terminal's native cursor
                    // Just render the character normally, terminal cursor will be positioned here
                    cell.set_char(ch);
                    cell.set_style(base_style);
                }
                CursorShape::Underline => {
                    // Underline + Reversed for Replace mode - more visible than underline alone
                    cell.set_char(ch);
                    cell.set_style(base_style.add_modifier(Modifier::UNDERLINED | Modifier::REVERSED));
                }
            }
        }
    }

    /// Returns true if the cursor shape uses the terminal's native cursor (not rendered by editor)
    pub fn uses_native_cursor(&self) -> bool {
        matches!(self.cursor_shape, CursorShape::Bar)
    }

    fn render_wrapped(&self, area: Rect, buf: &mut RatatuiBuffer) {
        // Account for line number gutter
        let gutter_width = if self.line_number_mode != LineNumberMode::None {
            self.line_number_width
        } else {
            0
        };
        let content_start_x = area.x + self.left_padding + gutter_width;
        let content_end_x = area.x + area.width.saturating_sub(self.right_padding);
        let content_width = content_end_x.saturating_sub(content_start_x) as usize;
        if content_width == 0 {
            return;
        }

        let cursor_pos = self.cursor.pos();
        let selection = if let Some((anchor_row, current_row)) = self.visual_line_selection {
            let (start_row, end_row) = if anchor_row <= current_row {
                (anchor_row, current_row)
            } else {
                (current_row, anchor_row)
            };
            let end_line_len = self.buffer.line(end_row).map(|l| l.chars().count()).unwrap_or(0);
            Some((
                Position { row: start_row, col: 0 },
                Position {
                    row: end_row,
                    col: end_line_len + 1,
                },
            ))
        } else {
            self.effective_selection_range()
        };
        let block_selection = self.visual_block_selection;
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

            // Render line numbers if enabled (only for first visual line of a row)
            if let Some(ln_str) = self.get_line_number_str(row, cursor_pos.row) {
                let ln_style = if is_cursor_line {
                    self.line_number_style.add_modifier(Modifier::BOLD)
                } else {
                    self.line_number_style
                };
                for (i, ch) in ln_str.chars().enumerate() {
                    if let Some(cell) = buf.cell_mut((area.x + self.left_padding + i as u16, screen_y)) {
                        cell.set_char(ch);
                        cell.set_style(ln_style);
                    }
                }
            }

            if chars.is_empty() {
                if is_cursor_line {
                    self.render_cursor_at(buf, content_start_x, screen_y, ' ', Style::default());
                }
                screen_y += 1;
                continue;
            }

            // Get cached row styles once per row (O(1) per char instead of O(H) per char)
            let row_styles = self.get_row_styles_cached(row);

            // Render line with wrapping
            let mut col = 0;
            let mut is_wrapped_continuation = false;
            while col < chars.len() {
                if screen_y >= area.y + area.height {
                    return;
                }

                let mut x = content_start_x;

                if is_wrapped_continuation && col < chars.len() && chars[col] == ' ' {
                    let is_cursor_on_space = is_cursor_line && col == cursor_pos.col;
                    if !is_cursor_on_space {
                        col += 1;
                        if col >= chars.len() {
                            if is_cursor_line && cursor_pos.col >= chars.len() {
                                self.render_cursor_at(buf, x, screen_y, ' ', Style::default());
                            }
                            screen_y += 1;
                            break;
                        }
                    }
                }

                while col < chars.len() && x < content_end_x {
                    let ch = chars[col];
                    let base_style = self.get_char_style_fast(&row_styles, col, row, selection, block_selection);
                    let is_cursor = is_cursor_line && col == cursor_pos.col;

                    let ch_width = char_display_width(ch, self.tab_width);
                    if ch == '\t' {
                        for i in 0..ch_width {
                            if x >= content_end_x {
                                break;
                            }
                            if i == 0 && is_cursor {
                                self.render_cursor_at(buf, x, screen_y, ' ', base_style);
                            } else if let Some(cell) = buf.cell_mut((x, screen_y)) {
                                cell.set_char(' ');
                                cell.set_style(base_style);
                            }
                            x += 1;
                        }
                    } else {
                        if is_cursor {
                            self.render_cursor_at(buf, x, screen_y, ch, base_style);
                        } else if let Some(cell) = buf.cell_mut((x, screen_y)) {
                            cell.set_char(ch);
                            cell.set_style(base_style);
                        }
                        x += ch_width;
                    }
                    col += 1;
                }

                // Render cursor at end of line if cursor is past last char
                // Use full area width to allow cursor in right padding
                if is_cursor_line && cursor_pos.col >= chars.len() && col == chars.len() {
                    if x < area.x + area.width {
                        self.render_cursor_at(buf, x, screen_y, ' ', Style::default());
                    }
                }

                is_wrapped_continuation = true;
                screen_y += 1;
            }
        }

        if self.buffer.is_empty() {
            self.render_cursor_at(buf, content_start_x, area.y, ' ', Style::default());
        }
    }

    fn render_no_wrap(&self, area: Rect, buf: &mut RatatuiBuffer) {
        // Account for line number gutter
        let gutter_width = if self.line_number_mode != LineNumberMode::None {
            self.line_number_width
        } else {
            0
        };
        let content_start_x = area.x + self.left_padding + gutter_width;
        let content_end_x = area.x + area.width.saturating_sub(self.right_padding);

        let cursor_pos = self.cursor.pos();
        let selection = if let Some((anchor_row, current_row)) = self.visual_line_selection {
            let (start_row, end_row) = if anchor_row <= current_row {
                (anchor_row, current_row)
            } else {
                (current_row, anchor_row)
            };
            let end_line_len = self.buffer.line(end_row).map(|l| l.chars().count()).unwrap_or(0);
            Some((
                Position { row: start_row, col: 0 },
                Position {
                    row: end_row,
                    col: end_line_len + 1,
                },
            ))
        } else {
            self.effective_selection_range()
        };
        let block_selection = self.visual_block_selection;
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

            // Render line numbers if enabled
            if let Some(ln_str) = self.get_line_number_str(row, cursor_pos.row) {
                let ln_style = if is_cursor_line {
                    self.line_number_style.add_modifier(Modifier::BOLD)
                } else {
                    self.line_number_style
                };
                for (i, ch) in ln_str.chars().enumerate() {
                    if let Some(cell) = buf.cell_mut((area.x + self.left_padding + i as u16, y)) {
                        cell.set_char(ch);
                        cell.set_style(ln_style);
                    }
                }
            }

            let row_styles = self.get_row_styles_cached(row);

            let mut x = content_start_x;
            for col in line_h_scroll..chars.len() {
                if x >= content_end_x {
                    break;
                }

                let ch = chars[col];
                let base_style = self.get_char_style_fast(&row_styles, col, row, selection, block_selection);
                let is_cursor = is_cursor_line && col == cursor_pos.col;

                let ch_width = char_display_width(ch, self.tab_width);
                if ch == '\t' {
                    for i in 0..ch_width {
                        if x >= content_end_x {
                            break;
                        }
                        if i == 0 && is_cursor {
                            self.render_cursor_at(buf, x, y, ' ', base_style);
                        } else if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_char(' ');
                            cell.set_style(base_style);
                        }
                        x += 1;
                    }
                } else {
                    if is_cursor {
                        self.render_cursor_at(buf, x, y, ch, base_style);
                    } else if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(ch);
                        cell.set_style(base_style);
                    }
                    x += ch_width;
                }
            }

            if is_cursor_line && cursor_pos.col >= chars.len() {
                if x < area.x + area.width {
                    self.render_cursor_at(buf, x, y, ' ', Style::default());
                }
            }

            y += 1;
        }

        if self.buffer.line_count() <= self.scroll_offset {
            self.render_cursor_at(buf, content_start_x, area.y, ' ', Style::default());
        }
    }

    #[allow(dead_code)]
    fn get_char_style(&self, row: usize, col: usize, selection: Option<(Position, Position)>, block_selection: Option<(Position, Position)>) -> Style {
        let row_styles = self.get_row_styles_cached(row);
        let base_style = row_styles.get(col).copied().unwrap_or_default();

        self.apply_selection_style(base_style, row, col, selection, block_selection)
    }

    #[inline]
    fn apply_selection_style(
        &self,
        base_style: Style,
        row: usize,
        col: usize,
        selection: Option<(Position, Position)>,
        block_selection: Option<(Position, Position)>,
    ) -> Style {
        // Block selection takes priority (rectangular selection)
        if let Some((anchor, current)) = block_selection {
            let (start_row, end_row) = if anchor.row <= current.row {
                (anchor.row, current.row)
            } else {
                (current.row, anchor.row)
            };
            let (start_col, end_col) = if anchor.col <= current.col {
                (anchor.col, current.col)
            } else {
                (current.col, anchor.col)
            };

            let in_block = row >= start_row && row <= end_row && col >= start_col && col <= end_col;

            if in_block {
                return self.selection_style;
            }
        }

        // Character-wise selection
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

        base_style
    }

    fn get_char_style_fast(
        &self,
        row_styles: &[Style],
        col: usize,
        row: usize,
        selection: Option<(Position, Position)>,
        block_selection: Option<(Position, Position)>,
    ) -> Style {
        let base_style = row_styles.get(col).copied().unwrap_or_default();
        self.apply_selection_style(base_style, row, col, selection, block_selection)
    }
}
