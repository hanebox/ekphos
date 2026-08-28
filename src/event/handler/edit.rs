use super::*;

pub(super) fn handle_edit_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if handle_wiki_autocomplete(app, key) {
        app.request_highlight_update();
        return;
    }
    if let ContextMenuState::Open { x, y, selected_index } = app.editor.context_menu_state {
        let items = ContextMenuItem::all();
        match key.code {
            KeyCode::Esc => {
                app.editor.context_menu_state = ContextMenuState::None;
            }
            KeyCode::Enter => {
                if let Some(&action) = items.get(selected_index) {
                    execute_context_menu_action(app, action);
                }
                app.editor.context_menu_state = ContextMenuState::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let new_index = (selected_index + 1) % items.len();
                app.editor.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_index };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_index = if selected_index == 0 { items.len() - 1 } else { selected_index - 1 };
                app.editor.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_index };
            }
            _ => {}
        }
        return;
    }
    if let Some(delete_type) = app.editor.pending_delete {
        match key.code {
            KeyCode::Char('d') => {
                app.editor.pending_delete = None;
                app.editor.cut();
                if delete_type == DeleteType::Line {
                    app.editor.delete_newline();
                }
            }
            KeyCode::Esc => {
                app.editor.pending_delete = None;
                app.editor.cancel_selection();
            }
            _ => {
                app.editor.pending_delete = None;
                app.editor.cancel_selection();
                dispatch_vim_input(app, key);
            }
        }
        app.request_highlight_update();
        app.update_editor_block();
        return;
    }
    dispatch_vim_input(app, key);
    app.request_highlight_update();
    app.update_editor_block();
}
fn dispatch_vim_input(app: &mut App, key: crossterm::event::KeyEvent) {
    match app.editor.vim.mode.input_mode() {
        VimInputMode::Normal => handle_vim_normal_mode(app, key),
        VimInputMode::Insert => handle_vim_insert_mode(app, key),
        VimInputMode::Replace => handle_vim_replace_mode(app, key),
        VimInputMode::Visual => handle_vim_visual_mode(app, key),
        VimInputMode::Command => handle_vim_command_mode(app, key),
        VimInputMode::Search => handle_vim_search_mode(app, key),
        VimInputMode::SearchLocked => handle_vim_search_locked_mode(app, key),
    }
}
