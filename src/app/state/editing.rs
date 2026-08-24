use super::*;

impl App {
    pub(super) fn content_cursor_for_source_line(&self, source_line: usize) -> usize {
        let mut best_idx = 0;
        for (idx, line) in self.content_items.iter().map(ContentItem::source_line).enumerate() {
            if line <= source_line {
                best_idx = idx;
            } else {
                break;
            }
        }
        best_idx
    }

    pub fn enter_edit_mode(&mut self) {
        // Cancel old document work before starting a fresh editor revision.
        if let Some(ref worker) = self.highlight_worker {
            worker.cancel();
        }
        self.highlight_pending = false;
        self.highlight_requested_rows = None;

        let content_start_line = self.current_note().map_or(0, |note| note.content_start_line);
        if let Some(document) = self.active_document.take() {
            self.edit_preview_position = Some((self.content_cursor, self.content_items.len()));
            let target_row = self
                .content_items
                .get(self.content_cursor)
                .map(ContentItem::source_line)
                .unwrap_or(0)
                .min(document.line_count().saturating_sub(1));
            let lines: Vec<String> = (0..document.line_count()).filter_map(|line| document.line(line).map(str::to_owned)).collect();
            let line_count = lines.len();

            self.content_items.clear();
            self.content_items.shrink_to_fit();
            self.document_tables.clear();
            self.document_tables.shrink_to_fit();
            self.document_links.clear();
            self.document_links.shrink_to_fit();
            self.document_link_ranges.clear();
            self.document_link_ranges.shrink_to_fit();
            self.content_render_scratch = ContentRenderScratch::default();
            self.outline.clear();
            self.outline.shrink_to_fit();
            self.content_item_rects.clear();
            self.inline_image_rects.clear();
            self.evict_document_services();
            drop(document);

            self.editor = Editor::new_with_clipboard(lines, Arc::clone(&self.dependencies.clipboard));
            self.editor.set_line_wrap(self.config.editor.line_wrap);
            self.editor.set_tab_width(self.config.editor.tab_width);
            self.editor.set_padding(self.config.editor.left_padding, self.config.editor.right_padding);
            self.editor.set_line_number_mode(self.config.editor.line_numbers);
            self.editor.set_scrolloff(self.config.editor.scrolloff as usize);

            self.vim_mode = VimMode::Normal;
            self.vim.mode = ekphos_vim::VimMode::Normal;
            self.vim.reset_pending();
            self.vim.command_buffer.clear();

            // Set wiki link styles from theme
            self.editor.set_wiki_link_styles(
                ratatui::style::Style::default().fg(self.theme.info),
                ratatui::style::Style::default().fg(self.theme.error),
            );

            // Set markdown highlighting colors from theme
            self.editor.set_markdown_colors(
                [
                    self.theme.editor.heading1,
                    self.theme.editor.heading2,
                    self.theme.editor.heading3,
                    self.theme.editor.heading4,
                    self.theme.editor.heading5,
                    self.theme.editor.heading6,
                ],
                self.theme.editor.code,
                self.theme.editor.link,
                self.theme.editor.blockquote,
                self.theme.editor.list_marker,
                Some(self.theme.editor.bold),
                Some(self.theme.editor.italic),
            );
            self.editor.set_frontmatter_color(self.theme.content.frontmatter);

            self.editor.set_cursor(target_row, 0);
            for source_line in 0..self.editor.line_count() {
                let Some(line) = self.editor.line(source_line) else {
                    continue;
                };
                let Some(heading) = ekphos_core::markdown::heading(line).filter(|heading| heading.level <= 3 && line[heading.level..].starts_with(' ')) else {
                    continue;
                };
                self.outline.push(OutlineItem {
                    level: heading.level as u8,
                    source_line: source_line as u32,
                    line: source_line,
                });
            }
            if !self.outline.is_empty() {
                self.outline_state.select(Some(0));
            }

            // Calculate scroll position:
            // - If frontmatter was hidden and we're near the top, start from line 0
            //   to show frontmatter in edit mode (unless it would push cursor off screen)
            // - Otherwise, try to maintain similar viewport position
            let view_height = self.editor_view_height.max(10);
            // content_scroll_offset is 1-indexed, so <= 1 means at the top
            let editor_scroll = if self.frontmatter_hidden && content_start_line > 0 && self.content_scroll_offset <= 1 {
                // Frontmatter was hidden, user was at/near top of content
                // Start from line 0 unless cursor would be off screen
                if target_row < view_height {
                    0
                } else {
                    target_row.saturating_sub(view_height / 2)
                }
            } else {
                // Normal case: try to preserve relative cursor position
                let preview_scroll_top = self.content_scroll_offset.saturating_sub(1);
                let cursor_offset_from_top = self.content_cursor.saturating_sub(preview_scroll_top);
                target_row.saturating_sub(cursor_offset_from_top)
            };

            self.editor.set_scroll_offset(editor_scroll.min(line_count.saturating_sub(1)));
            self.editor_scroll_top = self.editor.scroll_offset();

            self.update_editor_block();
            self.mode = Mode::Edit;
            self.focus = Focus::Content;

            self.request_highlight_update();
        }
    }

    pub fn update_editor_highlights(&mut self) {
        self.request_highlight_update();
    }

    pub fn update_editor_highlights_incremental(&mut self) {
        self.request_highlight_update();
    }

    pub fn update_editor_scroll(&mut self, view_height: usize) {
        self.editor_view_height = view_height;
        self.editor.update_scroll(view_height);
        self.editor_scroll_top = self.editor.scroll_offset();
        let rows = self.highlight_row_window();
        if self.mode == Mode::Edit && self.highlight_requested_rows != Some((rows.start, rows.end)) {
            self.request_highlight_update();
        }
    }

    pub fn update_editor_block(&mut self) {
        // Check for command mode first (from new vim state)
        let is_command_mode = self.vim.mode.is_command();

        let mode_str = if is_command_mode {
            "COMMAND"
        } else if let Some(ref block_state) = self.block_insert_state {
            match block_state.mode {
                BlockInsertMode::Insert => "V-BLK INSERT",
                BlockInsertMode::Append => "V-BLK APPEND",
            }
        } else {
            match self.vim_mode {
                VimMode::Normal => "NORMAL",
                VimMode::Insert => "INSERT",
                VimMode::Replace => "REPLACE",
                VimMode::Visual => "VISUAL",
                VimMode::VisualLine => "V-LINE",
                VimMode::VisualBlock => "V-BLOCK",
            }
        };
        let pending_str = match (&self.pending_delete, self.pending_operator) {
            (Some(_), _) => " [DEL]",
            (None, Some('d')) => " d-",
            _ => "",
        };
        let color = if is_command_mode {
            self.theme.info
        } else if self.block_insert_state.is_some() {
            self.theme.secondary // Use secondary color for block insert mode
        } else {
            match (&self.pending_delete, self.vim_mode) {
                (Some(_), _) => self.theme.error,
                (None, VimMode::Normal) if self.pending_operator.is_some() => self.theme.warning,
                (None, VimMode::Normal) => self.theme.primary,
                (None, VimMode::Insert) => self.theme.success,
                (None, VimMode::Replace) => self.theme.warning,
                (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => self.theme.secondary,
            }
        };
        let hint = if is_command_mode {
            "Enter: Execute, Esc: Cancel"
        } else if self.block_insert_state.is_some() {
            "Type text, Esc: Apply to all lines"
        } else {
            match (&self.pending_delete, self.vim_mode) {
                (Some(_), _) => "d: Confirm, Esc: Cancel",
                (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => "y: Yank, d: Delete, Esc: Cancel",
                (None, _) if self.pending_operator == Some('d') => "d: Line, w: Word→, b: Word←",
                _ => "Ctrl+S: Save, Esc: Exit",
            }
        };
        if self.zen_mode {
            self.editor.set_block(Block::default());
        } else {
            self.editor.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color))
                    .title(format!(" {}{} | {} ", mode_str, pending_str, hint)),
            );
        }
        self.editor
            .set_selection_style(Style::default().fg(self.theme.foreground).bg(self.theme.selection));
        self.editor.set_cursor_line_style(Style::default());
    }

    pub fn save_edit(&mut self) {
        // Clear search state and vim state when exiting edit mode
        self.end_buffer_search();
        self.vim.reset_pending();
        self.vim.command_buffer.clear();
        self.vim.mode = ekphos_vim::VimMode::Normal;
        self.vim_mode = VimMode::Normal;
        self.highlight_pending = false;
        self.highlight_requested_rows = None;
        if let Some(ref worker) = self.highlight_worker {
            worker.cancel();
        }

        let (cursor_row, _) = self.editor.cursor();
        let editor_scroll = self.editor.scroll_offset();

        let cursor_offset_from_top = cursor_row.saturating_sub(editor_scroll);

        let content = self.editor.text();
        if !self.persist_active_body(content) {
            return;
        }

        // Re-sort and rebuild sidebar to reflect updated modified time
        self.sort_tree();
        self.rebuild_sidebar_items();
        // Re-select the current note in the sidebar after re-sorting
        self.select_current_note_in_sidebar();

        self.mode = Mode::Normal;
        self.edit_preview_position = None;
        self.editor = Editor::new_with_clipboard(vec![String::new()], Arc::clone(&self.dependencies.clipboard));
        self.update_content_items();

        // Map editor row to content_cursor using source line mapping
        self.content_cursor = self.content_cursor_for_source_line(cursor_row);
        let preview_scroll = self.content_cursor.saturating_sub(cursor_offset_from_top);
        self.content_scroll_offset = preview_scroll + 1;
    }

    pub fn cancel_edit(&mut self) {
        self.end_buffer_search();
        self.vim.reset_pending();
        self.vim.command_buffer.clear();
        self.vim.mode = ekphos_vim::VimMode::Normal;
        self.vim_mode = VimMode::Normal;
        self.highlight_pending = false;
        self.highlight_requested_rows = None;
        if let Some(ref worker) = self.highlight_worker {
            worker.cancel();
        }

        let (cursor_row, _) = self.editor.cursor();
        let editor_scroll = self.editor.scroll_offset();

        let cursor_offset_from_top = cursor_row.saturating_sub(editor_scroll);
        self.mode = Mode::Normal;
        self.edit_preview_position = None;

        self.editor = Editor::new_with_clipboard(vec![String::new()], Arc::clone(&self.dependencies.clipboard));
        if !self.load_selected_note_body() {
            return;
        }
        self.update_content_items();

        self.content_cursor = self.content_cursor_for_source_line(cursor_row);
        let preview_scroll = self.content_cursor.saturating_sub(cursor_offset_from_top);
        self.content_scroll_offset = preview_scroll + 1;
    }

    pub fn has_unsaved_changes(&self) -> bool {
        if let Some(body) = self
            .current_note()
            .and_then(|note| note.file_path.as_ref())
            .and_then(|path| std::fs::read_to_string(path).ok())
        {
            // Compare line-by-line with the same semantics `enter_edit_mode` uses
            // (`str::lines()` drops trailing newlines). Comparing the raw strings instead
            // fires a false positive whenever the file ends with "\n" — which is most files.
            let mut note_lines = body.lines();
            (0..self.editor.line_count()).any(|row| self.editor.line(row) != note_lines.next()) || note_lines.next().is_some()
        } else {
            false
        }
    }
}
