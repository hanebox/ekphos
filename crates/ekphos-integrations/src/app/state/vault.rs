use super::*;

enum SidebarSelection {
    Folder(PathBuf),
    Note(NoteId),
}

struct WikiLinkUpdate {
    path: PathBuf,
    original: String,
    modified: String,
}

impl App {
    fn valid_entry_name(name: &str) -> bool {
        let mut components = std::path::Path::new(name).components();
        matches!(components.next(), Some(std::path::Component::Normal(_))) && components.next().is_none() && !name.contains(['/', '\\', '\0', '\n', '\r'])
    }

    pub(super) fn ensure_existing_path_in_vault(&self, path: &std::path::Path, allow_root: bool) -> Result<(), String> {
        let root = self.state.config.notes_path();
        if !allow_root && path == root {
            return Err("The vault root cannot be modified".to_string());
        }
        let canonical_root = root.canonicalize().map_err(|error| format!("Could not resolve vault root: {error}"))?;
        let canonical_path = path.canonicalize().map_err(|error| format!("Could not resolve path: {error}"))?;
        if canonical_path == canonical_root && !allow_root {
            return Err("The vault root cannot be modified".to_string());
        }
        if !canonical_path.starts_with(&canonical_root) {
            return Err("The selected path is outside the vault".to_string());
        }
        Ok(())
    }

    fn confined_child_path(&self, parent: &std::path::Path, name: &str, extension: Option<&str>) -> Result<PathBuf, String> {
        if !Self::valid_entry_name(name) {
            return Err("Names cannot contain path separators, traversal, or line breaks".to_string());
        }
        self.ensure_existing_path_in_vault(parent, true)?;
        let file_name = extension.map_or_else(|| name.to_string(), |extension| format!("{name}.{extension}"));
        Ok(parent.join(file_name))
    }

    pub(super) fn confined_vault_relative_path(&self, relative: &str) -> Result<PathBuf, String> {
        let relative = ekphos_core::VaultPath::new(relative).map_err(|_| "The path must stay inside the vault".to_string())?;
        let candidate = self.state.config.notes_path().join(relative.as_str());
        let mut existing = candidate.parent();
        while existing.is_some_and(|path| !path.exists()) {
            existing = existing.and_then(std::path::Path::parent);
        }
        let Some(existing) = existing else {
            return Err("Could not resolve the destination path".to_string());
        };
        self.ensure_existing_path_in_vault(existing, true)?;
        Ok(candidate)
    }

    pub fn selected_folder_is_vault_root(&self) -> bool {
        self.get_selected_folder_path().is_some_and(|path| path == self.state.config.notes_path())
    }

    pub(super) fn directory_has_notes(path: &std::path::Path) -> bool {
        ekphos_vault::contains_supported_document(path)
    }

    pub fn load_notes_from_dir(&mut self) {
        let notes_path = self.state.config.notes_path();
        if !notes_path.exists() {
            let _ = fs::create_dir_all(&notes_path);
        }
        let selected_note_id = self.current_note().map(|note| note.id);
        let selected_sidebar = self.vault.sidebar_items.get(self.vault.selected_sidebar_index).map(|item| match &item.kind {
            SidebarItemKind::Folder(folder) => SidebarSelection::Folder(folder.path.clone()),
            SidebarItemKind::Note { note_id } => SidebarSelection::Note(*note_id),
        });
        let selected_file_result = match &self.search.search_picker {
            SearchPickerState::Open { mode: SearchPickerMode::Files, file_results, selected_index, .. } => file_results.get(*selected_index).map(|result| result.note_id),
            _ => None,
        };
        let previous_fingerprints: HashMap<NoteId, ekphos_vault::FileFingerprint> = self.vault.notes.iter().filter_map(|note| self.vault.fingerprint(note.id).map(|fingerprint| (note.id, fingerprint))).collect();
        let (vault, catalog) = match ekphos_vault::Vault::scan(&notes_path) {
            Ok(result) => result,
            Err(error) => {
                self.show_error_toast(format!("Could not reload vault: {error}"));
                return;
            }
        };
        self.vault.replace(vault);
        self.vault.catalog_generation = self.vault.catalog_generation.wrapping_add(1);
        self.vault.body_cache.retain_valid(&self.vault.inner);
        let mut previous_notes: HashMap<NoteId, Note> = std::mem::take(&mut self.vault.notes).into_iter().map(|note| (note.id, note)).collect();
        self.vault.file_tree.clear();
        self.vault.file_tree = self.build_tree_from_catalog(catalog, 0);
        for note in &mut self.vault.notes {
            let unchanged = previous_fingerprints.get(&note.id).copied() == self.vault.inner.fingerprint(note.id);
            if unchanged {
                if let Some(previous) = previous_notes.remove(&note.id) {
                    *note = previous;
                }
            }
        }
        self.sort_tree();
        self.rebuild_sidebar_items();
        let restored_note_id = selected_note_id.filter(|id| self.note_index_for_id(*id).is_some());
        if let Some(note_id) = restored_note_id {
            self.vault.selected_note = self.note_index_for_id(note_id).unwrap_or(0);
        } else {
            self.vault.selected_note = 0;
        }
        self.vault.selected_sidebar_index = selected_sidebar.and_then(|selection| self.sidebar_index_for_selection(&selection)).or_else(|| restored_note_id.and_then(|id| self.sidebar_index_for_note_id(id))).unwrap_or(0);
        if restored_note_id.is_none() {
            if let Some(note_id) = self.vault.sidebar_items.iter().find_map(|item| match item.kind {
                SidebarItemKind::Note { note_id } => Some(note_id),
                SidebarItemKind::Folder(_) => None,
            }) {
                self.vault.selected_note = self.note_index_for_id(note_id).unwrap_or(0);
                self.vault.selected_sidebar_index = self.sidebar_index_for_note_id(note_id).unwrap_or(0);
            }
        }
        let _ = self.load_selected_note_body();
        self.update_content_items();
        self.update_outline();
        let current_ids: HashSet<NoteId> = self.vault.notes.iter().map(|note| note.id).collect();
        self.document.navigation_history.retain(|entry| current_ids.contains(&entry.note_id));
        self.document.navigation_index = self.document.navigation_index.min(self.document.navigation_history.len().saturating_sub(1));
        self.invalidate_graph_service();
        let picker = match &self.search.search_picker {
            SearchPickerState::Open { mode, query, .. } => Some((*mode, query.clone())),
            SearchPickerState::Closed => None,
        };
        self.release_search_service();
        if self.search.search_active && !self.search.search_query.is_empty() {
            self.update_filtered_indices();
        }
        match picker {
            Some((SearchPickerMode::Files, query)) => {
                let results = if query.is_empty() { Vec::new() } else { self.build_file_picker_results(&query) };
                if let SearchPickerState::Open { file_results, content_results, hydrated_content_results, content_preview, hydration_key, selected_index, scroll_offset, search_in_progress, .. } = &mut self.search.search_picker {
                    *selected_index = selected_file_result.and_then(|note_id| results.iter().position(|result| result.note_id == note_id)).unwrap_or(0).min(results.len().saturating_sub(1));
                    *scroll_offset = (*scroll_offset).min(*selected_index);
                    *file_results = results;
                    content_results.clear();
                    hydrated_content_results.clear();
                    *content_preview = None;
                    *hydration_key = None;
                    *search_in_progress = false;
                }
            }
            Some((SearchPickerMode::Content, query)) => {
                if let SearchPickerState::Open { content_results, hydrated_content_results, content_preview, hydration_key, selected_index, scroll_offset, search_in_progress, .. } = &mut self.search.search_picker {
                    content_results.clear();
                    hydrated_content_results.clear();
                    *content_preview = None;
                    *hydration_key = None;
                    *selected_index = 0;
                    *scroll_offset = 0;
                    *search_in_progress = false;
                }
                if !query.is_empty() {
                    self.start_content_search();
                }
            }
            None => {}
        }
    }
    fn build_tree_from_catalog(&mut self, entries: Vec<ekphos_vault::CatalogEntry>, depth: usize) -> Vec<FileTreeItem> {
        let mut items = Vec::new();
        for entry in entries {
            match entry {
                ekphos_vault::CatalogEntry::Folder(folder) => {
                    let folder = *folder;
                    let children = self.build_tree_from_catalog(folder.children, depth + 1);
                    if self.state.config.show_empty_dir || Self::tree_has_notes(&children) {
                        let expanded = self.vault.folder_states.get(&folder.absolute_path).copied().unwrap_or(false);
                        items.push(FileTreeItem::Folder(Box::new(FileTreeFolder { name: folder.name, path: folder.absolute_path, expanded, children, depth })));
                    }
                }
                ekphos_vault::CatalogEntry::Note(note) => {
                    let note = *note;
                    let frontmatter = note.has_frontmatter.then(|| note.metadata.frontmatter.into());
                    self.vault.notes.push(Note {
                        id: note.metadata.id,
                        kind: note.kind,
                        title: note.metadata.title,
                        file_path: Some(self.vault.root().join(note.metadata.path.as_str())),
                        file_size: note.metadata.file_size,
                        modified_time: note.modified_time,
                        created_time: note.created_time,
                        frontmatter,
                        content_start_line: note.content_start_line,
                    });
                    items.push(FileTreeItem::Note { note_id: note.metadata.id, depth });
                }
            }
        }
        items
    }
    pub fn note_index_for_id(&self, note_id: NoteId) -> Option<usize> {
        self.vault.notes.iter().position(|note| note.id == note_id)
    }

    #[doc(hidden)]
    pub fn current_body(&self) -> Option<&str> {
        let note_id = self.current_note().map(|note| note.id)?;
        (self.document.active_note_id == Some(note_id)).then(|| self.document.active_document.as_ref().map(DocumentSnapshot::body)).flatten()
    }
    pub(super) fn load_selected_note_body(&mut self) -> bool {
        let Some(note_id) = self.current_note().map(|note| note.id) else {
            self.document.active_note_id = None;
            self.document.active_fingerprint = None;
            self.document.active_document = None;
            return true;
        };
        self.load_note_body(note_id)
    }
    pub(super) fn load_note_body(&mut self, note_id: NoteId) -> bool {
        if self.document.active_note_id == Some(note_id) && self.document.active_document.is_some() && self.document.active_fingerprint == self.vault.fingerprint(note_id) && self.vault.validate(note_id).is_ok() {
            return true;
        }
        let request_generation = self.document.document_generation.wrapping_add(1);
        self.document.document_generation = request_generation;
        let body = match self.vault.body_cache.take_or_load(&self.vault.inner, note_id) {
            Ok(body) => body,
            Err(error) => {
                self.show_error_toast(format!("Could not load note: {error}"));
                return false;
            }
        };
        if self.document.document_generation != request_generation {
            return false;
        }
        if let (Some(old_id), Some(old_document)) = (self.document.active_note_id, self.document.active_document.take()) {
            if old_id != note_id {
                if let Some(fingerprint) = self.vault.fingerprint(old_id) {
                    self.vault.body_cache.insert(old_id, fingerprint, old_document.into_body());
                }
            }
        }
        self.document.active_note_id = Some(note_id);
        self.document.active_fingerprint = self.vault.fingerprint(note_id);
        self.document.active_document = Some(DocumentSnapshot::new(body));
        true
    }
    pub(crate) fn replace_active_body(&mut self, body: String) {
        let Some(note_id) = self.current_note().map(|note| note.id) else {
            return;
        };
        self.vault.body_cache.invalidate(note_id);
        self.document.active_note_id = Some(note_id);
        self.document.active_document = Some(DocumentSnapshot::new(Arc::<str>::from(body)));
        self.document.document_generation = self.document.document_generation.wrapping_add(1);
    }
    pub(crate) fn refresh_current_note_after_save(&mut self) {
        let Some(note_id) = self.current_note().map(|note| note.id) else {
            return;
        };
        self.vault.body_cache.invalidate(note_id);
        if let Ok(catalog_note) = self.vault.refresh_note(note_id) {
            self.document.active_fingerprint = Some(catalog_note.fingerprint);
            if let Some(index) = self.note_index_for_id(note_id) {
                let note = &mut self.vault.notes[index];
                note.file_size = catalog_note.metadata.file_size;
                note.modified_time = catalog_note.modified_time;
                note.created_time = catalog_note.created_time;
                note.frontmatter = catalog_note.has_frontmatter.then(|| catalog_note.metadata.frontmatter.into());
                note.content_start_line = catalog_note.content_start_line;
            }
        }
    }
    pub(crate) fn persist_active_body(&mut self, body: String) -> bool {
        let Some((note_id, path)) = self.current_note().and_then(|note| note.file_path.clone().map(|path| (note.id, path))) else {
            return false;
        };
        if let Err(error) = ekphos_vault::save_note(&path, &body) {
            self.show_error_toast(format!("Could not save note: {error}"));
            return false;
        }
        self.vault.body_cache.invalidate(note_id);
        self.replace_active_body(body);
        self.refresh_current_note_after_save();
        self.invalidate_graph_service();
        if let SearchPickerState::Open { content_results, hydrated_content_results, content_preview, hydration_key, search_in_progress, .. } = &mut self.search.search_picker {
            *content_results = Vec::new();
            *hydrated_content_results = Vec::new();
            *content_preview = None;
            *hydration_key = None;
            *search_in_progress = false;
        }
        self.release_search_service();
        true
    }
    fn sidebar_index_for_note_id(&self, note_id: NoteId) -> Option<usize> {
        self.vault.sidebar_items.iter().position(|item| matches!(item.kind, SidebarItemKind::Note { note_id: id } if id == note_id))
    }
    fn sidebar_index_for_selection(&self, selection: &SidebarSelection) -> Option<usize> {
        self.vault.sidebar_items.iter().position(|item| match (&item.kind, selection) {
            (SidebarItemKind::Folder(folder), SidebarSelection::Folder(selected)) => &folder.path == selected,
            (SidebarItemKind::Note { note_id }, SidebarSelection::Note(selected)) => note_id == selected,
            _ => false,
        })
    }
    pub(super) fn tree_has_notes(items: &[FileTreeItem]) -> bool {
        items.iter().any(|item| match item {
            FileTreeItem::Note { .. } => true,
            FileTreeItem::Folder(folder) => Self::tree_has_notes(&folder.children),
        })
    }
    pub(super) fn sort_tree(&mut self) {
        let sort_mode = self.vault.sort_mode;
        let folders_first = self.state.config.folders_first;
        Self::sort_tree_items(&mut self.vault.file_tree, &self.vault.notes, sort_mode, folders_first);
    }
    pub(super) fn sort_tree_items(items: &mut [FileTreeItem], notes: &[Note], sort_mode: SortMode, folders_first: bool) {
        items.sort_by(|a, b| {
            if folders_first {
                let is_folder_a = matches!(a, FileTreeItem::Folder(_));
                let is_folder_b = matches!(b, FileTreeItem::Folder(_));
                match (is_folder_a, is_folder_b) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }
            Self::compare_items(a, b, notes, sort_mode)
        });
        for item in items.iter_mut() {
            if let FileTreeItem::Folder(folder) = item {
                Self::sort_tree_items(&mut folder.children, notes, sort_mode, folders_first);
            }
        }
    }
    pub(super) fn compare_items(a: &FileTreeItem, b: &FileTreeItem, notes: &[Note], sort_mode: SortMode) -> std::cmp::Ordering {
        match sort_mode {
            SortMode::NameAsc => {
                let name_a = Self::get_tree_item_name(a, notes);
                let name_b = Self::get_tree_item_name(b, notes);
                name_a.to_lowercase().cmp(&name_b.to_lowercase())
            }
            SortMode::NameDesc => {
                let name_a = Self::get_tree_item_name(a, notes);
                let name_b = Self::get_tree_item_name(b, notes);
                name_b.to_lowercase().cmp(&name_a.to_lowercase())
            }
            SortMode::ModifiedOldest => {
                let time_a = Self::get_tree_item_modified(a, notes);
                let time_b = Self::get_tree_item_modified(b, notes);
                time_a.cmp(&time_b)
            }
            SortMode::ModifiedNewest => {
                let time_a = Self::get_tree_item_modified(a, notes);
                let time_b = Self::get_tree_item_modified(b, notes);
                time_b.cmp(&time_a)
            }
            SortMode::CreatedOldest => {
                let time_a = Self::get_tree_item_created(a, notes);
                let time_b = Self::get_tree_item_created(b, notes);
                time_a.cmp(&time_b)
            }
            SortMode::CreatedNewest => {
                let time_a = Self::get_tree_item_created(a, notes);
                let time_b = Self::get_tree_item_created(b, notes);
                time_b.cmp(&time_a)
            }
        }
    }
    pub(super) fn get_tree_item_name<'b>(item: &'b FileTreeItem, notes: &'b [Note]) -> &'b str {
        match item {
            FileTreeItem::Folder(folder) => &folder.name,
            FileTreeItem::Note { note_id, .. } => notes.iter().find(|note| note.id == *note_id).map(|note| note.title.as_str()).unwrap_or(""),
        }
    }
    pub(super) fn get_tree_item_modified(item: &FileTreeItem, notes: &[Note]) -> Option<std::time::SystemTime> {
        match item {
            FileTreeItem::Folder(folder) => fs::metadata(&folder.path).ok().and_then(|m| m.modified().ok()),
            FileTreeItem::Note { note_id, .. } => notes.iter().find(|note| note.id == *note_id).and_then(|note| note.modified_time),
        }
    }
    pub(super) fn get_tree_item_created(item: &FileTreeItem, notes: &[Note]) -> Option<std::time::SystemTime> {
        match item {
            FileTreeItem::Folder(folder) => fs::metadata(&folder.path).ok().and_then(|m| m.created().ok()),
            FileTreeItem::Note { note_id, .. } => notes.iter().find(|note| note.id == *note_id).and_then(|note| note.created_time),
        }
    }

    pub fn cycle_sort_mode(&mut self) {
        self.vault.sort_mode = self.vault.sort_mode.next();
        self.sort_tree();
        self.rebuild_sidebar_items();
    }

    pub fn rebuild_sidebar_items(&mut self) {
        self.vault.sidebar_items.clear();
        let notes_path = self.state.config.notes_path();
        let root_name = notes_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Notes".to_string());
        let root_expanded = self.vault.folder_states.get(&notes_path).copied().unwrap_or(true); // Root expanded by default
        self.vault.sidebar_items.push(SidebarItem { kind: SidebarItemKind::Folder(Box::new(SidebarFolder { path: notes_path, expanded: root_expanded })), depth: 0, display_name: root_name });
        if root_expanded {
            let tree_clone = self.vault.file_tree.clone();
            self.flatten_tree_into_sidebar(&tree_clone, 1); // Start at depth 1
        }
    }
    pub(super) fn flatten_tree_into_sidebar(&mut self, items: &[FileTreeItem], depth_offset: usize) {
        for item in items {
            match item {
                FileTreeItem::Folder(folder) => {
                    self.vault.sidebar_items.push(SidebarItem { kind: SidebarItemKind::Folder(Box::new(SidebarFolder { path: folder.path.clone(), expanded: folder.expanded })), depth: folder.depth + depth_offset, display_name: folder.name.clone() });
                    if folder.expanded {
                        self.flatten_tree_into_sidebar(&folder.children, depth_offset);
                    }
                }
                FileTreeItem::Note { note_id, depth } => {
                    if self.vault.notes.iter().any(|note| note.id == *note_id) {
                        self.vault.sidebar_items.push(SidebarItem { kind: SidebarItemKind::Note { note_id: *note_id }, depth: *depth + depth_offset, display_name: String::new() });
                    }
                }
            }
        }
    }

    pub fn sync_selected_note_from_sidebar(&mut self) {
        let note_id = self.vault.sidebar_items.get(self.vault.selected_sidebar_index).and_then(|item| if let SidebarItemKind::Note { note_id } = &item.kind { Some(*note_id) } else { None });
        if let Some(note_id) = note_id {
            let Some(new_note_idx) = self.note_index_for_id(note_id) else {
                return;
            };
            if self.vault.selected_note != new_note_idx {
                if !self.load_note_body(note_id) {
                    self.select_current_note_in_sidebar();
                    return;
                }
                self.end_buffer_search();
            } else if !self.load_note_body(note_id) {
                return;
            }
            self.vault.selected_note = new_note_idx;
            self.evict_document_services();
        }
    }

    /// find and select the current note in the sidebar after re sorting
    pub(super) fn select_current_note_in_sidebar(&mut self) {
        let Some(selected_id) = self.current_note().map(|note| note.id) else {
            return;
        };
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_id } = &item.kind {
                if *note_id == selected_id {
                    self.vault.selected_sidebar_index = idx;
                    return;
                }
            }
        }
    }

    pub fn create_note(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            self.state.dialog_error = Some("Note name cannot be empty".to_string());
            return false;
        }
        let parent_path = self.vault.target_folder.clone().unwrap_or_else(|| self.state.config.notes_path());
        let file_path = match self.confined_child_path(&parent_path, name, Some("md")) {
            Ok(path) => path,
            Err(error) => {
                self.state.dialog_error = Some(error);
                return false;
            }
        };
        if file_path.exists() {
            self.state.dialog_error = Some(format!("Note '{name}' already exists"));
            return false;
        }
        let content = format!("# {}\n\n", name);
        if let Err(error) = ekphos_vault::save_note(&file_path, &content) {
            self.state.dialog_error = Some(format!("Failed to create note: {error}"));
            return false;
        }
        if let Some(ref folder_path) = self.vault.target_folder {
            self.vault.folder_states.insert(folder_path.clone(), true);
        }
        self.load_notes_from_dir();
        let name_owned = name.to_string();
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_id } = &item.kind {
                if self.vault.notes.iter().any(|note| note.id == *note_id && note.title == name_owned) {
                    self.vault.selected_sidebar_index = idx;
                    self.vault.selected_note = self.note_index_for_id(*note_id).unwrap_or(0);
                    break;
                }
            }
        }
        let _ = self.load_selected_note_body();
        self.update_content_items();
        self.update_outline();
        self.state.focus = Focus::Content;
        self.state.dialog_error = None;
        self.vault.target_folder = None;
        true
    }

    pub fn create_folder(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let parent_path = self.vault.target_folder.clone().unwrap_or_else(|| self.state.config.notes_path());
        let folder_path = match self.confined_child_path(&parent_path, name, None) {
            Ok(path) => path,
            Err(error) => {
                self.state.dialog_error = Some(error);
                return false;
            }
        };
        if folder_path.exists() {
            self.state.dialog_error = Some(format!("Folder '{}' already exists", name));
            return false;
        }
        if fs::create_dir(&folder_path).is_ok() {
            self.vault.target_folder = Some(folder_path);
            self.state.dialog_error = None;
            true
        } else {
            self.state.dialog_error = Some("Failed to create folder".to_string());
            false
        }
    }

    pub fn get_current_context_folder(&self) -> Option<PathBuf> {
        if let Some(item) = self.vault.sidebar_items.get(self.vault.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder(folder) => Some(folder.path.clone()),
                SidebarItemKind::Note { note_id } => {
                    if let Some(note) = self.vault.notes.iter().find(|note| note.id == *note_id) {
                        if let Some(ref file_path) = note.file_path {
                            return file_path.parent().map(|p| p.to_path_buf());
                        }
                    }
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_selected_folder_path(&self) -> Option<PathBuf> {
        if let Some(item) = self.vault.sidebar_items.get(self.vault.selected_sidebar_index) {
            if let SidebarItemKind::Folder(folder) = &item.kind {
                return Some(folder.path.clone());
            }
        }
        None
    }

    pub fn get_selected_folder_name(&self) -> Option<String> {
        if let Some(item) = self.vault.sidebar_items.get(self.vault.selected_sidebar_index) {
            if let SidebarItemKind::Folder(_) = &item.kind {
                return Some(item.display_name.clone());
            }
        }
        None
    }

    pub fn delete_current_note(&mut self) -> bool {
        let path = self.vault.sidebar_items.get(self.vault.selected_sidebar_index).and_then(|item| match item.kind {
            SidebarItemKind::Note { note_id } => self.vault.notes.iter().find(|note| note.id == note_id).and_then(|note| note.file_path.clone()),
            SidebarItemKind::Folder(_) => None,
        });
        let Some(path) = path else {
            return false;
        };
        if let Err(error) = self.ensure_existing_path_in_vault(&path, false) {
            self.show_error_toast(error);
            return false;
        }
        if let Err(error) = fs::remove_file(&path) {
            self.show_error_toast(format!("Could not delete note: {error}"));
            return false;
        }
        self.load_notes_from_dir();
        if self.vault.selected_sidebar_index >= self.vault.sidebar_items.len() {
            self.vault.selected_sidebar_index = self.vault.sidebar_items.len().saturating_sub(1);
        }
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
        true
    }

    pub fn delete_current_folder(&mut self) -> bool {
        let Some(path) = self.get_selected_folder_path() else {
            return false;
        };
        if let Err(error) = self.ensure_existing_path_in_vault(&path, false) {
            self.show_error_toast(error);
            return false;
        }
        if let Err(error) = fs::remove_dir_all(&path) {
            self.show_error_toast(format!("Could not delete folder: {error}"));
            return false;
        }
        self.vault.folder_states.remove(&path);
        self.load_notes_from_dir();
        if self.vault.selected_sidebar_index >= self.vault.sidebar_items.len() {
            self.vault.selected_sidebar_index = self.vault.sidebar_items.len().saturating_sub(1);
        }
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
        true
    }

    pub fn rename_note(&mut self, new_name: &str) -> bool {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            self.state.dialog_error = Some("Note name cannot be empty".to_string());
            return false;
        }
        let note_id = self.vault.sidebar_items.get(self.vault.selected_sidebar_index).and_then(|item| match item.kind {
            SidebarItemKind::Note { note_id } => Some(note_id),
            SidebarItemKind::Folder(_) => None,
        });
        let Some(note_index) = note_id.and_then(|note_id| self.note_index_for_id(note_id)) else {
            return false;
        };
        let old_title = self.vault.notes[note_index].title.clone();
        let extension = self.vault.notes[note_index].kind.extension();
        if old_title == new_name {
            self.state.dialog_error = None;
            return true;
        }
        let Some(old_path) = self.vault.notes[note_index].file_path.clone() else {
            return false;
        };
        if let Err(error) = self.ensure_existing_path_in_vault(&old_path, false) {
            self.state.dialog_error = Some(error);
            return false;
        }
        let Some(parent) = old_path.parent() else {
            return false;
        };
        let new_file_path = match self.confined_child_path(parent, new_name, Some(extension)) {
            Ok(path) => path,
            Err(error) => {
                self.state.dialog_error = Some(error);
                return false;
            }
        };
        if new_file_path.exists() {
            self.state.dialog_error = Some(format!("Note '{new_name}' already exists"));
            return false;
        }
        let notes_root = self.state.config.notes_path();
        let old_wiki = Self::calculate_wiki_path(&old_path, &notes_root);
        let new_wiki = Self::calculate_wiki_path(&new_file_path, &notes_root);
        if let Err(error) = fs::rename(&old_path, &new_file_path) {
            self.state.dialog_error = Some(format!("Failed to rename note: {error}"));
            return false;
        }
        if let Err(error) = self.update_wiki_links_after_moves(&[(old_wiki, new_wiki, old_title)]) {
            let rollback = fs::rename(&new_file_path, &old_path).err().map(|rollback| format!("; rollback failed: {rollback}"));
            self.state.dialog_error = Some(format!("Failed to update wiki links: {error}{}", rollback.unwrap_or_default()));
            return false;
        }
        self.load_notes_from_dir();
        let new_name_owned = new_name.to_string();
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_id } = &item.kind {
                if self.vault.notes.iter().any(|note| note.id == *note_id && note.title == new_name_owned) {
                    self.vault.selected_sidebar_index = idx;
                    self.vault.selected_note = self.note_index_for_id(*note_id).unwrap_or(0);
                    break;
                }
            }
        }
        let _ = self.load_selected_note_body();
        self.update_content_items();
        self.update_outline();
        self.state.dialog_error = None;
        true
    }

    pub fn rename_folder(&mut self, new_name: &str) -> bool {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            self.state.dialog_error = Some("Folder name cannot be empty".to_string());
            return false;
        }
        let Some(old_path) = self.get_selected_folder_path() else {
            return false;
        };
        if let Err(error) = self.ensure_existing_path_in_vault(&old_path, false) {
            self.state.dialog_error = Some(error);
            return false;
        }
        let old_name = old_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if old_name == new_name {
            self.state.dialog_error = None;
            return true;
        }
        let Some(parent) = old_path.parent() else {
            return false;
        };
        let new_path = match self.confined_child_path(parent, new_name, None) {
            Ok(path) => path,
            Err(error) => {
                self.state.dialog_error = Some(error);
                return false;
            }
        };
        if new_path.exists() {
            self.state.dialog_error = Some(format!("Folder '{}' already exists", new_name));
            return false;
        }
        let notes_root = self.state.config.notes_path();
        let mut moves = Vec::new();
        for note in &self.vault.notes {
            if let Some(file_path) = note.file_path.as_ref().filter(|path| path.starts_with(&old_path)) {
                let old_wiki = Self::calculate_wiki_path(file_path, &notes_root);
                let relative = file_path.strip_prefix(&old_path).unwrap_or(file_path);
                let new_wiki = Self::calculate_wiki_path(&new_path.join(relative), &notes_root);
                moves.push((old_wiki, new_wiki, note.title.clone()));
            }
        }
        if let Err(error) = fs::rename(&old_path, &new_path) {
            self.state.dialog_error = Some(format!("Failed to rename folder: {error}"));
            return false;
        }
        if let Err(error) = self.update_wiki_links_after_moves(&moves) {
            let rollback = fs::rename(&new_path, &old_path).err().map(|rollback| format!("; rollback failed: {rollback}"));
            self.state.dialog_error = Some(format!("Failed to update wiki links: {error}{}", rollback.unwrap_or_default()));
            return false;
        }
        if let Some(expanded) = self.vault.folder_states.remove(&old_path) {
            self.vault.folder_states.insert(new_path.clone(), expanded);
        }
        self.load_notes_from_dir();
        let new_name_owned = new_name.to_string();
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Folder(folder) = &item.kind {
                if folder.path == new_path {
                    self.vault.selected_sidebar_index = idx;
                    break;
                }
            }
            if item.display_name == new_name_owned && matches!(item.kind, SidebarItemKind::Folder(_)) {
                self.vault.selected_sidebar_index = idx;
                break;
            }
        }
        self.update_content_items();
        self.update_outline();
        self.state.dialog_error = None;
        true
    }
    pub fn cut_selected_item(&mut self) -> bool {
        if let Some(item) = self.vault.sidebar_items.get(self.vault.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Note { note_id } => {
                    if let Some(note) = self.vault.notes.iter().find(|note| note.id == *note_id) {
                        if let Some(ref path) = note.file_path {
                            if let Err(error) = self.ensure_existing_path_in_vault(path, false) {
                                self.state.status_message = Some(error);
                                return false;
                            }
                            self.vault.cut_buffer = Some(CutItem::Note { source_path: path.clone(), title: note.title.clone() });
                            self.state.status_message = Some(format!("Cut: {}", note.title));
                            return true;
                        }
                    }
                }
                SidebarItemKind::Folder(folder) => {
                    if let Err(error) = self.ensure_existing_path_in_vault(&folder.path, false) {
                        self.state.status_message = Some(error);
                        return false;
                    }
                    let name = item.display_name.clone();
                    self.vault.cut_buffer = Some(CutItem::Folder { source_path: folder.path.clone(), name: name.clone() });
                    self.state.status_message = Some(format!("Cut: {}/", name));
                    return true;
                }
            }
        }
        false
    }
    pub fn clear_cut_buffer(&mut self) {
        if self.vault.cut_buffer.is_some() {
            self.vault.cut_buffer = None;
            self.state.status_message = Some("Cut cancelled".to_string());
        }
    }
    pub fn paste_cut_item(&mut self) -> Result<(), String> {
        let cut_item = match self.vault.cut_buffer.clone() {
            Some(item) => item,
            None => return Err("Nothing to paste".to_string()),
        };
        let dest_folder = self.get_paste_destination_folder();
        let result = match cut_item {
            CutItem::Note { source_path, title } => self.move_note(&source_path, &dest_folder, &title),
            CutItem::Folder { source_path, name } => self.move_folder(&source_path, &dest_folder, &name),
        };
        if result.is_ok() {
            self.vault.cut_buffer = None;
        }
        result
    }
    pub(super) fn get_paste_destination_folder(&self) -> PathBuf {
        if let Some(item) = self.vault.sidebar_items.get(self.vault.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder(folder) => {
                    return folder.path.clone();
                }
                SidebarItemKind::Note { note_id } => {
                    if let Some(note) = self.vault.notes.iter().find(|note| note.id == *note_id) {
                        if let Some(ref file_path) = note.file_path {
                            if let Some(parent) = file_path.parent() {
                                return parent.to_path_buf();
                            }
                        }
                    }
                }
            }
        }
        self.state.config.notes_path()
    }
    pub(super) fn move_note(&mut self, source: &std::path::Path, dest_folder: &std::path::Path, title: &str) -> Result<(), String> {
        if !source.exists() {
            return Err("Source file no longer exists".to_string());
        }
        self.ensure_existing_path_in_vault(source, false)?;
        let extension = source.extension().and_then(|extension| extension.to_str()).and_then(|extension| match extension {
            "md" | "base" | "canvas" => Some(extension),
            _ => None,
        });
        let Some(extension) = extension else {
            return Err("Unsupported document type".to_string());
        };
        let dest_path = self.confined_child_path(dest_folder, title, Some(extension))?;
        if source == dest_path {
            return Err("Already in this location".to_string());
        }
        if source.parent() == Some(dest_folder) {
            return Err("Already in this location".to_string());
        }
        if dest_path.exists() {
            return Err(format!("'{}' already exists in destination", title));
        }
        let notes_root = self.state.config.notes_path();
        let old_wiki_path = Self::calculate_wiki_path(source, &notes_root);
        let new_wiki_path = Self::calculate_wiki_path(&dest_path, &notes_root);
        fs::rename(source, &dest_path).map_err(|e| format!("Failed to move file: {}", e))?;
        if let Err(error) = self.update_wiki_links_after_moves(&[(old_wiki_path, new_wiki_path, title.to_string())]) {
            let rollback = fs::rename(&dest_path, source).err().map(|rollback| format!("; rollback failed: {rollback}")).unwrap_or_default();
            return Err(format!("Failed to update wiki links: {error}{rollback}"));
        }
        self.load_notes_from_dir();
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_id } = &item.kind {
                if let Some(note) = self.vault.notes.iter().find(|note| note.id == *note_id) {
                    if note.file_path.as_ref() == Some(&dest_path) {
                        self.vault.selected_sidebar_index = idx;
                        self.vault.selected_note = self.note_index_for_id(*note_id).unwrap_or(0);
                        break;
                    }
                }
            }
        }
        let _ = self.load_selected_note_body();
        self.update_content_items();
        self.update_outline();
        self.state.status_message = Some(format!("Moved: {}", title));
        Ok(())
    }
    pub(super) fn move_folder(&mut self, source: &std::path::Path, dest_folder: &std::path::Path, name: &str) -> Result<(), String> {
        if !source.exists() {
            return Err("Source folder no longer exists".to_string());
        }
        self.ensure_existing_path_in_vault(source, false)?;
        let dest_path = self.confined_child_path(dest_folder, name, None)?;
        if dest_folder.starts_with(source) {
            return Err("Cannot move folder into itself".to_string());
        }
        if source == dest_path {
            return Err("Already in this location".to_string());
        }
        if source.parent() == Some(dest_folder) {
            return Err("Already in this location".to_string());
        }
        if dest_path.exists() {
            return Err(format!("Folder '{}' already exists in destination", name));
        }
        let notes_root = self.state.config.notes_path();
        let mut old_new_paths: Vec<(String, String, String)> = Vec::new(); // (old_wiki, new_wiki, title)
        for note in &self.vault.notes {
            if let Some(ref file_path) = note.file_path {
                if file_path.starts_with(source) {
                    let old_wiki = Self::calculate_wiki_path(file_path, &notes_root);
                    let relative = file_path.strip_prefix(source).unwrap_or(file_path.as_path());
                    let new_file_path = dest_path.join(relative);
                    let new_wiki = Self::calculate_wiki_path(&new_file_path, &notes_root);
                    old_new_paths.push((old_wiki, new_wiki, note.title.clone()));
                }
            }
        }
        fs::rename(source, &dest_path).map_err(|e| format!("Failed to move folder: {}", e))?;
        if let Err(error) = self.update_wiki_links_after_moves(&old_new_paths) {
            let rollback = fs::rename(&dest_path, source).err().map(|rollback| format!("; rollback failed: {rollback}")).unwrap_or_default();
            return Err(format!("Failed to update wiki links: {error}{rollback}"));
        }
        let keys_to_update: Vec<PathBuf> = self.vault.folder_states.keys().filter(|k| k.starts_with(source)).cloned().collect();
        for old_key in keys_to_update {
            if let Some(expanded) = self.vault.folder_states.remove(&old_key) {
                let relative = old_key.strip_prefix(source).unwrap_or(&old_key);
                let new_key = dest_path.join(relative);
                self.vault.folder_states.insert(new_key, expanded);
            }
        }
        self.load_notes_from_dir();
        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Folder(folder) = &item.kind {
                if folder.path == dest_path {
                    self.vault.selected_sidebar_index = idx;
                    break;
                }
            }
        }
        self.update_content_items();
        self.update_outline();
        self.state.status_message = Some(format!("Moved: {}/", name));
        Ok(())
    }
    pub(super) fn update_wiki_links_after_moves(&self, moves: &[(String, String, String)]) -> Result<(), String> {
        if moves.is_empty() {
            return Ok(());
        }
        let notes_root = self.state.config.notes_path();
        let md_files = Self::collect_markdown_files(&notes_root);
        let mut updates = Vec::new();
        for file_path in md_files {
            let content = fs::read_to_string(&file_path).map_err(|error| format!("Could not read {}: {error}", file_path.display()))?;
            let modified_content = self.replace_wiki_links_in_content(&content, moves);
            if modified_content != content {
                updates.push(WikiLinkUpdate { path: file_path, original: content, modified: modified_content });
            }
        }
        for (applied, update) in updates.iter().enumerate() {
            if let Err(error) = ekphos_vault::save_note(&update.path, &update.modified) {
                let mut rollback_errors = Vec::new();
                for previous in updates[..applied].iter().rev() {
                    if let Err(rollback) = ekphos_vault::save_note(&previous.path, &previous.original) {
                        rollback_errors.push(format!("{}: {rollback}", previous.path.display()));
                    }
                }
                let rollback = if rollback_errors.is_empty() { String::new() } else { format!("; rollback failed for {}", rollback_errors.join(", ")) };
                return Err(format!("Could not update {}: {error}{rollback}", update.path.display()));
            }
        }
        Ok(())
    }
    pub(super) fn collect_markdown_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
                    continue;
                }
                if file_type.is_dir() {
                    files.extend(Self::collect_markdown_files(&path));
                } else if file_type.is_file() && path.extension().map(|ext| ext == "md").unwrap_or(false) {
                    files.push(path);
                }
            }
        }
        files
    }
    pub(super) fn replace_wiki_links_in_content(&self, content: &str, moves: &[(String, String, String)]) -> String {
        let mut line_offsets = Vec::new();
        let mut offset = 0usize;
        for line in content.split_inclusive('\n') {
            line_offsets.push(offset);
            offset = offset.saturating_add(line.len());
        }
        let mut replacements = Vec::new();
        let frontmatter_end = ekphos_core::markdown::frontmatter_end(content);
        ekphos_core::markdown::visit_document_wiki_links_with_tilde_fences(content, frontmatter_end, true, |located| {
            let Some(line_offset) = line_offsets.get(located.row).copied() else {
                return;
            };
            let target_lower = located.link.target.to_lowercase();
            let replacement = moves.iter().find(|(old_path, _, old_title)| target_lower == old_path.to_lowercase() || target_lower == old_title.to_lowercase());
            let Some((_, new_path, _)) = replacement else {
                return;
            };
            let start = line_offset.saturating_add(located.link.range.start).saturating_add(2);
            let end = start.saturating_add(located.link.target.len());
            if start <= end && end <= content.len() {
                replacements.push((start..end, new_path.clone()));
            }
        });
        if replacements.is_empty() {
            return content.to_string();
        }
        let mut result = String::with_capacity(content.len());
        let mut cursor = 0usize;
        for (range, replacement) in replacements {
            if range.start < cursor {
                continue;
            }
            result.push_str(&content[cursor..range.start]);
            result.push_str(&replacement);
            cursor = range.end;
        }
        result.push_str(&content[cursor..]);
        result
    }
    pub(super) fn calculate_wiki_path(file_path: &std::path::Path, notes_root: &std::path::Path) -> String {
        if let Ok(relative) = file_path.strip_prefix(notes_root) {
            let path_str = relative.to_string_lossy();
            if let Some(stripped) = path_str.strip_suffix(".md") {
                return stripped.to_string();
            }
            path_str.to_string()
        } else {
            file_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
        }
    }

    pub fn complete_onboarding(&mut self) {
        self.state.config.notes_dir = self.state.input_buffer.clone();
        let _ = self.state.config.save_to_dir(&self.dependencies.config_dir);
        let notes_path = self.state.config.notes_path();
        let _ = fs::create_dir_all(&notes_path);
        let _ = fs::write(notes_path.join("01-Getting Started.md"), GETTING_STARTED_CONTENT);
        let _ = fs::write(notes_path.join("02-Demo Note.md"), DEMO_NOTE_CONTENT);
        self.state.dialog = DialogState::None;
        self.load_notes_from_dir();
        self.state.show_welcome = true;
        self.state.needs_full_clear = true;
    }

    /// Create the notes directory when it doesn't exist
    pub fn create_notes_directory(&mut self) {
        let notes_path = self.state.config.notes_path();
        if fs::create_dir_all(&notes_path).is_ok() {
            self.load_notes_from_dir();
            if self.vault.notes.is_empty() {
                self.state.dialog = DialogState::EmptyDirectory;
            } else {
                self.state.dialog = DialogState::None;
            }
        }
    }

    pub fn dismiss_welcome(&mut self) {
        self.state.show_welcome = false;
    }
}
