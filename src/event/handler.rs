use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{App, ContextMenuItem, ContextMenuState, DeleteType, DialogState, Focus, Mode, SidebarItemKind, VimMode};
use crate::editor::CursorMove;
use crate::ui;

pub fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        app.poll_pending_images();
        app.poll_highlighter();

        terminal.draw(|f| ui::render(f, app))?;

        // Use shorter poll time for smoother auto-scroll
        let poll_duration = if app.mouse_button_held && app.mode == Mode::Edit {
            std::time::Duration::from_millis(50)
        } else {
            std::time::Duration::from_millis(100)
        };

        if event::poll(poll_duration)? {
            match event::read()? {
                Event::Mouse(mouse) => {
                    handle_mouse_event(app, mouse);
                }
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        if handle_key_event(app, key)? {
                            return Ok(());
                        }
                    }
                }
                _ => {}
            }
        } else {
            // No event - check for continuous auto-scroll while mouse is held
            if app.mouse_button_held && app.mode == Mode::Edit {
                handle_continuous_auto_scroll(app);
            }
        }
    }
}

fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent) {
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;

    // Handle context menu interactions first (highest priority)
    if let ContextMenuState::Open { x, y, selected_index: _ } = app.context_menu_state {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if click is inside context menu
                if let Some(action) = get_context_menu_click(mouse_x, mouse_y, x, y) {
                    execute_context_menu_action(app, action);
                }
                app.context_menu_state = ContextMenuState::None;
                return;
            }
            MouseEventKind::Moved => {
                // Update hover selection in context menu
                if let Some(new_idx) = get_context_menu_hover_index(mouse_x, mouse_y, x, y) {
                    app.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_idx };
                }
                return;
            }
            _ => {
                // Any other mouse event closes the context menu
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    app.context_menu_state = ContextMenuState::None;
                }
                return;
            }
        }
    }

    // Handle Edit mode mouse events
    if app.mode == Mode::Edit {
        handle_edit_mode_mouse(app, mouse);
        return;
    }

    // Handle Normal mode mouse events (existing logic)
    if app.mode == Mode::Normal && app.dialog == DialogState::None && !app.show_welcome {
        let in_content_area = mouse_x >= app.content_area.x
            && mouse_x < app.content_area.x + app.content_area.width
            && mouse_y >= app.content_area.y
            && mouse_y < app.content_area.y + app.content_area.height;

        match mouse.kind {
            MouseEventKind::Moved => {
                if in_content_area {
                    let hovered_item = app.content_item_rects.iter().find(|(_, rect)| {
                        mouse_y >= rect.y && mouse_y < rect.y + rect.height
                    }).map(|(idx, _)| *idx);

                    if let Some(idx) = hovered_item {
                        if app.item_link_at(idx).is_some() || app.item_is_image_at(idx).is_some() {
                            app.mouse_hover_item = Some(idx);
                        } else {
                            app.mouse_hover_item = None;
                        }
                    } else {
                        app.mouse_hover_item = None;
                    }
                } else {
                    app.mouse_hover_item = None;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if in_content_area {
                    let clicked_item = app.content_item_rects.iter().find(|(_, rect)| {
                        mouse_y >= rect.y && mouse_y < rect.y + rect.height
                    }).map(|(idx, _)| *idx);

                    if let Some(idx) = clicked_item {
                        if let Some(url) = app.find_clicked_link(idx, mouse_x, app.content_area.x) {
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(&url).spawn();
                            #[cfg(target_os = "linux")]
                            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                            #[cfg(target_os = "windows")]
                            let _ = std::process::Command::new("cmd").args(["/c", "start", "", &url]).spawn();
                        }
                        else if let Some(path) = app.item_is_image_at(idx) {
                            let is_url = path.starts_with("http://") || path.starts_with("https://");
                            let should_open = is_url || std::path::PathBuf::from(path).exists();
                            if should_open {
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(path).spawn();
                                #[cfg(target_os = "linux")]
                                let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("cmd").args(["/c", "start", "", path]).spawn();
                            }
                        }
                        else if app.item_is_details_at(idx) {
                            app.toggle_details_at(idx);
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                match app.focus {
                    Focus::Sidebar => app.next_sidebar_item(),
                    Focus::Content => {
                        if app.floating_cursor_mode {
                            app.floating_move_down();
                        } else {
                            app.next_content_line();
                        }
                        app.sync_outline_to_content();
                    }
                    Focus::Outline => app.next_outline(),
                }
            }
            MouseEventKind::ScrollUp => {
                match app.focus {
                    Focus::Sidebar => app.previous_sidebar_item(),
                    Focus::Content => {
                        if app.floating_cursor_mode {
                            app.floating_move_up();
                        } else {
                            app.previous_content_line();
                        }
                        app.sync_outline_to_content();
                    }
                    Focus::Outline => app.previous_outline(),
                }
            }
            _ => {}
        }
    }
}

fn handle_edit_mode_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Close context menu if open
            app.context_menu_state = ContextMenuState::None;

            if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                // Clamp to valid line count
                let line_count = app.editor.line_count();
                let row = row.min(line_count.saturating_sub(1));
                let line_len = app.editor.lines().get(row).map(|l| l.chars().count()).unwrap_or(0);
                let col = col.min(line_len);

                // Start selection if in Normal vim mode, switch to Visual
                if app.vim_mode == VimMode::Normal {
                    // Move cursor to clicked position first
                    move_editor_cursor_to(app, row, col);
                    app.vim_mode = VimMode::Visual;
                    app.editor.start_selection();
                } else if app.vim_mode == VimMode::Visual {
                    // Already in visual, cancel and restart
                    app.editor.cancel_selection();
                    move_editor_cursor_to(app, row, col);
                    app.editor.start_selection();
                } else {
                    // In Insert mode, just move cursor
                    move_editor_cursor_to(app, row, col);
                }

                app.mouse_button_held = true;
                app.mouse_drag_start = Some((row as u16, col as u16));
                app.update_editor_block();
            }
        }

        MouseEventKind::Down(MouseButton::Right) => {
            // Right-click shows context menu
            app.context_menu_state = ContextMenuState::Open {
                x: mouse_x,
                y: mouse_y,
                selected_index: 0,
            };
        }

        MouseEventKind::Up(MouseButton::Left) => {
            app.mouse_button_held = false;
            app.mouse_drag_start = None;
        }

        MouseEventKind::Drag(MouseButton::Left) => {
            if app.mouse_button_held {
                // Store last mouse Y for continuous scrolling
                app.last_mouse_y = mouse_y;

                // Handle auto-scroll when near edges
                handle_auto_scroll(app, mouse_y);

                if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                    let line_count = app.editor.line_count();
                    let row = row.min(line_count.saturating_sub(1));
                    let line_len = app.editor.lines().get(row).map(|l| l.chars().count()).unwrap_or(0);
                    let col = col.min(line_len);

                    // Extend selection to new position
                    move_editor_cursor_to(app, row, col);
                }
            }
        }

        MouseEventKind::ScrollUp => {
            if app.editor_scroll_top > 0 {
                app.editor_scroll_top = app.editor_scroll_top.saturating_sub(3);
                app.editor.set_scroll_offset(app.editor_scroll_top);
            }
        }

        MouseEventKind::ScrollDown => {
            let max_scroll = app.editor.line_count().saturating_sub(app.editor_view_height);
            if app.editor_scroll_top < max_scroll {
                app.editor_scroll_top = (app.editor_scroll_top + 3).min(max_scroll);
                app.editor.set_scroll_offset(app.editor_scroll_top);
            }
        }

        _ => {}
    }
}

fn handle_auto_scroll(app: &mut App, mouse_y: u16) {
    let direction = app.get_auto_scroll_direction(mouse_y);
    if direction == 0 {
        return;
    }

    perform_auto_scroll(app, direction);
}

/// Continuous auto-scroll when mouse is held near edges (called from main loop)
fn handle_continuous_auto_scroll(app: &mut App) {
    let direction = app.get_auto_scroll_direction(app.last_mouse_y);
    if direction == 0 {
        return;
    }

    perform_auto_scroll(app, direction);
}

/// Perform the actual scrolling in the given direction
fn perform_auto_scroll(app: &mut App, direction: i8) {
    if direction < 0 {
        // Scroll up
        if app.editor_scroll_top > 0 {
            app.editor_scroll_top = app.editor_scroll_top.saturating_sub(1);
            app.editor.set_scroll_offset(app.editor_scroll_top);
            app.editor.move_cursor(CursorMove::Up);
        }
    } else {
        // Scroll down
        let max_scroll = app.editor.line_count().saturating_sub(app.editor_view_height);
        if app.editor_scroll_top < max_scroll {
            app.editor_scroll_top += 1;
            app.editor.set_scroll_offset(app.editor_scroll_top);
            app.editor.move_cursor(CursorMove::Down);
        }
    }
}

/// Move editor cursor to specific row/col position
fn move_editor_cursor_to(app: &mut App, target_row: usize, target_col: usize) {
    let (current_row, _) = app.editor.cursor();

    // Move to target row
    if target_row < current_row {
        for _ in 0..(current_row - target_row) {
            app.editor.move_cursor(CursorMove::Up);
        }
    } else if target_row > current_row {
        for _ in 0..(target_row - current_row) {
            app.editor.move_cursor(CursorMove::Down);
        }
    }

    // Move to start of line, then to target column
    app.editor.move_cursor(CursorMove::Head);
    for _ in 0..target_col {
        app.editor.move_cursor(CursorMove::Forward);
    }
}

// ==================== Context Menu Helpers ====================

const MENU_WIDTH: u16 = 14;

fn get_context_menu_click(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<ContextMenuItem> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2; // +2 for borders

    // Check if click is within menu bounds
    if mouse_x >= menu_x && mouse_x < menu_x + MENU_WIDTH &&
       mouse_y >= menu_y && mouse_y < menu_y + menu_height {
        let relative_y = mouse_y.saturating_sub(menu_y).saturating_sub(1); // -1 for top border
        let index = relative_y as usize;
        if index < items.len() {
            return Some(items[index]);
        }
    }

    None
}

fn get_context_menu_hover_index(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<usize> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2;

    if mouse_x >= menu_x && mouse_x < menu_x + MENU_WIDTH &&
       mouse_y > menu_y && mouse_y < menu_y + menu_height - 1 {
        let index = (mouse_y - menu_y - 1) as usize;
        if index < items.len() {
            return Some(index);
        }
    }
    None
}

fn execute_context_menu_action(app: &mut App, action: ContextMenuItem) {
    match action {
        ContextMenuItem::Copy => {
            app.editor.copy();
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Normal;
        }
        ContextMenuItem::Cut => {
            app.editor.cut();
            app.vim_mode = VimMode::Normal;
        }
        ContextMenuItem::Paste => {
            app.editor.paste();
        }
        ContextMenuItem::SelectAll => {
            app.editor.move_cursor(CursorMove::Top);
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::Bottom);
            app.vim_mode = VimMode::Visual;
        }
    }
    app.update_editor_block();
}

/// Returns true if the app should quit
fn handle_key_event(app: &mut App, key: crossterm::event::KeyEvent) -> io::Result<bool> {
    // Handle dialogs first
    match app.dialog {
        DialogState::Onboarding => {
            handle_onboarding_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateNote => {
            handle_create_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateFolder => {
            handle_create_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::CreateNoteInFolder => {
            handle_create_note_in_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::DeleteConfirm => {
            handle_delete_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::DeleteFolderConfirm => {
            handle_delete_folder_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameNote => {
            handle_rename_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameFolder => {
            handle_rename_folder_dialog(app, key);
            return Ok(false);
        }
        DialogState::Help => {
            handle_help_dialog(app, key);
            return Ok(false);
        }
        DialogState::EmptyDirectory => {
            handle_empty_directory_dialog(app, key);
            return Ok(false);
        }
        DialogState::DirectoryNotFound => {
            return Ok(handle_directory_not_found_dialog(app, key));
        }
        DialogState::UnsavedChanges => {
            handle_unsaved_changes_dialog(app, key);
            return Ok(false);
        }
        DialogState::None => {}
    }

    // Handle welcome dialog
    if app.show_welcome {
        handle_welcome_dialog(app, key);
        return Ok(false);
    }

    // Handle search input
    if app.search_active {
        handle_search_input(app, key);
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
            handle_edit_mode(app, key);
        }
    }

    Ok(false)
}

fn handle_onboarding_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
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

fn handle_create_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
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
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.target_folder = None;
            app.dialog_error = None;
            app.dialog = DialogState::None;
        }
        KeyCode::Char(c) => {
            app.dialog_error = None;
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.dialog_error = None;
            app.input_buffer.pop();
        }
        _ => {}
    }
}

fn handle_create_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
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
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.dialog_error = None;
            app.target_folder = None;
            app.dialog = DialogState::None;
        }
        KeyCode::Char(c) => {
            app.dialog_error = None; // Clear error on new input
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.dialog_error = None; // Clear error on edit
            app.input_buffer.pop();
        }
        _ => {}
    }
}

fn handle_create_note_in_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
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
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.target_folder = None;
            app.dialog_error = None;
            app.dialog = DialogState::None;
            app.load_notes_from_dir();
        }
        KeyCode::Char(c) => {
            app.dialog_error = None;
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.dialog_error = None;
            app.input_buffer.pop();
        }
        _ => {}
    }
}

fn handle_delete_confirm_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
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

fn handle_delete_folder_confirm_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
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

fn handle_unsaved_changes_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            app.dialog = DialogState::None;
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            app.cancel_edit();
            app.vim_mode = VimMode::Normal;
            app.dialog = DialogState::None;
        }
        KeyCode::Esc => {
            app.dialog = DialogState::None;
        }
        _ => {}
    }
}

fn handle_rename_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let new_name = app.input_buffer.clone();
            app.rename_note(&new_name);
            app.input_buffer.clear();
            app.dialog = DialogState::None;
        }
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.dialog = DialogState::None;
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

fn handle_rename_folder_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let new_name = app.input_buffer.clone();
            app.rename_folder(&new_name);
            if app.dialog_error.is_none() {
                app.input_buffer.clear();
                app.dialog = DialogState::None;
            }
        }
        KeyCode::Esc => {
            app.input_buffer.clear();
            app.dialog_error = None;
            app.dialog = DialogState::None;
        }
        KeyCode::Char(c) => {
            app.dialog_error = None; 
            app.input_buffer.push(c);
        }
        KeyCode::Backspace => {
            app.dialog_error = None; 
            app.input_buffer.pop();
        }
        _ => {}
    }
}

fn handle_help_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.dialog = DialogState::None;
        }
        _ => {}
    }
}

fn handle_empty_directory_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
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

fn handle_directory_not_found_dialog(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.create_notes_directory();
            false
        }
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            true
        }
        _ => false,
    }
}

fn handle_welcome_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
            app.dismiss_welcome();
        }
        _ => {}
    }
}

fn handle_search_input(app: &mut App, key: crossterm::event::KeyEvent) {
    let is_nav_down = key.code == KeyCode::Down
        || (key.code == KeyCode::Char('j') && key.modifiers == KeyModifiers::CONTROL)
        || (key.code == KeyCode::Char('n') && key.modifiers == KeyModifiers::CONTROL);
    let is_nav_up = key.code == KeyCode::Up
        || (key.code == KeyCode::Char('k') && key.modifiers == KeyModifiers::CONTROL)
        || (key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::CONTROL);

    if is_nav_down {
        let visible_indices = app.get_visible_sidebar_indices();
        if !visible_indices.is_empty() {
            let current_pos = visible_indices.iter()
                .position(|&i| i == app.selected_sidebar_index)
                .unwrap_or(0);
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
            let current_pos = visible_indices.iter()
                .position(|&i| i == app.selected_sidebar_index)
                .unwrap_or(0);
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

/// Returns true if the app should quit
fn handle_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Tab => app.toggle_focus(false),
        KeyCode::BackTab => app.toggle_focus(true),
        KeyCode::Char('e') => app.enter_edit_mode(),
        KeyCode::Char('n') => {
            app.input_buffer.clear();
            app.dialog_error = None;
            let context_folder = app.get_current_context_folder();
            if context_folder.as_ref() != Some(&app.config.notes_path()) {
                app.target_folder = context_folder;
            } else {
                app.target_folder = None;
            }
            app.dialog = DialogState::CreateNote;
        }
        KeyCode::Char('N') => {
            app.input_buffer.clear();
            app.dialog_error = None;
            let context_folder = app.get_current_context_folder();
            if context_folder.as_ref() != Some(&app.config.notes_path()) {
                app.target_folder = context_folder;
            } else {
                app.target_folder = None;
            }
            app.dialog = DialogState::CreateFolder;
        }
        KeyCode::Char('d') => {
            if let Some(item) = app.sidebar_items.get(app.selected_sidebar_index) {
                match &item.kind {
                    SidebarItemKind::Note { .. } => {
                        app.dialog = DialogState::DeleteConfirm;
                    }
                    SidebarItemKind::Folder { .. } => {
                        app.dialog = DialogState::DeleteFolderConfirm;
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            if let Some(item) = app.sidebar_items.get(app.selected_sidebar_index) {
                match &item.kind {
                    SidebarItemKind::Note { note_index } => {
                        app.input_buffer = app.notes[*note_index].title.clone();
                        app.dialog_error = None;
                        app.dialog = DialogState::RenameNote;
                    }
                    SidebarItemKind::Folder { .. } => {
                        app.input_buffer = item.display_name.clone();
                        app.dialog_error = None;
                        app.dialog = DialogState::RenameFolder;
                    }
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            match app.focus {
                Focus::Sidebar => app.next_sidebar_item(),
                Focus::Outline => app.next_outline(),
                Focus::Content => {
                    if app.floating_cursor_mode {
                        app.floating_move_down();
                    } else {
                        app.next_content_line();
                    }
                    app.sync_outline_to_content();
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            match app.focus {
                Focus::Sidebar => app.previous_sidebar_item(),
                Focus::Outline => app.previous_outline(),
                Focus::Content => {
                    if app.floating_cursor_mode {
                        app.floating_move_up();
                    } else {
                        app.previous_content_line();
                    }
                    app.sync_outline_to_content();
                }
            }
        }
        KeyCode::Enter => {
            match app.focus {
                Focus::Content => app.open_current_image(),
                Focus::Outline => app.jump_to_outline(),
                Focus::Sidebar => app.handle_sidebar_enter(),
            }
        }
        KeyCode::Char('o') if key.modifiers == KeyModifiers::CONTROL => {
            app.toggle_outline_collapsed();
        }
        KeyCode::Char('o') => {
            if app.focus == Focus::Content {
                app.open_current_image();
            } else if app.focus == Focus::Outline {
                // 'o' on outline just jumps to content view without edit
                app.jump_to_outline();
            }
        }
        KeyCode::Char('?') => {
            app.dialog = DialogState::Help;
        }
        KeyCode::Char('/') => {
            if app.focus == Focus::Sidebar {
                app.search_active = true;
                app.search_query.clear();
            }
        }
        KeyCode::Char(' ') => {
            if app.focus == Focus::Content {
                if let Some(crate::app::ContentItem::TaskItem { .. }) = app.content_items.get(app.content_cursor) {
                    app.toggle_current_task();
                } else if let Some(crate::app::ContentItem::Details { .. }) = app.content_items.get(app.content_cursor) {
                    app.toggle_current_details();
                } else if app.current_item_link().is_some() {
                    app.open_current_link();
                }
            }
        }
        KeyCode::Char(']') => {
            if app.focus == Focus::Content {
                app.next_link();
            }
        }
        KeyCode::Char('[') => {
            if app.focus == Focus::Content {
                app.previous_link();
            }
        }
        KeyCode::Char('J') | KeyCode::Char('K') => {
            if app.focus == Focus::Content {
                app.toggle_floating_cursor();
            }
        }
        KeyCode::Char('b') if key.modifiers == KeyModifiers::CONTROL => {
            app.toggle_sidebar_collapsed();
        }
        _ => {}
    }
    false
}

fn handle_edit_mode(app: &mut App, key: crossterm::event::KeyEvent) {
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
                app.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_index };
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let new_index = if selected_index == 0 { items.len() - 1 } else { selected_index - 1 };
                app.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_index };
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
                    VimMode::Visual => handle_vim_visual_mode(app, key),
                }
            }
        }
        app.update_editor_block();
        return;
    }

    match app.vim_mode {
        VimMode::Normal => handle_vim_normal_mode(app, key),
        VimMode::Insert => handle_vim_insert_mode(app, key),
        VimMode::Visual => handle_vim_visual_mode(app, key),
    }
    app.update_editor_block();
}

fn handle_vim_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Char('i') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('a') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.editor.move_cursor(CursorMove::Forward);
        }
        KeyCode::Char('A') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.editor.move_cursor(CursorMove::End);
        }
        KeyCode::Char('I') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.editor.move_cursor(CursorMove::Head);
        }
        KeyCode::Char('o') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.editor.move_cursor(CursorMove::End);
            app.editor.insert_newline();
        }
        KeyCode::Char('O') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.editor.move_cursor(CursorMove::Head);
            app.editor.insert_newline();
            app.editor.move_cursor(CursorMove::Up);
        }
        KeyCode::Char('v') => {
            app.pending_operator = None;
            app.vim_mode = VimMode::Visual;
            app.editor.cancel_selection();
            app.editor.start_selection();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Back);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Down);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Up);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Forward);
        }
        KeyCode::Char('w') => {
            if app.pending_operator == Some('d') {
                // dw: delete word forward - highlight then delete on next key
                app.pending_operator = None;
                app.editor.cancel_selection();
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::WordForward);
                app.pending_delete = Some(DeleteType::Word);
            } else {
                app.pending_operator = None;
                app.editor.move_cursor(CursorMove::WordForward);
            }
        }
        KeyCode::Char('b') => {
            if app.pending_operator == Some('d') {
                // db: delete word backward - highlight then delete on next key
                app.pending_operator = None;
                app.editor.cancel_selection();
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::WordBack);
                app.pending_delete = Some(DeleteType::Word);
            } else {
                app.pending_operator = None;
                app.editor.move_cursor(CursorMove::WordBack);
            }
        }
        KeyCode::Char('0') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Head);
        }
        KeyCode::Char('$') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::End);
        }
        KeyCode::Char('g') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Top);
        }
        KeyCode::Char('G') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Bottom);
        }
        KeyCode::Char('x') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.delete_char();
        }
        KeyCode::Char('d') => {
            if app.pending_operator == Some('d') {
                app.pending_operator = None;
                app.editor.cancel_selection();
                app.editor.move_cursor(CursorMove::Head);
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::End);
                app.pending_delete = Some(DeleteType::Line);
            } else {
                app.pending_operator = Some('d');
            }
        }
        KeyCode::Char('y') => {
            app.pending_operator = None;
            // Yank current selection
            app.editor.copy();
        }
        KeyCode::Char('p') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.paste();
        }
        KeyCode::Char('u') => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.undo();
        }
        KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.editor.redo();
        }
        KeyCode::Esc => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            if app.has_unsaved_changes() {
                app.dialog = DialogState::UnsavedChanges;
            } else {
                app.cancel_edit();
                app.vim_mode = VimMode::Normal;
            }
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.pending_operator = None;
            app.editor.cancel_selection();
            app.save_edit();
            app.vim_mode = VimMode::Normal;
        }
        _ => {
            app.pending_operator = None;
        }
    }
}

fn handle_vim_insert_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.save_edit();
            app.vim_mode = VimMode::Normal;
        }
        _ => {
            app.editor.input(key);
        }
    }
}

fn handle_vim_visual_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.editor.move_cursor(CursorMove::Back);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.editor.move_cursor(CursorMove::Down);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.editor.move_cursor(CursorMove::Up);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.editor.move_cursor(CursorMove::Forward);
        }
        KeyCode::Char('w') => {
            app.editor.move_cursor(CursorMove::WordForward);
        }
        KeyCode::Char('b') => {
            app.editor.move_cursor(CursorMove::WordBack);
        }
        KeyCode::Char('0') => {
            app.editor.move_cursor(CursorMove::Head);
        }
        KeyCode::Char('$') => {
            app.editor.move_cursor(CursorMove::End);
        }
        KeyCode::Char('g') => {
            app.editor.move_cursor(CursorMove::Top);
        }
        KeyCode::Char('G') => {
            app.editor.move_cursor(CursorMove::Bottom);
        }
        KeyCode::Char('y') => {
            app.editor.copy();
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            app.editor.cut();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.cancel_selection();
            app.save_edit();
            app.vim_mode = VimMode::Normal;
        }
        _ => {}
    }
}
