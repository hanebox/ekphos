use super::*;

impl Editor {
    pub fn new(lines: Vec<String>) -> Self {
        Self::new_with_clipboard(lines, Arc::new(MemoryClipboard::default()))
    }

    pub fn new_with_clipboard(lines: Vec<String>, clipboard_port: Arc<dyn Clipboard>) -> Self {
        Self {
            buffer: TextBuffer::from_lines(lines),
            cursor: Cursor::new(),
            history: History::new(),
            wrap_cache: WrapCache::new(),
            scroll_offset: 0,
            h_scroll_offset: 0,
            view_height: 0,
            view_width: 0,
            preferred_visual_x: None,
            line_wrap_enabled: true,
            tab_width: 4,
            left_padding: 0,
            right_padding: 1,
            block: None,
            cursor_line_style: Style::default(),
            selection_style: Style::default().bg(ratatui::style::Color::DarkGray),
            clipboard: None,
            clipboard_linewise: false,
            clipboard_port,
            highlight_index: HighlightIndex::new(),
            row_style_cache: RefCell::new(RowStyleCache::new()),
            code_block_rows: HashSet::new(),
            frontmatter_end: None,
            wiki_link_ranges: Vec::new(),
            wiki_link_valid_style: Style::default().fg(Color::Cyan),
            wiki_link_invalid_style: Style::default().fg(Color::Red),
            visual_line_selection: None,
            visual_block_selection: None,
            inclusive_selection: false,
            heading_colors: [Color::Blue, Color::Green, Color::Yellow, Color::Magenta, Color::Cyan, Color::Gray],
            code_color: Color::Green,
            link_color: Color::Cyan,
            blockquote_color: Color::Cyan,
            list_marker_color: Color::Yellow,
            bold_color: None,
            italic_color: None,
            frontmatter_color: Color::DarkGray,
            line_number_mode: LineNumberMode::Absolute,
            line_number_style: Style::default().fg(Color::DarkGray),
            line_number_width: 4, // Default width for line numbers
            scrolloff: 0,
            cursor_shape: CursorShape::Block,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.buffer.retained_bytes()
            + self.history.retained_bytes()
            + self.clipboard.as_ref().map_or(0, String::capacity)
            + self.highlight_index.retained_bytes
            + self.row_style_cache.borrow().retained_bytes
            + self.wiki_link_ranges.capacity() * std::mem::size_of::<WikiLinkRange>()
            + self.code_block_rows.capacity() * std::mem::size_of::<usize>()
    }

    pub fn history_stats(&self) -> HistoryStats {
        self.history.stats()
    }

    pub fn set_history_limits(&mut self, max_entries: usize, max_payload_bytes: usize) {
        self.history.set_limits(max_entries, max_payload_bytes);
    }

    pub fn set_line_number_mode(&mut self, mode: LineNumberMode) {
        self.line_number_mode = mode;
        // Update width based on line count
        self.update_line_number_width();
    }

    pub(super) fn update_line_number_width(&mut self) {
        if self.line_number_mode == LineNumberMode::None {
            self.line_number_width = 0;
        } else {
            let line_count = self.buffer.line_count();
            self.line_number_width = (line_count.to_string().len() as u16).max(2) + 1;
            // +1 for spacing
        }
    }

    pub(super) fn get_line_number_str(&self, row: usize, cursor_row: usize) -> Option<String> {
        match self.line_number_mode {
            LineNumberMode::None => None,
            LineNumberMode::Absolute => Some(format!("{:>width$}", row + 1, width = (self.line_number_width - 1) as usize)),
            LineNumberMode::Relative => {
                let rel = (row as isize - cursor_row as isize).unsigned_abs();
                Some(format!("{:>width$}", rel, width = (self.line_number_width - 1) as usize))
            }
            LineNumberMode::Hybrid => {
                if row == cursor_row {
                    Some(format!("{:>width$}", row + 1, width = (self.line_number_width - 1) as usize))
                } else {
                    let rel = (row as isize - cursor_row as isize).unsigned_abs();
                    Some(format!("{:>width$}", rel, width = (self.line_number_width - 1) as usize))
                }
            }
        }
    }

    pub fn from_text(text: &str) -> Self {
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

    pub fn set_tab_width(&mut self, width: u16) {
        self.tab_width = width.max(1);
    }

    pub fn set_padding(&mut self, left: u16, right: u16) {
        self.left_padding = left;
        self.right_padding = right;
    }

    pub fn set_scrolloff(&mut self, scrolloff: usize) {
        self.scrolloff = scrolloff;
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

    pub fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.cursor_shape = shape;
    }

    pub fn set_visual_line_selection(&mut self, anchor_row: usize, current_row: usize) {
        self.visual_line_selection = Some((anchor_row, current_row));
    }

    pub fn clear_visual_line_selection(&mut self) {
        self.visual_line_selection = None;
    }

    pub fn set_visual_block_selection(&mut self, anchor: Position, current: Position) {
        self.visual_block_selection = Some((anchor, current));
    }

    pub fn clear_visual_block_selection(&mut self) {
        self.visual_block_selection = None;
    }

    pub fn visual_line_selected_text(&self) -> Option<String> {
        let (anchor_row, current_row) = self.visual_line_selection?;
        let (start_row, end_row) = if anchor_row <= current_row {
            (anchor_row, current_row)
        } else {
            (current_row, anchor_row)
        };

        let mut result = String::new();
        for row in start_row..=end_row {
            if let Some(line) = self.buffer.line(row) {
                result.push_str(line);
                result.push('\n');
            }
        }
        Some(result)
    }

    pub fn copy_visual_lines(&mut self) {
        if let Some(text) = self.visual_line_selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = true;
            let _ = self.clipboard_port.set_text(&text);
        }
    }

    pub fn cut_visual_lines(&mut self) {
        if let Some((anchor_row, current_row)) = self.visual_line_selection {
            let (start_row, end_row) = if anchor_row <= current_row {
                (anchor_row, current_row)
            } else {
                (current_row, anchor_row)
            };

            // Collect lines for undo and clipboard
            let mut deleted_lines: Vec<String> = Vec::new();
            for row in start_row..=end_row {
                if let Some(line) = self.buffer.line(row) {
                    deleted_lines.push(line.to_string());
                }
            }

            // Set clipboard (with newlines for vim compatibility)
            let clipboard_text = deleted_lines.join("\n") + "\n";
            self.clipboard = Some(clipboard_text.clone());
            self.clipboard_linewise = true;
            let _ = self.clipboard_port.set_text(&clipboard_text);

            let cursor_before = self.cursor.pos();

            // Delete lines from end to start to preserve row indices
            for row in (start_row..=end_row).rev() {
                self.buffer.delete_line(row);
                self.wrap_cache.remove_line(row);
            }

            // Move cursor to start of deleted region
            let new_row = start_row.min(self.buffer.line_count().saturating_sub(1));
            self.cursor.move_to(new_row, 0);
            self.cursor.cancel_selection();

            self.history.record(
                EditOperation::LineDelete {
                    row: start_row,
                    lines: deleted_lines,
                },
                cursor_before,
                Position { row: new_row, col: 0 },
            );

            self.ensure_cursor_visible();
        }
    }

    pub fn visual_block_selected_text(&self) -> Option<String> {
        let (anchor, current) = self.visual_block_selection?;
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

        let mut result = Vec::new();
        for row in start_row..=end_row {
            if let Some(line) = self.buffer.line(row) {
                let line_len = line.chars().count();
                // Extract only the columns within the block
                let actual_start = start_col.min(line_len);
                let actual_end = (end_col + 1).min(line_len);
                if actual_start < actual_end {
                    result.push(buffer::char_slice(line, actual_start, actual_end).to_owned());
                } else {
                    result.push(String::new());
                }
            }
        }
        Some(result.join("\n"))
    }

    pub fn copy_visual_block(&mut self) {
        if let Some(text) = self.visual_block_selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = false;
            let _ = self.clipboard_port.set_text(&text);
        }
    }

    pub fn cut_visual_block(&mut self) {
        if let Some((anchor, current)) = self.visual_block_selection {
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

            // Collect deleted text for each line (for undo)
            let mut deleted_lines = Vec::new();
            for row in start_row..=end_row {
                if let Some(line) = self.buffer.line(row) {
                    let line_len = line.chars().count();
                    let actual_start = start_col.min(line_len);
                    let actual_end = (end_col + 1).min(line_len);
                    if actual_start < actual_end {
                        deleted_lines.push(buffer::char_slice(line, actual_start, actual_end).to_owned());
                    } else {
                        deleted_lines.push(String::new());
                    }
                }
            }

            // Get the text for clipboard (newline-separated)
            let clipboard_text = deleted_lines.join("\n");
            self.clipboard = Some(clipboard_text.clone());
            self.clipboard_linewise = false;
            let _ = self.clipboard_port.set_text(&clipboard_text);

            let cursor_before = self.cursor.pos();

            // Delete block from each line (process from end to preserve indices)
            for row in (start_row..=end_row).rev() {
                if let Some(line) = self.buffer.line(row) {
                    let line_len = line.chars().count();
                    let actual_start = start_col.min(line_len);
                    let actual_end = (end_col + 1).min(line_len);
                    if actual_start < actual_end {
                        self.buffer.delete_range(row, actual_start, actual_end);
                    }
                }
            }

            // Invalidate wrap cache
            self.wrap_cache.invalidate_from(start_row);

            // Move cursor to start of deleted region
            self.cursor.move_to(start_row, start_col);
            self.cursor.cancel_selection();

            // Record in history using BlockDelete for proper undo
            self.history.record(
                EditOperation::BlockDelete {
                    start_row,
                    end_row,
                    start_col,
                    end_col,
                    deleted_lines,
                },
                cursor_before,
                Position {
                    row: start_row,
                    col: start_col,
                },
            );

            self.ensure_cursor_visible();
        }
    }
}
