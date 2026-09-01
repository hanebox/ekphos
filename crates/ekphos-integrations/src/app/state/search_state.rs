use super::*;

impl App {
    pub fn start_index_build(&mut self) {
        if self.search.indexing_in_progress {
            return;
        }
        let notes_dir = self.state.config.notes_path();
        let index_path = search::get_index_path_in(&self.dependencies.cache_dir, &notes_dir);
        let sources = self.search_index_sources();
        if sources.is_empty() {
            self.search.index_progress.store(0, Ordering::Relaxed);
            self.search.index_total.store(0, Ordering::Relaxed);
            self.search.search_index = SearchIndex::build_from_loader(&notes_dir, &sources, |_| None).ok().map(Arc::new);
            return;
        }
        self.search.indexing_in_progress = true;
        self.search.index_started_at = Some(std::time::Instant::now());
        self.search.index_progress.store(0, Ordering::Relaxed);
        self.search.index_total.store(sources.len(), Ordering::Relaxed);
        let progress = Arc::clone(&self.search.index_progress);
        let total = Arc::clone(&self.search.index_total);
        let generation = self.search.search_generation;
        let generation_signal = Arc::clone(&self.search.search_generation_signal);
        let (sender, receiver) = mpsc::sync_channel(1);
        self.workers.index_receiver = receiver;
        let spawn = std::thread::Builder::new().name("ekphos-index".to_string()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let cached = search::load_index(&index_path);
                let mut loaded = 0usize;
                let mut load = |source: &search::SearchSource| {
                    if generation_signal.load(Ordering::Acquire) != generation {
                        return None;
                    }
                    let body = fs::read_to_string(&source.absolute_path).ok().map(Arc::<str>::from);
                    loaded += 1;
                    progress.store(loaded, Ordering::Relaxed);
                    body
                };
                let index = if let Some(cached) = cached {
                    if cached.matches_sources(&notes_dir, &sources) {
                        Ok(cached)
                    } else {
                        cached.update_from_loader(&notes_dir, &sources, &mut load)
                    }
                } else {
                    SearchIndex::build_from_loader(&notes_dir, &sources, &mut load)
                };
                progress.store(sources.len(), Ordering::Relaxed);
                total.store(sources.len(), Ordering::Relaxed);
                if let Ok(index) = index {
                    if generation_signal.load(Ordering::Acquire) != generation {
                        return;
                    }
                    let installed = if search::save_index(&index, &index_path).is_ok() { search::load_index(&index_path).unwrap_or(index) } else { index };
                    let _ = sender.send((generation, installed));
                }
            }));
            let _ = result;
        });
        if let Err(error) = spawn {
            self.search.indexing_in_progress = false;
            self.search.index_started_at = None;
            self.search.index_progress.store(0, Ordering::Relaxed);
            self.search.index_total.store(0, Ordering::Relaxed);
            self.show_error_toast(format!("Could not start search indexing: {error}"));
        }
    }

    pub fn poll_index_build(&mut self) {
        if !self.search.indexing_in_progress {
            return;
        }
        while let Ok((generation, index)) = self.workers.index_receiver.try_recv() {
            if generation == self.search.search_generation {
                self.search.search_index = Some(Arc::new(index));
                self.search.indexing_in_progress = false;
                self.search.index_started_at = None;
                self.search.index_progress.store(0, Ordering::Relaxed);
                self.search.index_total.store(0, Ordering::Relaxed);
                return;
            }
        }
        const INDEXING_TIMEOUT_SECS: u64 = 60;
        if let Some(started) = self.search.index_started_at {
            if started.elapsed().as_secs() > INDEXING_TIMEOUT_SECS {
                self.search.indexing_in_progress = false;
                self.search.index_started_at = None;
                self.search.index_progress.store(0, Ordering::Relaxed);
                self.search.index_total.store(0, Ordering::Relaxed);
            }
        }
    }

    #[doc(hidden)]
    pub fn headless_content_search(&self, query: &str) -> Vec<ContentSearchResult> {
        let hits = self.headless_content_search_hits(query);
        let indexed_hits: Vec<_> = hits.iter().copied().enumerate().collect();
        self.hydrate_search_hits(&indexed_hits).into_iter().map(|entry| entry.result).collect()
    }

    #[doc(hidden)]
    pub fn headless_file_search(&self, query: &str) -> Vec<FilePickerResult> {
        self.build_file_picker_results(query)
    }
    fn search_index_sources(&self) -> Vec<search::SearchSource> {
        let notes_dir = self.state.config.notes_path();
        self.vault
            .notes
            .iter()
            .filter(|note| note.kind.is_markdown())
            .filter_map(|note| {
                let absolute_path = note.file_path.clone()?;
                let relative_path = absolute_path.strip_prefix(&notes_dir).ok()?.to_string_lossy().to_string().into_boxed_str();
                let fingerprint = self.vault.fingerprint(note.id)?;
                Some(search::SearchSource { note_id: note.id, relative_path, absolute_path, fingerprint: search::SearchFileFingerprint { size: fingerprint.size, modified_nanos: fingerprint.modified_nanos.map(std::num::NonZeroU64::get).unwrap_or(0) } })
            })
            .collect()
    }

    #[doc(hidden)]
    pub fn headless_content_search_hits(&self, query: &str) -> Vec<search::SearchHit> {
        let sources = self.content_search_sources();
        search::search_sources(&sources, query, self.search.search_index.as_deref(), || false).unwrap_or_default()
    }
    fn content_search_sources(&self) -> Arc<[search::ContentSearchSource]> {
        self.vault.notes.iter().filter(|note| note.kind.is_markdown()).filter_map(|note| Some(search::ContentSearchSource { note_id: note.id, title: note.title.clone().into_boxed_str(), absolute_path: note.file_path.clone()? })).collect::<Vec<_>>().into()
    }
    fn search_body(&self, note_id: NoteId) -> Option<Arc<str>> {
        if self.document.active_note_id == Some(note_id) {
            return self.document.active_document.as_ref().map(DocumentSnapshot::body_arc);
        }
        let path = self.vault.notes.iter().find(|note| note.id == note_id)?.file_path.as_ref()?;
        fs::read_to_string(path).ok().map(Arc::from)
    }
    fn hydrate_search_hits(&self, requested: &[(usize, search::SearchHit)]) -> Vec<HydratedSearchResult> {
        let mut by_note: HashMap<NoteId, Vec<(usize, usize, search::SearchHit)>> = HashMap::new();
        for (output_index, (result_index, hit)) in requested.iter().copied().enumerate() {
            by_note.entry(hit.note_id).or_default().push((output_index, result_index, hit));
        }
        let mut output = vec![None; requested.len()];
        for (note_id, entries) in by_note {
            let Some(body) = self.search_body(note_id) else {
                continue;
            };
            let wanted: HashMap<u32, Vec<(usize, usize, search::SearchHit)>> = entries.into_iter().fold(HashMap::new(), |mut lines, entry| {
                lines.entry(entry.2.line_number).or_default().push(entry);
                lines
            });
            for (line_number, line) in body.lines().enumerate() {
                let Ok(line_number) = u32::try_from(line_number) else {
                    break;
                };
                let Some(entries) = wanted.get(&line_number) else {
                    continue;
                };
                for &(output_index, result_index, hit) in entries {
                    if let Some(result) = self.hydrate_search_line(hit, line) {
                        output[output_index] = Some(HydratedSearchResult { result_index, result });
                    }
                }
            }
        }
        output.into_iter().flatten().collect()
    }
    fn hydrate_search_line(&self, hit: search::SearchHit, line: &str) -> Option<ContentSearchResult> {
        let note_index = self.note_index_for_id(hit.note_id)?;
        let note = self.vault.notes.get(note_index)?;
        let wiki_path = self.get_wiki_path_for_note(note_index);
        let folder_hint = wiki_path.as_ref().and_then(|path| path.rfind('/').map(|position| path[..position].to_string()));
        let line_chars: Vec<char> = line.chars().collect();
        let match_start = (hit.match_start as usize).min(line_chars.len());
        let match_end = (hit.match_end as usize).min(line_chars.len()).max(match_start);
        let context_size = 25;
        let start = match_start.saturating_sub(context_size);
        let end = (match_end + context_size).min(line_chars.len());
        let mut matched_line: String = line_chars[start..end].iter().collect();
        if start > 0 {
            matched_line.insert_str(0, "...");
        }
        if end < line_chars.len() {
            matched_line.push_str("...");
        }
        let ellipsis = usize::from(start > 0) * 3;
        Some(ContentSearchResult { display_name: note.title.clone(), matched_line, line_number: hit.line_number as usize + 1, note_index, folder_hint, score: hit.score, match_start: match_start - start + ellipsis, match_end: match_end - start + ellipsis })
    }
    pub fn ensure_search_hydrated(&mut self) {
        const VISIBLE_RESULTS: usize = 18;
        const PREVIEW_BEFORE: usize = 5;
        const PREVIEW_AFTER: usize = 8;
        let (search_id, selected_index, scroll_offset, hits, already_hydrated) = match &self.search.search_picker {
            SearchPickerState::Open { mode: SearchPickerMode::Content, search_id, selected_index, scroll_offset, content_results, hydration_key, .. } => {
                let key = (*search_id, *selected_index, *scroll_offset);
                (*search_id, *selected_index, *scroll_offset, content_results.clone(), *hydration_key == Some(key))
            }
            _ => return,
        };
        if already_hydrated {
            return;
        }
        let requested: Vec<_> = hits.iter().copied().enumerate().skip(scroll_offset).take(VISIBLE_RESULTS).collect();
        let hydrated = self.hydrate_search_hits(&requested);
        let preview = hits.get(selected_index).and_then(|hit| {
            let body = self.search_body(hit.note_id)?;
            let match_line = hit.line_number as usize;
            let start_line = match_line.saturating_sub(PREVIEW_BEFORE);
            let lines = body.lines().skip(start_line).take(PREVIEW_BEFORE + PREVIEW_AFTER + 1).map(str::to_owned).collect();
            Some(ContentSearchPreview { note_id: hit.note_id, start_line, lines })
        });
        if let SearchPickerState::Open { search_id: current_id, hydrated_content_results, content_preview, hydration_key, .. } = &mut self.search.search_picker {
            if *current_id == search_id {
                *hydrated_content_results = hydrated;
                *content_preview = preview;
                *hydration_key = Some((search_id, selected_index, scroll_offset));
            }
        }
    }

    /// Convert mouse screen coordinates to editor row/col.
    /// Returns None if mouse is outside the editor area.
    pub fn screen_to_editor_coords(&self, mouse_x: u16, mouse_y: u16) -> Option<(usize, usize)> {
        let (inner_x, inner_y, inner_width, inner_height) = if self.state.zen_mode {
            (self.editor.editor_area.x, self.editor.editor_area.y, self.editor.editor_area.width, self.editor.editor_area.height)
        } else {
            (self.editor.editor_area.x + 1, self.editor.editor_area.y + 1, self.editor.editor_area.width.saturating_sub(2), self.editor.editor_area.height.saturating_sub(2))
        };
        if mouse_x < inner_x || mouse_x >= inner_x + inner_width || mouse_y < inner_y || mouse_y >= inner_y + inner_height {
            return None;
        }
        let content_x_offset = self.editor.content_x_offset();
        let content_start_x = inner_x + content_x_offset;
        let rel_x = if mouse_x >= content_start_x { (mouse_x - content_start_x) as usize } else { 0 };
        let rel_y = (mouse_y - inner_y) as usize;
        let (row, col) = self.editor.visual_to_logical_coords(rel_y, rel_x);
        Some((row, col))
    }

    /// Check if mouse is in the auto-scroll zone (top or bottom edge).
    /// Returns scroll direction: -1 for up, 1 for down, 0 for no scroll.
    pub fn get_auto_scroll_direction(&self, mouse_y: u16) -> i8 {
        const SCROLL_THRESHOLD: u16 = 2;
        let (inner_y, inner_height) = if self.state.zen_mode { (self.editor.editor_area.y, self.editor.editor_area.height) } else { (self.editor.editor_area.y.saturating_add(1), self.editor.editor_area.height.saturating_sub(2)) };
        if inner_height == 0 {
            return 0;
        }
        let threshold = SCROLL_THRESHOLD.min(inner_height);
        let top_boundary = inner_y.saturating_add(threshold);
        let bottom_boundary = inner_y.saturating_add(inner_height.saturating_sub(threshold));
        if mouse_y < top_boundary && self.editor.editor_scroll_top > 0 {
            -1 // Scroll up
        } else if mouse_y >= bottom_boundary {
            1 // Scroll down
        } else {
            0
        }
    }

    pub fn open_search_picker(&mut self) {
        self.search.search_picker = SearchPickerState::Open {
            mode: SearchPickerMode::Files,
            query: String::new(),
            file_results: Vec::new(),
            content_results: Vec::new(),
            hydrated_content_results: Vec::new(),
            content_preview: None,
            hydration_key: None,
            selected_index: 0,
            scroll_offset: 0,
            search_in_progress: false,
            search_id: 0,
        };
    }

    pub fn close_search_picker(&mut self) {
        self.search.search_picker = SearchPickerState::Closed;
        self.release_search_service();
        self.request_memory_reclaim();
    }
    pub(super) fn release_search_service(&mut self) {
        if let Some(worker) = &self.workers.search {
            worker.cancel();
        }
        self.workers.search = None;
        self.search.search_generation = self.search.search_generation.wrapping_add(1);
        self.search.search_generation_signal.store(self.search.search_generation, Ordering::Release);
        self.search.search_index = None;
        self.search.indexing_in_progress = false;
        self.search.index_started_at = None;
        self.search.index_progress.store(0, Ordering::Relaxed);
        self.search.index_total.store(0, Ordering::Relaxed);
    }

    pub fn toggle_search_picker_mode(&mut self) {
        let (new_mode, query) = if let SearchPickerState::Open { mode, query, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
            *mode = match *mode {
                SearchPickerMode::Files => SearchPickerMode::Content,
                SearchPickerMode::Content => SearchPickerMode::Files,
            };
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
                if let SearchPickerState::Open { content_results, hydrated_content_results, content_preview, hydration_key, search_in_progress, .. } = &mut self.search.search_picker {
                    *content_results = Vec::new();
                    *hydrated_content_results = Vec::new();
                    *content_preview = None;
                    *hydration_key = None;
                    *search_in_progress = false;
                }
                self.release_search_service();
                if query.is_empty() {
                    if let SearchPickerState::Open { file_results, .. } = &mut self.search.search_picker {
                        file_results.clear();
                    }
                } else {
                    let new_results = self.build_file_picker_results(&query);
                    if let SearchPickerState::Open { file_results, .. } = &mut self.search.search_picker {
                        *file_results = new_results;
                    }
                }
            }
        }
    }
    pub(super) fn build_file_picker_results(&self, query: &str) -> Vec<FilePickerResult> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<FilePickerResult> = self
            .vault
            .notes
            .iter()
            .enumerate()
            .filter_map(|(idx, note)| {
                let wiki_path = self.get_wiki_path_for_note(idx);
                let score = fuzzy_match(&note.title, query).or_else(|| wiki_path.as_ref().and_then(|p| fuzzy_match(p, query))).or_else(|| {
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
                Some(FilePickerResult { display_name: note.title.clone(), folder_hint, note_id: note.id, score })
            })
            .collect();
        results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.display_name.cmp(&b.display_name)));
        results
    }

    pub fn start_content_search(&mut self) {
        let query = if let SearchPickerState::Open { query, mode, .. } = &self.search.search_picker {
            if *mode != SearchPickerMode::Content || query.is_empty() {
                return;
            }
            query.clone()
        } else {
            return;
        };
        if !self.search.indexing_in_progress && self.search.search_index.is_none() {
            self.start_index_build();
        }
        self.search.next_search_id = self.search.next_search_id.wrapping_add(1);
        let search_id = self.search.next_search_id;
        if let SearchPickerState::Open { search_in_progress, search_id: state_search_id, hydrated_content_results, content_preview, hydration_key, .. } = &mut self.search.search_picker {
            *search_in_progress = true;
            *state_search_id = search_id;
            hydrated_content_results.clear();
            *content_preview = None;
            *hydration_key = None;
        }
        let sources = self.content_search_sources();
        let worker = self.workers.search.get_or_insert_with(SearchWorker::new);
        worker.submit(search_id, self.search.search_generation, query, sources, self.search.search_index.clone());
    }

    /// Polls for content search results (call in main loop)
    pub fn poll_content_search(&mut self) {
        let response = self.workers.search.as_ref().and_then(SearchWorker::try_take);
        if let Some(response) = response {
            if response.generation != self.search.search_generation {
                return;
            }
            if let SearchPickerState::Open { search_id, content_results, hydrated_content_results, content_preview, hydration_key, search_in_progress, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
                if response.query_id == *search_id {
                    *content_results = response.hits;
                    hydrated_content_results.clear();
                    *content_preview = None;
                    *hydration_key = None;
                    *search_in_progress = false;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
            }
        }
        self.ensure_search_hydrated();
    }

    pub fn is_content_search_in_progress(&self) -> bool {
        if let SearchPickerState::Open { search_in_progress, .. } = &self.search.search_picker {
            *search_in_progress
        } else {
            false
        }
    }

    pub fn update_search_picker_results(&mut self) {
        let (query, mode) = if let SearchPickerState::Open { query, mode, .. } = &self.search.search_picker {
            (query.clone(), *mode)
        } else {
            return;
        };
        match mode {
            SearchPickerMode::Files => {
                if query.is_empty() {
                    if let SearchPickerState::Open { file_results, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
                        file_results.clear();
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                } else {
                    let new_results = self.build_file_picker_results(&query);
                    if let SearchPickerState::Open { file_results, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
                        *file_results = new_results;
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                }
            }
            SearchPickerMode::Content => {
                if query.is_empty() {
                    if let SearchPickerState::Open { content_results, hydrated_content_results, content_preview, hydration_key, selected_index, scroll_offset, search_in_progress, .. } = &mut self.search.search_picker {
                        *content_results = Vec::new();
                        *hydrated_content_results = Vec::new();
                        *content_preview = None;
                        *hydration_key = None;
                        *selected_index = 0;
                        *scroll_offset = 0;
                        *search_in_progress = false;
                    }
                    self.release_search_service();
                } else {
                    self.start_content_search();
                }
            }
        }
    }

    pub fn select_search_picker_result(&mut self) {
        let result_info = if let SearchPickerState::Open { mode, file_results, content_results, selected_index, .. } = &self.search.search_picker {
            match mode {
                SearchPickerMode::Files => file_results.get(*selected_index).and_then(|result| self.note_index_for_id(result.note_id).map(|index| (index, None))),
                SearchPickerMode::Content => content_results.get(*selected_index).and_then(|result| self.note_index_for_id(result.note_id).map(|index| (index, Some(result.line_number as usize + 1)))),
            }
        } else {
            None
        };
        let Some((note_index, line_number)) = result_info else {
            self.close_search_picker();
            return;
        };
        if note_index < self.vault.notes.len() {
            if let Some(note) = self.vault.notes.get(note_index) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.state.config.notes_path();
                    let mut current = file_path.parent();
                    let mut needs_rebuild = false;
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        if !self.vault.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                            self.vault.folder_states.insert(parent.to_path_buf(), true);
                            needs_rebuild = true;
                        }
                        current = parent.parent();
                    }
                    if needs_rebuild {
                        Self::update_tree_expanded_states(&mut self.vault.file_tree, &self.vault.folder_states);
                        self.rebuild_sidebar_items();
                    }
                }
            }
            let target_id = self.vault.notes[note_index].id;
            for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_id } = &item.kind {
                    if *note_id == target_id {
                        if !self.load_note_body(target_id) {
                            break;
                        }
                        self.end_buffer_search();
                        self.vault.selected_sidebar_index = idx;
                        self.vault.selected_note = note_index;
                        self.push_navigation_history(note_index);
                        self.document.content_cursor = 0;
                        self.document.content_scroll_offset = 0;
                        self.update_content_items();
                        self.update_outline();
                        if let Some(target_line) = line_number {
                            let target_line_0indexed = target_line.saturating_sub(1);
                            let mut best_match_idx = 0;
                            let mut best_match_diff = usize::MAX;
                            for (i, source_line) in self.document.content_items.iter().map(ContentItem::source_line).enumerate() {
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
                            self.document.content_cursor = best_match_idx.min(self.document.content_items.len().saturating_sub(1));
                            let visible_height = 20usize; // Approximate visible lines
                            let target_scroll = self.document.content_cursor.saturating_sub(visible_height / 3);
                            self.document.content_scroll_offset = target_scroll;
                        }
                        self.state.focus = Focus::Content;
                        break;
                    }
                }
            }
        }
        self.close_search_picker();
    }

    pub fn search_picker_select_prev(&mut self) {
        const MAX_VISIBLE_FILES: usize = 10;
        const MAX_VISIBLE_CONTENT: usize = 18;
        if let SearchPickerState::Open { mode, file_results, content_results, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
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
        const MAX_VISIBLE_FILES: usize = 10;
        const MAX_VISIBLE_CONTENT: usize = 18;
        if let SearchPickerState::Open { mode, file_results, content_results, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
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
        if let SearchPickerState::Open { query, .. } = &mut self.search.search_picker {
            query.push(c);
        }
        self.update_search_picker_results();
    }

    pub fn search_picker_pop_char(&mut self) {
        if let SearchPickerState::Open { query, .. } = &mut self.search.search_picker {
            query.pop();
        }
        self.update_search_picker_results();
    }
    pub fn is_inside_search_picker(&self, x: u16, y: u16) -> bool {
        let area = self.search.search_picker_area;
        x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
    }
    /// Handle mouse click on search picker results
    pub fn search_picker_click(&mut self, x: u16, y: u16) -> u8 {
        let results_area = self.search.search_picker_results_area;
        if x < results_area.x || x >= results_area.x + results_area.width || y < results_area.y || y >= results_area.y + results_area.height {
            return 0;
        }
        let clicked_row = (y - results_area.y) as usize;
        if let SearchPickerState::Open { mode, file_results, content_results, selected_index, scroll_offset, .. } = &mut self.search.search_picker {
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
                let is_double_click = if let Some((last_time, last_index)) = self.search.search_picker_last_click { last_index == clicked_index && now.duration_since(last_time).as_millis() < 400 } else { false };
                self.search.search_picker_last_click = Some((now, clicked_index));
                return if is_double_click { 2 } else { 1 };
            }
        }
        0
    }

    pub fn search_picker_scroll_up(&mut self) {
        if let SearchPickerState::Open { scroll_offset, .. } = &mut self.search.search_picker {
            if *scroll_offset > 0 {
                *scroll_offset -= 1;
            }
        }
    }
    pub fn search_picker_scroll_down(&mut self) {
        const MAX_VISIBLE_FILES: usize = 10; // Must match POPUP_MAX_VISIBLE_ITEMS
        const MAX_VISIBLE_CONTENT: usize = 18; // Must match POPUP_MAX_VISIBLE_ITEMS_CONTENT
        if let SearchPickerState::Open { mode, file_results, content_results, scroll_offset, .. } = &mut self.search.search_picker {
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
