use super::*;

impl App {
    pub(super) fn content_cursor_for_source_line(&self, source_line: usize) -> usize {
        let mut best_idx = 0;
        for (idx, line) in self.document.content_items.iter().map(ContentItem::source_line).enumerate() {
            if line <= source_line {
                best_idx = idx;
            } else {
                break;
            }
        }
        best_idx
    }

    pub fn enter_edit_mode(&mut self) {
        if let Some(ref worker) = self.workers.highlight {
            worker.cancel();
        }
        self.editor.highlight_pending = false;
        self.editor.highlight_requested_rows = None;
        let content_start_line = self.current_note().map_or(0, |note| note.content_start_line);
        if let Some(document) = self.document.active_document.take() {
            self.editor.edit_preview_position = Some((self.document.content_cursor, self.document.content_items.len()));
            let target_row = self.document.content_items.get(self.document.content_cursor).map(ContentItem::source_line).unwrap_or(0).min(document.line_count().saturating_sub(1));
            let lines: Vec<String> = (0..document.line_count()).filter_map(|line| document.line(line).map(str::to_owned)).collect();
            let line_count = lines.len();
            self.document.content_items.clear();
            self.document.content_items.shrink_to_fit();
            self.document.document_tables.clear();
            self.document.document_tables.shrink_to_fit();
            self.document.document_links.clear();
            self.document.document_links.shrink_to_fit();
            self.document.document_link_ranges.clear();
            self.document.document_link_ranges.shrink_to_fit();
            self.document.content_render_scratch = ContentRenderScratch::default();
            self.document.outline.clear();
            self.document.outline.shrink_to_fit();
            self.state.content_item_rects.clear();
            self.state.inline_image_rects.clear();
            self.evict_document_services();
            drop(document);
            self.editor.replace(Editor::new_with_clipboard(lines, Arc::clone(&self.dependencies.clipboard)));
            self.editor.set_line_wrap(self.state.config.editor.line_wrap);
            self.editor.set_tab_width(self.state.config.editor.tab_width);
            self.editor.set_padding(self.state.config.editor.left_padding, self.state.config.editor.right_padding);
            self.editor.set_line_number_mode(self.state.config.editor.line_numbers);
            self.editor.set_scrolloff(self.state.config.editor.scrolloff as usize);
            self.editor.vim.mode = VimMode::Normal;
            self.editor.vim.reset_pending();
            self.editor.vim.command_buffer.clear();
            self.editor.set_wiki_link_styles(ratatui::style::Style::default().fg(self.state.theme.info), ratatui::style::Style::default().fg(self.state.theme.error));
            self.editor.set_markdown_colors(ekphos_editor::MarkdownColors {
                headings: [self.state.theme.editor.heading1, self.state.theme.editor.heading2, self.state.theme.editor.heading3, self.state.theme.editor.heading4, self.state.theme.editor.heading5, self.state.theme.editor.heading6],
                code: self.state.theme.editor.code,
                link: self.state.theme.editor.link,
                blockquote: self.state.theme.editor.blockquote,
                list_marker: self.state.theme.editor.list_marker,
                bold: Some(self.state.theme.editor.bold),
                italic: Some(self.state.theme.editor.italic),
            });
            self.editor.set_frontmatter_color(self.state.theme.content.frontmatter);
            self.editor.set_cursor(target_row, 0);
            for source_line in 0..self.editor.line_count() {
                let Some(line) = self.editor.line(source_line) else {
                    continue;
                };
                let Some(heading) = ekphos_core::markdown::heading(line).filter(|heading| heading.level <= 3 && line[heading.level..].starts_with(' ')) else {
                    continue;
                };
                self.document.outline.push(OutlineItem { level: heading.level as u8, source_line: source_line as u32, line: source_line });
            }
            if !self.document.outline.is_empty() {
                self.document.outline_state.select(Some(0));
            }
            let view_height = self.editor.editor_view_height.max(10);
            let editor_scroll = if self.document.frontmatter_hidden && content_start_line > 0 && self.document.content_scroll_offset <= 1 {
                if target_row < view_height {
                    0
                } else {
                    target_row.saturating_sub(view_height / 2)
                }
            } else {
                let preview_scroll_top = self.document.content_scroll_offset.saturating_sub(1);
                let cursor_offset_from_top = self.document.content_cursor.saturating_sub(preview_scroll_top);
                target_row.saturating_sub(cursor_offset_from_top)
            };
            self.editor.set_scroll_offset(editor_scroll.min(line_count.saturating_sub(1)));
            self.editor.editor_scroll_top = self.editor.scroll_offset();
            self.update_editor_block();
            self.editor.mode = Mode::Edit;
            self.state.focus = Focus::Content;
            self.request_highlight_update();
            self.request_memory_reclaim();
        }
    }

    pub fn update_editor_highlights(&mut self) {
        self.request_highlight_update();
    }

    pub fn update_editor_highlights_incremental(&mut self) {
        self.request_highlight_update();
    }

    pub fn update_editor_scroll(&mut self, view_height: usize) {
        self.editor.editor_view_height = view_height;
        self.editor.update_scroll(view_height);
        self.editor.editor_scroll_top = self.editor.scroll_offset();
        let rows = self.highlight_row_window();
        if self.editor.mode == Mode::Edit && self.editor.highlight_requested_rows != Some((rows.start, rows.end)) {
            self.request_highlight_update();
        }
    }

    pub fn update_editor_block(&mut self) {
        let is_command_mode = self.editor.vim.mode.is_command();
        let mode_str = if is_command_mode {
            "COMMAND"
        } else if let Some(ref block_state) = self.editor.block_insert_state {
            match block_state.mode {
                BlockInsertMode::Insert => "V-BLK INSERT",
                BlockInsertMode::Append => "V-BLK APPEND",
            }
        } else {
            self.editor.vim.mode.display_name()
        };
        let pending_str = match (&self.editor.pending_delete, self.editor.pending_operator) {
            (Some(_), _) => " [DEL]",
            (None, Some('d')) => " d-",
            _ => "",
        };
        let color = if is_command_mode {
            self.state.theme.info
        } else if self.editor.block_insert_state.is_some() {
            self.state.theme.secondary // Use secondary color for block insert mode
        } else {
            match (&self.editor.pending_delete, self.editor.vim.mode) {
                (Some(_), _) => self.state.theme.error,
                (None, VimMode::Normal) if self.editor.pending_operator.is_some() => self.state.theme.warning,
                (None, VimMode::Normal) => self.state.theme.primary,
                (None, VimMode::Insert) => self.state.theme.success,
                (None, VimMode::Replace) => self.state.theme.warning,
                (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => self.state.theme.secondary,
                (None, _) => self.state.theme.info,
            }
        };
        let hint = if is_command_mode {
            "Enter: Execute, Esc: Cancel"
        } else if self.editor.block_insert_state.is_some() {
            "Type text, Esc: Apply to all lines"
        } else {
            match (&self.editor.pending_delete, self.editor.vim.mode) {
                (Some(_), _) => "d: Confirm, Esc: Cancel",
                (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => "y: Yank, d: Delete, Esc: Cancel",
                (None, _) if self.editor.pending_operator == Some('d') => "d: Line, w: Word→, b: Word←",
                _ => "Ctrl+S: Save, Esc: Exit",
            }
        };
        if self.state.zen_mode {
            self.editor.set_block(Block::default());
        } else {
            self.editor.set_block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(color)).title(format!(" {}{} | {} ", mode_str, pending_str, hint)));
        }
        self.editor.set_selection_style(Style::default().fg(self.state.theme.foreground).bg(self.state.theme.selection));
        self.editor.set_cursor_line_style(Style::default());
    }

    pub fn save_edit(&mut self) {
        self.end_buffer_search();
        self.editor.vim.reset_pending();
        self.editor.vim.command_buffer.clear();
        self.editor.vim.mode = VimMode::Normal;
        self.editor.highlight_pending = false;
        self.editor.highlight_requested_rows = None;
        if let Some(ref worker) = self.workers.highlight {
            worker.cancel();
        }
        let (cursor_row, _) = self.editor.cursor();
        let editor_scroll = self.editor.scroll_offset();
        let cursor_offset_from_top = cursor_row.saturating_sub(editor_scroll);
        let content = self.editor.text();
        if !self.persist_active_body(content) {
            return;
        }
        self.sort_tree();
        self.rebuild_sidebar_items();
        self.select_current_note_in_sidebar();
        self.editor.mode = Mode::Normal;
        self.editor.edit_preview_position = None;
        self.editor.replace(Editor::new_with_clipboard(vec![String::new()], Arc::clone(&self.dependencies.clipboard)));
        self.update_content_items();
        self.document.content_cursor = self.content_cursor_for_source_line(cursor_row);
        let preview_scroll = self.document.content_cursor.saturating_sub(cursor_offset_from_top);
        self.document.content_scroll_offset = preview_scroll + 1;
        self.request_memory_reclaim();
    }

    pub fn cancel_edit(&mut self) {
        self.end_buffer_search();
        self.editor.vim.reset_pending();
        self.editor.vim.command_buffer.clear();
        self.editor.vim.mode = VimMode::Normal;
        self.editor.highlight_pending = false;
        self.editor.highlight_requested_rows = None;
        if let Some(ref worker) = self.workers.highlight {
            worker.cancel();
        }
        let (cursor_row, _) = self.editor.cursor();
        let editor_scroll = self.editor.scroll_offset();
        let cursor_offset_from_top = cursor_row.saturating_sub(editor_scroll);
        self.editor.mode = Mode::Normal;
        self.editor.edit_preview_position = None;
        self.editor.replace(Editor::new_with_clipboard(vec![String::new()], Arc::clone(&self.dependencies.clipboard)));
        if !self.load_selected_note_body() {
            return;
        }
        self.update_content_items();
        self.document.content_cursor = self.content_cursor_for_source_line(cursor_row);
        let preview_scroll = self.document.content_cursor.saturating_sub(cursor_offset_from_top);
        self.document.content_scroll_offset = preview_scroll + 1;
        self.request_memory_reclaim();
    }

    pub fn has_unsaved_changes(&self) -> bool {
        if let Some(body) = self.current_note().and_then(|note| note.file_path.as_ref()).and_then(|path| std::fs::read_to_string(path).ok()) {
            let mut note_lines = body.lines();
            (0..self.editor.line_count()).any(|row| self.editor.line(row) != note_lines.next()) || note_lines.next().is_some()
        } else {
            false
        }
    }
}
