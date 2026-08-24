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
    if app.keybinding_warning.is_some() {
        handle_keybinding_warning(app, key);
        return Ok(false);
    }

    // Handle dialogs first
    match app.dialog {
        DialogState::Onboarding => {
            app.keymap.reset_pending();
            handle_onboarding_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateNote => {
            app.keymap.reset_pending();
            handle_create_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateFolder => {
            app.keymap.reset_pending();
            handle_create_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateNoteInFolder => {
            app.keymap.reset_pending();
            handle_create_note_in_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::DeleteConfirm => {
            app.keymap.reset_pending();
            handle_delete_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::DeleteFolderConfirm => {
            app.keymap.reset_pending();
            handle_delete_folder_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameNote => {
            app.keymap.reset_pending();
            handle_rename_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameFolder => {
            app.keymap.reset_pending();
            handle_rename_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::Help => {
            app.keymap.reset_pending();
            handle_help_dialog(app, key);
            return Ok(false);
        }
        DialogState::EmptyDirectory => {
            app.keymap.reset_pending();
            handle_empty_directory_dialog(app, key);
            return Ok(false);
        }
        DialogState::DirectoryNotFound => {
            app.keymap.reset_pending();
            return Ok(handle_directory_not_found_dialog(app, key));
        }
        DialogState::UnsavedChanges => {
            app.keymap.reset_pending();
            handle_unsaved_changes_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateWikiNote => {
            app.keymap.reset_pending();
            handle_create_wiki_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::GraphView => {
            app.keymap.reset_pending();
            handle_graph_view_dialog(app, key);
            return Ok(false);
        }
        DialogState::ThemeSelector => {
            app.keymap.reset_pending();
            handle_theme_selector_dialog(app, key);
            return Ok(false);
        }
        DialogState::None => {}
    }

    // Handle welcome dialog
    if app.show_welcome {
        app.keymap.reset_pending();
        handle_welcome_dialog(app, key);
        return Ok(false);
    }

    // Handle search picker input (high priority)
    if !matches!(app.search_picker, SearchPickerState::Closed) {
        app.keymap.reset_pending();
        handle_search_picker_input(app, key);
        return Ok(false);
    }

    // Handle sidebar search input
    if app.search_active {
        app.keymap.reset_pending();
        handle_search_input(app, key);
        return Ok(false);
    }

    if app.buffer_search.active {
        app.keymap.reset_pending();
        handle_buffer_search_input(app, key);
        return Ok(false);
    }

    // Handle mode-specific input
    match app.mode {
        Mode::Normal => {
            if handle_normal_mode(app, key) {
                return Ok(true);
            }
        }
        Mode::Edit => {
            app.keymap.reset_pending();
            handle_edit_mode(app, key);
        }
    }

    Ok(false)
}

pub(super) fn handle_keybinding_warning(app: &mut App, key: crossterm::event::KeyEvent) {
    app.keymap.reset_pending();
    if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
        app.keybinding_warning = None;
        return;
    }
    let Some(warning) = app.keybinding_warning.as_mut() else {
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
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        _ => {}
    }
}

pub(super) fn handle_create_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.input_buffer, &mut app.dialog_error, key, true) {
        DialogCommand::Submit => {
            let name = app.input_buffer.trim().to_string();
            if name.is_empty() {
                app.dialog_error = Some("Note name cannot be empty".to_string());
                return;
            }
            app.create_note(&name);
            app.input_buffer.clear();
            app.dialog_error = None;
            app.dialog = DialogState::None;
        }
        DialogCommand::Cancel => {
            app.input_buffer.clear();
            app.target_folder = None;
            app.dialog_error = None;
            app.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_create_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.input_buffer, &mut app.dialog_error, key, true) {
        DialogCommand::Submit => {
            let name = app.input_buffer.trim().to_string();
            if name.is_empty() {
                app.dialog_error = Some("Folder name cannot be empty".to_string());
                return;
            }
            if app.create_folder(&name) {
                app.input_buffer.clear();
                app.dialog_error = None;
                app.dialog = DialogState::CreateNoteInFolder;
            }
        }
        DialogCommand::Cancel => {
            app.input_buffer.clear();
            app.dialog_error = None;
            app.target_folder = None;
            app.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_create_note_in_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.input_buffer, &mut app.dialog_error, key, true) {
        DialogCommand::Submit => {
            let name = app.input_buffer.trim().to_string();
            if name.is_empty() {
                app.dialog_error = Some("Note name cannot be empty".to_string());
                return;
            }
            app.create_note(&name);
            app.input_buffer.clear();
            app.dialog_error = None;
            app.dialog = DialogState::None;
        }
        DialogCommand::Cancel => {
            app.input_buffer.clear();
            app.target_folder = None;
            app.dialog_error = None;
            app.dialog = DialogState::None;
            app.load_notes_from_dir();
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_delete_confirm_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.delete_current_note();
            app.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_delete_folder_confirm_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.delete_current_folder();
            app.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_unsaved_changes_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
            app.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.cancel_edit();
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
            app.dialog = DialogState::None;
        }
        KeyCode::Esc => {
            app.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_create_wiki_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if let Some(target) = app.pending_wiki_target.take() {
                app.create_note_from_wiki_target(&target);
            }
            app.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.pending_wiki_target = None;
            app.dialog = DialogState::None;
        }
        _ => {}
    }
}

pub(super) fn handle_wiki_autocomplete(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    let is_open = matches!(app.wiki_autocomplete, WikiAutocompleteState::Open { .. });
    if !is_open {
        return false;
    }

    let (query, suggestions_len, mode, target_note) = if let WikiAutocompleteState::Open {
        ref query,
        ref suggestions,
        ref mode,
        ref target_note,
        ..
    } = app.wiki_autocomplete
    {
        (query.clone(), suggestions.len(), mode.clone(), target_note.clone())
    } else {
        return false;
    };

    match key.code {
        KeyCode::Esc => {
            app.wiki_autocomplete = WikiAutocompleteState::None;
            return true;
        }
        KeyCode::Enter | KeyCode::Tab => {
            if mode == WikiAutocompleteMode::Alias {
                let (row, col) = app.editor.cursor();
                let already_closed = app
                    .editor
                    .line(row)
                    .is_some_and(|line| line.chars().nth(col) == Some(']') && line.chars().nth(col + 1) == Some(']'));

                if !already_closed {
                    app.editor.insert_str("]]");
                }
                app.wiki_autocomplete = WikiAutocompleteState::None;
                app.update_editor_highlights();
                return true;
            }

            let suggestion = if let WikiAutocompleteState::Open {
                ref suggestions,
                selected_index,
                ..
            } = app.wiki_autocomplete
            {
                suggestions.get(selected_index).cloned()
            } else {
                None
            };

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
                        app.editor
                            .line(row)
                            .is_some_and(|line| line.chars().nth(col) == Some(']') && line.chars().nth(col + 1) == Some(']'))
                    };
                    if !already_closed {
                        app.editor.insert_str("]]");
                    }
                    app.wiki_autocomplete = WikiAutocompleteState::None;
                    app.update_editor_highlights();
                } else if suggestion.is_folder {
                    app.editor.insert_str(&suggestion.insert_text);
                    let new_query = suggestion.insert_text.clone();
                    let new_suggestions = app.build_wiki_suggestions(&new_query);
                    app.wiki_autocomplete = WikiAutocompleteState::Open {
                        trigger_pos: (0, 0),
                        query: new_query,
                        suggestions: new_suggestions,
                        selected_index: 0,
                        mode: WikiAutocompleteMode::Note,
                        target_note: None,
                    };
                } else {
                    app.editor.insert_str(&suggestion.insert_text);
                    let already_closed = {
                        let (row, col) = app.editor.cursor();
                        app.editor
                            .line(row)
                            .is_some_and(|line| line.chars().nth(col) == Some(']') && line.chars().nth(col + 1) == Some(']'))
                    };
                    if !already_closed {
                        app.editor.insert_str("]]");
                    }
                    app.wiki_autocomplete = WikiAutocompleteState::None;
                    app.update_editor_highlights();
                }
            }
            return true;
        }
        KeyCode::Down => {
            if mode != WikiAutocompleteMode::Alias && suggestions_len > 0 {
                if let WikiAutocompleteState::Open { ref mut selected_index, .. } = app.wiki_autocomplete {
                    *selected_index = (*selected_index + 1) % suggestions_len;
                }
            }
            return true;
        }
        KeyCode::Up => {
            if mode != WikiAutocompleteMode::Alias && suggestions_len > 0 {
                if let WikiAutocompleteState::Open { ref mut selected_index, .. } = app.wiki_autocomplete {
                    *selected_index = if *selected_index == 0 { suggestions_len - 1 } else { *selected_index - 1 };
                }
            }
            return true;
        }
        KeyCode::Backspace => {
            if query.is_empty() {
                match mode {
                    WikiAutocompleteMode::Note => {
                        // Close autocomplete and delete the [[
                        app.editor.delete_newline(); // Delete first [
                        app.editor.delete_newline(); // Delete second [
                        app.wiki_autocomplete = WikiAutocompleteState::None;
                    }
                    WikiAutocompleteMode::Heading => {
                        app.editor.delete_newline();
                        if let Some(ref target) = target_note {
                            let new_suggestions = app.build_wiki_suggestions(target);
                            app.wiki_autocomplete = WikiAutocompleteState::Open {
                                trigger_pos: (0, 0),
                                query: target.clone(),
                                suggestions: new_suggestions,
                                selected_index: 0,
                                mode: WikiAutocompleteMode::Note,
                                target_note: None,
                            };
                        } else {
                            app.wiki_autocomplete = WikiAutocompleteState::None;
                        }
                    }
                    WikiAutocompleteMode::Alias => {
                        app.editor.delete_newline();
                        if let Some(ref target) = target_note {
                            if target.contains('#') {
                                let (note_part, heading_part) = target.split_once('#').unwrap_or((target, ""));
                                let heading_suggestions = app.build_heading_suggestions(note_part, heading_part);
                                app.wiki_autocomplete = WikiAutocompleteState::Open {
                                    trigger_pos: (0, 0),
                                    query: heading_part.to_string(),
                                    suggestions: heading_suggestions,
                                    selected_index: 0,
                                    mode: WikiAutocompleteMode::Heading,
                                    target_note: Some(note_part.to_string()),
                                };
                            } else {
                                let new_suggestions = app.build_wiki_suggestions(target);
                                app.wiki_autocomplete = WikiAutocompleteState::Open {
                                    trigger_pos: (0, 0),
                                    query: target.clone(),
                                    suggestions: new_suggestions,
                                    selected_index: 0,
                                    mode: WikiAutocompleteMode::Note,
                                    target_note: None,
                                };
                            }
                        } else {
                            app.wiki_autocomplete = WikiAutocompleteState::None;
                        }
                    }
                }
            } else {
                // Delete character from query and editor
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

                app.wiki_autocomplete = WikiAutocompleteState::Open {
                    trigger_pos: (0, 0),
                    query: new_query,
                    suggestions: new_suggestions,
                    selected_index: 0,
                    mode: mode.clone(),
                    target_note: target_note.clone(),
                };
            }
            return true;
        }
        KeyCode::Char(']') => {
            // Check if user is closing the wiki link manually
            app.editor.insert_char(']');

            // Get the current line to check if we have ]]
            let (row, col) = app.editor.cursor();
            if let Some(line) = app.editor.line(row) {
                // Check for ]] pattern (current char should be ])
                if col >= 2 {
                    if line.chars().nth(col.saturating_sub(2)) == Some(']') && line.chars().nth(col.saturating_sub(1)) == Some(']') {
                        // User typed ]], close autocomplete
                        app.wiki_autocomplete = WikiAutocompleteState::None;
                        app.update_editor_highlights();
                    }
                }
            }
            return true;
        }
        KeyCode::Char('#') if mode == WikiAutocompleteMode::Note => {
            let note_target = query.clone();

            app.editor.insert_char('#');

            let heading_suggestions = app.build_heading_suggestions(&note_target, "");

            app.wiki_autocomplete = WikiAutocompleteState::Open {
                trigger_pos: (0, 0),
                query: String::new(),
                suggestions: heading_suggestions,
                selected_index: 0,
                mode: WikiAutocompleteMode::Heading,
                target_note: Some(note_target),
            };
            return true;
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

            app.wiki_autocomplete = WikiAutocompleteState::Open {
                trigger_pos: (0, 0),
                query: String::new(),
                suggestions: Vec::new(),
                selected_index: 0,
                mode: WikiAutocompleteMode::Alias,
                target_note: Some(full_target),
            };
            return true;
        }
        KeyCode::Char(c) => {
            // Add character to query and editor
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

            app.wiki_autocomplete = WikiAutocompleteState::Open {
                trigger_pos: (0, 0),
                query: new_query,
                suggestions: new_suggestions,
                selected_index: 0,
                mode: mode.clone(),
                target_note: target_note.clone(),
            };
            return true;
        }
        _ => {
            app.wiki_autocomplete = WikiAutocompleteState::None;
            return false;
        }
    }
}

pub(super) fn handle_rename_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.input_buffer, &mut app.dialog_error, key, false) {
        DialogCommand::Submit => {
            let new_name = app.input_buffer.clone();
            app.rename_note(&new_name);
            app.input_buffer.clear();
            app.dialog = DialogState::None;
        }
        DialogCommand::Cancel => {
            app.input_buffer.clear();
            app.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_rename_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match apply_text_dialog_key(&mut app.input_buffer, &mut app.dialog_error, key, true) {
        DialogCommand::Submit => {
            let new_name = app.input_buffer.clone();
            app.rename_folder(&new_name);
            if app.dialog_error.is_none() {
                app.input_buffer.clear();
                app.dialog = DialogState::None;
            }
        }
        DialogCommand::Cancel => {
            app.input_buffer.clear();
            app.dialog_error = None;
            app.dialog = DialogState::None;
        }
        DialogCommand::Edited | DialogCommand::Ignore => {}
    }
}

pub(super) fn handle_help_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    // Max scroll is approximately the right column content length (the longer one)
    const MAX_HELP_LINES: usize = 90;

    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.help_scroll = 0;
            app.dialog = DialogState::None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.help_scroll = app.help_scroll.saturating_add(1).min(MAX_HELP_LINES);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.help_scroll = app.help_scroll.saturating_sub(1);
        }
        KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            app.help_scroll = app.help_scroll.saturating_add(10).min(MAX_HELP_LINES);
        }
        KeyCode::Char('u') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            app.help_scroll = app.help_scroll.saturating_sub(10);
        }
        KeyCode::Char('g') => {
            app.help_scroll = 0;
        }
        KeyCode::Char('G') => {
            app.help_scroll = MAX_HELP_LINES;
        }
        _ => {}
    }
}

/// Zoom the graph view, anchoring on the selected node or graph center
pub(super) fn handle_empty_directory_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => {
            app.dialog = DialogState::None;
        }
        KeyCode::Char('n') => {
            // Dismiss and open create note dialog
            app.dialog = DialogState::None;
            app.input_buffer.clear();
            app.dialog = DialogState::CreateNote;
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

        assert_eq!(
            apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Char('c')), true),
            DialogCommand::Edited
        );
        assert_eq!(input, "abc");
        assert_eq!(error, None);

        error = Some(String::from("invalid"));
        assert_eq!(
            apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Backspace), true),
            DialogCommand::Edited
        );
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
        assert_eq!(
            apply_text_dialog_key(&mut input, &mut error, key(KeyCode::Char('x')), false),
            DialogCommand::Edited
        );
        assert_eq!(input, "oldx");
        assert_eq!(error.as_deref(), Some("unchanged"));
    }
}
