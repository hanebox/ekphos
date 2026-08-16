use super::*;

impl App {
    pub fn start_index_build(&mut self) {
        if self.indexing_in_progress {
            return;
        }

        let notes_dir = self.config.notes_path();
        let index_path = search::get_index_path_in(&self.dependencies.cache_dir, &notes_dir);
        let notes_dir_str = notes_dir.to_string_lossy().to_string();

        let note_data: Vec<(usize, String, String, u64)> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(idx, note)| {
                let path = note.file_path.as_ref()?;
                let rel_path = path.strip_prefix(&notes_dir).ok()?.to_string_lossy().to_string();
                let mtime = note.modified_time?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                Some((idx, rel_path, note.content.clone(), mtime))
            })
            .collect();

        if note_data.is_empty() {
            self.index_progress.store(0, Ordering::Relaxed);
            self.index_total.store(0, Ordering::Relaxed);
            self.search_index = SearchIndex {
                version: 2,
                notes_dir: notes_dir_str,
                ready: true,
                indexing_complete: true,
                ..Default::default()
            };
            return;
        }

        self.indexing_in_progress = true;
        self.index_started_at = Some(std::time::Instant::now());

        self.index_progress.store(0, Ordering::Relaxed);
        self.index_total.store(note_data.len(), Ordering::Relaxed);
        let progress = Arc::clone(&self.index_progress);
        let total = Arc::clone(&self.index_total);
        let (sender, receiver) = mpsc::channel();
        self.index_receiver = receiver;

        std::thread::spawn(move || {
            let build_full_with_progress = |note_data: &[(usize, String, String, u64)], notes_dir: &str, progress: &Arc<AtomicUsize>| -> SearchIndex {
                let mut index = SearchIndex {
                    version: 2,
                    notes_dir: notes_dir.to_string(),
                    ..Default::default()
                };
                for (i, (note_idx, rel_path, content, mtime)) in note_data.iter().enumerate() {
                    index.index_note_pub(*note_idx, rel_path, content, *mtime);
                    progress.store(i + 1, Ordering::Relaxed);
                }
                index
            };

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let existing_index = search::load_index(&index_path);

                let mut index = if let Some(mut cached) = existing_index {
                    if cached.notes_dir == notes_dir_str {
                        let current_files: Vec<(String, u64)> = note_data.iter().map(|(_, path, _, mtime)| (path.clone(), *mtime)).collect();
                        let current_paths: Vec<String> = current_files.iter().map(|(p, _)| p.clone()).collect();

                        cached.remove_deleted(&current_paths);
                        let stale = cached.get_stale_files(&current_files);

                        if stale.is_empty() {
                            progress.store(note_data.len(), Ordering::Relaxed);
                            total.store(note_data.len(), Ordering::Relaxed);
                            cached
                        } else {
                            total.store(stale.len(), Ordering::Relaxed);
                            progress.store(0, Ordering::Relaxed);

                            let stale_notes: Vec<_> = note_data.iter().filter(|(_, path, _, _)| stale.contains(path)).cloned().collect();

                            for (i, note) in stale_notes.iter().enumerate() {
                                cached.update_with_notes(&[note.clone()]);
                                progress.store(i + 1, Ordering::Relaxed);
                            }
                            cached
                        }
                    } else {
                        build_full_with_progress(&note_data, &notes_dir_str, &progress)
                    }
                } else {
                    build_full_with_progress(&note_data, &notes_dir_str, &progress)
                };

                index.ready = true;
                index.indexing_complete = true;
                let _ = search::save_index(&index, &index_path);
                let _ = sender.send(index);
            }));

            if result.is_err() {
                let mut empty = SearchIndex::default();
                empty.ready = true;
                empty.indexing_complete = true;
                let _ = sender.send(empty);
            }
        });
    }

    pub fn poll_index_build(&mut self) {
        // Early return if not indexing
        if !self.indexing_in_progress {
            return;
        }

        if let Ok(index) = self.index_receiver.try_recv() {
            self.search_index = index;
            self.indexing_in_progress = false;
            self.index_started_at = None;
            // Reset progress counters
            self.index_progress.store(0, Ordering::Relaxed);
            self.index_total.store(0, Ordering::Relaxed);
            return;
        }

        const INDEXING_TIMEOUT_SECS: u64 = 60;
        if let Some(started) = self.index_started_at {
            if started.elapsed().as_secs() > INDEXING_TIMEOUT_SECS {
                self.indexing_in_progress = false;
                self.index_started_at = None;
                self.index_progress.store(0, Ordering::Relaxed);
                self.index_total.store(0, Ordering::Relaxed);
                self.search_index.ready = true;
                self.search_index.indexing_complete = true;
            }
        }
    }

    /// Search using the index (fast path)
    pub(super) fn search_with_index(&self, query: &str) -> Vec<ContentSearchResult> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // i think most people should be fine with 15k limits
        const MAX_RESULTS: usize = 15000;
        const MAX_EXACT_MATCHES: usize = 15000;
        const MAX_PREFIX_MATCHES: usize = 15000;
        const MAX_PREFIX_TERMS_SCANNED: usize = 15000;
        const MAX_LINE_SCAN_NOTES: usize = 15000;

        let create_result = |note_idx: usize, line_num: usize, line: &str, query_lower: &str| -> Option<ContentSearchResult> {
            let note = self.notes.get(note_idx)?;
            let wiki_path = self.get_wiki_path_for_note(note_idx);
            let folder_hint = wiki_path.as_ref().and_then(|wp| wp.rfind('/').map(|pos| wp[..pos].to_string()));

            let line_lower = line.to_lowercase();
            let match_byte_pos = line_lower.find(query_lower)?;
            let line_chars: Vec<char> = line.chars().collect();
            let match_start_char = line_lower[..match_byte_pos].chars().count();
            let match_end_char = match_start_char + query_lower.chars().count();

            let mut score = 100;
            let title_lower = note.title.to_lowercase();
            if title_lower.contains(query_lower) {
                score += 50;
            }
            if match_start_char == 0 {
                score += 20;
            }
            if match_start_char == 0 || !line_chars.get(match_start_char.saturating_sub(1)).map(|c| c.is_alphanumeric()).unwrap_or(false) {
                score += 10;
            }

            let context_size = 25;
            let start = match_start_char.saturating_sub(context_size);
            let end = (match_end_char + context_size).min(line_chars.len());

            let mut matched_line: String = line_chars[start..end].iter().collect();
            let display_match_start = match_start_char - start;
            let display_match_end = match_end_char - start;

            if start > 0 {
                matched_line = format!("...{}", matched_line);
            }
            if end < line_chars.len() {
                matched_line.push_str("...");
            }

            Some(ContentSearchResult {
                display_name: note.title.clone(),
                matched_line,
                line_number: line_num + 1,
                note_index: note_idx,
                folder_hint,
                score,
                match_start: display_match_start + if start > 0 { 3 } else { 0 },
                match_end: display_match_end + if start > 0 { 3 } else { 0 },
            })
        };

        if let Some(positions) = self.search_index.terms.get(&query_lower) {
            for &(note_idx, line_num, _) in positions.iter().take(MAX_EXACT_MATCHES) {
                if seen.insert((note_idx, line_num)) {
                    if let Some(lines) = self.search_index.lines.get(note_idx) {
                        if let Some(line) = lines.get(line_num) {
                            if let Some(result) = create_result(note_idx, line_num, line, &query_lower) {
                                results.push(result);
                            }
                        }
                    }
                }
            }
        }

        // Phase 2 Prefix matches - limit terms scanned to prevent freeze
        if results.len() < MAX_RESULTS {
            let mut terms_scanned = 0;
            let mut prefix_matches = 0;

            for (word, positions) in &self.search_index.terms {
                // Early exit conditions
                if terms_scanned >= MAX_PREFIX_TERMS_SCANNED || prefix_matches >= MAX_PREFIX_MATCHES {
                    break;
                }
                terms_scanned += 1;

                if word.starts_with(&query_lower) && word != &query_lower {
                    for &(note_idx, line_num, _) in positions.iter().take(50) {
                        if prefix_matches >= MAX_PREFIX_MATCHES {
                            break;
                        }
                        if seen.insert((note_idx, line_num)) {
                            if let Some(lines) = self.search_index.lines.get(note_idx) {
                                if let Some(line) = lines.get(line_num) {
                                    if let Some(result) = create_result(note_idx, line_num, line, &query_lower) {
                                        results.push(result);
                                        prefix_matches += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Phase 3 Line scan fallback for substring matches
        if results.len() < MAX_RESULTS {
            let mut notes_scanned = 0;
            'outer: for (note_idx, lines) in self.search_index.lines.iter().enumerate() {
                if notes_scanned >= MAX_LINE_SCAN_NOTES || results.len() >= MAX_RESULTS {
                    break;
                }
                notes_scanned += 1;

                for (line_num, line) in lines.iter().enumerate() {
                    if seen.contains(&(note_idx, line_num)) {
                        continue;
                    }
                    if line.to_lowercase().contains(&query_lower) {
                        if let Some(result) = create_result(note_idx, line_num, line, &query_lower) {
                            seen.insert((note_idx, line_num));
                            results.push(result);
                            if results.len() >= MAX_RESULTS {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.display_name.cmp(&b.display_name))
                .then_with(|| a.line_number.cmp(&b.line_number))
        });
        results.truncate(MAX_RESULTS);
        results
    }

    #[doc(hidden)]
    pub fn headless_content_search(&self, query: &str) -> Vec<ContentSearchResult> {
        self.search_with_index(query)
    }

    #[doc(hidden)]
    pub fn headless_file_search(&self, query: &str) -> Vec<FilePickerResult> {
        self.build_file_picker_results(query)
    }

    // ==================== Mouse Selection Helpers ====================

    /// Convert mouse screen coordinates to editor row/col.
    /// Returns None if mouse is outside the editor area.
    pub fn screen_to_editor_coords(&self, mouse_x: u16, mouse_y: u16) -> Option<(usize, usize)> {
        let (inner_x, inner_y, inner_width, inner_height) = if self.zen_mode {
            (self.editor_area.x, self.editor_area.y, self.editor_area.width, self.editor_area.height)
        } else {
            (
                self.editor_area.x + 1,
                self.editor_area.y + 1,
                self.editor_area.width.saturating_sub(2),
                self.editor_area.height.saturating_sub(2),
            )
        };

        if mouse_x < inner_x || mouse_x >= inner_x + inner_width || mouse_y < inner_y || mouse_y >= inner_y + inner_height {
            return None;
        }

        let content_x_offset = self.editor.content_x_offset();
        let content_start_x = inner_x + content_x_offset;
        let rel_x = if mouse_x >= content_start_x {
            (mouse_x - content_start_x) as usize
        } else {
            0
        };
        let rel_y = (mouse_y - inner_y) as usize;

        let (row, col) = self.editor.visual_to_logical_coords(rel_y, rel_x);

        Some((row, col))
    }

    /// Check if mouse is in the auto-scroll zone (top or bottom edge).
    /// Returns scroll direction: -1 for up, 1 for down, 0 for no scroll.
    pub fn get_auto_scroll_direction(&self, mouse_y: u16) -> i8 {
        const SCROLL_THRESHOLD: u16 = 2;

        let (inner_y, inner_height) = if self.zen_mode {
            (self.editor_area.y, self.editor_area.height)
        } else {
            (self.editor_area.y + 1, self.editor_area.height.saturating_sub(2))
        };

        if mouse_y < inner_y + SCROLL_THRESHOLD && self.editor_scroll_top > 0 {
            -1 // Scroll up
        } else if mouse_y >= inner_y + inner_height - SCROLL_THRESHOLD {
            1 // Scroll down
        } else {
            0
        }
    }

    pub fn open_search_picker(&mut self) {
        self.search_picker = SearchPickerState::Open {
            mode: SearchPickerMode::Files,
            query: String::new(),
            file_results: Vec::new(),
            content_results: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            search_in_progress: false,
            search_id: 0,
        };
    }

    pub fn close_search_picker(&mut self) {
        self.search_picker = SearchPickerState::Closed;
    }

    pub fn toggle_search_picker_mode(&mut self) {
        let (new_mode, query) = if let SearchPickerState::Open {
            mode,
            query,
            selected_index,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            *mode = match *mode {
                SearchPickerMode::Files => SearchPickerMode::Content,
                SearchPickerMode::Content => SearchPickerMode::Files,
            };
            // Reset selection and scroll
            *selected_index = 0;
            *scroll_offset = 0;
            (*mode, query.clone())
        } else {
            return;
        };

        match new_mode {
            SearchPickerMode::Content => {
                if !query.is_empty() {
                    self.start_content_search();
                }
            }
            SearchPickerMode::Files => {
                if query.is_empty() {
                    if let SearchPickerState::Open { file_results, .. } = &mut self.search_picker {
                        file_results.clear();
                    }
                } else {
                    let new_results = self.build_file_picker_results(&query);
                    if let SearchPickerState::Open { file_results, .. } = &mut self.search_picker {
                        *file_results = new_results;
                    }
                }
            }
        }
    }

    pub(super) fn build_file_picker_results(&self, query: &str) -> Vec<FilePickerResult> {
        let query_lower = query.to_lowercase();

        let mut results: Vec<FilePickerResult> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(idx, note)| {
                let wiki_path = self.get_wiki_path_for_note(idx);

                let score = fuzzy_match(&note.title, query)
                    .or_else(|| wiki_path.as_ref().and_then(|p| fuzzy_match(p, query)))
                    .or_else(|| {
                        let title_lower = note.title.to_lowercase();
                        if title_lower.contains(&query_lower) {
                            Some(100)
                        } else if let Some(ref wp) = wiki_path {
                            if wp.to_lowercase().contains(&query_lower) {
                                Some(50)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });
                let score = score?;

                let folder_hint = wiki_path.and_then(|wp| wp.rfind('/').map(|pos| wp[..pos].to_string()));

                Some(FilePickerResult {
                    display_name: note.title.clone(),
                    folder_hint,
                    note_index: idx,
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.display_name.cmp(&b.display_name)));

        results
    }

    pub fn start_content_search(&mut self) {
        let query = if let SearchPickerState::Open { query, mode, .. } = &self.search_picker {
            if *mode != SearchPickerMode::Content || query.is_empty() {
                return;
            }
            query.clone()
        } else {
            return;
        };

        if self.search_index.ready {
            let results = self.search_with_index(&query);
            if let SearchPickerState::Open {
                content_results,
                search_in_progress,
                selected_index,
                scroll_offset,
                ..
            } = &mut self.search_picker
            {
                *content_results = results;
                *search_in_progress = false;
                *selected_index = 0;
                *scroll_offset = 0;
            }
            return;
        }

        self.next_search_id += 1;
        let search_id = self.next_search_id;

        if let SearchPickerState::Open {
            search_in_progress,
            search_id: state_search_id,
            ..
        } = &mut self.search_picker
        {
            *search_in_progress = true;
            *state_search_id = search_id;
        }

        let notes: Vec<(usize, String, String, Option<String>)> = self
            .notes
            .iter()
            .enumerate()
            .map(|(idx, note)| {
                let wiki_path = self.get_wiki_path_for_note(idx);
                let folder_hint = wiki_path.as_ref().and_then(|wp| wp.rfind('/').map(|pos| wp[..pos].to_string()));
                (idx, note.title.clone(), note.content.clone(), folder_hint)
            })
            .collect();

        let sender = self.content_search_sender.clone();

        // Spawn background thread for content search
        std::thread::spawn(move || {
            let query_lower = query.to_lowercase();
            let mut results: Vec<ContentSearchResult> = Vec::new();

            for (note_idx, title, content, folder_hint) in notes {
                let title_lower = title.to_lowercase();
                let title_matches = title_lower.contains(&query_lower);

                for (line_num, line) in content.lines().enumerate() {
                    let line_lower = line.to_lowercase();
                    if let Some(match_byte_pos) = line_lower.find(&query_lower) {
                        // Convert byte position to character position for Unicode support
                        let line_chars: Vec<char> = line.chars().collect();
                        let match_start_char = line_lower[..match_byte_pos].chars().count();
                        let match_end_char = match_start_char + query_lower.chars().count();

                        // Calculate score
                        let mut score = 100;
                        if title_matches {
                            score += 50;
                        }
                        if match_start_char == 0 {
                            score += 20;
                        }
                        // Word boundary bonus - use char position, not byte position
                        if match_start_char == 0 || !line_chars.get(match_start_char.saturating_sub(1)).map(|c| c.is_alphanumeric()).unwrap_or(false) {
                            score += 10;
                        }

                        // Get context around match (max 60 chars total)
                        let context_size = 25;
                        let start = match_start_char.saturating_sub(context_size);
                        let end = (match_end_char + context_size).min(line_chars.len());

                        let mut matched_line: String = line_chars[start..end].iter().collect();
                        let display_match_start = match_start_char - start;
                        let display_match_end = match_end_char - start;

                        // Add ellipsis if truncated
                        if start > 0 {
                            matched_line = format!("...{}", matched_line);
                        }
                        if end < line_chars.len() {
                            matched_line.push_str("...");
                        }

                        results.push(ContentSearchResult {
                            display_name: title.clone(),
                            matched_line,
                            line_number: line_num + 1,
                            note_index: note_idx,
                            folder_hint: folder_hint.clone(),
                            score,
                            match_start: display_match_start + if start > 0 { 3 } else { 0 },
                            match_end: display_match_end + if start > 0 { 3 } else { 0 },
                        });
                    }
                }
            }

            results.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| a.display_name.cmp(&b.display_name))
                    .then_with(|| a.line_number.cmp(&b.line_number))
            });

            results.truncate(500);

            let _ = sender.send(ContentSearchResponse { search_id, results });
        });
    }

    /// Polls for content search results (call in main loop)
    pub fn poll_content_search(&mut self) {
        while let Ok(response) = self.content_search_receiver.try_recv() {
            if let SearchPickerState::Open {
                search_id,
                content_results,
                search_in_progress,
                selected_index,
                scroll_offset,
                ..
            } = &mut self.search_picker
            {
                if response.search_id == *search_id {
                    *content_results = response.results;
                    *search_in_progress = false;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
            }
        }
    }

    pub fn is_content_search_in_progress(&self) -> bool {
        if let SearchPickerState::Open { search_in_progress, .. } = &self.search_picker {
            *search_in_progress
        } else {
            false
        }
    }

    pub fn update_search_picker_results(&mut self) {
        let (query, mode) = if let SearchPickerState::Open { query, mode, .. } = &self.search_picker {
            (query.clone(), *mode)
        } else {
            return;
        };

        match mode {
            SearchPickerMode::Files => {
                if query.is_empty() {
                    if let SearchPickerState::Open {
                        file_results,
                        selected_index,
                        scroll_offset,
                        ..
                    } = &mut self.search_picker
                    {
                        file_results.clear();
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                } else {
                    let new_results = self.build_file_picker_results(&query);
                    if let SearchPickerState::Open {
                        file_results,
                        selected_index,
                        scroll_offset,
                        ..
                    } = &mut self.search_picker
                    {
                        *file_results = new_results;
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                }
            }
            SearchPickerMode::Content => {
                if query.is_empty() {
                    if let SearchPickerState::Open {
                        content_results,
                        selected_index,
                        scroll_offset,
                        search_in_progress,
                        ..
                    } = &mut self.search_picker
                    {
                        content_results.clear();
                        *selected_index = 0;
                        *scroll_offset = 0;
                        *search_in_progress = false;
                    }
                } else {
                    self.start_content_search();
                }
            }
        }
    }

    pub fn select_search_picker_result(&mut self) {
        let result_info = if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            selected_index,
            ..
        } = &self.search_picker
        {
            match mode {
                SearchPickerMode::Files => file_results.get(*selected_index).map(|r| (r.note_index, None)),
                SearchPickerMode::Content => content_results.get(*selected_index).map(|r| (r.note_index, Some(r.line_number))),
            }
        } else {
            None
        };

        let Some((note_index, line_number)) = result_info else {
            self.search_picker = SearchPickerState::Closed;
            return;
        };

        if note_index < self.notes.len() {
            if let Some(note) = self.notes.get(note_index) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.config.notes_path();
                    let mut current = file_path.parent();
                    let mut needs_rebuild = false;
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        if !self.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                            self.folder_states.insert(parent.to_path_buf(), true);
                            needs_rebuild = true;
                        }
                        current = parent.parent();
                    }
                    if needs_rebuild {
                        Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
                        self.rebuild_sidebar_items();
                    }
                }
            }

            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index: idx_note } = &item.kind {
                    if *idx_note == note_index {
                        self.end_buffer_search();
                        self.selected_sidebar_index = idx;
                        self.selected_note = note_index;
                        self.push_navigation_history(note_index);
                        self.content_cursor = 0;
                        self.content_scroll_offset = 0;
                        self.update_content_items();
                        self.update_outline();

                        if let Some(target_line) = line_number {
                            let target_line_0indexed = target_line.saturating_sub(1);
                            let mut best_match_idx = 0;
                            let mut best_match_diff = usize::MAX;

                            for (i, &source_line) in self.content_item_source_lines.iter().enumerate() {
                                if source_line == target_line_0indexed {
                                    best_match_idx = i;
                                    break;
                                } else if source_line < target_line_0indexed {
                                    let diff = target_line_0indexed - source_line;
                                    if diff < best_match_diff {
                                        best_match_diff = diff;
                                        best_match_idx = i;
                                    }
                                } else {
                                    let diff = source_line - target_line_0indexed;
                                    if diff < best_match_diff {
                                        best_match_idx = i;
                                    }
                                    break;
                                }
                            }

                            self.content_cursor = best_match_idx.min(self.content_items.len().saturating_sub(1));

                            let visible_height = 20usize; // Approximate visible lines
                            let target_scroll = self.content_cursor.saturating_sub(visible_height / 3);
                            self.content_scroll_offset = target_scroll;
                        }

                        self.focus = Focus::Content;
                        break;
                    }
                }
            }
        }

        self.search_picker = SearchPickerState::Closed;
    }

    pub fn search_picker_select_prev(&mut self) {
        // Must match POPUP_MAX_VISIBLE_ITEMS / POPUP_MAX_VISIBLE_ITEMS_CONTENT in ui/file_picker.rs
        const MAX_VISIBLE_FILES: usize = 10;
        const MAX_VISIBLE_CONTENT: usize = 18;
        if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            selected_index,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            let (results_len, max_visible) = match mode {
                SearchPickerMode::Files => (file_results.len(), MAX_VISIBLE_FILES),
                SearchPickerMode::Content => (content_results.len(), MAX_VISIBLE_CONTENT),
            };

            if results_len == 0 {
                return;
            }

            if *selected_index > 0 {
                *selected_index -= 1;
            } else {
                *selected_index = results_len - 1;
                *scroll_offset = results_len.saturating_sub(max_visible);
                return;
            }

            if *selected_index < *scroll_offset {
                *scroll_offset = *selected_index;
            }
        }
    }

    pub fn search_picker_select_next(&mut self) {
        // Must match POPUP_MAX_VISIBLE_ITEMS / POPUP_MAX_VISIBLE_ITEMS_CONTENT in ui/file_picker.rs
        const MAX_VISIBLE_FILES: usize = 10;
        const MAX_VISIBLE_CONTENT: usize = 18;
        if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            selected_index,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            let (results_len, max_visible) = match mode {
                SearchPickerMode::Files => (file_results.len(), MAX_VISIBLE_FILES),
                SearchPickerMode::Content => (content_results.len(), MAX_VISIBLE_CONTENT),
            };

            if results_len == 0 {
                return;
            }

            if *selected_index < results_len - 1 {
                *selected_index += 1;
            } else {
                *selected_index = 0;
                *scroll_offset = 0;
                return;
            }

            let visible_end = *scroll_offset + max_visible;
            if *selected_index >= visible_end {
                *scroll_offset = *selected_index - max_visible + 1;
            }
        }
    }

    pub fn search_picker_push_char(&mut self, c: char) {
        if let SearchPickerState::Open { query, .. } = &mut self.search_picker {
            query.push(c);
        }
        self.update_search_picker_results();
    }

    pub fn search_picker_pop_char(&mut self) {
        if let SearchPickerState::Open { query, .. } = &mut self.search_picker {
            query.pop();
        }
        self.update_search_picker_results();
    }
    pub fn is_inside_search_picker(&self, x: u16, y: u16) -> bool {
        let area = self.search_picker_area;
        x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
    }
    /// Handle mouse click on search picker results
    pub fn search_picker_click(&mut self, x: u16, y: u16) -> u8 {
        let results_area = self.search_picker_results_area;

        // Check if click is in results area
        if x < results_area.x || x >= results_area.x + results_area.width || y < results_area.y || y >= results_area.y + results_area.height {
            return 0;
        }

        // Calculate which row was clicked (relative to results area)
        let clicked_row = (y - results_area.y) as usize;

        if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            selected_index,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            let clicked_index = match mode {
                SearchPickerMode::Content => *scroll_offset + clicked_row,
                SearchPickerMode::Files => {
                    let mut accumulated_lines = 0;
                    let mut target_index = None;

                    for (i, result) in file_results.iter().enumerate().skip(*scroll_offset) {
                        let item_lines = if result.folder_hint.is_some() { 2 } else { 1 };
                        if clicked_row < accumulated_lines + item_lines {
                            target_index = Some(i);
                            break;
                        }
                        accumulated_lines += item_lines;
                    }

                    target_index.unwrap_or(*scroll_offset + clicked_row)
                }
            };

            let results_len = match mode {
                SearchPickerMode::Files => file_results.len(),
                SearchPickerMode::Content => content_results.len(),
            };

            if clicked_index < results_len {
                *selected_index = clicked_index;
                let now = std::time::Instant::now();
                let is_double_click = if let Some((last_time, last_index)) = self.search_picker_last_click {
                    last_index == clicked_index && now.duration_since(last_time).as_millis() < 400
                } else {
                    false
                };

                self.search_picker_last_click = Some((now, clicked_index));

                return if is_double_click { 2 } else { 1 };
            }
        }
        0
    }

    pub fn search_picker_scroll_up(&mut self) {
        if let SearchPickerState::Open { scroll_offset, .. } = &mut self.search_picker {
            if *scroll_offset > 0 {
                *scroll_offset -= 1;
            }
        }
    }
    pub fn search_picker_scroll_down(&mut self) {
        const MAX_VISIBLE_FILES: usize = 10; // Must match POPUP_MAX_VISIBLE_ITEMS
        const MAX_VISIBLE_CONTENT: usize = 18; // Must match POPUP_MAX_VISIBLE_ITEMS_CONTENT
        if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            let (results_len, max_visible) = match mode {
                SearchPickerMode::Files => (file_results.len(), MAX_VISIBLE_FILES),
                SearchPickerMode::Content => (content_results.len(), MAX_VISIBLE_CONTENT),
            };

            if *scroll_offset + max_visible < results_len {
                *scroll_offset += 1;
            }
        }
    }
}
