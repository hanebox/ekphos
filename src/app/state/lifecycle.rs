use super::*;

impl App {
    pub fn new() -> Self {
        // Check if config exists before loading (determines if onboarding is needed)
        // This must be checked before load_or_create() which creates the config
        let config_exists = Config::exists();
        let config = Config::load_or_create();
        AppBuilder::configured(config, !config_exists).build()
    }

    /// Create a new App instance with an optional initial path.
    /// If the path is a directory, it becomes the notes directory.
    /// If the path is a file, its parent becomes the notes directory and the file is selected.
    pub fn new_with_path(initial_path: Option<PathBuf>) -> Self {
        let initial_path = match initial_path {
            Some(path) => path,
            None => return Self::new(),
        };
        let (notes_dir, target_file) = if initial_path.is_dir() {
            (initial_path, None)
        } else if initial_path.is_file() {
            let parent = initial_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| initial_path.clone());
            (parent, Some(initial_path))
        } else {
            return Self::new();
        };
        let mut config = Config::load_or_create();
        config.notes_dir = notes_dir.to_string_lossy().to_string();
        AppBuilder::explicit(config, target_file).build()
    }

    /// Construct an application without consulting process-global config,
    /// cache, clipboard, clock, or network state.
    pub fn new_injected(mut config: Config, vault_path: PathBuf, target_file: Option<PathBuf>, dependencies: AppDependencies) -> Self {
        config.notes_dir = vault_path.to_string_lossy().to_string();
        AppBuilder::injected(config, target_file, dependencies).build()
    }

    /// Select a note by its file path, expanding collapsed ancestors as needed.
    pub fn select_note_by_path(&mut self, target_path: &PathBuf) -> bool {
        let Some(note_idx) = self.notes.iter().position(|note| note.file_path.as_ref() == Some(target_path)) else {
            return false;
        };

        self.go_to_note_without_history(note_idx, Some(0), Some(0))
    }

    pub fn reload_on_focus(&mut self) {
        if self.mode == Mode::Edit {
            return;
        }
        let scroll_offset = self.content_scroll_offset;
        let content_cursor = self.content_cursor;
        self.load_notes_from_dir();
        // Rebuild content_items for the restored note BEFORE clamping positions,
        // so that content_items.len() reflects the correct note's length
        self.update_content_items();
        let len = self.content_items.len();
        self.content_cursor = content_cursor.min(len.saturating_sub(1));
        self.content_scroll_offset = if len == 0 { 0 } else { scroll_offset.clamp(1, len) };
        self.update_outline();
    }

    pub fn reload_config(&mut self) {
        if self.mode == Mode::Edit {
            return;
        }

        let config = Config::load_from_dir(&self.dependencies.config_dir);
        match Keymap::from_config(&config.keybindings) {
            Ok(mut keymap) => {
                keymap.reset_pending();
                self.keymap = keymap;
                self.keybinding_warning = None;
            }
            Err(error) => {
                self.keymap.reset_pending();
                self.keybinding_warning = Some(KeybindingWarning::new(error, KeybindingFallback::Previous));
            }
        }
        self.config = config;

        self.theme = Theme::from_name_in(&self.config.theme, &Config::themes_dir_in(&self.dependencies.config_dir));

        self.editor.set_line_wrap(self.config.editor.line_wrap);
        self.editor.set_tab_width(self.config.editor.tab_width);
        self.editor.set_padding(self.config.editor.left_padding, self.config.editor.right_padding);
        self.editor.set_line_number_mode(self.config.editor.line_numbers);
        self.editor.set_scrolloff(self.config.editor.scrolloff as usize);
        self.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.primary))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        self.editor
            .set_selection_style(Style::default().fg(self.theme.foreground).bg(self.theme.selection));

        self.syntax_service.configure_theme(&self.config.syntax_theme);
        self.syntax_service.clear_results();
        self.syntax_service.retry();
        self.load_notes_from_dir();
        self.update_content_items();
        self.update_outline();
    }

    /// Swap the active runtime theme without touching config or reloading notes
    /// from disk. Content/editor views read `self.theme` live each frame, so the
    /// whole UI re-skins on the next render; the syntect code-block highlighter
    /// keys off `syntax_theme` (unchanged here) so it is intentionally left
    /// alone. Used for both live preview and final apply in the theme selector.
    pub(super) fn apply_theme_named(&mut self, name: &str) {
        self.theme = Theme::from_name_in(name, &Config::themes_dir_in(&self.dependencies.config_dir));
        self.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.primary))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        self.editor
            .set_selection_style(Style::default().fg(self.theme.foreground).bg(self.theme.selection));
        self.needs_full_clear = true;
    }

    /// Open the theme selector modal (Ctrl+T). Snapshots the current theme so it
    /// can be restored on cancel, and pre-selects the active theme.
    pub fn open_theme_selector(&mut self) {
        if self.mode != Mode::Normal {
            return;
        }
        let themes = ThemeFile::list_available_in(&Config::themes_dir_in(&self.dependencies.config_dir));
        if themes.is_empty() {
            return;
        }
        let selected = themes.iter().position(|t| t.name == self.config.theme).unwrap_or(0);
        self.theme_picker = ThemePicker {
            themes,
            selected,
            scroll_offset: 0,
            original_theme_name: self.config.theme.clone(),
        };
        self.dialog = DialogState::ThemeSelector;
    }

    pub(super) fn preview_selected_theme(&mut self) {
        if let Some(entry) = self.theme_picker.themes.get(self.theme_picker.selected) {
            let name = entry.name.clone();
            self.apply_theme_named(&name);
        }
    }

    pub fn theme_selector_select_next(&mut self) {
        let len = self.theme_picker.themes.len();
        if len == 0 {
            return;
        }
        self.theme_picker.selected = (self.theme_picker.selected + 1) % len;
        self.preview_selected_theme();
    }

    pub fn theme_selector_select_prev(&mut self) {
        let len = self.theme_picker.themes.len();
        if len == 0 {
            return;
        }
        self.theme_picker.selected = if self.theme_picker.selected == 0 {
            len - 1
        } else {
            self.theme_picker.selected - 1
        };
        self.preview_selected_theme();
    }

    /// Persist the highlighted theme to config and close the modal.
    pub fn confirm_theme_selection(&mut self) {
        if let Some(entry) = self.theme_picker.themes.get(self.theme_picker.selected) {
            let name = entry.name.clone();
            self.config.theme = name.clone();
            let _ = self.config.save_to_dir(&self.dependencies.config_dir);
            self.apply_theme_named(&name);
            self.status_message = Some(format!("Theme: {}", name));
        }
        self.dialog = DialogState::None;
        self.theme_picker = ThemePicker::default();
    }

    /// Restore the theme that was active when the modal opened and close it.
    pub fn cancel_theme_selection(&mut self) {
        let original = self.theme_picker.original_theme_name.clone();
        if !original.is_empty() {
            self.apply_theme_named(&original);
        }
        self.dialog = DialogState::None;
        self.theme_picker = ThemePicker::default();
    }

    /// Journal mode (`t`): open today's daily note, creating it in the
    /// configured journal directory and local-year subdirectory when needed.
    /// A same-day root-level journal from an older version is opened in place.
    pub fn open_or_create_journal(&mut self) {
        if self.mode != Mode::Normal {
            return;
        }
        let notes_dir = self.config.notes_path();
        let date = self.dependencies.clock.today();
        let entry = match ekphos_vault::journal::open_or_create_entry(&notes_dir, &self.config.journal_dir, date) {
            Ok(entry) => entry,
            Err(error) => {
                self.status_message = Some(format!("Journal failed: {error}"));
                return;
            }
        };

        let display_path = entry.path.strip_prefix(&notes_dir).unwrap_or(&entry.path).display().to_string();

        self.load_notes_from_dir();
        if self.select_note_by_path(&entry.path) {
            let action = match entry.action {
                ekphos_vault::journal::JournalEntryAction::Created => "Created",
                ekphos_vault::journal::JournalEntryAction::Opened => "Opened",
            };
            self.status_message = Some(format!("{action} {display_path}"));
            self.focus = Focus::Content;
        } else {
            self.status_message = Some(format!("Journal failed to load: {display_path}"));
        }
    }
}
