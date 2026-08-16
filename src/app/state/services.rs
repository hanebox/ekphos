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

    /// Find the content item index for a given source line.
    /// Returns the index of the content item that starts at or before the given line.
    pub fn poll_pending_images(&mut self) {
        while let Ok((url, img)) = self.image_receiver.try_recv() {
            self.pending_images.remove(&url);
            self.cache_image(&url, img);
        }
    }
    pub fn cache_image(&self, key: &str, img: DynamicImage) {
        let resized = resize_for_cache(img);
        let path = self.image_cache_dir.join(cache_key_to_filename(key));
        let _ = resized.save(&path);
    }
    pub fn get_cached_image(&self, key: &str) -> Option<DynamicImage> {
        let path = self.image_cache_dir.join(cache_key_to_filename(key));
        image::open(&path).ok()
    }
    pub fn is_image_cached(&self, key: &str) -> bool {
        let path = self.image_cache_dir.join(cache_key_to_filename(key));
        path.exists()
    }

    pub fn is_image_pending(&self, url: &str) -> bool {
        self.pending_images.contains(url)
    }

    pub fn start_remote_image_fetch(&mut self, url: &str) {
        if self.pending_images.contains(url) || self.is_image_cached(url) {
            return;
        }

        self.pending_images.insert(url.to_string());
        let url_owned = url.to_string();
        let sender = self.image_sender.clone();
        let service = Arc::clone(&self.dependencies.network_images);

        std::thread::spawn(move || {
            if let Some(img) = service.fetch(&url_owned) {
                let _ = sender.send((url_owned, img));
            }
        });
    }

    // ==================== Highlighter Lazy Loading ====================

    // Syntect syntax highlighter takes around extra 30mb of memory, which I think it should be considered
    // as quite bloated, the threshold of ekphos should be no more than 15mb if possible
    // but unfortunately still can't find a better syntax highlighter than syntect for now
    // I will enable this lazy load by default so markdown file without code syntax won't need to take extra 30mb of memory

    pub fn poll_highlighter(&mut self) {
        if let Ok(highlighter) = self.highlighter_receiver.try_recv() {
            self.highlighter = Some(highlighter);
            self.highlighter_loading = false;
        }
    }

    pub fn ensure_highlighter(&mut self) {
        if self.highlighter.is_some() || self.highlighter_loading {
            return;
        }

        self.highlighter_loading = true;
        let syntax_theme = self.config.syntax_theme.clone();
        let sender = self.highlighter_sender.clone();

        std::thread::spawn(move || {
            let highlighter = Highlighter::new(&syntax_theme);
            let _ = sender.send(highlighter);
        });
    }

    // Background Highlight Worker

    pub fn request_highlight_update(&mut self) {
        self.highlight_version += 1;
        self.highlight_pending = true;

        if let Some(ref worker) = self.highlight_worker {
            let content = self.editor.lines().join("\n");
            let colors = self.get_highlight_colors();
            worker.request(content, self.highlight_version, colors);
        }
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
        let notes_path = self.config.notes_path();
        let mut valid_targets: HashSet<String> = HashSet::new();

        for note in &self.notes {
            if let Some(file_path) = &note.file_path {
                if let Ok(relative) = file_path.strip_prefix(&notes_path) {
                    let path_str = relative.to_string_lossy();
                    if let Some(stripped) = path_str.strip_suffix(".md") {
                        valid_targets.insert(stripped.to_string());
                        valid_targets.insert(note.title.clone());
                        valid_targets.insert(note.title.to_lowercase());
                    }
                }
            }
        }

        let validated_ranges: Vec<ekphos_editor::WikiLinkRange> = ranges
            .iter()
            .map(|range| {
                // Extract target from the wiki link at this position
                let is_valid = self.validate_wiki_link_at(range.row, range.start_col, &valid_targets);
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
        let line = match self.editor.lines().get(row) {
            Some(l) => *l,
            None => return false,
        };

        let chars: Vec<char> = line.chars().collect();
        if start_col + 2 >= chars.len() {
            return false;
        }

        let after_open: String = chars[start_col + 2..].iter().collect();
        if let Some(end_pos) = after_open.find("]]") {
            let raw_content = &after_open[..end_pos];

            let content = if let Some(pipe_pos) = raw_content.find('|') {
                &raw_content[..pipe_pos]
            } else {
                raw_content
            };
            let target = if let Some(hash_pos) = content.find('#') {
                &content[..hash_pos]
            } else {
                content
            };

            if valid_targets.contains(target) {
                return true;
            }
            if !target.contains('/') {
                return valid_targets.contains(&target.to_lowercase());
            }
        }
        false
    }

    pub fn has_highlight_work(&self) -> bool {
        self.highlight_pending
    }

    pub fn get_highlighter(&self) -> Option<&Highlighter> {
        self.highlighter.as_ref()
    }

    // ==================== Search Index ====================
}
