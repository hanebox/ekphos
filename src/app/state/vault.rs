use super::*;

impl App {
    pub(super) fn directory_has_notes(path: &PathBuf) -> bool {
        ekphos_vault::contains_markdown(path)
    }

    pub fn load_notes_from_dir(&mut self) {
        self.notes.clear();
        self.file_tree.clear();
        let notes_path = self.config.notes_path();

        if !notes_path.exists() {
            let _ = fs::create_dir_all(&notes_path);
        }

        let catalog = ekphos_vault::scan(&notes_path).unwrap_or_default();
        self.file_tree = self.build_tree_from_catalog(catalog, 0);

        // Sort the tree according to current sort mode
        self.sort_tree();

        self.rebuild_sidebar_items();

        self.selected_sidebar_index = 0;
        self.sync_selected_note_from_sidebar();

        self.update_content_items();
        self.update_outline();
        self.start_graph_index_build();
    }

    fn build_tree_from_catalog(&mut self, entries: Vec<ekphos_vault::CatalogEntry>, depth: usize) -> Vec<FileTreeItem> {
        let mut items = Vec::new();
        for entry in entries {
            match entry {
                ekphos_vault::CatalogEntry::Folder(folder) => {
                    let children = self.build_tree_from_catalog(folder.children, depth + 1);
                    if self.config.show_empty_dir || Self::tree_has_notes(&children) {
                        let expanded = self.folder_states.get(&folder.absolute_path).copied().unwrap_or(false);
                        items.push(FileTreeItem::Folder {
                            name: folder.name,
                            path: folder.absolute_path,
                            expanded,
                            children,
                            depth,
                        });
                    }
                }
                ekphos_vault::CatalogEntry::Note(note) => {
                    let note = *note;
                    let note_index = self.notes.len();
                    self.notes.push(Note {
                        id: note.metadata.id,
                        title: note.metadata.title,
                        content: note.content,
                        file_path: Some(note.absolute_path),
                        modified_time: note.modified_time,
                        created_time: note.created_time,
                        frontmatter: note.frontmatter,
                        content_start_line: note.content_start_line,
                    });
                    items.push(FileTreeItem::Note { note_index, depth });
                }
            }
        }
        items
    }

    pub(super) fn tree_has_notes(items: &[FileTreeItem]) -> bool {
        items.iter().any(|item| match item {
            FileTreeItem::Note { .. } => true,
            FileTreeItem::Folder { children, .. } => Self::tree_has_notes(children),
        })
    }

    pub(super) fn sort_tree(&mut self) {
        let sort_mode = self.sort_mode;
        let folders_first = self.config.folders_first;
        Self::sort_tree_items(&mut self.file_tree, &self.notes, sort_mode, folders_first);
    }

    pub(super) fn sort_tree_items(items: &mut [FileTreeItem], notes: &[Note], sort_mode: SortMode, folders_first: bool) {
        items.sort_by(|a, b| {
            if folders_first {
                let is_folder_a = matches!(a, FileTreeItem::Folder { .. });
                let is_folder_b = matches!(b, FileTreeItem::Folder { .. });

                match (is_folder_a, is_folder_b) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }
            Self::compare_items(a, b, notes, sort_mode)
        });

        for item in items.iter_mut() {
            if let FileTreeItem::Folder { children, .. } = item {
                Self::sort_tree_items(children, notes, sort_mode, folders_first);
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
            FileTreeItem::Folder { name, .. } => name,
            FileTreeItem::Note { note_index, .. } => &notes[*note_index].title,
        }
    }

    pub(super) fn get_tree_item_modified(item: &FileTreeItem, notes: &[Note]) -> Option<std::time::SystemTime> {
        match item {
            FileTreeItem::Folder { path, .. } => fs::metadata(path).ok().and_then(|m| m.modified().ok()),
            FileTreeItem::Note { note_index, .. } => notes[*note_index].modified_time,
        }
    }

    pub(super) fn get_tree_item_created(item: &FileTreeItem, notes: &[Note]) -> Option<std::time::SystemTime> {
        match item {
            FileTreeItem::Folder { path, .. } => fs::metadata(path).ok().and_then(|m| m.created().ok()),
            FileTreeItem::Note { note_index, .. } => notes[*note_index].created_time,
        }
    }

    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.sort_tree();
        self.rebuild_sidebar_items();
    }

    pub fn rebuild_sidebar_items(&mut self) {
        self.sidebar_items.clear();

        // Add root folder first
        let notes_path = self.config.notes_path();
        let root_name = notes_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Notes".to_string());

        let root_expanded = self.folder_states.get(&notes_path).copied().unwrap_or(true); // Root expanded by default

        self.sidebar_items.push(SidebarItem {
            kind: SidebarItemKind::Folder {
                path: notes_path,
                expanded: root_expanded,
            },
            depth: 0,
            display_name: root_name,
        });

        // Only add children if root is expanded
        if root_expanded {
            let tree_clone = self.file_tree.clone();
            self.flatten_tree_into_sidebar(&tree_clone, 1); // Start at depth 1
        }
    }

    pub(super) fn flatten_tree_into_sidebar(&mut self, items: &[FileTreeItem], depth_offset: usize) {
        for item in items {
            match item {
                FileTreeItem::Folder {
                    name,
                    path,
                    expanded,
                    children,
                    depth,
                } => {
                    self.sidebar_items.push(SidebarItem {
                        kind: SidebarItemKind::Folder {
                            path: path.clone(),
                            expanded: *expanded,
                        },
                        depth: *depth + depth_offset,
                        display_name: name.clone(),
                    });

                    if *expanded {
                        self.flatten_tree_into_sidebar(children, depth_offset);
                    }
                }
                FileTreeItem::Note { note_index, depth } => {
                    self.sidebar_items.push(SidebarItem {
                        kind: SidebarItemKind::Note { note_index: *note_index },
                        depth: *depth + depth_offset,
                        display_name: self.notes[*note_index].title.clone(),
                    });
                }
            }
        }
    }

    pub fn sync_selected_note_from_sidebar(&mut self) {
        let note_index = self.sidebar_items.get(self.selected_sidebar_index).and_then(|item| {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                Some(*note_index)
            } else {
                None
            }
        });

        if let Some(new_note_idx) = note_index {
            if self.selected_note != new_note_idx {
                self.end_buffer_search();
            }
            self.selected_note = new_note_idx;
            self.image_states.clear();
        }
    }

    /// find and select the current note in the sidebar after re sorting
    pub(super) fn select_current_note_in_sidebar(&mut self) {
        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if *note_index == self.selected_note {
                    self.selected_sidebar_index = idx;
                    return;
                }
            }
        }
    }

    pub fn create_note(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }

        let parent_path = self.target_folder.clone().unwrap_or_else(|| self.config.notes_path());
        let file_path = parent_path.join(format!("{}.md", name));

        // Don't overwrite existing files
        if file_path.exists() {
            return;
        }

        let content = format!("# {}\n\n", name);
        if fs::write(&file_path, &content).is_ok() {
            if let Some(ref folder_path) = self.target_folder {
                self.folder_states.insert(folder_path.clone(), true);
            }

            self.load_notes_from_dir();

            let name_owned = name.to_string();
            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    if self.notes[*note_index].title == name_owned {
                        self.selected_sidebar_index = idx;
                        self.selected_note = *note_index;
                        break;
                    }
                }
            }

            self.update_content_items();
            self.update_outline();
            self.focus = Focus::Content;
        }

        self.target_folder = None;
    }

    pub fn create_folder(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }

        let parent_path = self.target_folder.clone().unwrap_or_else(|| self.config.notes_path());
        let folder_path = parent_path.join(name);

        if folder_path.exists() {
            self.dialog_error = Some(format!("Folder '{}' already exists", name));
            return false;
        }

        if fs::create_dir(&folder_path).is_ok() {
            self.target_folder = Some(folder_path);
            self.dialog_error = None;
            true
        } else {
            self.dialog_error = Some("Failed to create folder".to_string());
            false
        }
    }

    pub fn get_current_context_folder(&self) -> Option<PathBuf> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => Some(path.clone()),
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
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
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Folder { path, .. } = &item.kind {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn get_selected_folder_name(&self) -> Option<String> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Folder { .. } = &item.kind {
                return Some(item.display_name.clone());
            }
        }
        None
    }

    pub fn delete_current_note(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if let Some(ref path) = self.notes[*note_index].file_path {
                    let _ = fs::remove_file(path);
                }

                self.load_notes_from_dir();

                if self.selected_sidebar_index >= self.sidebar_items.len() {
                    self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
                }
                self.sync_selected_note_from_sidebar();

                self.update_content_items();
                self.update_outline();
            }
        }
    }

    pub fn delete_current_folder(&mut self) {
        if let Some(path) = self.get_selected_folder_path() {
            if fs::remove_dir_all(&path).is_ok() {
                self.folder_states.remove(&path);

                self.load_notes_from_dir();

                if self.selected_sidebar_index >= self.sidebar_items.len() {
                    self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
                }
                self.sync_selected_note_from_sidebar();

                self.update_content_items();
                self.update_outline();
            }
        }
    }

    pub fn rename_note(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                let note_index = *note_index;

                if self.notes[note_index].title == new_name {
                    return;
                }

                let new_file_path = if let Some(ref old_path) = self.notes[note_index].file_path {
                    if let Some(parent) = old_path.parent() {
                        parent.join(format!("{}.md", new_name))
                    } else {
                        return;
                    }
                } else {
                    return;
                };

                if new_file_path.exists() {
                    return;
                }

                if let Some(ref old_path) = self.notes[note_index].file_path {
                    if fs::rename(old_path, &new_file_path).is_ok() {
                        self.load_notes_from_dir();

                        let new_name_owned = new_name.to_string();
                        for (idx, item) in self.sidebar_items.iter().enumerate() {
                            if let SidebarItemKind::Note { note_index } = &item.kind {
                                if self.notes[*note_index].title == new_name_owned {
                                    self.selected_sidebar_index = idx;
                                    self.selected_note = *note_index;
                                    break;
                                }
                            }
                        }

                        self.update_content_items();
                        self.update_outline();
                    }
                }
            }
        }
    }

    pub fn rename_folder(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if let Some(old_path) = self.get_selected_folder_path() {
            let old_name = old_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

            if old_name == new_name {
                return;
            }

            if let Some(parent) = old_path.parent() {
                let new_path = parent.join(new_name);

                if new_path.exists() {
                    self.dialog_error = Some(format!("Folder '{}' already exists", new_name));
                    return;
                }

                if fs::rename(&old_path, &new_path).is_ok() {
                    if let Some(expanded) = self.folder_states.remove(&old_path) {
                        self.folder_states.insert(new_path.clone(), expanded);
                    }

                    self.load_notes_from_dir();

                    let new_name_owned = new_name.to_string();
                    for (idx, item) in self.sidebar_items.iter().enumerate() {
                        if let SidebarItemKind::Folder { path, .. } = &item.kind {
                            if path == &new_path {
                                self.selected_sidebar_index = idx;
                                break;
                            }
                        }
                        if item.display_name == new_name_owned {
                            if let SidebarItemKind::Folder { .. } = &item.kind {
                                self.selected_sidebar_index = idx;
                                break;
                            }
                        }
                    }

                    self.update_content_items();
                    self.update_outline();
                }
            }
        }
    }

    // ==================== Cut/Paste/Move Operations ====================
    pub fn cut_selected_item(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
                        if let Some(ref path) = note.file_path {
                            self.cut_buffer = Some(CutItem::Note {
                                source_path: path.clone(),
                                title: note.title.clone(),
                            });
                            self.status_message = Some(format!("Cut: {}", note.title));
                        }
                    }
                }
                SidebarItemKind::Folder { path, .. } => {
                    let name = item.display_name.clone();
                    self.cut_buffer = Some(CutItem::Folder {
                        source_path: path.clone(),
                        name: name.clone(),
                    });
                    self.status_message = Some(format!("Cut: {}/", name));
                }
            }
        }
    }
    pub fn clear_cut_buffer(&mut self) {
        if self.cut_buffer.is_some() {
            self.cut_buffer = None;
            self.status_message = Some("Cut cancelled".to_string());
        }
    }
    pub fn paste_cut_item(&mut self) -> Result<(), String> {
        let cut_item = match self.cut_buffer.take() {
            Some(item) => item,
            None => return Err("Nothing to paste".to_string()),
        };
        let dest_folder = self.get_paste_destination_folder();

        match cut_item {
            CutItem::Note { source_path, title } => self.move_note(&source_path, &dest_folder, &title),
            CutItem::Folder { source_path, name } => self.move_folder(&source_path, &dest_folder, &name),
        }
    }
    pub(super) fn get_paste_destination_folder(&self) -> PathBuf {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => {
                    return path.clone();
                }
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
                        if let Some(ref file_path) = note.file_path {
                            if let Some(parent) = file_path.parent() {
                                return parent.to_path_buf();
                            }
                        }
                    }
                }
            }
        }
        self.config.notes_path()
    }
    pub(super) fn move_note(&mut self, source: &std::path::Path, dest_folder: &std::path::Path, title: &str) -> Result<(), String> {
        if !source.exists() {
            return Err("Source file no longer exists".to_string());
        }
        let dest_path = dest_folder.join(format!("{}.md", title));
        if source == &dest_path {
            return Err("Already in this location".to_string());
        }
        if source.parent() == Some(dest_folder) {
            return Err("Already in this location".to_string());
        }
        if dest_path.exists() {
            return Err(format!("'{}' already exists in destination", title));
        }
        let notes_root = self.config.notes_path();
        let old_wiki_path = Self::calculate_wiki_path(source, &notes_root);
        let new_wiki_path = Self::calculate_wiki_path(&dest_path, &notes_root);
        fs::rename(source, &dest_path).map_err(|e| format!("Failed to move file: {}", e))?;
        self.update_wiki_links_after_move(&old_wiki_path, &new_wiki_path, title);
        self.load_notes_from_dir();
        self.start_index_build();
        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if let Some(note) = self.notes.get(*note_index) {
                    if note.file_path.as_ref() == Some(&dest_path) {
                        self.selected_sidebar_index = idx;
                        self.selected_note = *note_index;
                        break;
                    }
                }
            }
        }
        self.update_content_items();
        self.update_outline();
        self.status_message = Some(format!("Moved: {}", title));

        Ok(())
    }
    pub(super) fn move_folder(&mut self, source: &std::path::Path, dest_folder: &std::path::Path, name: &str) -> Result<(), String> {
        if !source.exists() {
            return Err("Source folder no longer exists".to_string());
        }
        let dest_path = dest_folder.join(name);
        if dest_folder.starts_with(source) {
            return Err("Cannot move folder into itself".to_string());
        }
        if source == &dest_path {
            return Err("Already in this location".to_string());
        }
        if source.parent() == Some(dest_folder) {
            return Err("Already in this location".to_string());
        }
        if dest_path.exists() {
            return Err(format!("Folder '{}' already exists in destination", name));
        }

        let notes_root = self.config.notes_path();
        let mut old_new_paths: Vec<(String, String, String)> = Vec::new(); // (old_wiki, new_wiki, title)

        for note in &self.notes {
            if let Some(ref file_path) = note.file_path {
                if file_path.starts_with(source) {
                    let old_wiki = Self::calculate_wiki_path(file_path, &notes_root);
                    // Calculate new path by replacing source prefix with dest
                    let relative = file_path.strip_prefix(source).unwrap_or(file_path.as_path());
                    let new_file_path = dest_path.join(relative);
                    let new_wiki = Self::calculate_wiki_path(&new_file_path, &notes_root);
                    old_new_paths.push((old_wiki, new_wiki, note.title.clone()));
                }
            }
        }

        fs::rename(source, &dest_path).map_err(|e| format!("Failed to move folder: {}", e))?;

        let keys_to_update: Vec<PathBuf> = self.folder_states.keys().filter(|k| k.starts_with(source)).cloned().collect();

        for old_key in keys_to_update {
            if let Some(expanded) = self.folder_states.remove(&old_key) {
                let relative = old_key.strip_prefix(source).unwrap_or(&old_key);
                let new_key = dest_path.join(relative);
                self.folder_states.insert(new_key, expanded);
            }
        }

        for (old_wiki, new_wiki, title) in old_new_paths {
            self.update_wiki_links_after_move(&old_wiki, &new_wiki, &title);
        }

        self.load_notes_from_dir();
        self.start_index_build();
        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Folder { path, .. } = &item.kind {
                if path == &dest_path {
                    self.selected_sidebar_index = idx;
                    break;
                }
            }
        }

        self.update_content_items();
        self.update_outline();
        self.status_message = Some(format!("Moved: {}/", name));

        Ok(())
    }

    pub(super) fn update_wiki_links_after_move(&mut self, old_path: &str, new_path: &str, title: &str) {
        let notes_root = self.config.notes_path();
        let md_files = Self::collect_markdown_files(&notes_root);

        for file_path in md_files {
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let modified_content = self.replace_wiki_links_in_content(&content, old_path, new_path, title);

            if modified_content != content {
                let _ = fs::write(&file_path, modified_content);
            }
        }
    }
    pub(super) fn collect_markdown_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(Self::collect_markdown_files(&path));
                } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                    files.push(path);
                }
            }
        }
        files
    }

    pub(super) fn replace_wiki_links_in_content(&self, content: &str, old_path: &str, new_path: &str, old_title: &str) -> String {
        let mut result = String::new();
        let mut remaining = content;

        while let Some(start) = remaining.find("[[") {
            result.push_str(&remaining[..start]);
            remaining = &remaining[start + 2..];

            if let Some(end) = remaining.find("]]") {
                let link_content = &remaining[..end];

                let (target, suffix) = if let Some(hash_pos) = link_content.find('#') {
                    (&link_content[..hash_pos], &link_content[hash_pos..])
                } else if let Some(pipe_pos) = link_content.find('|') {
                    (&link_content[..pipe_pos], &link_content[pipe_pos..])
                } else {
                    (link_content, "")
                };

                let target_lower = target.to_lowercase();
                let old_path_lower = old_path.to_lowercase();
                let old_title_lower = old_title.to_lowercase();

                let should_replace = target_lower == old_path_lower || target_lower == old_title_lower;

                if should_replace {
                    let new_target = if new_path.contains('/') {
                        new_path.to_string()
                    } else {
                        old_title.to_string()
                    };
                    result.push_str("[[");
                    result.push_str(&new_target);
                    result.push_str(suffix);
                    result.push_str("]]");
                } else {
                    // Keep original
                    result.push_str("[[");
                    result.push_str(link_content);
                    result.push_str("]]");
                }

                remaining = &remaining[end + 2..];
            } else {
                result.push_str("[[");
            }
        }

        result.push_str(remaining);
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
        // 1. Save config
        self.config.notes_dir = self.input_buffer.clone();
        let _ = self.config.save_to_dir(&self.dependencies.config_dir);

        let notes_path = self.config.notes_path();
        let _ = fs::create_dir_all(&notes_path);

        let _ = fs::write(notes_path.join("01-Getting Started.md"), GETTING_STARTED_CONTENT);
        let _ = fs::write(notes_path.join("02-Demo Note.md"), DEMO_NOTE_CONTENT);
        self.dialog = DialogState::None;
        self.load_notes_from_dir();

        self.show_welcome = true;
        self.needs_full_clear = true;
    }

    /// Create the notes directory when it doesn't exist
    pub fn create_notes_directory(&mut self) {
        let notes_path = self.config.notes_path();
        if fs::create_dir_all(&notes_path).is_ok() {
            self.load_notes_from_dir();
            // Show empty directory dialog since we just created an empty directory
            if self.notes.is_empty() {
                self.dialog = DialogState::EmptyDirectory;
            } else {
                self.dialog = DialogState::None;
            }
        }
    }

    pub fn dismiss_welcome(&mut self) {
        self.show_welcome = false;
    }
}
