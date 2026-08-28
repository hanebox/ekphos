use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogCommand {
    Submit,
    Cancel,
    Edited,
    Ignore,
}
fn apply_text_dialog_key(input: &mut String, error: &mut Option<String>, key: crossterm::event::KeyEvent, clear_error_on_edit: bool) -> DialogCommand {
    match key.code {
        KeyCode::Enter => DialogCommand::Submit,
        KeyCode::Esc => DialogCommand::Cancel,
        KeyCode::Char(ch) => {
            input.push(ch);
            if clear_error_on_edit {
                *error = None;
            }
            DialogCommand::Edited
        }
        KeyCode::Backspace => {
            input.pop();
            if clear_error_on_edit {
                *error = None;
            }
            DialogCommand::Edited
        }
        _ => DialogCommand::Ignore,
    }
}

/// Returns true if the app should quit.
pub(super) fn handle_key_event(app: &mut App, key: crossterm::event::KeyEvent) -> io::Result<bool> {
    if app.state.keybinding_warning.is_some() {
        handle_keybinding_warning(app, key);
        return Ok(false);
    }
    match app.state.dialog {
        DialogState::Onboarding => {
            app.state.keymap.reset_pending();
            handle_onboarding_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateNote => {
            app.state.keymap.reset_pending();
            handle_create_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateFolder => {
            app.state.keymap.reset_pending();
            handle_create_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateNoteInFolder => {
            app.state.keymap.reset_pending();
            handle_create_note_in_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::DeleteConfirm => {
            app.state.keymap.reset_pending();
            handle_delete_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::DeleteFolderConfirm => {
            app.state.keymap.reset_pending();
            handle_delete_folder_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameNote => {
            app.state.keymap.reset_pending();
            handle_rename_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameFolder => {
            app.state.keymap.reset_pending();
            handle_rename_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::Help => {
            app.state.keymap.reset_pending();
            handle_help_dialog(app, key);
            return Ok(false);
        }
        DialogState::EmptyDirectory => {
            app.state.keymap.reset_pending();
            handle_empty_directory_dialog(app, key);
            return Ok(false);
        }
        DialogState::DirectoryNotFound => {
            app.state.keymap.reset_pending();
            return Ok(handle_directory_not_found_dialog(app, key));
        }
        DialogState::UnsavedChanges => {
            app.state.keymap.reset_pending();
            handle_unsaved_changes_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateWikiNote => {
            app.state.keymap.reset_pending();
            handle_create_wiki_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::GraphView => {
            app.state.keymap.reset_pending();
            handle_graph_view_dialog(app, key);
            return Ok(false);
        }
        DialogState::ThemeSelector => {
            app.state.keymap.reset_pending();
            handle_theme_selector_dialog(app, key);
            return Ok(false);
        }
        DialogState::None => {}
    }
    if app.state.show_welcome {
        app.state.keymap.reset_pending();
        handle_welcome_dialog(app, key);
        return Ok(false);
    }
    if !matches!(app.search.search_picker, SearchPickerState::Closed) {
        app.state.keymap.reset_pending();
        handle_search_picker_input(app, key);
        return Ok(false);
    }
    if app.search.search_active {
        app.state.keymap.reset_pending();
        handle_search_input(app, key);
        return Ok(false);
    }
    if app.search.buffer_search.active {
        app.state.keymap.reset_pending();
        handle_buffer_search_input(app, key);
        return Ok(false);
    }
    match app.editor.mode {
        Mode::Normal => {
            if handle_normal_mode(app, key) {
                return Ok(true);
            }
        }
        Mode::Edit => {
            app.state.keymap.reset_pending();
            handle_edit_mode(app, key);
        }
    }
    Ok(false)
}

pub(super) fn handle_keybinding_warning(app: &mut App, key: crossterm::event::KeyEvent) {
    app.state.keymap.reset_pending();
    if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
        app.state.keybinding_warning = None;
        return;
    }
    let Some(warning) = app.state.keybinding_warning.as_mut() else {
        return;
    };
    let max_scroll = warning.issues.len().saturating_mul(8).saturating_sub(1);
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            warning.scroll = warning.scroll.saturating_add(1).min(max_scroll);
        }
        KeyCode::Up | KeyCode::Char('k') => warning.scroll = warning.scroll.saturating_sub(1),
        KeyCode::PageDown => {
            warning.scroll = warning.scroll.saturating_add(5).min(max_scroll);
        }
        KeyCode::PageUp => warning.scroll = warning.scroll.saturating_sub(5),
        _ => {}
    }
}

pub(super) fn handle_onboarding_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            app.complete_onboarding();
        }
        KeyCode::Char(c) => {
            app.state.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.state.input_buffer.pop();
        }
        _ => {}
    }
}

pub(super) fn handle_create_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.state.input_buffer, &mut app.state.dialog_error, key, true) {
        DialogCommand::Submit => {
            let name = app.state.input_buffer.trim().to_string();
            if name.is_empty() {
                app.state.dialog_error = Some("Note name cannot be empty".to_string());
                return;
            }
            if app.create_note(&name) {
                app.state.input_buffer.clear();
                app.state.dialog_error = None;
                app.state.dialog = DialogState::None;
            }
        }
        DialogCommand::Cancel => {
            app.state.input_buffer.clear();
            app.vault.target_folder = None;
            app.state.dialog_error = None;
            app.state.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_create_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.state.input_buffer, &mut app.state.dialog_error, key, true) {
        DialogCommand::Submit => {
            let name = app.state.input_buffer.trim().to_string();
            if name.is_empty() {
                app.state.dialog_error = Some("Folder name cannot be empty".to_string());
                return;
            }
            if app.create_folder(&name) {
                app.state.input_buffer.clear();
                app.state.dialog_error = None;
                app.state.dialog = DialogState::CreateNoteInFolder;
            }
        }
        DialogCommand::Cancel => {
            app.state.input_buffer.clear();
            app.state.dialog_error = None;
            app.vault.target_folder = None;
            app.state.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_create_note_in_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.state.input_buffer, &mut app.state.dialog_error, key, true) {
        DialogCommand::Submit => {
            let name = app.state.input_buffer.trim().to_string();
            if name.is_empty() {
                app.state.dialog_error = Some("Note name cannot be empty".to_string());
                return;
            }
            if app.create_note(&name) {
                app.state.input_buffer.clear();
                app.state.dialog_error = None;
                app.state.dialog = DialogState::None;
            }
        }
        DialogCommand::Cancel => {
            app.state.input_buffer.clear();
            app.vault.target_folder = None;
            app.state.dialog_error = None;
            app.state.dialog = DialogState::None;
            app.load_notes_from_dir();
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_delete_confirm_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.delete_current_note();
            app.state.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.state.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_delete_folder_confirm_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.delete_current_folder();
            app.state.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.state.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_unsaved_changes_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.save_edit();
            app.editor.vim.mode = VimMode::Normal;
            update_cursor_style(app);
            app.state.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.cancel_edit();
            app.editor.vim.mode = VimMode::Normal;
            update_cursor_style(app);
            app.state.dialog = DialogState::None;
        }
        KeyCode::Esc => {
            app.state.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_create_wiki_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if let Some(target) = app.editor.pending_wiki_target.take() {
                app.create_note_from_wiki_target(&target);
            }
            app.state.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.editor.pending_wiki_target = None;
            app.state.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_wiki_autocomplete(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    let is_open = matches!(app.editor.wiki_autocomplete, WikiAutocompleteState::Open { .. });
    if !is_open {
        return false;
    }
    let (query, suggestions_len, mode, target_note) = if let WikiAutocompleteState::Open { ref query, ref suggestions, ref mode, ref target_note, .. } = app.editor.wiki_autocomplete {
        (query.clone(), suggestions.len(), mode.clone(), target_note.clone())
    } else {
        return false;
    };
    match key.code {
        KeyCode::Esc => {
            app.editor.wiki_autocomplete = WikiAutocompleteState::None;
            true
        }
        KeyCode::Enter | KeyCode::Tab => {
            if mode == WikiAutocompleteMode::Alias {
                let (row, col) = app.editor.cursor();
                let already_closed = app.editor.line(row).is_some_and(|line| line.chars().nth(col) == Some(']') && line.chars().nth(col + 1) == Some(']'));
                if !already_closed {
                    app.editor.insert_str("]]");
                }
                app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                app.update_editor_highlights();
                return true;
            }
            let suggestion = if let WikiAutocompleteState::Open { ref suggestions, selected_index, .. } = app.editor.wiki_autocomplete { suggestions.get(selected_index).cloned() } else { None };
            if let Some(suggestion) = suggestion {
                let chars_to_delete = match mode {
                    WikiAutocompleteMode::Note => query.chars().count(),
                    WikiAutocompleteMode::Heading => query.chars().count(),
                    WikiAutocompleteMode::Alias => 0,
                };
                for _ in 0..chars_to_delete {
                    app.editor.delete_newline();
                }
                if mode == WikiAutocompleteMode::Heading {
                    app.editor.insert_str(&suggestion.insert_text);
                    let already_closed = {
                        let (row, col) = app.editor.cursor();
                        app.editor.line(row).is_some_and(|line| line.chars().nth(col) == Some(']') && line.chars().nth(col + 1) == Some(']'))
                    };
                    if !already_closed {
                        app.editor.insert_str("]]");
                    }
                    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                    app.update_editor_highlights();
                } else if suggestion.is_folder {
                    app.editor.insert_str(&suggestion.insert_text);
                    let new_query = suggestion.insert_text.clone();
                    let new_suggestions = app.build_wiki_suggestions(&new_query);
                    app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: new_query, suggestions: new_suggestions, selected_index: 0, mode: WikiAutocompleteMode::Note, target_note: None };
                } else {
                    app.editor.insert_str(&suggestion.insert_text);
                    let already_closed = {
                        let (row, col) = app.editor.cursor();
                        app.editor.line(row).is_some_and(|line| line.chars().nth(col) == Some(']') && line.chars().nth(col + 1) == Some(']'))
                    };
                    if !already_closed {
                        app.editor.insert_str("]]");
                    }
                    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                    app.update_editor_highlights();
                }
            }
            true
        }
        KeyCode::Down => {
            if mode != WikiAutocompleteMode::Alias && suggestions_len > 0 {
                if let WikiAutocompleteState::Open { ref mut selected_index, .. } = app.editor.wiki_autocomplete {
                    *selected_index = (*selected_index + 1) % suggestions_len;
                }
            }
            true
        }
        KeyCode::Up => {
            if mode != WikiAutocompleteMode::Alias && suggestions_len > 0 {
                if let WikiAutocompleteState::Open { ref mut selected_index, .. } = app.editor.wiki_autocomplete {
                    *selected_index = if *selected_index == 0 { suggestions_len - 1 } else { *selected_index - 1 };
                }
            }
            true
        }
        KeyCode::Backspace => {
            if query.is_empty() {
                match mode {
                    WikiAutocompleteMode::Note => {
                        app.editor.delete_newline(); // Delete first [
                        app.editor.delete_newline(); // Delete second [
                        app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                    }
                    WikiAutocompleteMode::Heading => {
                        app.editor.delete_newline();
                        if let Some(ref target) = target_note {
                            let new_suggestions = app.build_wiki_suggestions(target);
                            app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: target.clone(), suggestions: new_suggestions, selected_index: 0, mode: WikiAutocompleteMode::Note, target_note: None };
                        } else {
                            app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                        }
                    }
                    WikiAutocompleteMode::Alias => {
                        app.editor.delete_newline();
                        if let Some(ref target) = target_note {
                            if target.contains('#') {
                                let (note_part, heading_part) = target.split_once('#').unwrap_or((target, ""));
                                let heading_suggestions = app.build_heading_suggestions(note_part, heading_part);
                                app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: heading_part.to_string(), suggestions: heading_suggestions, selected_index: 0, mode: WikiAutocompleteMode::Heading, target_note: Some(note_part.to_string()) };
                            } else {
                                let new_suggestions = app.build_wiki_suggestions(target);
                                app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: target.clone(), suggestions: new_suggestions, selected_index: 0, mode: WikiAutocompleteMode::Note, target_note: None };
                            }
                        } else {
                            app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                        }
                    }
                }
            } else {
                let mut new_query = query.clone();
                new_query.pop();
                app.editor.delete_newline();
                let new_suggestions = match mode {
                    WikiAutocompleteMode::Note => app.build_wiki_suggestions(&new_query),
                    WikiAutocompleteMode::Heading => {
                        if let Some(ref target) = target_note {
                            app.build_heading_suggestions(target, &new_query)
                        } else {
                            Vec::new()
                        }
                    }
                    WikiAutocompleteMode::Alias => Vec::new(), // No suggestions in alias mode
                };
                app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: new_query, suggestions: new_suggestions, selected_index: 0, mode: mode.clone(), target_note: target_note.clone() };
            }
            true
        }
        KeyCode::Char(']') => {
            app.editor.insert_char(']');
            let (row, col) = app.editor.cursor();
            if let Some(line) = app.editor.line(row) {
                if col >= 2 && line.chars().nth(col.saturating_sub(2)) == Some(']') && line.chars().nth(col.saturating_sub(1)) == Some(']') {
                    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
                    app.update_editor_highlights();
                }
            }
            true
        }
        KeyCode::Char('#') if mode == WikiAutocompleteMode::Note => {
            let note_target = query.clone();
            app.editor.insert_char('#');
            let heading_suggestions = app.build_heading_suggestions(&note_target, "");
            app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: String::new(), suggestions: heading_suggestions, selected_index: 0, mode: WikiAutocompleteMode::Heading, target_note: Some(note_target) };
            true
        }
        KeyCode::Char('|') if mode == WikiAutocompleteMode::Note || mode == WikiAutocompleteMode::Heading => {
            app.editor.insert_char('|');
            let full_target = if mode == WikiAutocompleteMode::Heading {
                if let Some(ref target) = target_note {
                    format!("{}#{}", target, query)
                } else {
                    query.clone()
                }
            } else {
                query.clone()
            };
            app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: String::new(), suggestions: Vec::new(), selected_index: 0, mode: WikiAutocompleteMode::Alias, target_note: Some(full_target) };
            true
        }
        KeyCode::Char(c) => {
            let mut new_query = query.clone();
            new_query.push(c);
            app.editor.insert_char(c);
            let new_suggestions = match mode {
                WikiAutocompleteMode::Note => app.build_wiki_suggestions(&new_query),
                WikiAutocompleteMode::Heading => {
                    if let Some(ref target) = target_note {
                        app.build_heading_suggestions(target, &new_query)
                    } else {
                        Vec::new()
                    }
                }
                WikiAutocompleteMode::Alias => Vec::new(),
            };
            app.editor.wiki_autocomplete = WikiAutocompleteState::Open { trigger_pos: (0, 0), query: new_query, suggestions: new_suggestions, selected_index: 0, mode: mode.clone(), target_note: target_note.clone() };
            true
        }
        _ => {
            app.editor.wiki_autocomplete = WikiAutocompleteState::None;
            false
        }
    }
}

pub(super) fn handle_rename_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.state.input_buffer, &mut app.state.dialog_error, key, false) {
        DialogCommand::Submit => {
            let new_name = app.state.input_buffer.clone();
            if app.rename_note(&new_name) {
                app.state.input_buffer.clear();
                app.state.dialog_error = None;
                app.state.dialog = DialogState::None;
            }
        }
        DialogCommand::Cancel => {
            app.state.input_buffer.clear();
            app.state.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_rename_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.state.input_buffer, &mut app.state.dialog_error, key, true) {
        DialogCommand::Submit => {
            let new_name = app.state.input_buffer.clone();
            if app.rename_folder(&new_name) {
                app.state.input_buffer.clear();
                app.state.dialog_error = None;
                app.state.dialog = DialogState::None;
            }
        }
        DialogCommand::Cancel => {
            app.state.input_buffer.clear();
            app.state.dialog_error = None;
            app.state.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_help_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    const MAX_HELP_LINES: usize = 90;
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.state.help_scroll = 0;
            app.state.dialog = DialogState::None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.state.help_scroll = app.state.help_scroll.saturating_add(1).min(MAX_HELP_LINES);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.state.help_scroll = app.state.help_scroll.saturating_sub(1);
        }
        KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            app.state.help_scroll = app.state.help_scroll.saturating_add(10).min(MAX_HELP_LINES);
        }
        KeyCode::Char('u') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            app.state.help_scroll = app.state.help_scroll.saturating_sub(10);
        }
        KeyCode::Char('g') => {
            app.state.help_scroll = 0;
        }
        KeyCode::Char('G') => {
            app.state.help_scroll = MAX_HELP_LINES;
        }
        _ => {}
    }
}

/// Zoom the graph view, anchoring on the selected node or graph center
pub(super) fn handle_empty_directory_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => {
            app.state.dialog = DialogState::None;
        }
        KeyCode::Char('n') => {
            app.state.dialog = DialogState::None;
            app.state.input_buffer.clear();
            app.state.dialog = DialogState::CreateNote;
        }
        _ => {}
    }
}

pub(super) fn handle_directory_not_found_dialog(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.create_notes_directory();
            false
        }
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => true,
        _ => false,
    }
}

pub(super) fn handle_welcome_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
            app.dismiss_welcome();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> crossterm::event::KeyEvent {
        crossterm::event::KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn text_dialog_commands_preserve_existing_key_behavior() {
        let mut input = String::from("ab");
        let mut error = Some(String::from("invalid"));
        assert_eq!(apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Char('c')), true), DialogCommand::Edited);
        assert_eq!(input, "abc");
        assert_eq!(error, None);
        error = Some(String::from("invalid"));
        assert_eq!(apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Backspace), true), DialogCommand::Edited);
        assert_eq!(input, "ab");
        assert_eq!(error, None);
        assert_eq!(apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Enter), true), DialogCommand::Submit);
        assert_eq!(apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Esc), true), DialogCommand::Cancel);
        assert_eq!(apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Left), true), DialogCommand::Ignore);
    }

    #[test]
    fn rename_note_editing_keeps_its_existing_error_policy() {
        let mut input = String::from("old");
        let mut error = Some(String::from("unchanged"));
        assert_eq!(apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Char('x')), false), DialogCommand::Edited);
        assert_eq!(input, "oldx");
        assert_eq!(error.as_deref(), Some("unchanged"));
    }
}
