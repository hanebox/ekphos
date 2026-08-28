use super::*;

pub(super) fn handle_standard_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if let Some(movement) = standard_movement(key) {
        let extend = key.modifiers.contains(KeyModifiers::SHIFT);
        app.editor.move_cursor_with_selection(movement, extend);
        return;
    }

    match key.code {
        KeyCode::Esc => leave_standard_editor(app),
        KeyCode::Char(_) if control_key(key, 's') || control_key(key, 'o') => {
            app.save_edit_in_place();
        }
        KeyCode::Char(_) if control_key(key, 'f') || control_key(key, 'w') => {
            app.editor.cancel_selection();
            app.start_buffer_search();
        }
        KeyCode::Char(_) if control_key(key, 'a') => app.editor.select_all(),
        KeyCode::Char(_) if control_key(key, 'c') => app.editor.copy(),
        KeyCode::Char(_) if control_key(key, 'x') => app.editor.cut(),
        KeyCode::Char(_) if control_key(key, 'v') || control_key(key, 'u') => paste_into_editor(app, None),
        KeyCode::Char(_) if control_key(key, 'k') => {
            if app.editor.has_selection() {
                app.editor.cut();
            } else {
                app.editor.delete_current_line();
            }
            app.update_editor_highlights();
        }
        KeyCode::Char(_) if control_key(key, 'z') && key.modifiers.contains(KeyModifiers::SHIFT) => {
            app.editor.cancel_selection();
            app.editor.redo();
            app.update_editor_highlights();
        }
        KeyCode::Char(_) if control_key(key, 'z') => {
            app.editor.cancel_selection();
            app.editor.undo();
            app.update_editor_highlights();
        }
        KeyCode::Char(_) if control_key(key, 'y') => {
            app.editor.cancel_selection();
            app.editor.redo();
            app.update_editor_highlights();
        }
        KeyCode::Backspace | KeyCode::Delete if app.editor.has_selection() => {
            app.editor.delete_selection();
            app.update_editor_highlights();
        }
        KeyCode::Char(_) if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => {}
        KeyCode::Backspace | KeyCode::Delete if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) => {}
        KeyCode::Tab if !key.modifiers.is_empty() => {}
        KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace | KeyCode::Delete | KeyCode::Tab => handle_editor_text_input(app, key),
        _ => {}
    }
}

fn control_key(key: crossterm::event::KeyEvent, expected: char) -> bool {
    let KeyCode::Char(actual) = key.code else {
        return false;
    };
    actual.eq_ignore_ascii_case(&expected) && key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
}

fn standard_movement(key: crossterm::event::KeyEvent) -> Option<CursorMove> {
    let word_modifier = key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let document_modifier = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Left if word_modifier => Some(CursorMove::WordBack),
        KeyCode::Right if word_modifier => Some(CursorMove::WordForward),
        KeyCode::Left => Some(CursorMove::Back),
        KeyCode::Right => Some(CursorMove::Forward),
        KeyCode::Up => Some(CursorMove::Up),
        KeyCode::Down => Some(CursorMove::Down),
        KeyCode::Home if document_modifier => Some(CursorMove::Top),
        KeyCode::End if document_modifier => Some(CursorMove::Bottom),
        KeyCode::Home => Some(CursorMove::Head),
        KeyCode::End => Some(CursorMove::End),
        KeyCode::PageUp => Some(CursorMove::PageUp),
        KeyCode::PageDown => Some(CursorMove::PageDown),
        _ => None,
    }
}

fn leave_standard_editor(app: &mut App) {
    app.editor.cancel_selection();
    if app.has_unsaved_changes() {
        app.state.dialog = DialogState::UnsavedChanges;
    } else {
        app.cancel_edit();
        update_cursor_style(app);
    }
}

pub(super) fn switch_editing_mode(app: &mut App) {
    if let Some(state) = app.editor.block_insert_state.take() {
        apply_block_insert(app, state);
    }
    if app.editor.vim.macros.is_recording() {
        app.editor.vim.macros.stop_recording();
    }
    app.end_buffer_search();
    app.editor.clear_search_highlights();
    app.editor.cancel_selection();
    app.editor.clear_visual_line_selection();
    app.editor.clear_visual_block_selection();
    app.editor.visual_line_anchor = None;
    app.editor.visual_line_current = None;
    app.editor.visual_block_anchor = None;
    app.editor.pending_operator = None;
    app.editor.pending_delete = None;
    app.editor.context_menu_state = ContextMenuState::None;
    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
    app.editor.vim.reset_pending();
    app.editor.vim.command_buffer.clear();
    app.editor.vim.search_buffer.clear();
    app.editor.vim.status_message = None;
    app.editor.vim.mode = VimMode::Normal;
    app.toggle_editing_mode_preference();
    update_cursor_style(app);
    app.update_editor_block();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppDependencies;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct StandardApp {
        app: App,
        root: PathBuf,
        note_path: PathBuf,
    }

    impl StandardApp {
        fn new() -> Self {
            let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-standard-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            fs::create_dir_all(&vault).unwrap();
            let note_path = vault.join("fixture.md");
            fs::write(&note_path, "hello world").unwrap();
            let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, editor: crate::config::EditorConfig { mode: EditingMode::Standard, ..Default::default() }, ..Default::default() };
            let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            let mut app = App::new_injected(config, vault, None, dependencies);
            app.state.show_welcome = false;
            app.state.dialog = DialogState::None;
            app.enter_edit_mode();
            Self { app, root, note_path }
        }
    }

    impl Drop for StandardApp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn key(code: KeyCode, modifiers: KeyModifiers) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, modifiers)
    }

    #[test]
    fn direct_typing_and_shift_movement_use_standard_selection() {
        let mut fixture = StandardApp::new();
        fixture.app.editor.set_cursor(0, 0);
        handle_standard_mode(&mut fixture.app, key(KeyCode::Right, KeyModifiers::SHIFT));
        handle_standard_mode(&mut fixture.app, key(KeyCode::Right, KeyModifiers::SHIFT));
        assert_eq!(fixture.app.editor.selected_text().as_deref(), Some("he"));
        handle_standard_mode(&mut fixture.app, key(KeyCode::Char('H'), KeyModifiers::SHIFT));
        assert_eq!(fixture.app.editor.text(), "Hllo world");
    }

    #[test]
    fn ctrl_s_saves_without_leaving_edit_mode() {
        let mut fixture = StandardApp::new();
        fixture.app.editor.set_cursor(0, 11);
        handle_standard_mode(&mut fixture.app, key(KeyCode::Char('!'), KeyModifiers::NONE));
        handle_standard_mode(&mut fixture.app, key(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(fixture.app.editor.mode, Mode::Edit);
        assert_eq!(fs::read_to_string(&fixture.note_path).unwrap(), "hello world!");
        assert!(!fixture.app.has_unsaved_changes());
    }

    #[test]
    fn escape_prompts_only_when_changes_are_unsaved() {
        let mut fixture = StandardApp::new();
        handle_standard_mode(&mut fixture.app, key(KeyCode::Char('!'), KeyModifiers::NONE));
        handle_standard_mode(&mut fixture.app, key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(fixture.app.state.dialog, DialogState::UnsavedChanges);
        assert_eq!(fixture.app.editor.mode, Mode::Edit);
    }

    #[test]
    fn standard_movement_supports_word_and_document_modifiers() {
        assert_eq!(standard_movement(key(KeyCode::Left, KeyModifiers::CONTROL)), Some(CursorMove::WordBack));
        assert_eq!(standard_movement(key(KeyCode::Right, KeyModifiers::ALT)), Some(CursorMove::WordForward));
        assert_eq!(standard_movement(key(KeyCode::Home, KeyModifiers::CONTROL)), Some(CursorMove::Top));
        assert_eq!(standard_movement(key(KeyCode::End, KeyModifiers::CONTROL)), Some(CursorMove::Bottom));
    }

    #[test]
    fn double_bracket_completion_keeps_the_precise_trigger_position() {
        let mut fixture = StandardApp::new();
        fixture.app.editor.set_cursor(0, 11);
        handle_standard_mode(&mut fixture.app, key(KeyCode::Char('['), KeyModifiers::NONE));
        handle_standard_mode(&mut fixture.app, key(KeyCode::Char('['), KeyModifiers::NONE));
        let WikiAutocompleteState::Open { trigger_pos, .. } = fixture.app.editor.wiki_autocomplete else {
            panic!("double brackets should open wiki completion");
        };
        assert_eq!(trigger_pos, (0, 11));
    }

    #[test]
    fn editing_mode_preference_is_persisted() {
        let mut fixture = StandardApp::new();
        assert_eq!(fixture.app.toggle_editing_mode_preference(), EditingMode::Vim);
        let config_dir = fixture.app.config_path().parent().unwrap().to_path_buf();
        let persisted = Config::load_from_dir(&config_dir);
        assert_eq!(persisted.editor.mode, EditingMode::Vim);
    }
}
