use super::*;

impl App {
    pub fn resolve_image_path(&self, path: &str) -> Option<PathBuf> {
        let normalized = normalize_image_destination(path);
        let path = normalized.as_str();

        if path.starts_with("http://") || path.starts_with("https://") {
            return Some(PathBuf::from(path));
        }

        let path_buf = if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path[2..])
            } else {
                PathBuf::from(path)
            }
        } else if path == "~" {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(path))
        } else {
            PathBuf::from(path)
        };

        if path_buf.is_absolute() && path_buf.exists() {
            return Some(path_buf);
        }

        if let Some(note) = self.current_note() {
            if let Some(ref file_path) = note.file_path {
                if let Some(note_dir) = file_path.parent() {
                    let resolved = note_dir.join(&path_buf);
                    if resolved.exists() {
                        return Some(resolved);
                    }
                }
            }
        }

        if path_buf.exists() {
            return Some(path_buf);
        }

        None
    }

    /// Poll the bounded image worker pool. Completed stale-document work is
    /// discarded by the service before it reaches application state.
    pub fn poll_pending_images(&mut self) -> bool {
        let changed = self.image_service.poll();
        if changed {
            self.trim_image_memory();
        }
        changed
    }

    pub fn cache_image(&mut self, key: &str, image: DynamicImage) {
        let _ = self.image_service.insert_ready(key, image);
        self.trim_image_memory();
    }

    pub fn get_cached_image(&mut self, key: &str) -> Option<DynamicImage> {
        self.image_service.load_cached_now(key)
    }

    pub fn is_image_cached(&self, key: &str) -> bool {
        self.image_service.is_cached_on_disk(key)
    }

    pub fn is_image_pending(&self, key: &str) -> bool {
        self.image_service.is_pending(key)
    }

    pub fn pending_image_count(&self) -> usize {
        self.image_service.stats().pending_requests
    }

    pub fn image_has_background_work(&self) -> bool {
        self.pending_image_count() > 0
    }

    pub fn start_remote_image_fetch(&mut self, url: &str) {
        self.image_service.request_remote(url, url);
    }

    pub(crate) fn request_image_load(&mut self, key: &str, resolved_path: Option<&std::path::Path>, remote_url: Option<&str>) {
        if let Some(url) = remote_url {
            self.image_service.request_remote(key, url);
        } else if let Some(path) = resolved_path {
            self.image_service.request_local(key, path.to_path_buf());
        }
    }

    pub(crate) fn decoded_image(&mut self, key: &str) -> Option<Arc<DynamicImage>> {
        self.image_service.decoded(key)
    }

    pub fn image_load_failed(&self, key: &str) -> bool {
        self.image_service.is_failed(key)
    }

    pub(crate) fn begin_image_frame(&mut self) {
        self.image_render_epoch = self.image_render_epoch.wrapping_add(1);
        if self.image_render_epoch == 0 {
            self.image_render_epoch = 1;
        }
    }

    pub(crate) fn finish_image_frame(&mut self) {
        let epoch = self.image_render_epoch;
        let generation = self.document_generation;
        self.image_states
            .retain(|_, state| state.last_visible_epoch == epoch && state.document_generation == generation);
        self.image_protocol_bytes = self.image_states.values().map(|state| state.source_bytes).sum();
        self.trim_image_memory();
    }

    pub(crate) fn touch_image_state(&mut self, key: &str, size: Size) -> bool {
        let Some(state) = self.image_states.get_mut(key) else {
            return false;
        };
        if state.size != size || state.document_generation != self.document_generation {
            self.remove_image_state(key);
            return false;
        }
        state.last_visible_epoch = self.image_render_epoch;
        true
    }

    pub(crate) fn remove_image_state(&mut self, key: &str) {
        if let Some(state) = self.image_states.remove(key) {
            self.image_protocol_bytes = self.image_protocol_bytes.saturating_sub(state.source_bytes);
        }
    }

    pub(crate) fn insert_image_state(&mut self, key: String, image: SlicedProtocol, size: Size, source_bytes: usize) {
        self.remove_image_state(&key);
        self.image_protocol_bytes = self.image_protocol_bytes.saturating_add(source_bytes);
        self.image_states.insert(
            key,
            ImageState {
                image,
                size,
                source_bytes,
                document_generation: self.document_generation,
                last_visible_epoch: self.image_render_epoch,
            },
        );
        self.trim_image_memory();
    }

    pub(crate) fn evict_document_services(&mut self) {
        self.image_states.clear();
        self.image_protocol_bytes = 0;
        self.image_service.begin_document(self.document_generation);
        self.syntax_service.clear_results();
    }

    fn trim_image_memory(&mut self) {
        const MAX_PROTOCOL_PLACEMENTS: usize = 64;
        let decoded_budget = crate::image_service::DEFAULT_IMAGE_MEMORY_BUDGET.saturating_sub(self.image_protocol_bytes);
        self.image_service.trim_to_budget(decoded_budget);

        while (self.image_protocol_bytes + self.image_service.decoded_bytes() > crate::image_service::DEFAULT_IMAGE_MEMORY_BUDGET
            || self.image_states.len() > MAX_PROTOCOL_PLACEMENTS)
            && self.image_states.len() > 1
        {
            let Some(oldest_key) = self
                .image_states
                .iter()
                .min_by_key(|(_, state)| state.last_visible_epoch)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove_image_state(&oldest_key);
        }
    }

    // ==================== Highlighter Lazy Loading ====================

    // Syntect syntax highlighter takes around extra 30mb of memory, which I think it should be considered
    // as quite bloated, the threshold of ekphos should be no more than 15mb if possible
    // but unfortunately still can't find a better syntax highlighter than syntect for now
    // I will enable this lazy load by default so markdown file without code syntax won't need to take extra 30mb of memory

    pub fn poll_highlighter(&mut self) -> bool {
        self.syntax_service.poll()
    }

    pub fn ensure_highlighter(&mut self) {
        self.syntax_service.ensure_loaded();
    }

    // Background Highlight Worker

    pub fn request_highlight_update(&mut self) {
        self.highlight_version += 1;
        self.highlight_pending = true;
        let rows = self.highlight_row_window();
        self.highlight_requested_rows = Some((rows.start, rows.end));

        if let Some(ref worker) = self.highlight_worker {
            let snapshot = self.editor.snapshot();
            let colors = self.get_highlight_colors();
            worker.request(snapshot, self.highlight_version, colors, rows);
        }
    }

    pub(super) fn highlight_row_window(&self) -> std::ops::Range<usize> {
        let active_rows = self.editor_view_height.max(40);
        let start = self.editor.scroll_offset().saturating_sub(active_rows);
        let end = self
            .editor
            .scroll_offset()
            .saturating_add(active_rows.saturating_mul(2))
            .min(self.editor.line_count());
        start..end
    }

    pub(super) fn get_highlight_colors(&self) -> HighlightColors {
        HighlightColors {
            heading_colors: [
                self.theme.editor.heading1,
                self.theme.editor.heading2,
                self.theme.editor.heading3,
                self.theme.editor.heading4,
                self.theme.editor.heading5,
                self.theme.editor.heading6,
            ],
            code_color: self.theme.editor.code,
            link_color: self.theme.editor.link,
            blockquote_color: self.theme.editor.blockquote,
            list_marker_color: self.theme.editor.list_marker,
            bold_color: Some(self.theme.editor.bold),
            italic_color: Some(self.theme.editor.italic),
            frontmatter_color: self.theme.content.frontmatter,
            details_color: self.theme.editor.link,               // Use link color for HTML details tags
            horizontal_rule_color: self.theme.editor.blockquote, // Use blockquote color for horizontal rules
        }
    }

    pub fn poll_highlight_worker(&mut self) -> bool {
        let result = if let Some(ref worker) = self.highlight_worker {
            worker.try_recv()
        } else {
            return false;
        };

        if let Some(result) = result {
            let applied = self.apply_highlight_result(result);
            if applied {
                self.highlight_pending = false;
            }
            applied
        } else {
            false
        }
    }

    pub(super) fn apply_highlight_result(&mut self, result: HighlightResult) -> bool {
        if result.version != self.highlight_version {
            return false;
        }

        self.editor.clear_highlights();
        self.editor.add_highlights(result.highlights);
        self.update_editor_wiki_links_with_ranges(&result.wiki_links);
        self.editor.invalidate_all_styles();
        true
    }

    pub(super) fn update_editor_wiki_links_with_ranges(&mut self, ranges: &[ekphos_editor::WikiLinkRange]) {
        if self.wiki_target_cache_generation != self.catalog_generation {
            let notes_path = self.config.notes_path();
            self.wiki_target_cache.clear();
            for note in &self.notes {
                if let Some(file_path) = &note.file_path {
                    if let Ok(relative) = file_path.strip_prefix(&notes_path) {
                        let path_str = relative.to_string_lossy();
                        if let Some(stripped) = path_str.strip_suffix(".md") {
                            self.wiki_target_cache.insert(stripped.to_string());
                            self.wiki_target_cache.insert(note.title.clone());
                            self.wiki_target_cache.insert(note.title.to_lowercase());
                        }
                    }
                }
            }
            self.wiki_target_cache_generation = self.catalog_generation;
        }

        let validated_ranges: Vec<ekphos_editor::WikiLinkRange> = ranges
            .iter()
            .map(|range| {
                // Extract target from the wiki link at this position
                let is_valid = self.validate_wiki_link_at(range.row, range.start_col, &self.wiki_target_cache);
                ekphos_editor::WikiLinkRange {
                    row: range.row,
                    start_col: range.start_col,
                    end_col: range.end_col,
                    is_valid,
                }
            })
            .collect();

        self.editor.set_wiki_link_ranges(validated_ranges);
    }

    pub(super) fn validate_wiki_link_at(&self, row: usize, start_col: usize, valid_targets: &HashSet<String>) -> bool {
        let line = match self.editor.line(row) {
            Some(line) => line,
            None => return false,
        };

        let byte_start = line.char_indices().nth(start_col).map_or(line.len(), |(index, _)| index);
        let Some(link) = ekphos_core::markdown::wiki_link_at(line, byte_start) else {
            return false;
        };
        valid_targets.contains(link.target) || (!link.target.contains('/') && valid_targets.contains(&link.target.to_lowercase()))
    }

    pub fn has_highlight_work(&self) -> bool {
        self.highlight_pending
    }

    pub fn get_highlighter(&self) -> Option<&Highlighter> {
        self.syntax_service.highlighter()
    }

    pub fn syntax_service_status(&self) -> crate::syntax_service::SyntaxServiceStatus {
        self.syntax_service.status()
    }

    pub fn syntax_service_failure(&self) -> Option<&str> {
        self.syntax_service.failure()
    }

    // ==================== Search Index ====================
}
