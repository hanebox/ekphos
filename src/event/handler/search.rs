use super::*;

pub(super) fn handle_search_picker_input(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.close_search_picker();
        }
        KeyCode::Enter => {
            app.select_search_picker_result();
        }
        KeyCode::Left | KeyCode::Right => {
            app.toggle_search_picker_mode();
        }
        KeyCode::Up | KeyCode::BackTab => {
            app.search_picker_select_prev();
        }
        KeyCode::Down | KeyCode::Tab => {
            app.search_picker_select_next();
        }
        KeyCode::Char('j') if key.modifiers == KeyModifiers::CONTROL => {
            app.search_picker_select_next();
        }
        KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => {
            app.search_picker_select_prev();
        }
        KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
            app.search_picker_select_next();
        }
        KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
            app.search_picker_select_prev();
        }
        KeyCode::Backspace => {
            app.search_picker_pop_char();
        }
        KeyCode::Char(c) => {
            app.search_picker_push_char(c);
        }
        _ => {}
    }
}

pub(super) fn handle_theme_selector_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.cancel_theme_selection();
        }
        KeyCode::Enter => {
            app.confirm_theme_selection();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.theme_selector_select_prev();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.theme_selector_select_next();
        }
        KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
            app.theme_selector_select_next();
        }
        KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
            app.theme_selector_select_prev();
        }
        _ => {}
    }
}

pub(super) fn handle_search_input(app: &mut App, key: crossterm::event::KeyEvent) {
    let is_nav_down = key.code == KeyCode::Down
        || (key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::CONTROL)
        || (key.code == KeyCode::Char('n') && key.modifiers == KeyModifiers::CONTROL);
    let is_nav_up = key.code == KeyCode::Up
        || (key.code == KeyCode::Char('k') && key.modifiers == KeyModifiers::CONTROL)
        || (key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::CONTROL);

    if is_nav_down {
        let visible_indices = app.get_visible_sidebar_indices();
        if !visible_indices.is_empty() {
            let current_pos = visible_indices.iter().position(|&i| i == app.selected_sidebar_index).unwrap_or(0);
            let next_pos = (current_pos + 1) % visible_indices.len();
            app.selected_sidebar_index = visible_indices[next_pos];
            app.sync_selected_note_from_sidebar();
            app.update_outline();
            app.update_content_items();
        }
        return;
    }

    if is_nav_up {
        let visible_indices = app.get_visible_sidebar_indices();
        if !visible_indices.is_empty() {
            let current_pos = visible_indices.iter().position(|&i| i == app.selected_sidebar_index).unwrap_or(0);
            let prev_pos = if current_pos == 0 { visible_indices.len() - 1 } else { current_pos - 1 };
            app.selected_sidebar_index = visible_indices[prev_pos];
            app.sync_selected_note_from_sidebar();
            app.update_outline();
            app.update_content_items();
        }
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.clear_search();
        }
        KeyCode::Enter => {
            app.search_active = false;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.update_filtered_indices();
        }
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.update_filtered_indices();
        }
        _ => {}
    }
}

pub(super) fn handle_buffer_search_input(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.end_buffer_search();
            if app.mode == Mode::Edit {
                app.editor.clear_search_highlights();
            }
        }
        KeyCode::Enter => {
            if !app.buffer_search.matches.is_empty() {
                app.buffer_search_next();
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Backspace => {
            app.buffer_search.query.pop();
            app.perform_buffer_search();
            if !app.buffer_search.matches.is_empty() {
                app.scroll_to_current_match();
            }
            update_editor_search_highlights(app);
        }
        KeyCode::Char(c) if key.modifiers == KeyModifiers::SHIFT => {
            app.buffer_search.query.push(c);
            app.perform_buffer_search();
            if !app.buffer_search.matches.is_empty() {
                app.scroll_to_current_match();
            }
            update_editor_search_highlights(app);
        }
        KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
            if !app.buffer_search.matches.is_empty() {
                app.buffer_search_next();
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
            if !app.buffer_search.matches.is_empty() {
                app.buffer_search_prev();
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => {
            app.buffer_search.case_sensitive = !app.buffer_search.case_sensitive;
            app.perform_buffer_search();
            if !app.buffer_search.matches.is_empty() {
                app.scroll_to_current_match();
            }
            update_editor_search_highlights(app);
        }
        KeyCode::Char(c) => {
            app.buffer_search.query.push(c);
            app.perform_buffer_search();
            if !app.buffer_search.matches.is_empty() {
                app.scroll_to_current_match();
            }
            update_editor_search_highlights(app);
        }
        KeyCode::Down | KeyCode::Tab => {
            if !app.buffer_search.matches.is_empty() {
                app.buffer_search_next();
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Up | KeyCode::BackTab => {
            if !app.buffer_search.matches.is_empty() {
                app.buffer_search_prev();
                update_editor_search_highlights(app);
            }
        }
        _ => {}
    }
}

pub(super) fn update_editor_search_highlights(app: &mut App) {
    if app.mode == Mode::Edit {
        let matches: Vec<(usize, usize, usize)> = app.buffer_search.matches.iter().map(|m| (m.row, m.start_col, m.end_col)).collect();
        let current_idx = app.buffer_search.current_match_index;
        let match_color = app.theme.search.match_highlight;
        let current_color = app.theme.search.match_current;
        app.editor.set_search_highlights(&matches, current_idx, match_color, current_color);
    }
}
