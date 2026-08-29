use super::*;

impl App {
    /// Resolve a wiki link target to a note index
    /// "note" -> searches all notes recursively for matching title (root first, then subfolders)
    /// "folder/note" -> searches for note in specific folder
    pub fn resolve_wiki_link(&self, target: &str) -> Option<usize> {
        if target.is_empty() {
            return None;
        }
        let notes_path = self.state.config.notes_path();
        if target.contains('/') {
            let expected_path = notes_path.join(format!("{}.md", target));
            let expected_str = expected_path.to_string_lossy();
            for (idx, note) in self.vault.notes.iter().enumerate() {
                if let Some(file_path) = &note.file_path {
                    if file_path.to_string_lossy() == expected_str {
                        return Some(idx);
                    }
                }
            }
        } else {
            for (idx, note) in self.vault.notes.iter().enumerate() {
                if note.title.eq_ignore_ascii_case(target) {
                    if let Some(file_path) = &note.file_path {
                        if file_path.parent() == Some(notes_path.as_path()) {
                            return Some(idx);
                        }
                    }
                }
            }
            for (idx, note) in self.vault.notes.iter().enumerate() {
                if note.title.eq_ignore_ascii_case(target) {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Check if a wiki link target exists
    pub fn wiki_link_exists(&self, target: &str) -> bool {
        self.resolve_wiki_link(target).is_some()
    }

    /// Check if cursor position is inside code (inline code or code block)
    pub fn is_cursor_in_code(&self, row: usize, col: usize) -> bool {
        let mut in_code_block = false;
        for line in self.editor.iter_lines().take(row) {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
            }
        }
        if let Some(current_line) = self.editor.line(row) {
            if current_line.trim_start().starts_with("```") {
                return true;
            }
        }
        if in_code_block {
            return true;
        }
        if let Some(line) = self.editor.line(row) {
            let mut chars = line.chars().peekable();
            let mut open_ticks = None;
            let mut current_col = 0;
            while current_col < col {
                let Some(ch) = chars.next() else {
                    break;
                };
                current_col += 1;
                if ch != '`' {
                    continue;
                }
                let mut count = 1;
                while chars.next_if_eq(&'`').is_some() {
                    count += 1;
                    current_col += 1;
                }
                if open_ticks == Some(count) {
                    open_ticks = None;
                } else if open_ticks.is_none() {
                    open_ticks = Some(count);
                }
            }
            if open_ticks.is_some() {
                return true;
            }
        }
        false
    }

    /// Check if cursor is inside an unclosed wikilink and return the current state
    /// Returns: Option<(note_query, heading_query, alias_query, mode)>
    /// - note_query: the part before # or |
    /// - heading_query: the part after # (if present)
    /// - alias_query: the part after | (if present)
    /// - mode: WikiAutocompleteMode indicating current position
    pub fn detect_unclosed_wikilink(&self, row: usize, col: usize) -> Option<(String, Option<String>, Option<String>, WikiAutocompleteMode)> {
        let line = self.editor.line(row)?;
        let cursor_byte = line.char_indices().nth(col).map_or(line.len(), |(index, _)| index);
        let prefix = &line[..cursor_byte];
        let open_byte = prefix.rfind("[[")?;
        if prefix.rfind("]]").is_some_and(|close| close > open_byte) {
            return None;
        }
        let start = line[..open_byte].chars().count() + 2;
        if self.is_cursor_in_code(row, start) {
            return None;
        }
        let content = &prefix[open_byte + 2..];
        if let Some(pipe_pos) = content.find('|') {
            let before_pipe = &content[..pipe_pos];
            let alias_query = content[pipe_pos + 1..].to_string();
            if let Some(hash_pos) = before_pipe.find('#') {
                let note_query = before_pipe[..hash_pos].to_string();
                let heading_query = before_pipe[hash_pos + 1..].to_string();
                Some((note_query, Some(heading_query), Some(alias_query), WikiAutocompleteMode::Alias))
            } else {
                Some((before_pipe.to_string(), None, Some(alias_query), WikiAutocompleteMode::Alias))
            }
        } else if let Some(hash_pos) = content.find('#') {
            let note_query = content[..hash_pos].to_string();
            let heading_query = content[hash_pos + 1..].to_string();
            Some((note_query, Some(heading_query), None, WikiAutocompleteMode::Heading))
        } else {
            Some((content.to_string(), None, None, WikiAutocompleteMode::Note))
        }
    }

    pub fn get_wiki_path_for_note(&self, note_idx: usize) -> Option<String> {
        let note = self.vault.notes.get(note_idx)?;
        let file_path = note.file_path.as_ref()?;
        let notes_path = self.state.config.notes_path();
        if let Ok(relative) = file_path.strip_prefix(&notes_path) {
            let path_str = relative.to_string_lossy();
            if let Some(stripped) = path_str.strip_suffix(".md") {
                return Some(stripped.to_string());
            }
        }
        Some(note.title.clone())
    }

    pub fn item_wiki_links_at(&self, index: usize) -> Vec<WikiLinkInfo> {
        let text = match self.document.content_items.get(index) {
            Some(ContentItem::TextLine { range, .. }) => self.document_slice(*range),
            Some(ContentItem::TaskItem { text, .. }) => self.document_slice(*text),
            _ => return Vec::new(),
        };
        self.extract_wiki_links_from_text(text)
    }

    pub fn extract_wiki_links_from_text(&self, text: &str) -> Vec<WikiLinkInfo> {
        use unicode_width::UnicodeWidthStr;
        ekphos_core::markdown::wiki_links(text)
            .into_iter()
            .map(|link| {
                let rendered_start = Self::calc_wiki_rendered_pos(text, link.range.start);
                let rendered_end = rendered_start + link.display_text().width();
                WikiLinkInfo { target: link.target.to_string(), heading: link.heading.map(str::to_string), display_text: link.alias.map(str::to_string), start_col: rendered_start, end_col: rendered_end, is_valid: self.wiki_link_exists(link.target) }
            })
            .collect()
    }
    pub(super) fn calc_wiki_rendered_pos(text: &str, target_pos: usize) -> usize {
        use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
        let mut rendered_pos = 0;
        let mut i = 0;
        while i < target_pos && i < text.len() {
            let remaining = &text[i..];
            if remaining.starts_with('$') {
                if let Some(math) = ekphos_core::markdown::inline_math_at(text, i) {
                    if math.range.end <= target_pos {
                        rendered_pos += math.source.width();
                        i = math.range.end;
                        continue;
                    }
                    break;
                }
            }
            if remaining.starts_with("!![") {
                if let Some(bracket_end) = remaining[2..].find("](") {
                    let after_bracket = &remaining[2 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[3..2 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 2 + bracket_end + 2 + paren_end + 1;
                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() { url.width() } else { alt_text.width() };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            if remaining.starts_with("![") {
                if let Some(bracket_end) = remaining[1..].find("](") {
                    let after_bracket = &remaining[1 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[2..1 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 1 + bracket_end + 2 + paren_end + 1;
                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() { 6 + url.width() + 1 } else { 6 + alt_text.width() + 1 };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            if let Some(wiki) = remaining.strip_prefix("[[") {
                if let Some(end_pos) = wiki.find("]]") {
                    let target = &wiki[..end_pos];
                    let full_link_len = 2 + end_pos + 2;
                    if i + full_link_len <= target_pos {
                        rendered_pos += target.width();
                        i += full_link_len;
                        continue;
                    } else {
                        break;
                    }
                }
            }
            if remaining.starts_with('[') {
                if let Some(bracket_end) = remaining.find("](") {
                    let after_bracket = &remaining[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &remaining[1..bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = bracket_end + 2 + paren_end + 1;
                        if i + full_link_len <= target_pos {
                            let display_len = if link_text.is_empty() { url.width() } else { link_text.width() };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            rendered_pos += remaining.chars().next().map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) }).unwrap_or(0);
            i += remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
        rendered_pos
    }

    pub fn current_wiki_link_target(&self) -> Option<String> {
        let wiki_links = self.item_wiki_links_at(self.document.content_cursor);
        wiki_links.get(self.document.selected_link_index).map(|info| info.target.clone())
    }

    pub fn navigate_to_wiki_link(&mut self, target: &str) -> bool {
        self.navigate_to_wiki_link_with_heading(target, None)
    }

    pub fn navigate_to_wiki_link_with_heading(&mut self, target: &str, heading: Option<&str>) -> bool {
        if let Some(note_idx) = self.resolve_wiki_link(target) {
            if let Some(note) = self.vault.notes.get(note_idx) {
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
            let target_id = self.vault.notes[note_idx].id;
            for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_id } = &item.kind {
                    if *note_id == target_id {
                        if !self.load_note_body(target_id) {
                            return false;
                        }
                        self.end_buffer_search();
                        self.vault.selected_sidebar_index = idx;
                        self.vault.selected_note = note_idx;
                        self.push_navigation_history(note_idx);
                        self.document.content_cursor = 0;
                        self.document.content_scroll_offset = 0;
                        self.document.selected_link_index = 0;
                        self.update_content_items();
                        self.update_outline();
                        if let Some(heading_text) = heading {
                            self.navigate_to_heading(heading_text);
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Navigate to a heading in the current note's content.
    ///
    /// Matches against the GitHub-style heading slug (lowercased, whitespace
    /// to dashes, punctuation stripped). Also handles `%`-encoded fragments.
    pub(super) fn navigate_to_heading(&mut self, heading: &str) {
        let decoded = percent_decode(heading);
        let target_slug = slugify_heading(&decoded);
        if target_slug.is_empty() {
            return;
        }
        for (idx, item) in self.document.content_items.iter().enumerate() {
            if let ContentItem::TextLine { range, .. } = item {
                let line = self.document_slice(*range);
                if let Some(title) = heading_text(line) {
                    if slugify_heading(title) == target_slug {
                        self.document.content_cursor = idx;
                        self.document.content_scroll_offset = idx.saturating_sub(2);
                        return;
                    }
                }
            }
        }
    }

    /// push a note to navigation history
    /// called when navigating to a new note
    pub fn push_navigation_history(&mut self, note_idx: usize) {
        let Some(note_id) = self.vault.notes.get(note_idx).map(|note| note.id) else {
            return;
        };
        if let Some(current) = self.document.navigation_history.get(self.document.navigation_index) {
            if current.note_id == note_id {
                return;
            }
        }
        if let Some(current) = self.document.navigation_history.get_mut(self.document.navigation_index) {
            current.content_cursor = self.document.content_cursor;
            current.content_scroll_offset = self.document.content_scroll_offset;
        }
        if self.document.navigation_index + 1 < self.document.navigation_history.len() {
            self.document.navigation_history.truncate(self.document.navigation_index + 1);
        }
        self.document.navigation_history.push(NavigationEntry { note_id, content_cursor: 0, content_scroll_offset: 0 });
        self.document.navigation_index = self.document.navigation_history.len().saturating_sub(1);
        const MAX_HISTORY: usize = 100;
        if self.document.navigation_history.len() > MAX_HISTORY {
            let remove_count = self.document.navigation_history.len() - MAX_HISTORY;
            self.document.navigation_history.drain(0..remove_count);
            self.document.navigation_index = self.document.navigation_index.saturating_sub(remove_count);
        }
    }

    pub fn navigate_back(&mut self) -> bool {
        if self.document.navigation_index == 0 || self.document.navigation_history.is_empty() {
            return false;
        }
        if let Some(current) = self.document.navigation_history.get_mut(self.document.navigation_index) {
            current.content_cursor = self.document.content_cursor;
            current.content_scroll_offset = self.document.content_scroll_offset;
        }
        self.document.navigation_index -= 1;
        if let Some(entry) = self.document.navigation_history.get(self.document.navigation_index).cloned() {
            if let Some(note_idx) = self.note_index_for_id(entry.note_id) {
                return self.go_to_note_without_history(note_idx, Some(entry.content_cursor), Some(entry.content_scroll_offset));
            }
        }
        false
    }

    /// navigate to next note in history
    pub fn navigate_forward(&mut self) -> bool {
        if self.document.navigation_index + 1 >= self.document.navigation_history.len() {
            return false;
        }
        if let Some(current) = self.document.navigation_history.get_mut(self.document.navigation_index) {
            current.content_cursor = self.document.content_cursor;
            current.content_scroll_offset = self.document.content_scroll_offset;
        }
        self.document.navigation_index += 1;
        if let Some(entry) = self.document.navigation_history.get(self.document.navigation_index).cloned() {
            if let Some(note_idx) = self.note_index_for_id(entry.note_id) {
                return self.go_to_note_without_history(note_idx, Some(entry.content_cursor), Some(entry.content_scroll_offset));
            }
        }
        false
    }

    /// Navigate directly to a note, expanding its sidebar ancestors as needed.
    /// Graph and search results use note indices, so activation must not depend
    /// on the note already being visible in the flattened sidebar.
    pub fn navigate_to_note(&mut self, note_idx: usize) -> bool {
        if note_idx >= self.vault.notes.len() {
            return false;
        }
        self.push_navigation_history(note_idx);
        self.go_to_note_without_history(note_idx, Some(0), Some(0))
    }

    /// go to a note without pushing to history used by back/forward to prevent infinite loop
    pub(super) fn go_to_note_without_history(&mut self, note_idx: usize, cursor: Option<usize>, scroll: Option<usize>) -> bool {
        if note_idx >= self.vault.notes.len() {
            return false;
        }
        if let Some(note) = self.vault.notes.get(note_idx) {
            if let Some(ref file_path) = note.file_path {
                let notes_root = self.state.config.notes_path();
                let mut current = file_path.parent();
                let mut needs_rebuild = false;
                while let Some(parent) = current {
                    let is_root = parent == notes_root;
                    let expanded_by_default = is_root;
                    if !self.vault.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(expanded_by_default) {
                        self.vault.folder_states.insert(parent.to_path_buf(), true);
                        needs_rebuild = true;
                    }
                    if is_root {
                        break;
                    }
                    current = parent.parent();
                }
                if needs_rebuild {
                    Self::update_tree_expanded_states(&mut self.vault.file_tree, &self.vault.folder_states);
                    self.rebuild_sidebar_items();
                }
            }
        }
        let target_id = self.vault.notes[note_idx].id;
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_id } = &item.kind {
                if *note_id == target_id {
                    if !self.load_note_body(target_id) {
                        return false;
                    }
                    self.end_buffer_search();
                    self.vault.selected_sidebar_index = idx;
                    self.vault.selected_note = note_idx;
                    self.document.selected_link_index = 0;
                    self.update_content_items();
                    self.update_outline();
                    let max_cursor = self.document.content_items.len().saturating_sub(1);
                    self.document.content_cursor = cursor.unwrap_or(0).min(max_cursor);
                    self.document.content_scroll_offset = scroll.unwrap_or(0).min(max_cursor);
                    return true;
                }
            }
        }
        false
    }

    pub fn can_navigate_back(&self) -> bool {
        self.document.navigation_index > 0 && !self.document.navigation_history.is_empty()
    }

    pub fn can_navigate_forward(&self) -> bool {
        self.document.navigation_index + 1 < self.document.navigation_history.len()
    }

    /// Build the vault-wide graph index off the UI thread. Results are tagged
    /// with a generation so a rapid reload can never install stale note IDs.
    pub fn build_wiki_suggestions(&self, query: &str) -> Vec<WikiSuggestion> {
        let mut suggestions = Vec::new();
        let notes_path = self.state.config.notes_path();
        let (folder_prefix, note_query) = if let Some(last_slash) = query.rfind('/') { (&query[..=last_slash], &query[last_slash + 1..]) } else { ("", query) };
        for (idx, note) in self.vault.notes.iter().enumerate() {
            if let Some(wiki_path) = self.get_wiki_path_for_note(idx) {
                if !folder_prefix.is_empty() && !wiki_path.to_lowercase().starts_with(&folder_prefix.to_lowercase()) {
                    continue;
                }
                if let Some(score) = fuzzy_match(&note.title, note_query) {
                    let folder_hint = wiki_path.rfind('/').map(|last_slash| wiki_path[..last_slash].to_string());
                    suggestions.push(WikiSuggestion { display_name: note.title.clone(), insert_text: note.title.clone(), is_folder: false, path: note.file_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(), score, folder_hint });
                }
            }
        }
        for item in &self.vault.sidebar_items {
            if let SidebarItemKind::Folder(folder) = &item.kind {
                if let Ok(relative) = folder.path.strip_prefix(&notes_path) {
                    let folder_path = relative.to_string_lossy().to_string();
                    if folder_path.is_empty() {
                        continue;
                    }
                    if !folder_prefix.is_empty() && !folder_path.to_lowercase().starts_with(folder_prefix.to_lowercase().trim_end_matches('/')) {
                        continue;
                    }
                    if let Some(score) = fuzzy_match(&item.display_name, note_query) {
                        suggestions.push(WikiSuggestion { display_name: item.display_name.clone(), insert_text: format!("{}/", folder_path), is_folder: true, path: folder.path.display().to_string(), score, folder_hint: None });
                    }
                }
            }
        }
        suggestions.sort_by(|a, b| match (a.is_folder, b.is_folder) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => b.score.cmp(&a.score).then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())),
        });
        suggestions
    }

    /// Build heading suggestions for a note target
    /// This extracts headings from the note's content and filters by query
    pub fn build_heading_suggestions(&self, note_target: &str, query: &str) -> Vec<WikiSuggestion> {
        let mut suggestions = Vec::new();
        for (idx, note) in self.vault.notes.iter().enumerate() {
            if let Some(wiki_path) = self.get_wiki_path_for_note(idx) {
                if wiki_path.to_lowercase() == note_target.to_lowercase() || note.title.to_lowercase() == note_target.to_lowercase() {
                    let body = if self.document.active_note_id == Some(note.id) { self.document.active_document.as_ref().map(DocumentSnapshot::body_arc) } else { self.vault.load_body(note.id).ok() };
                    let Some(body) = body else {
                        break;
                    };
                    for line in body.lines() {
                        let heading: Option<(usize, String)> = if line.starts_with("### ") {
                            Some((3, line.trim_start_matches("### ").to_string()))
                        } else if line.starts_with("## ") {
                            Some((2, line.trim_start_matches("## ").to_string()))
                        } else if line.starts_with("# ") {
                            Some((1, line.trim_start_matches("# ").to_string()))
                        } else {
                            None
                        };
                        if let Some((level, title)) = heading {
                            let score = if query.is_empty() {
                                1000
                            } else if let Some(s) = fuzzy_match(&title, query) {
                                s
                            } else {
                                continue;
                            };
                            let prefix = "  ".repeat(level.saturating_sub(1));
                            suggestions.push(WikiSuggestion {
                                display_name: format!("{}{}", prefix, title),
                                insert_text: title.clone(), // Just the heading text for insertion
                                is_folder: false,
                                path: format!("{}#{}", wiki_path, title),
                                score,
                                folder_hint: None,
                            });
                        }
                    }
                    break;
                }
            }
        }
        suggestions.sort_by(|a, b| b.score.cmp(&a.score));
        suggestions
    }

    pub fn create_note_from_wiki_target(&mut self, target: &str) -> bool {
        let relative = format!("{target}.md");
        let file_path = match self.confined_vault_relative_path(&relative) {
            Ok(path) => path,
            Err(error) => {
                self.show_error_toast(error);
                return false;
            }
        };
        if file_path.exists() {
            return false;
        }
        if let Some(parent) = file_path.parent() {
            if !parent.exists() && fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        let title = target.rsplit('/').next().unwrap_or(target);
        let content = format!("# {}\n\n", title);
        if ekphos_vault::save_note(&file_path, &content).is_err() {
            return false;
        }
        self.load_notes_from_dir();
        self.navigate_to_wiki_link(target)
    }

    pub fn open_current_image(&self) {
        if let Some(path) = self.current_item_is_image() {
            self.open_path_or_url(path);
        }
    }

    pub fn open_path_or_url(&self, path: &str) {
        let normalized = normalize_image_destination(path);
        let is_url = normalized.starts_with("http://") || normalized.starts_with("https://");
        let open_path = if is_url {
            normalized
        } else if let Some(resolved) = self.resolve_image_path(path) {
            resolved.to_string_lossy().to_string()
        } else {
            normalized
        };
        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(&open_path).spawn();
        #[cfg(any(target_os = "android", target_os = "freebsd", target_os = "linux"))]
        let _ = Command::new("xdg-open").arg(&open_path).spawn();
        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd").args(["/c", "start", "", &open_path]).spawn();
    }
}
