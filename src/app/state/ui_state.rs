use super::*;

impl App {
    pub(crate) fn clipboard(&self) -> &dyn crate::clipboard::Clipboard {
        self.dependencies.clipboard.as_ref()
    }

    pub fn config_path(&self) -> PathBuf {
        Config::config_path_in(&self.dependencies.config_dir)
    }

    pub fn next_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = (self.selected_sidebar_index + 1) % self.sidebar_items.len();
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn previous_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = if self.selected_sidebar_index == 0 {
            self.sidebar_items.len() - 1
        } else {
            self.selected_sidebar_index - 1
        };
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn goto_first_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = 0;
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn goto_last_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = self.sidebar_items.len() - 1;
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn handle_sidebar_enter(&mut self) {
        let item_info = self.sidebar_items.get(self.selected_sidebar_index).map(|item| match &item.kind {
            SidebarItemKind::Folder(folder) => (true, folder.path.clone(), None),
            SidebarItemKind::Note { note_id } => (false, PathBuf::new(), self.note_index_for_id(*note_id)),
        });

        if let Some((is_folder, path, note_index)) = item_info {
            if is_folder {
                self.toggle_folder(path);
            } else if let Some(note_index) = note_index {
                self.toggle_focus(false);
                self.push_navigation_history(note_index);
            }
        }
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        let new_state = !self.folder_states.get(&path).copied().unwrap_or(false);
        self.folder_states.insert(path.clone(), new_state);

        Self::update_folder_in_tree(&mut self.file_tree, &path, new_state);

        self.rebuild_sidebar_items();

        if self.selected_sidebar_index >= self.sidebar_items.len() {
            self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
        }

        self.sync_selected_note_from_sidebar();
    }

    pub(super) fn update_folder_in_tree(items: &mut [FileTreeItem], target_path: &PathBuf, new_state: bool) {
        for item in items {
            if let FileTreeItem::Folder(folder) = item {
                if &folder.path == target_path {
                    folder.expanded = new_state;
                    return;
                }
                Self::update_folder_in_tree(&mut folder.children, target_path, new_state);
            }
        }
    }

    pub fn toggle_focus(&mut self, backwards: bool) {
        self.focus = match self.focus {
            Focus::Sidebar => {
                if backwards {
                    Focus::Outline
                } else {
                    Focus::Content
                }
            }
            Focus::Content => {
                if backwards {
                    Focus::Sidebar
                } else {
                    Focus::Outline
                }
            }
            Focus::Outline => {
                if backwards {
                    Focus::Content
                } else {
                    Focus::Sidebar
                }
            }
        };
    }

    pub fn toggle_sidebar_collapsed(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    pub fn toggle_outline_collapsed(&mut self) {
        self.outline_collapsed = !self.outline_collapsed;
    }

    pub fn is_sidebar_minimized(&self) -> bool {
        self.sidebar_collapsed || Config::panel_width_is_minimized(self.config.sidebar_width_percent)
    }

    pub fn is_outline_minimized(&self) -> bool {
        self.outline_collapsed || Config::panel_width_is_minimized(self.config.outline_width_percent)
    }

    pub fn resize_focused_panel(&mut self, delta: i64) {
        if self.zen_mode {
            self.status_message = Some("Panel resizing is unavailable in zen mode".to_string());
            return;
        }

        let (panel_name, collapsed, current_width) = match self.focus {
            Focus::Sidebar => ("Sidebar", self.sidebar_collapsed, self.config.sidebar_width_percent),
            Focus::Outline => ("Outline", self.outline_collapsed, self.config.outline_width_percent),
            Focus::Content => {
                self.status_message = Some("Focus the sidebar or outline to resize".to_string());
                return;
            }
        };

        if collapsed {
            self.status_message = Some(format!("{} is collapsed", panel_name));
            return;
        }

        let effective_width = i64::from(Config::effective_panel_width_percent(current_width));
        let resized_width = Config::resized_panel_width_percent(current_width, delta);
        if resized_width == effective_width {
            let boundary = if delta < 0 { "minimum" } else { "maximum" };
            self.status_message = Some(format!("{} width is already at the {} ({}%)", panel_name, boundary, effective_width));
            return;
        }

        match self.focus {
            Focus::Sidebar => self.config.sidebar_width_percent = resized_width,
            Focus::Outline => self.config.outline_width_percent = resized_width,
            Focus::Content => unreachable!(),
        }
        let minimized = if Config::panel_width_is_minimized(resized_width) {
            " (minimized)"
        } else {
            ""
        };
        self.status_message = Some(format!("{} width: {}%{}", panel_name, resized_width, minimized));

        if let Err(error) = self.config.save_to_dir(&self.dependencies.config_dir) {
            self.show_error_toast(format!("Could not save panel width: {}", error));
        }
    }

    pub fn toggle_zen_mode(&mut self) {
        self.zen_mode = !self.zen_mode;
        if self.zen_mode {
            self.focus = Focus::Content;
        }
    }

    pub fn toggle_frontmatter_hidden(&mut self) {
        self.frontmatter_hidden = !self.frontmatter_hidden;
        self.update_content_items();
    }

    pub fn update_filtered_indices(&mut self) {
        if self.search_query.is_empty() {
            self.search_matched_notes.clear();
            self.filtered_indices.clear();
            return;
        }

        let query = self.search_query.to_lowercase();

        self.search_matched_notes = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, note)| note.title.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        for &note_index in &self.search_matched_notes {
            if let Some(note) = self.notes.get(note_index) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.config.notes_path();
                    let mut current = file_path.parent();
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        self.folder_states.insert(parent.to_path_buf(), true);
                        current = parent.parent();
                    }
                }
            }
        }

        Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);

        self.rebuild_sidebar_items();

        self.filtered_indices = self
            .sidebar_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let SidebarItemKind::Note { note_id } = &item.kind {
                    self.note_index_for_id(*note_id).is_some_and(|index| self.search_matched_notes.contains(&index))
                } else {
                    false
                }
            })
            .map(|(i, _)| i)
            .collect();

        if !self.filtered_indices.is_empty() {
            self.selected_sidebar_index = self.filtered_indices[0];
            self.sync_selected_note_from_sidebar();
            self.update_content_items();
            self.update_outline();
        }
    }

    pub(super) fn update_tree_expanded_states(items: &mut [FileTreeItem], folder_states: &HashMap<PathBuf, bool>) {
        for item in items {
            if let FileTreeItem::Folder(folder) = item {
                if let Some(&state) = folder_states.get(&folder.path) {
                    folder.expanded = state;
                }
                Self::update_tree_expanded_states(&mut folder.children, folder_states);
            }
        }
    }

    pub fn activate_sidebar_search(&mut self) {
        self.pre_search_folder_states = Some(self.folder_states.clone());
        self.pre_search_sidebar_index = Some(self.selected_sidebar_index);
        self.search_active = true;
        self.search_query.clear();
    }

    pub fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.filtered_indices.clear();
        self.search_matched_notes.clear();
        if let Some(saved_states) = self.pre_search_folder_states.take() {
            self.folder_states = saved_states;
            Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
            self.rebuild_sidebar_items();
        }
        if let Some(saved_index) = self.pre_search_sidebar_index.take() {
            self.selected_sidebar_index = saved_index.min(self.sidebar_items.len().saturating_sub(1));
        }
    }

    pub fn start_buffer_search(&mut self) {
        self.start_buffer_search_with_direction(SearchDirection::Forward);
    }

    #[allow(dead_code)]
    pub fn start_buffer_search_backward(&mut self) {
        self.start_buffer_search_with_direction(SearchDirection::Backward);
    }

    pub fn start_buffer_search_with_direction(&mut self, direction: SearchDirection) {
        self.buffer_search.active = true;
        self.buffer_search.query.clear();
        self.buffer_search.matches.clear();
        self.buffer_search.current_match_index = 0;
        self.buffer_search.direction = direction;
    }

    pub fn end_buffer_search(&mut self) {
        self.buffer_search.clear();
    }

    pub fn perform_buffer_search(&mut self) {
        self.buffer_search.matches.clear();
        self.buffer_search.current_match_index = 0;

        if self.buffer_search.query.is_empty() {
            return;
        }

        let query = if self.buffer_search.case_sensitive {
            self.buffer_search.query.clone()
        } else {
            self.buffer_search.query.to_lowercase()
        };

        let matches = if self.mode == Mode::Edit {
            let snapshot = self.editor.snapshot();
            find_buffer_matches(snapshot.iter_lines(), &query, self.buffer_search.case_sensitive)
        } else if let Some(body) = self.current_body() {
            find_buffer_matches(body.lines(), &query, self.buffer_search.case_sensitive)
        } else {
            return;
        };
        self.buffer_search.matches = matches;
    }

    pub fn scroll_to_current_match(&mut self) {
        if let Some(m) = self.buffer_search.current_match() {
            let target_row = m.row;

            if self.mode == Mode::Edit {
                let start_col = m.start_col;
                self.editor.set_cursor(target_row, start_col);
                let half_height = self.editor_view_height / 2;
                if target_row > half_height {
                    self.editor_scroll_top = target_row - half_height;
                } else {
                    self.editor_scroll_top = 0;
                }
            } else {
                for (idx, source_line) in self.content_items.iter().map(ContentItem::source_line).enumerate() {
                    if source_line >= target_row {
                        self.content_cursor = idx;
                        let content_height = self.content_area.height.saturating_sub(2) as usize;
                        let half_height = content_height / 2;
                        if idx > half_height {
                            self.content_scroll_offset = idx - half_height;
                        } else {
                            self.content_scroll_offset = 0;
                        }
                        break;
                    }
                }
            }
        }
    }

    pub fn buffer_search_next(&mut self) {
        self.buffer_search.next_match();
        self.scroll_to_current_match();
    }

    pub fn buffer_search_prev(&mut self) {
        self.buffer_search.prev_match();
        self.scroll_to_current_match();
    }

    pub fn get_visible_sidebar_indices(&self) -> Vec<usize> {
        if self.search_active && !self.search_query.is_empty() {
            self.filtered_indices.clone()
        } else {
            (0..self.sidebar_items.len()).collect()
        }
    }

    pub fn next_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => (i + 1) % self.outline.len(),
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    pub fn previous_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.outline.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    pub fn goto_first_outline(&mut self) {
        if !self.outline.is_empty() {
            self.outline_state.select(Some(0));
        }
    }

    pub fn goto_last_outline(&mut self) {
        if !self.outline.is_empty() {
            self.outline_state.select(Some(self.outline.len() - 1));
        }
    }

    pub fn jump_to_outline(&mut self) {
        if let Some(selected) = self.outline_state.selected() {
            if let Some(outline_item) = self.outline.get(selected) {
                let target_line = outline_item.line;
                // Set content cursor to the target line
                if target_line < self.content_items.len() {
                    self.unfold_heading_at(target_line);
                    self.content_cursor = target_line;
                }
                // Switch focus to content
                self.focus = Focus::Content;
            }
        }
    }

    pub fn current_note(&self) -> Option<&Note> {
        self.notes.get(self.selected_note)
    }

    /// Show a transient toast notification, replacing any current one.
    pub fn show_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toast = Some(Toast {
            message: message.into(),
            kind,
            shown_at: self.dependencies.clock.now(),
        });
    }

    /// Surface a recoverable error to the user as a toast. Use this instead of
    /// printing — stdout/stderr writes corrupt the alternate-screen TUI.
    pub fn show_error_toast(&mut self, message: impl Into<String>) {
        self.show_toast(message, ToastKind::Error);
    }

    /// Drop the active toast once it has outlived its TTL. Returns `true` when
    /// the screen needs a redraw because a toast was just dismissed.
    pub fn tick_toast(&mut self) -> bool {
        let now = self.dependencies.clock.now();
        if self.toast.as_ref().map_or(false, |t| t.is_expired_at(now)) {
            self.toast = None;
            true
        } else {
            false
        }
    }

    pub fn save_last_opened_note_to_cache(&self) {
        if let Some(note) = self.current_note() {
            if let Some(ref path) = note.file_path {
                save_last_opened_note(&self.dependencies.cache_dir, path);
            }
        }
    }
}

fn find_buffer_matches<'a>(lines: impl Iterator<Item = &'a str>, query: &str, case_sensitive: bool) -> Vec<BufferSearchMatch> {
    let query_len = query.chars().count();
    if query_len == 0 {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for (row, line) in lines.enumerate() {
        let folded;
        let search_line = if case_sensitive {
            line
        } else {
            folded = line.to_lowercase();
            &folded
        };
        for (col, (byte_start, _)) in search_line.char_indices().enumerate() {
            if search_line[byte_start..].starts_with(query) {
                matches.push(BufferSearchMatch {
                    row,
                    start_col: col,
                    end_col: col + query_len,
                });
            }
        }
    }
    matches
}
