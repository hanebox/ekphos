use super::*;

pub(super) fn handle_edit_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if handle_wiki_autocomplete(app, key) {
        app.request_highlight_update();
        return;
    }

    // Handle context menu keyboard navigation first
    if let ContextMenuState::Open { x, y, selected_index } = app.context_menu_state {
        let items = ContextMenuItem::all();
        match key.code {
            KeyCode::Esc => {
                app.context_menu_state = ContextMenuState::None;
            }
            KeyCode::Enter => {
                if let Some(&action) = items.get(selected_index) {
                    execute_context_menu_action(app, action);
                }
                app.context_menu_state = ContextMenuState::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let new_index = (selected_index + 1) % items.len();
                app.context_menu_state = ContextMenuState::Open {
                    x,
                    y,
                    selected_index: new_index,
                };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_index = if selected_index == 0 { items.len() - 1 } else { selected_index - 1 };
                app.context_menu_state = ContextMenuState::Open {
                    x,
                    y,
                    selected_index: new_index,
                };
            }
            _ => {}
        }
        return;
    }

    // Handle pending delete confirmation
    if let Some(delete_type) = app.pending_delete {
        match key.code {
            KeyCode::Char('d') => {
                app.pending_delete = None;
                app.editor.cut();
                if delete_type == DeleteType::Line {
                    app.editor.delete_newline();
                }
            }
            KeyCode::Esc => {
                app.pending_delete = None;
                app.editor.cancel_selection();
            }
            _ => {
                app.pending_delete = None;
                app.editor.cancel_selection();
                match app.vim_mode {
                    VimMode::Normal => handle_vim_normal_mode(app, key),
                    VimMode::Insert => handle_vim_insert_mode(app, key),
                    VimMode::Replace => handle_vim_replace_mode(app, key),
                    VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => handle_vim_visual_mode(app, key),
                }
            }
        }
        app.request_highlight_update();
        app.update_editor_block();
        return;
    }

    // Check the new vim state mode for command mode
    if app.vim.mode.is_command() {
        handle_vim_command_mode(app, key);
        app.request_highlight_update();
        app.update_editor_block();
        return;
    }
    if app.vim.mode.is_search() {
        handle_vim_search_mode(app, key);
        app.request_highlight_update();
        app.update_editor_block();
        return;
    }
    if matches!(app.vim.mode, VimModeNew::SearchLocked { .. }) {
        handle_vim_search_locked_mode(app, key);
        app.request_highlight_update();
        app.update_editor_block();
        return;
    }

    match app.vim_mode {
        VimMode::Normal => handle_vim_normal_mode(app, key),
        VimMode::Insert => handle_vim_insert_mode(app, key),
        VimMode::Replace => handle_vim_replace_mode(app, key),
        VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => handle_vim_visual_mode(app, key),
    }
    app.request_highlight_update();
    app.update_editor_block();
}
