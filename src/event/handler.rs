use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{App, BlockInsertMode, BlockInsertState, ContextMenuItem, ContextMenuState, DeleteType, DialogState, Focus, Mode, SidebarItemKind, VimMode, WikiAutocompleteMode, WikiAutocompleteState};
use crate::clipboard::{self, ClipboardContent};
use crate::editor::{CursorMove, Position};
use crate::ui;
use crate::vim::{FindState, PendingFind, PendingMacro, PendingMark, TextObject, TextObjectScope, VimMode as VimModeNew};
use crate::vim::command::{parse_command, Command};

pub fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    let mut needs_render = true;

    loop {
        let pending_before = app.pending_images.len();
        let highlighter_was_loading = app.highlighter_loading;
        app.poll_pending_images();
        app.poll_highlighter();

        if app.pending_images.len() < pending_before
            || (highlighter_was_loading && !app.highlighter_loading)
        {
            needs_render = true;
        }

        if app.needs_full_clear {
            app.needs_full_clear = false;
            needs_render = true;
        }

        if needs_render {
            terminal.draw(|f| ui::render(f, app))?;
            needs_render = false;
        }

        let has_background_work = !app.pending_images.is_empty()
            || app.highlighter_loading
            || app.mouse_button_held;

        if has_background_work {
            let timeout = if app.mouse_button_held {
                std::time::Duration::from_millis(33)
            } else {
                std::time::Duration::from_millis(100)
            };

            if event::poll(timeout)? {
                if process_events(terminal, app, &mut needs_render)? {
                    return Ok(());
                }
            } else if app.mouse_button_held && app.mode == Mode::Edit && app.vim_mode == VimMode::Visual {
                handle_continuous_auto_scroll(app);
                needs_render = true;
            }
        } else {
            // idle block until event to avoid unnecessary cpu usage
            if process_events(terminal, app, &mut needs_render)? {
                return Ok(());
            }
        }
    }
}

// Default event handling can't keep up with fast frame update
// this one is okayish solution to batch event 
fn process_events(
    _terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    needs_render: &mut bool,
) -> io::Result<bool> {
    const MAX_EVENTS_PER_BATCH: u8 = 8;
    let mut count = 0u8;

    loop {
        let event = event::read()?;
        count += 1;
        *needs_render = true;

        match event {
            Event::FocusGained => {
                app.reload_on_focus();
                app.needs_full_clear = true;
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if handle_key_event(app, key)? {
                    return Ok(true);
                }
            }
            Event::Mouse(mouse) => handle_mouse_event(app, mouse),
            Event::Paste(text) => handle_paste_event(app, text),
            Event::Resize(_, _) => {
            }
            _ => {}
        }

        if count >= MAX_EVENTS_PER_BATCH || !event::poll(std::time::Duration::ZERO)? {
            break;
        }
    }

    Ok(false)
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

    if app.dialog == DialogState::GraphView {
        handle_graph_view_mouse(app, mouse);
        return;
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
                        if app.item_has_link_at(idx) || app.item_is_image_at(idx).is_some() {
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
                let in_sidebar_area = app.sidebar_area.width > 0
                    && mouse_x >= app.sidebar_area.x
                    && mouse_x < app.sidebar_area.x + app.sidebar_area.width
                    && mouse_y >= app.sidebar_area.y
                    && mouse_y < app.sidebar_area.y + app.sidebar_area.height;

                let in_outline_area = app.outline_area.width > 0
                    && mouse_x >= app.outline_area.x
                    && mouse_x < app.outline_area.x + app.outline_area.width
                    && mouse_y >= app.outline_area.y
                    && mouse_y < app.outline_area.y + app.outline_area.height;

                if in_sidebar_area {
                    let inner_y = mouse_y.saturating_sub(app.sidebar_area.y + 1); // +1 for top border
                    let clicked_index = inner_y as usize;

                    if clicked_index < app.sidebar_items.len() {
                        app.selected_sidebar_index = clicked_index;
                        let item_info = app.sidebar_items.get(clicked_index).map(|item| {
                            match &item.kind {
                                SidebarItemKind::Folder { path, .. } => Some((true, path.clone(), 0)),
                                SidebarItemKind::Note { note_index } => Some((false, std::path::PathBuf::new(), *note_index)),
                            }
                        }).flatten();

                        if let Some((is_folder, path, note_index)) = item_info {
                            if is_folder {
                                app.focus = Focus::Sidebar;
                                app.toggle_folder(path);
                            } else {
                                app.focus = Focus::Content;
                                app.sync_selected_note_from_sidebar();
                                app.update_content_items();
                                app.update_outline();
                                // Push to navigation history
                                app.push_navigation_history(note_index);
                            }
                        }
                    }
                } else if in_outline_area {
                    let inner_y = mouse_y.saturating_sub(app.outline_area.y + 1); // +1 for top border
                    let clicked_index = inner_y as usize;

                    if clicked_index < app.outline.len() {
                        app.outline_state.select(Some(clicked_index));
                        app.focus = Focus::Outline;
                        app.jump_to_outline();
                    }
                } else if in_content_area {
                    let clicked_item = app.content_item_rects.iter().find(|(_, rect)| {
                        mouse_y >= rect.y && mouse_y < rect.y + rect.height
                    }).map(|(idx, _)| *idx);

                    if let Some(idx) = clicked_item {
                        if app.is_click_on_task_checkbox(idx, mouse_x, app.content_area.x) {
                            app.toggle_task_at(idx);
                        }
                        else if let Some(url) = app.find_clicked_link(idx, mouse_x, app.content_area.x) {
                            #[cfg(target_os = "macos")]
                            let _ = std::process::Command::new("open").arg(&url).spawn();
                            #[cfg(target_os = "linux")]
                            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                            #[cfg(target_os = "windows")]
                            let _ = std::process::Command::new("cmd").args(["/c", "start", "", &url]).spawn();
                        }
                        else if let Some(wiki_link) = app.find_clicked_wiki_link(idx, mouse_x, app.content_area.x) {
                            if wiki_link.is_valid {
                                app.navigate_to_wiki_link_with_heading(&wiki_link.target, wiki_link.heading.as_deref());
                            } else {
                                app.pending_wiki_target = Some(wiki_link.target);
                                app.dialog = DialogState::CreateWikiNote;
                            }
                        }
                        else if let Some(path) = app.item_is_image_at(idx) {
                            let is_url = path.starts_with("http://") || path.starts_with("https://");
                            let open_path = if is_url {
                                Some(path.to_string())
                            } else {
                                app.resolve_image_path(path).map(|p| p.to_string_lossy().to_string())
                            };
                            if let Some(open_path) = open_path {
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(&open_path).spawn();
                                #[cfg(target_os = "linux")]
                                let _ = std::process::Command::new("xdg-open").arg(&open_path).spawn();
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("cmd").args(["/c", "start", "", &open_path]).spawn();
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

fn handle_paste_event(app: &mut App, text: String) {
    // Only handle paste in Edit mode
    if app.mode != Mode::Edit {
        return;
    }

    // Close any open menus/autocomplete
    app.context_menu_state = ContextMenuState::None;
    app.wiki_autocomplete = WikiAutocompleteState::None;

    // If in Normal or Visual mode, switch to Insert mode
    if app.vim_mode == VimMode::Normal || app.vim_mode == VimMode::Visual {
        app.editor.cancel_selection();
        app.vim_mode = VimMode::Insert;
    }

    // Try to get html from clipboard and convert to Markdown
    // falls back to plain text if html not available or conversion fails
    let paste_text = match clipboard::get_content_as_markdown() {
        Ok(ClipboardContent::Markdown(md)) => md,
        Ok(ClipboardContent::PlainText(txt)) => txt,
        Ok(ClipboardContent::Empty) => text.clone(),
        Err(_) => text.clone(),
    };

    // Force full clear for multiline paste to prevent ghosting
    if paste_text.contains('\n') {
        app.needs_full_clear = true;
    }

    // Insert the entire pasted text at once
    app.editor.insert_str(&paste_text);
    app.update_editor_highlights();
    app.update_editor_block();

    if let Some(view_height) = app.editor_view_height.checked_sub(2) {
        if view_height > 0 {
            app.update_editor_scroll(view_height);
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

                if app.vim_mode == VimMode::Visual {
                    app.editor.cancel_selection();
                    app.vim_mode = VimMode::Normal;
                }
                move_editor_cursor_to(app, row, col);

                app.mouse_button_held = true;
                app.mouse_drag_start = Some((row as u16, col as u16));
                app.last_mouse_y = mouse_y; // Initialize to prevent stale auto-scroll
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

                // Start Visual mode on first drag if in Normal mode
                if app.vim_mode == VimMode::Normal {
                    app.vim_mode = VimMode::Visual;
                    app.editor.start_selection();
                    app.update_editor_block();
                }

                // Only auto-scroll when in Visual mode (actively selecting)
                if app.vim_mode == VimMode::Visual {
                    handle_auto_scroll(app, mouse_y);
                }

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

                let (cursor_row, cursor_col) = app.editor.cursor();
                let line_count = app.editor.line_count();
                let max_row = line_count.saturating_sub(1);
                let viewport_bottom = (app.editor_scroll_top + app.editor_view_height.saturating_sub(1)).min(max_row);
                if cursor_row > viewport_bottom {
                    move_editor_cursor_to(app, viewport_bottom, cursor_col);
                }
            }
        }

        MouseEventKind::ScrollDown => {
            let line_count = app.editor.line_count();
            let max_scroll = line_count.saturating_sub(app.editor_view_height);
            if app.editor_scroll_top < max_scroll {
                app.editor_scroll_top = (app.editor_scroll_top + 3).min(max_scroll);
                app.editor.set_scroll_offset(app.editor_scroll_top);

                let (cursor_row, cursor_col) = app.editor.cursor();
                let target_row = app.editor_scroll_top.min(line_count.saturating_sub(1));
                if cursor_row < app.editor_scroll_top {
                    move_editor_cursor_to(app, target_row, cursor_col);
                }
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
        DialogState::CreateWikiNote => {
            handle_create_wiki_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::GraphView => {
            handle_graph_view_dialog(app, key);
            return Ok(false);
        }
        DialogState::None => {}
    }

    // Handle welcome dialog
    if app.show_welcome {
        handle_welcome_dialog(app, key);
        return Ok(false);
    }

    // Handle sidebar search input
    if app.search_active {
        handle_search_input(app, key);
        return Ok(false);
    }

    if app.buffer_search.active {
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

fn handle_create_wiki_note_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
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

fn handle_wiki_autocomplete(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
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
                let lines = app.editor.lines();
                let already_closed = if let Some(line) = lines.get(row) {
                    let chars: Vec<char> = line.chars().collect();
                    chars.get(col) == Some(&']') && chars.get(col + 1) == Some(&']')
                } else {
                    false
                };

                if !already_closed {
                    app.editor.insert_str("]]");
                }
                app.wiki_autocomplete = WikiAutocompleteState::None;
                app.update_editor_highlights();
                return true;
            }

            let suggestion = if let WikiAutocompleteState::Open { ref suggestions, selected_index, .. } = app.wiki_autocomplete {
                suggestions.get(selected_index).cloned()
            } else {
                None
            };

            if let Some(suggestion) = suggestion {
                let chars_to_delete = match mode {
                    WikiAutocompleteMode::Note => query.chars().count(),
                    WikiAutocompleteMode::Heading => {
                        query.chars().count()
                    }
                    WikiAutocompleteMode::Alias => 0,
                };

                for _ in 0..chars_to_delete {
                    app.editor.delete_newline();
                }

                if mode == WikiAutocompleteMode::Heading {
                    app.editor.insert_str(&suggestion.insert_text);
                    let already_closed = {
                        let (row, col) = app.editor.cursor();
                        let lines = app.editor.lines();
                        if let Some(line) = lines.get(row) {
                            let chars: Vec<char> = line.chars().collect();
                            chars.get(col) == Some(&']') && chars.get(col + 1) == Some(&']')
                        } else {
                            false
                        }
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
                        let lines = app.editor.lines();
                        if let Some(line) = lines.get(row) {
                            let chars: Vec<char> = line.chars().collect();
                            chars.get(col) == Some(&']') && chars.get(col + 1) == Some(&']')
                        } else {
                            false
                        }
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
                    *selected_index = if *selected_index == 0 {
                        suggestions_len - 1
                    } else {
                        *selected_index - 1
                    };
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
                                let parts: Vec<&str> = target.splitn(2, '#').collect();
                                let note_part = parts[0];
                                let heading_part = parts.get(1).unwrap_or(&"");
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
            let lines = app.editor.lines();
            if let Some(line) = lines.get(row) {
                let chars: Vec<char> = line.chars().collect();
                // Check for ]] pattern (current char should be ])
                if col >= 2 {
                    if chars.get(col.saturating_sub(2)) == Some(&']')
                        && chars.get(col.saturating_sub(1)) == Some(&']')
                    {
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

fn handle_graph_view_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
        if let Some(node_idx) = app.graph_view.selected_node {
            if node_idx < app.graph_view.nodes.len() {
                let move_amount = 2.0;
                match key.code {
                    KeyCode::Char('h') => {
                        app.graph_view.nodes[node_idx].x -= move_amount;
                        return;
                    }
                    KeyCode::Char('j') => {
                        app.graph_view.nodes[node_idx].y += move_amount;
                        return;
                    }
                    KeyCode::Char('k') => {
                        app.graph_view.nodes[node_idx].y -= move_amount;
                        return;
                    }
                    KeyCode::Char('l') => {
                        app.graph_view.nodes[node_idx].x += move_amount;
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.dialog = DialogState::None;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            navigate_graph_node(app, GraphDirection::Left);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            navigate_graph_node(app, GraphDirection::Down);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            navigate_graph_node(app, GraphDirection::Up);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            navigate_graph_node(app, GraphDirection::Right);
        }
        KeyCode::Enter => {
            if let Some(node_idx) = app.graph_view.selected_node {
                if let Some(node) = app.graph_view.nodes.get(node_idx) {
                    let note_idx = node.note_index;
                    for (idx, item) in app.sidebar_items.iter().enumerate() {
                        if let SidebarItemKind::Note { note_index } = &item.kind {
                            if *note_index == note_idx {
                                app.selected_sidebar_index = idx;
                                app.selected_note = note_idx;
                                app.content_cursor = 0;
                                app.content_scroll_offset = 0;
                                app.update_content_items();
                                app.update_outline();
                                app.dialog = DialogState::None;
                                app.focus = Focus::Content;
                                app.push_navigation_history(note_idx);
                                return;
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('H') => {
            app.graph_view.viewport_x -= 10.0;
        }
        KeyCode::Char('J') => {
            app.graph_view.viewport_y += 5.0;
        }
        KeyCode::Char('K') => {
            app.graph_view.viewport_y -= 5.0;
        }
        KeyCode::Char('L') => {
            app.graph_view.viewport_x += 10.0;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.graph_view.zoom = (app.graph_view.zoom * 1.1).min(3.0);
        }
        KeyCode::Char('-') => {
            app.graph_view.zoom = (app.graph_view.zoom / 1.1).max(0.3);
        }
        KeyCode::Char('0') => {
            app.graph_view.zoom = 1.0;
            app.graph_view.viewport_x = 0.0;
            app.graph_view.viewport_y = 0.0;
            app.graph_view.dirty = true;
        }
        KeyCode::Char('g') => {
            if !app.graph_view.nodes.is_empty() {
                app.graph_view.selected_node = Some(0);
                center_on_selected_node(app);
            }
        }
        KeyCode::Char('G') => {
            if !app.graph_view.nodes.is_empty() {
                app.graph_view.selected_node = Some(app.graph_view.nodes.len() - 1);
                center_on_selected_node(app);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy)]
enum GraphDirection {
    Left,
    Right,
    Up,
    Down,
}

fn navigate_graph_node(app: &mut App, direction: GraphDirection) {
    if app.graph_view.nodes.is_empty() {
        return;
    }

    let current = app.graph_view.selected_node.unwrap_or(0);
    if current >= app.graph_view.nodes.len() {
        app.graph_view.selected_node = Some(0);
        return;
    }

    let current_node = &app.graph_view.nodes[current];
    let current_x = current_node.x;
    let current_y = current_node.y;

    let mut best_idx = None;
    let mut best_dist = f32::MAX;

    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        if idx == current {
            continue;
        }

        let dx = node.x - current_x;
        let dy = node.y - current_y;

        let in_direction = match direction {
            GraphDirection::Left => dx < -5.0,
            GraphDirection::Right => dx > 5.0,
            GraphDirection::Up => dy < -2.0,
            GraphDirection::Down => dy > 2.0,
        };

        if in_direction {
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best_dist = dist;
                best_idx = Some(idx);
            }
        }
    }

    if let Some(idx) = best_idx {
        app.graph_view.selected_node = Some(idx);
        center_on_selected_node(app);
    }
}

fn center_on_selected_node(app: &mut App) {
    app.graph_view.needs_center = true;
}

fn handle_graph_view_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(idx) = find_node_at_position(app, mouse_x, mouse_y) {
                app.graph_view.selected_node = Some(idx);
                app.graph_view.dragging_node = Some(idx);
                app.graph_view.drag_start = Some((mouse_x, mouse_y));
                app.graph_view.is_panning = false;
            } else {
                app.graph_view.dragging_node = None;
                app.graph_view.is_panning = true;
                app.graph_view.drag_start = Some((mouse_x, mouse_y));
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.graph_view.is_panning = false;
            app.graph_view.dragging_node = None;
            app.graph_view.drag_start = None;
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((start_x, start_y)) = app.graph_view.drag_start {
                let dx = mouse_x as f32 - start_x as f32;
                let dy = mouse_y as f32 - start_y as f32;

                if let Some(node_idx) = app.graph_view.dragging_node {
                    // Dragging a node - move the node position
                    if node_idx < app.graph_view.nodes.len() {
                        app.graph_view.nodes[node_idx].x += dx / app.graph_view.zoom;
                        app.graph_view.nodes[node_idx].y += dy / app.graph_view.zoom;
                    }
                } else if app.graph_view.is_panning {
                    // Panning the viewport
                    app.graph_view.viewport_x -= dx / app.graph_view.zoom;
                    app.graph_view.viewport_y -= dy / app.graph_view.zoom;
                }

                app.graph_view.drag_start = Some((mouse_x, mouse_y));
            }
        }
        MouseEventKind::ScrollUp => {
            app.graph_view.zoom = (app.graph_view.zoom * 1.1).min(3.0);
        }
        MouseEventKind::ScrollDown => {
            app.graph_view.zoom = (app.graph_view.zoom / 1.1).max(0.3);
        }
        _ => {}
    }
}

fn find_node_at_position(app: &App, mouse_x: u16, mouse_y: u16) -> Option<usize> {
    const NODE_HEIGHT: u16 = 3;

    let vx = app.graph_view.viewport_x;
    let vy = app.graph_view.viewport_y;
    let zoom = app.graph_view.zoom;

    let inner_x = 1u16;
    let inner_y = 1u16;

    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        let screen_x = ((node.x - vx) * zoom + inner_x as f32) as i32;
        let screen_y = ((node.y - vy) * zoom + inner_y as f32) as i32;
        let node_width = node.width as i32;

        if mouse_x as i32 >= screen_x
            && (mouse_x as i32) < (screen_x + node_width)
            && mouse_y as i32 >= screen_y
            && (mouse_y as i32) < (screen_y + NODE_HEIGHT as i32)
        {
            return Some(idx);
        }
    }
    None
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

fn handle_buffer_search_input(app: &mut App, key: crossterm::event::KeyEvent) {
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

fn update_editor_search_highlights(app: &mut App) {
    if app.mode == Mode::Edit {
        let matches: Vec<(usize, usize, usize)> = app
            .buffer_search
            .matches
            .iter()
            .map(|m| (m.row, m.start_col, m.end_col))
            .collect();
        let current_idx = app.buffer_search.current_match_index;
        let match_color = app.theme.search.match_highlight;
        let current_color = app.theme.search.match_current;
        app.editor.set_search_highlights(&matches, current_idx, match_color, current_color);
    }
}

/// Returns true if the app should quit
fn handle_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    let was_pending_g = app.pending_g;
    app.pending_g = false;

    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Tab if !app.zen_mode => app.toggle_focus(false),
        KeyCode::BackTab if !app.zen_mode => app.toggle_focus(true),
        KeyCode::Char('e') => {
            app.push_navigation_history(app.selected_note);
            app.enter_edit_mode();
        }
        KeyCode::Char('n') if !app.zen_mode => {
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
        KeyCode::Char('N') if !app.zen_mode => {
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
        KeyCode::Char('d') if !app.zen_mode => {
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
        KeyCode::Char('r') if !app.zen_mode => {
            if let Some(item) = app.sidebar_items.get(app.selected_sidebar_index) {
                match &item.kind {
                    SidebarItemKind::Note { note_index } => {
                        if let Some(note) = app.notes.get(*note_index) {
                            app.input_buffer = note.title.clone();
                            app.dialog_error = None;
                            app.dialog = DialogState::RenameNote;
                        }
                    }
                    SidebarItemKind::Folder { .. } => {
                        app.input_buffer = item.display_name.clone();
                        app.dialog_error = None;
                        app.dialog = DialogState::RenameFolder;
                    }
                }
            }
        }
        KeyCode::Char('R') if key.modifiers == KeyModifiers::SHIFT | KeyModifiers::CONTROL => {
            app.reload_config();
            app.needs_full_clear = true;
        }
        KeyCode::Char('R') => {
            app.reload_on_focus();
            app.needs_full_clear = true;
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
        KeyCode::Char('-') if app.mode == Mode::Normal && app.focus != Focus::Sidebar => {
            app.navigate_back();
        }
        KeyCode::Char('=') if app.mode == Mode::Normal && app.focus != Focus::Sidebar => {
            app.navigate_forward();
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
        KeyCode::Char('s') => {
            if app.focus == Focus::Sidebar {
                app.cycle_sort_mode();
            }
        }
        KeyCode::Char(' ') => {
            if app.focus == Focus::Content {
                // task items: toggle if checkbox selected, otherwise follow link
                if let Some(crate::app::ContentItem::TaskItem { .. }) = app.content_items.get(app.content_cursor) {
                    if app.is_task_checkbox_selected() {
                        app.toggle_current_task();
                    } else if let Some(link) = app.current_selected_link() {
                        match link {
                            crate::app::LinkInfo::Markdown { url, .. } => {
                                app.open_path_or_url(&url);
                            }
                            crate::app::LinkInfo::Wiki { target, heading, is_valid, .. } => {
                                if is_valid {
                                    app.navigate_to_wiki_link_with_heading(&target, heading.as_deref());
                                } else {
                                    app.pending_wiki_target = Some(target);
                                    app.dialog = DialogState::CreateWikiNote;
                                }
                            }
                        }
                    } else {
                        // No links in task, just toggle
                        app.toggle_current_task();
                    }
                } else if let Some(crate::app::ContentItem::Details { .. }) = app.content_items.get(app.content_cursor) {
                    app.toggle_current_details();
                } else if let Some(link) = app.current_selected_link() {
                    match link {
                        crate::app::LinkInfo::Markdown { url, .. } => {
                            app.open_path_or_url(&url);
                        }
                        crate::app::LinkInfo::Wiki { target, heading, is_valid, .. } => {
                            if is_valid {
                                app.navigate_to_wiki_link_with_heading(&target, heading.as_deref());
                            } else {
                                app.pending_wiki_target = Some(target);
                                app.dialog = DialogState::CreateWikiNote;
                            }
                        }
                    }
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
        KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
            app.start_buffer_search();
        }
        KeyCode::Char('g') if key.modifiers == KeyModifiers::CONTROL => {
            app.build_graph();
            app.dialog = DialogState::GraphView;
        }
        KeyCode::Char('z') => {
            app.toggle_zen_mode();
        }
        KeyCode::Char('g') => {
            if was_pending_g {
                match app.focus {
                    Focus::Sidebar => app.goto_first_sidebar_item(),
                    Focus::Outline => app.goto_first_outline(),
                    Focus::Content => {
                        app.goto_first_content_line();
                        app.sync_outline_to_content();
                    }
                }
            } else {
                app.pending_g = true;
            }
        }
        KeyCode::Char('G') => {
            match app.focus {
                Focus::Sidebar => app.goto_last_sidebar_item(),
                Focus::Outline => app.goto_last_outline(),
                Focus::Content => {
                    app.goto_last_content_line();
                    app.sync_outline_to_content();
                }
            }
        }
        _ => {}
    }
    false
}

fn handle_edit_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if handle_wiki_autocomplete(app, key) {
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
                    VimMode::Replace => handle_vim_replace_mode(app, key),
                    VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => {
                        handle_vim_visual_mode(app, key)
                    }
                }
            }
        }
        app.update_editor_block();
        return;
    }

    // Check the new vim state mode for command mode
    if app.vim.mode.is_command() {
        handle_vim_command_mode(app, key);
        app.update_editor_block();
        return;
    }
    if app.vim.mode.is_search() {
        handle_vim_search_mode(app, key);
        app.update_editor_block();
        return;
    }
    if matches!(app.vim.mode, VimModeNew::SearchLocked { .. }) {
        handle_vim_search_locked_mode(app, key);
        app.update_editor_block();
        return;
    }

    match app.vim_mode {
        VimMode::Normal => handle_vim_normal_mode(app, key),
        VimMode::Insert => handle_vim_insert_mode(app, key),
        VimMode::Replace => handle_vim_replace_mode(app, key),
        VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => {
            handle_vim_visual_mode(app, key)
        }
    }
    app.update_editor_block();
}

fn handle_vim_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    app.vim.status_message = None;

    // Record key for macros (skip q which toggles recording)
    if app.vim.macros.is_recording() && key.code != KeyCode::Char('q') {
        app.vim.macros.record_key(key);
    }

    // Handle pending find (f/F/t/T waiting for char)
    if let Some(pending) = app.vim.pending_find.take() {
        if let KeyCode::Char(c) = key.code {
            let find = pending.into_find_state(c);
            app.vim.last_find = Some(find);
            execute_find(app, find);
        }
        app.vim.reset_pending();
        return;
    }

    // Handle pending register selection ("a, "+, etc.)
    if app.vim.pending_register {
        app.vim.pending_register = false;
        if let KeyCode::Char(c) = key.code {
            // Valid register chars: a-z, A-Z, 0-9, ", -, +, *, etc.
            if c.is_ascii_alphanumeric() || matches!(c, '"' | '-' | '+' | '*' | '_') {
                app.vim.registers.select(c);
            }
        }
        return;
    }

    // Handle awaiting replace char
    if app.vim.awaiting_replace {
        app.vim.awaiting_replace = false;
        if let KeyCode::Char(c) = key.code {
            app.editor.delete_char();
            app.editor.insert_char(c);
            app.editor.move_cursor(CursorMove::Back);
            app.vim.last_change = Some(crate::vim::LastChange::ReplaceChar(c));
        }
        app.vim.reset_pending();
        return;
    }

    // Handle pending text object scope (i or a was pressed)
    if let Some(scope) = app.vim.pending_text_object_scope.take() {
        if let KeyCode::Char(c) = key.code {
            if let Some((_, obj)) = TextObject::parse(if scope == TextObjectScope::Inner { 'i' } else { 'a' }, c) {
                execute_text_object(app, scope, obj);
            }
        }
        app.vim.reset_pending();
        return;
    }

    // Handle pending macro (q or @ was pressed)
    if let Some(pending) = app.vim.pending_macro.take() {
        if let KeyCode::Char(c) = key.code {
            if c.is_ascii_lowercase() {
                match pending {
                    PendingMacro::Record => {
                        if app.vim.macros.is_recording() {
                            app.vim.macros.stop_recording();
                        } else {
                            app.vim.macros.start_recording(c);
                        }
                    }
                    PendingMacro::Play => {
                        if let Some(keys) = app.vim.macros.get_macro(c).cloned() {
                            app.vim.macros.set_last_played(c);
                            let count = app.vim.get_count();
                            for _ in 0..count {
                                for k in &keys {
                                    // Dispatch to correct handler based on current mode
                                    match app.vim_mode {
                                        VimMode::Insert => handle_vim_insert_mode(app, *k),
                                        VimMode::Replace => handle_vim_replace_mode(app, *k),
                                        VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => {
                                            handle_vim_visual_mode(app, *k)
                                        }
                                        _ => handle_vim_normal_mode(app, *k),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        app.vim.reset_pending();
        return;
    }

    // Handle pending mark (m, `, or ' was pressed)
    if let Some(pending) = app.vim.pending_mark.take() {
        if let KeyCode::Char(c) = key.code {
            match pending {
                PendingMark::Set => {
                    let pos = app.editor.cursor();
                    app.vim.marks.set(c, crate::editor::Position::new(pos.0, pos.1));
                }
                PendingMark::GotoExact => {
                    if let Some(pos) = app.vim.marks.get(c) {
                        app.vim.marks.set_last_jump(crate::editor::Position::new(app.editor.cursor().0, app.editor.cursor().1));
                        app.editor.move_cursor(CursorMove::GoToLine(pos.row + 1));
                        for _ in 0..pos.col { app.editor.move_cursor(CursorMove::Forward); }
                    }
                }
                PendingMark::GotoLine => {
                    if let Some(pos) = app.vim.marks.get(c) {
                        app.vim.marks.set_last_jump(crate::editor::Position::new(app.editor.cursor().0, app.editor.cursor().1));
                        app.editor.move_cursor(CursorMove::GoToLine(pos.row + 1));
                        app.editor.move_cursor(CursorMove::FirstNonBlank);
                    }
                }
            }
        }
        app.vim.reset_pending();
        return;
    }

    // Handle pending g (gg, ge, gE, etc.)
    if app.vim.pending_g {
        app.vim.pending_g = false;
        match key.code {
            KeyCode::Char('g') => {
                // Handle operator + gg (linewise motion to start of file or specific line)
                if let Some(op) = app.pending_operator.take() {
                    let target_line = if let Some(count) = app.vim.count.take() {
                        count.saturating_sub(1)
                    } else {
                        0
                    };

                    let (current_row, _) = app.editor.cursor();
                    let (start_row, end_row) = if target_line <= current_row {
                        (target_line, current_row)
                    } else {
                        (current_row, target_line)
                    };

                    app.editor.set_cursor(start_row, 0);
                    app.editor.start_selection();
                    app.editor.set_cursor(end_row, 0);
                    app.editor.move_cursor(CursorMove::End);

                    match op {
                        'd' => {
                            app.editor.cut();
                            if start_row < app.editor.lines().len() {
                                app.editor.set_cursor(start_row, 0);
                            }
                        }
                        'c' => {
                            app.editor.cut();
                            app.vim_mode = VimMode::Insert;
                        }
                        'y' => {
                            app.editor.copy();
                            app.editor.cancel_selection();
                            app.editor.set_cursor(current_row, 0);
                        }
                        _ => {
                            app.editor.cancel_selection();
                        }
                    }
                } else {
                    if let Some(count) = app.vim.count.take() {
                        app.editor.move_cursor(CursorMove::GoToLine(count));
                    } else {
                        app.editor.move_cursor(CursorMove::Top);
                    }
                }
            }
            KeyCode::Char('e') => {
                let count = app.vim.get_count();
                for _ in 0..count { app.editor.move_cursor(CursorMove::WordEndBackward); }
            }
            KeyCode::Char('E') => {
                let count = app.vim.get_count();
                for _ in 0..count { app.editor.move_cursor(CursorMove::BigWordEndBackward); }
            }
            _ => {}
        }
        app.vim.reset_pending();
        return;
    }

    // Handle pending z (zz, zt, zb for scrolling)
    if app.vim.pending_z {
        app.vim.pending_z = false;
        match key.code {
            KeyCode::Char('z') => {
                // zz - center cursor line on screen
                app.editor.center_cursor();
            }
            KeyCode::Char('t') => {
                // zt - scroll cursor line to top
                app.editor.scroll_cursor_to_top();
            }
            KeyCode::Char('b') => {
                // zb - scroll cursor line to bottom
                app.editor.scroll_cursor_to_bottom();
            }
            _ => {}
        }
        app.vim.reset_pending();
        return;
    }

    match key.code {
        // Count accumulation
        KeyCode::Char(c @ '1'..='9') => {
            let digit = c.to_digit(10).unwrap() as usize;
            app.vim.accumulate_count(digit);
            return;
        }
        KeyCode::Char('0') if app.vim.count.is_some() => {
            app.vim.accumulate_count(0);
            return;
        }

        // Register selection - set pending and wait for register char
        KeyCode::Char('"') => {
            app.vim.pending_register = true;
            return;
        }

        // Text object triggers (must come before mode changes)
        KeyCode::Char('i') if app.pending_operator.is_some() => {
            app.vim.pending_text_object_scope = Some(TextObjectScope::Inner);
        }
        KeyCode::Char('a') if app.pending_operator.is_some() => {
            app.vim.pending_text_object_scope = Some(TextObjectScope::Around);
        }

        // Mode changes
        KeyCode::Char('i') => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('a') => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Forward);
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('A') => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::End);
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('I') => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::FirstNonBlank);
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('o') => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.insert_newline();
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('O') => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Head);
            app.editor.insert_newline();
            app.editor.move_cursor(CursorMove::Up);
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('v') if key.modifiers == KeyModifiers::CONTROL => {
            // Visual block mode (Ctrl-V)
            app.vim.reset_pending();
            app.vim_mode = VimMode::VisualBlock;
            app.editor.cancel_selection();
            let (row, col) = app.editor.cursor();
            let anchor = Position { row, col };
            app.visual_block_anchor = Some(anchor);
            app.editor.set_visual_block_selection(anchor, anchor);
        }
        KeyCode::Char('v') => {
            app.vim.reset_pending();
            app.vim_mode = VimMode::Visual;
            app.editor.cancel_selection();
            app.editor.start_selection();
        }
        KeyCode::Char('V') => {
            app.vim.reset_pending();
            app.vim_mode = VimMode::VisualLine;
            let (row, _) = app.editor.cursor();
            app.visual_line_anchor = Some(row);
            app.visual_line_current = Some(row);
            app.editor.set_visual_line_selection(row, row);
        }
        KeyCode::Char('R') => {
            // Replace mode - overwrite characters instead of inserting
            app.vim.reset_pending();
            app.vim_mode = VimMode::Replace;
            app.editor.cancel_selection();
        }
        KeyCode::Char(':') => {
            app.vim.enter_command_mode();
        }

        // Macros (q to record, @ to play)
        KeyCode::Char('q') if key.modifiers.is_empty() => {
            if app.vim.macros.is_recording() {
                app.vim.macros.stop_recording();
            } else {
                app.vim.pending_macro = Some(PendingMacro::Record);
            }
        }
        KeyCode::Char('@') => {
            app.vim.pending_macro = Some(PendingMacro::Play);
        }

        // Marks (m to set, ` or ' to jump)
        KeyCode::Char('m') if key.modifiers.is_empty() => {
            app.vim.pending_mark = Some(PendingMark::Set);
        }
        KeyCode::Char('`') => {
            app.vim.pending_mark = Some(PendingMark::GotoExact);
        }
        KeyCode::Char('\'') => {
            app.vim.pending_mark = Some(PendingMark::GotoLine);
        }

        // Basic motions
        KeyCode::Char('h') | KeyCode::Left => execute_motion_n(app, CursorMove::Back),
        KeyCode::Char('j') | KeyCode::Down => execute_motion_n(app, CursorMove::Down),
        KeyCode::Char('k') | KeyCode::Up => execute_motion_n(app, CursorMove::Up),
        KeyCode::Char('l') | KeyCode::Right => execute_motion_n(app, CursorMove::Forward),

        // Scrolling with Ctrl (must come before plain keys)
        KeyCode::Char('b') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::PageUp);
        }

        // Word motions
        KeyCode::Char('w') => execute_motion_or_operator(app, CursorMove::WordForward),
        KeyCode::Char('W') => execute_motion_or_operator(app, CursorMove::BigWordForward),
        KeyCode::Char('b') => execute_motion_or_operator(app, CursorMove::WordBack),
        KeyCode::Char('B') => execute_motion_or_operator(app, CursorMove::BigWordBack),
        KeyCode::Char('e') => execute_motion_or_operator(app, CursorMove::WordEndForward),
        KeyCode::Char('E') => execute_motion_or_operator(app, CursorMove::BigWordEndForward),

        // Line motions
        KeyCode::Char('0') => execute_motion_or_operator(app, CursorMove::Head),
        KeyCode::Char('^') => execute_motion_or_operator(app, CursorMove::FirstNonBlank),
        KeyCode::Char('$') => execute_motion_or_operator(app, CursorMove::End),

        // Document motions
        KeyCode::Char('g') => {
            app.vim.pending_g = true;
        }
        KeyCode::Char('G') => {
            // Handle operator + G (linewise motion to end of file or specific line)
            if let Some(op) = app.pending_operator.take() {
                let target_line = if let Some(count) = app.vim.count.take() {
                    count.saturating_sub(1) // Convert to 0-indexed
                } else {
                    app.editor.lines().len().saturating_sub(1)
                };

                // Select from current line to target line (linewise)
                let (current_row, _) = app.editor.cursor();
                let (start_row, end_row) = if target_line >= current_row {
                    (current_row, target_line)
                } else {
                    (target_line, current_row)
                };

                app.editor.set_cursor(start_row, 0);
                app.editor.start_selection();
                app.editor.set_cursor(end_row, 0);
                app.editor.move_cursor(CursorMove::End);

                match op {
                    'd' => {
                        app.editor.cut();
                        // Delete from start line to end line (inclusive)
                        if start_row < app.editor.lines().len() {
                            app.editor.set_cursor(start_row, 0);
                        }
                    }
                    'c' => {
                        app.editor.cut();
                        app.vim_mode = VimMode::Insert;
                    }
                    'y' => {
                        app.editor.copy();
                        app.editor.cancel_selection();
                        app.editor.set_cursor(current_row, 0);
                    }
                    _ => {
                        app.editor.cancel_selection();
                    }
                }
            } else {
                if let Some(count) = app.vim.count.take() {
                    app.editor.move_cursor(CursorMove::GoToLine(count));
                } else {
                    app.editor.move_cursor(CursorMove::Bottom);
                }
            }
            app.vim.reset_pending();
        }

        // Paragraph motions
        KeyCode::Char('{') => execute_motion_n(app, CursorMove::ParagraphBack),
        KeyCode::Char('}') => execute_motion_n(app, CursorMove::ParagraphForward),

        // Screen motions
        KeyCode::Char('H') => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::ScreenTop);
        }
        KeyCode::Char('M') => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::ScreenMiddle);
        }
        KeyCode::Char('L') => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::ScreenBottom);
        }

        // z commands (zz, zt, zb for scroll positioning)
        KeyCode::Char('z') => {
            app.vim.pending_z = true;
        }

        // Find char
        KeyCode::Char('f') if key.modifiers.is_empty() => {
            app.vim.pending_find = Some(PendingFind::new(true, false));
        }
        KeyCode::Char('F') => {
            app.vim.pending_find = Some(PendingFind::new(false, false));
        }
        KeyCode::Char('t') if key.modifiers.is_empty() => {
            app.vim.pending_find = Some(PendingFind::new(true, true));
        }
        KeyCode::Char('T') => {
            app.vim.pending_find = Some(PendingFind::new(false, true));
        }
        KeyCode::Char(';') => {
            if let Some(find) = app.vim.last_find {
                let count = app.vim.get_count();
                for _ in 0..count { execute_find(app, find); }
            }
            app.vim.reset_pending();
        }
        KeyCode::Char(',') => {
            if let Some(find) = app.vim.last_find {
                let count = app.vim.get_count();
                for _ in 0..count { execute_find(app, find.reversed()); }
            }
            app.vim.reset_pending();
        }

        // Matching bracket
        KeyCode::Char('%') => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::MatchingBracket);
        }

        // Scrolling
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::HalfPageUp);
        }
        KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.reset_pending();
            app.editor.move_cursor(CursorMove::HalfPageDown);
        }
        KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.reset_pending();
            app.start_buffer_search();
        }

        // Operators
        KeyCode::Char('d') => {
            if app.pending_operator == Some('d') {
                // dd: delete line
                app.pending_operator = None;
                let count = app.vim.get_count();
                for _ in 0..count {
                    app.editor.delete_current_line();
                }
                app.vim.last_change = Some(crate::vim::LastChange::DeleteLine(count));
                app.vim.reset_pending();
            } else {
                app.pending_operator = Some('d');
            }
        }
        KeyCode::Char('c') => {
            if app.pending_operator == Some('c') {
                // cc: change line
                app.pending_operator = None;
                app.editor.move_cursor(CursorMove::Head);
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::End);
                app.editor.cut();
                app.vim_mode = VimMode::Insert;
                app.vim.reset_pending();
            } else {
                app.pending_operator = Some('c');
            }
        }
        KeyCode::Char('y') if key.modifiers.is_empty() => {
            if app.pending_operator == Some('y') {
                // yy: yank line
                app.pending_operator = None;
                app.editor.move_cursor(CursorMove::Head);
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::End);
                app.editor.copy();
                app.editor.cancel_selection();
                app.vim.reset_pending();
            } else {
                app.pending_operator = Some('y');
            }
        }
        KeyCode::Char('>') => {
            if app.pending_operator == Some('>') {
                // >>: indent line
                app.pending_operator = None;
                let count = app.vim.get_count();
                for _ in 0..count {
                    app.editor.move_cursor(CursorMove::Head);
                    app.editor.insert_str("    ");
                }
                app.vim.reset_pending();
            } else {
                app.pending_operator = Some('>');
            }
        }
        KeyCode::Char('<') => {
            if app.pending_operator == Some('<') {
                // <<: outdent line
                app.pending_operator = None;
                // Simplified: remove up to 4 spaces from start
                let count = app.vim.get_count();
                for _ in 0..count {
                    app.editor.move_cursor(CursorMove::Head);
                    for _ in 0..4 {
                        let pos = app.editor.cursor();
                        if let Some(line) = app.editor.lines().get(pos.0) {
                            if line.starts_with(' ') || line.starts_with('\t') {
                                app.editor.delete_char();
                            }
                        }
                    }
                }
                app.vim.reset_pending();
            } else {
                app.pending_operator = Some('<');
            }
        }

        // Quick actions
        KeyCode::Char('x') => {
            let count = app.vim.get_count();
            let mut deleted = 0;
            for _ in 0..count {
                let (row, col) = app.editor.cursor();
                let line_len = app.editor.lines().get(row).map_or(0, |l| l.chars().count());
                if col < line_len {
                    app.editor.delete_char();
                    deleted += 1;
                } else {
                    break;
                }
            }
            if deleted > 0 {
                app.vim.last_change = Some(crate::vim::LastChange::DeleteCharForward(deleted));
            }
            app.vim.reset_pending();
        }
        KeyCode::Char('X') => {
            let count = app.vim.get_count();
            for _ in 0..count { app.editor.delete_newline(); }
            app.vim.last_change = Some(crate::vim::LastChange::DeleteCharBackward(count));
            app.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers.is_empty() => {
            app.editor.delete_char();
            app.vim_mode = VimMode::Insert;
            app.vim.reset_pending();
        }
        KeyCode::Char('S') => {
            app.editor.move_cursor(CursorMove::Head);
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
            app.vim_mode = VimMode::Insert;
            app.vim.reset_pending();
        }
        KeyCode::Char('D') => {
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
            app.vim.reset_pending();
        }
        KeyCode::Char('C') => {
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
            app.vim_mode = VimMode::Insert;
            app.vim.reset_pending();
        }
        KeyCode::Char('Y') => {
            app.editor.move_cursor(CursorMove::Head);
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.copy();
            app.editor.cancel_selection();
            app.vim.reset_pending();
        }
        KeyCode::Char('r') if key.modifiers.is_empty() => {
            app.vim.awaiting_replace = true;
        }
        KeyCode::Char('J') => {
            // Join lines
            app.editor.move_cursor(CursorMove::End);
            app.editor.delete_char();
            app.editor.insert_char(' ');
            app.vim.reset_pending();
        }
        KeyCode::Char('~') => {
            // Toggle case
            let pos = app.editor.cursor();
            if let Some(line) = app.editor.lines().get(pos.0) {
                let chars: Vec<char> = line.chars().collect();
                if let Some(&c) = chars.get(pos.1) {
                    app.editor.delete_char();
                    if c.is_uppercase() {
                        app.editor.insert_char(c.to_lowercase().next().unwrap_or(c));
                    } else {
                        app.editor.insert_char(c.to_uppercase().next().unwrap_or(c));
                    }
                }
            }
            app.vim.reset_pending();
        }

        // Paste
        KeyCode::Char('p') => {
            app.editor.paste_after();
            app.vim.reset_pending();
        }
        KeyCode::Char('P') => {
            app.editor.paste_before();
            app.vim.reset_pending();
        }

        // Undo/Redo
        KeyCode::Char('u') if key.modifiers.is_empty() => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.undo();
        }
        KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.redo();
        }

        // Repeat last change (.)
        KeyCode::Char('.') => {
            if let Some(change) = app.vim.last_change.clone() {
                repeat_last_change(app, change);
            }
            app.vim.reset_pending();
        }

        // Save/Exit
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.reset_pending();
            app.editor.cancel_selection();
            app.save_edit();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Esc => {
            app.vim.reset_pending();
            app.pending_operator = None;
            app.editor.cancel_selection();

            if app.has_unsaved_changes() {
                app.dialog = DialogState::UnsavedChanges;
            } else {
                app.cancel_edit();
                app.vim_mode = VimMode::Normal;
            }
        }

        // Search - enter search mode in status bar
        KeyCode::Char('/') => {
            app.vim.reset_pending();
            app.vim.search_buffer.clear();
            app.buffer_search.query.clear();
            app.buffer_search.matches.clear();
            update_editor_search_highlights(app);
            app.vim.mode = VimModeNew::Search { forward: true };
        }
        KeyCode::Char('?') => {
            app.vim.reset_pending();
            app.vim.search_buffer.clear();
            app.buffer_search.query.clear();
            app.buffer_search.matches.clear();
            update_editor_search_highlights(app);
            app.vim.mode = VimModeNew::Search { forward: false };
        }
        KeyCode::Char('n') => {
            app.vim.reset_pending();
            if !app.buffer_search.matches.is_empty() {
                match app.buffer_search.direction {
                    crate::app::SearchDirection::Forward => app.buffer_search_next(),
                    crate::app::SearchDirection::Backward => app.buffer_search_prev(),
                }
            }
        }
        KeyCode::Char('N') => {
            app.vim.reset_pending();
            if !app.buffer_search.matches.is_empty() {
                match app.buffer_search.direction {
                    crate::app::SearchDirection::Forward => app.buffer_search_prev(),
                    crate::app::SearchDirection::Backward => app.buffer_search_next(),
                }
            }
        }
        KeyCode::Char('*') => {
            // Search word under cursor forward
            app.vim.reset_pending();
            // Get word under cursor and search
        }
        KeyCode::Char('#') => {
            // Search word under cursor backward
            app.vim.reset_pending();
        }

        _ => {
            app.vim.reset_pending();
            app.pending_operator = None;
        }
    }
}

/// Repeat the last change command (. dot command)
fn repeat_last_change(app: &mut App, change: crate::vim::LastChange) {
    use crate::vim::LastChange;
    match change {
        LastChange::DeleteLine(count) => {
            for _ in 0..count {
                app.editor.delete_current_line();
            }
        }
        LastChange::DeleteCharForward(count) => {
            for _ in 0..count {
                app.editor.delete_char();
            }
        }
        LastChange::DeleteCharBackward(count) => {
            for _ in 0..count {
                app.editor.delete_newline();
            }
        }
        LastChange::ReplaceChar(c) => {
            app.editor.delete_char();
            app.editor.insert_char(c);
            app.editor.move_cursor(CursorMove::Back);
        }
        LastChange::DeleteToEnd => {
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
        }
        LastChange::DeleteWordForward(count) => {
            for _ in 0..count {
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::WordForward);
                app.editor.cut();
            }
        }
        LastChange::DeleteWordBackward(count) => {
            for _ in 0..count {
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::WordBack);
                app.editor.cut();
            }
        }
        // These require insert mode text replay - complex, skip for now
        LastChange::ChangeLine(_, _) |
        LastChange::YankLine(_) |
        LastChange::ChangeToEnd(_) |
        LastChange::SubstituteChar(_) |
        LastChange::Insert(_, _) |
        LastChange::ChangeWord(_, _) => {
            // TODO: Implement insert text replay
        }
    }
}

fn execute_motion_n(app: &mut App, movement: CursorMove) {
    let count = app.vim.get_count();
    app.vim.reset_pending();
    app.editor.cancel_selection();
    for _ in 0..count {
        app.editor.move_cursor(movement);
    }
}

fn execute_motion_or_operator(app: &mut App, movement: CursorMove) {
    use crate::vim::LastChange;

    let count = app.vim.get_count();
    if let Some(op) = app.pending_operator.take() {
        let start_pos = app.editor.cursor();
        let start_row = start_pos.0;

        app.editor.cancel_selection();
        app.editor.start_selection();

        let is_word_forward = matches!(movement, CursorMove::WordForward | CursorMove::BigWordForward);

        if is_word_forward {
            // For word forward motions with operators, we need special handling:
            // 1. dw should delete to end of line if word motion would cross lines
            // 2. cw should behave like ce (change to end of word, not including trailing space)
            for _ in 0..count {
                let (row, _) = app.editor.cursor();
                let line = app.editor.lines().get(row).map(|s| s.to_string());
                let line_len = line.as_ref().map_or(0, |l| l.chars().count());
                app.editor.move_cursor(movement);

                let (new_row, _) = app.editor.cursor();
                if new_row > row {
                    app.editor.set_cursor(row, line_len);
                    break;
                }
            }

            if op == 'c' {
                let (end_row, end_col) = app.editor.cursor();
                if end_row == start_row {
                    if let Some(line) = app.editor.lines().get(end_row) {
                        let chars: Vec<char> = line.chars().collect();
                        let mut adjusted_col = end_col;
                        while adjusted_col > start_pos.1 && adjusted_col > 0 {
                            if let Some(&c) = chars.get(adjusted_col.saturating_sub(1)) {
                                if c.is_whitespace() {
                                    adjusted_col -= 1;
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        if adjusted_col > start_pos.1 {
                            app.editor.set_cursor(end_row, adjusted_col);
                        }
                    }
                }
            }
        } else {
            for _ in 0..count { app.editor.move_cursor(movement); }
        }

        match op {
            'd' => {
                app.editor.cut();
                // Record last_change for dot command
                match movement {
                    CursorMove::WordForward | CursorMove::BigWordForward => {
                        app.vim.last_change = Some(LastChange::DeleteWordForward(count));
                    }
                    CursorMove::WordBack | CursorMove::BigWordBack => {
                        app.vim.last_change = Some(LastChange::DeleteWordBackward(count));
                    }
                    CursorMove::End => {
                        app.vim.last_change = Some(LastChange::DeleteToEnd);
                    }
                    _ => {}
                }
            }
            'c' => {
                app.editor.cut();
                app.vim_mode = VimMode::Insert;
                // Note: Change operations need insert text to be recorded on exit from insert mode
            }
            'y' => { app.editor.copy(); app.editor.cancel_selection(); }
            '>' => {
                if let Some((start, _)) = app.editor.selection_range() {
                    app.editor.cancel_selection();
                    app.editor.set_cursor(start.row, 0);
                    app.editor.insert_str("    ");
                }
            }
            '<' => {
                if let Some((start, _)) = app.editor.selection_range() {
                    app.editor.cancel_selection();
                    app.editor.set_cursor(start.row, 0);
                    for _ in 0..4 {
                        let pos = app.editor.cursor();
                        if let Some(line) = app.editor.lines().get(pos.0) {
                            if line.starts_with(' ') || line.starts_with('\t') {
                                app.editor.delete_char();
                            }
                        }
                    }
                }
            }
            _ => { app.editor.cancel_selection(); }
        }
    } else {
        app.editor.cancel_selection();
        for _ in 0..count { app.editor.move_cursor(movement); }
    }
    app.vim.reset_pending();
}

fn execute_find(app: &mut App, find: FindState) {
    let pos = app.editor.cursor();
    if let Some(line) = app.editor.lines().get(pos.0) {
        if let Some(new_col) = find.find_in_line(line, pos.1) {
            // Check for pending operator (d, c, y, etc.)
            if let Some(op) = app.pending_operator.take() {
                app.editor.start_selection();
                app.editor.set_cursor(pos.0, new_col);
                match op {
                    'd' => {
                        app.editor.cut();
                    }
                    'c' => {
                        app.editor.cut();
                        app.vim_mode = VimMode::Insert;
                    }
                    'y' => {
                        app.editor.copy();
                        app.editor.cancel_selection();
                        // Return to start position for yank
                        app.editor.set_cursor(pos.0, pos.1);
                    }
                    _ => {
                        app.editor.cancel_selection();
                    }
                }
            } else {
                app.editor.set_cursor(pos.0, new_col);
            }
        }
    }
}

fn execute_text_object(app: &mut App, scope: TextObjectScope, obj: TextObject) {
    let pos = app.editor.cursor();
    let lines_owned = app.editor.lines();
    let lines: Vec<&str> = lines_owned.iter().map(|s| &**s).collect();
    let cursor_pos = crate::editor::Position::new(pos.0, pos.1);

    if let Some((start, end)) = obj.find_bounds(scope, &lines, cursor_pos) {
        if let Some(op) = app.pending_operator.take() {
            app.editor.set_cursor(start.row, start.col);
            app.editor.start_selection();
            app.editor.set_cursor(end.row, end.col);
            match op {
                'd' => { app.editor.cut(); }
                'c' => { app.editor.cut(); app.vim_mode = VimMode::Insert; }
                'y' => { app.editor.copy(); app.editor.cancel_selection(); app.editor.set_cursor(start.row, start.col); }
                _ => { app.editor.cancel_selection(); }
            }
        }
    }
}

/// apply block insert/append text to all lines in the visual block selection
fn apply_block_insert(app: &mut App, state: BlockInsertState) {
    let (current_row, current_col) = app.editor.cursor();
    let lines = app.editor.lines();
    if let Some(line) = lines.get(state.active_row) {
        let chars: Vec<char> = line.chars().collect();
        let insert_start = state.start_col;
        let insert_end = current_col;

        if insert_end > insert_start {
            let inserted_text: String = chars
                .iter()
                .skip(insert_start)
                .take(insert_end - insert_start)
                .collect();
            let (start_row, end_row) = state.rows;
            for row in start_row..=end_row {
                if row == state.active_row {
                    continue; 
                }

                let line_len = app.editor.lines().get(row).map(|l| l.chars().count()).unwrap_or(0);
                let insert_pos = match state.mode {
                    BlockInsertMode::Insert => state.insert_col.min(line_len),
                    BlockInsertMode::Append => {
                        state.insert_col
                    }
                };

                app.editor.set_cursor(row, insert_pos);
                if state.mode == BlockInsertMode::Append && insert_pos > line_len {
                    let padding: String = " ".repeat(insert_pos - line_len);
                    for c in padding.chars() {
                        app.editor.insert_char(c);
                    }
                }

                for c in inserted_text.chars() {
                    app.editor.insert_char(c);
                }
            }

            app.editor.set_cursor(current_row, current_col);
        }
    }
}

fn handle_vim_insert_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.vim.macros.is_recording() {
        app.vim.macros.record_key(key);
    }

    match key.code {
        KeyCode::Esc => {
            if let Some(state) = app.block_insert_state.take() {
                apply_block_insert(app, state);
            }
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            if let Some(state) = app.block_insert_state.take() {
                apply_block_insert(app, state);
            }
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
            app.vim.mode = VimModeNew::Normal;
            app.start_buffer_search();
        }
        KeyCode::Char('[') => {
            app.editor.input(key);

            let (row, col) = app.editor.cursor();
            if !app.is_cursor_in_code(row, col) {
                let lines = app.editor.lines();
                if let Some(line) = lines.get(row) {
                    let chars: Vec<char> = line.chars().collect();
                    if col >= 2 {
                        if chars.get(col.saturating_sub(2)) == Some(&'[')
                            && chars.get(col.saturating_sub(1)) == Some(&'[')
                        {
                            let trigger_pos = (row, col.saturating_sub(2));
                            let suggestions = app.build_wiki_suggestions("");
                            app.wiki_autocomplete = WikiAutocompleteState::Open {
                                trigger_pos,
                                query: String::new(),
                                suggestions,
                                selected_index: 0,
                                mode: WikiAutocompleteMode::Note,
                                target_note: None,
                            };
                        }
                    }
                }
            }
        }
        _ => {
            app.editor.input(key);
            if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete | KeyCode::Enter) {
                app.update_editor_highlights();

                if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) {
                    let (row, col) = app.editor.cursor();
                    if !app.is_cursor_in_code(row, col) {
                        if let Some((note_query, heading_query, alias_query, mode)) = app.detect_unclosed_wikilink(row, col) {
                            let (query, suggestions, target_note) = match mode {
                                WikiAutocompleteMode::Note => {
                                    let suggestions = app.build_wiki_suggestions(&note_query);
                                    (note_query, suggestions, None)
                                }
                                WikiAutocompleteMode::Heading => {
                                    let heading_q = heading_query.unwrap_or_default();
                                    let suggestions = app.build_heading_suggestions(&note_query, &heading_q);
                                    (heading_q, suggestions, Some(note_query))
                                }
                                WikiAutocompleteMode::Alias => {
                                    let full_target = if let Some(ref h) = heading_query {
                                        format!("{}#{}", note_query, h)
                                    } else {
                                        note_query
                                    };
                                    (alias_query.unwrap_or_default(), Vec::new(), Some(full_target))
                                }
                            };

                            app.wiki_autocomplete = WikiAutocompleteState::Open {
                                trigger_pos: (row, 0),
                                query,
                                suggestions,
                                selected_index: 0,
                                mode,
                                target_note,
                            };
                        }
                    }
                }
            }
        }
    }
}

fn handle_vim_replace_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.vim.macros.is_recording() {
        app.vim.macros.record_key(key);
    }

    match key.code {
        KeyCode::Esc => {
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Backspace => {
            // In Replace mode, backspace just moves cursor back
            app.editor.move_cursor(CursorMove::Back);
        }
        KeyCode::Left => {
            app.editor.move_cursor(CursorMove::Back);
        }
        KeyCode::Right => {
            app.editor.move_cursor(CursorMove::Forward);
        }
        KeyCode::Up => {
            app.editor.move_cursor(CursorMove::Up);
        }
        KeyCode::Down => {
            app.editor.move_cursor(CursorMove::Down);
        }
        KeyCode::Enter => {
            // Enter creates a new line in replace mode
            app.editor.insert_newline();
            app.update_editor_highlights();
        }
        KeyCode::Char(c) => {
            // Overwrite: delete current char (if not at end of line) then insert new char
            let (row, col) = app.editor.cursor();
            if let Some(line) = app.editor.lines().get(row) {
                if col < line.chars().count() {
                    app.editor.delete_char();
                }
            }
            app.editor.insert_char(c);
            app.update_editor_highlights();
        }
        _ => {}
    }
}

fn handle_vim_visual_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.vim.macros.is_recording() {
        app.vim.macros.record_key(key);
    }

    // Helper to update visual line selection in VisualLine mode
    // target_row is where the cursor logically should be (determines selection extent)
    let reselect_lines_at = |app: &mut App, target_row: usize| {
        if app.vim_mode == VimMode::VisualLine {
            if let Some(anchor) = app.visual_line_anchor {
                // Update current row tracker
                app.visual_line_current = Some(target_row);
                // Update editor's visual line selection for rendering
                app.editor.set_visual_line_selection(anchor, target_row);
                // Move cursor to the target row
                app.editor.set_cursor(target_row, app.editor.cursor().1);
            }
        }
    };

    // Helper to update visual block selection in VisualBlock mode
    let update_block_selection = |app: &mut App| {
        if app.vim_mode == VimMode::VisualBlock {
            if let Some(anchor) = app.visual_block_anchor {
                let (row, col) = app.editor.cursor();
                let current = Position { row, col };
                app.editor.set_visual_block_selection(anchor, current);
            }
        }
    };

    match key.code {
        KeyCode::Esc => {
            app.editor.cancel_selection();
            app.editor.clear_visual_line_selection();
            app.editor.clear_visual_block_selection();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.visual_line_anchor = None;
            app.visual_line_current = None;
            app.visual_block_anchor = None;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if app.vim_mode == VimMode::VisualLine {
                let (current_row, _) = app.editor.cursor();
                app.editor.move_cursor(CursorMove::Back);
                let (new_row, _) = app.editor.cursor();
                if new_row != current_row {
                    reselect_lines_at(app, new_row);
                }
            } else {
                app.editor.move_cursor(CursorMove::Back);
                update_block_selection(app);
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if app.vim_mode == VimMode::VisualLine {
                if let Some(current_row) = app.visual_line_current {
                    let line_count = app.editor.lines().len();
                    if current_row + 1 < line_count {
                        let new_row = current_row + 1;
                        reselect_lines_at(app, new_row);
                    }
                }
            } else {
                app.editor.move_cursor(CursorMove::Down);
                update_block_selection(app);
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.vim_mode == VimMode::VisualLine {
                if let Some(current_row) = app.visual_line_current {
                    if current_row > 0 {
                        let new_row = current_row - 1;
                        reselect_lines_at(app, new_row);
                    }
                }
            } else {
                app.editor.move_cursor(CursorMove::Up);
                update_block_selection(app);
            }
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if app.vim_mode == VimMode::VisualLine {
                let (current_row, _) = app.editor.cursor();
                app.editor.move_cursor(CursorMove::Forward);
                let (new_row, _) = app.editor.cursor();
                if new_row != current_row {
                    reselect_lines_at(app, new_row);
                }
            } else {
                app.editor.move_cursor(CursorMove::Forward);
                update_block_selection(app);
            }
        }
        KeyCode::Char('w') => {
            if app.vim_mode == VimMode::VisualLine {
                let (current_row, _) = app.editor.cursor();
                app.editor.move_cursor(CursorMove::WordForward);
                let (new_row, _) = app.editor.cursor();
                if new_row != current_row {
                    reselect_lines_at(app, new_row);
                } else {
                    reselect_lines_at(app, current_row);
                }
            } else {
                app.editor.move_cursor(CursorMove::WordForward);
                update_block_selection(app);
            }
        }
        KeyCode::Char('b') => {
            if app.vim_mode == VimMode::VisualLine {
                let (current_row, _) = app.editor.cursor();
                app.editor.move_cursor(CursorMove::WordBack);
                let (new_row, _) = app.editor.cursor();
                if new_row != current_row {
                    reselect_lines_at(app, new_row);
                } else {
                    reselect_lines_at(app, current_row);
                }
            } else {
                app.editor.move_cursor(CursorMove::WordBack);
                update_block_selection(app);
            }
        }
        KeyCode::Char('0') => {
            if app.vim_mode != VimMode::VisualLine {
                app.editor.move_cursor(CursorMove::Head);
                update_block_selection(app);
            }
        }
        KeyCode::Char('$') => {
            if app.vim_mode != VimMode::VisualLine {
                app.editor.move_cursor(CursorMove::End);
                update_block_selection(app);
            }
        }
        KeyCode::Char('g') => {
            if app.vim_mode == VimMode::VisualLine {
                reselect_lines_at(app, 0);
            } else {
                app.editor.move_cursor(CursorMove::Top);
                update_block_selection(app);
            }
        }
        KeyCode::Char('G') => {
            if app.vim_mode == VimMode::VisualLine {
                let line_count = app.editor.lines().len();
                reselect_lines_at(app, line_count.saturating_sub(1));
            } else {
                app.editor.move_cursor(CursorMove::Bottom);
                update_block_selection(app);
            }
        }
        KeyCode::Char('y') => {
            if app.vim_mode == VimMode::VisualLine {
                app.editor.copy_visual_lines();
            } else if app.vim_mode == VimMode::VisualBlock {
                app.editor.copy_visual_block();
            } else {
                app.editor.copy();
            }
            app.editor.cancel_selection();
            app.editor.clear_visual_line_selection();
            app.editor.clear_visual_block_selection();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.visual_line_anchor = None;
            app.visual_line_current = None;
            app.visual_block_anchor = None;
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            if app.vim_mode == VimMode::VisualLine {
                app.editor.cut_visual_lines();
            } else if app.vim_mode == VimMode::VisualBlock {
                app.editor.cut_visual_block();
            } else {
                app.editor.cut();
            }
            app.editor.cancel_selection();
            app.editor.clear_visual_line_selection();
            app.editor.clear_visual_block_selection();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.visual_line_anchor = None;
            app.visual_line_current = None;
            app.visual_block_anchor = None;
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.cancel_selection();
            app.editor.clear_visual_line_selection();
            app.editor.clear_visual_block_selection();
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.visual_line_anchor = None;
            app.visual_line_current = None;
            app.visual_block_anchor = None;
        }
        KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
            // Open buffer search (cancel selection first)
            app.editor.cancel_selection();
            app.editor.clear_visual_line_selection();
            app.editor.clear_visual_block_selection();
            app.vim_mode = VimMode::Normal;
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.visual_line_anchor = None;
            app.visual_line_current = None;
            app.visual_block_anchor = None;
            app.start_buffer_search();
        }
        KeyCode::Char('I') if app.vim_mode == VimMode::VisualBlock => {
            if let Some(anchor) = app.visual_block_anchor {
                let (current_row, current_col) = app.editor.cursor();
                let current = Position { row: current_row, col: current_col };

                let (start_row, end_row) = if anchor.row <= current.row {
                    (anchor.row, current.row)
                } else {
                    (current.row, anchor.row)
                };
                let insert_col = anchor.col.min(current.col);
                app.block_insert_state = Some(BlockInsertState {
                    mode: BlockInsertMode::Insert,
                    rows: (start_row, end_row),
                    insert_col,
                    active_row: start_row,
                    start_col: insert_col,
                });
                app.editor.clear_visual_block_selection();
                app.visual_block_anchor = None;
                app.editor.set_cursor(start_row, insert_col);
                app.vim_mode = VimMode::Insert;
                app.vim.mode = VimModeNew::Insert;
            }
        }
        KeyCode::Char('A') if app.vim_mode == VimMode::VisualBlock => {
            if let Some(anchor) = app.visual_block_anchor {
                let (current_row, current_col) = app.editor.cursor();
                let current = Position { row: current_row, col: current_col };
                let (start_row, end_row) = if anchor.row <= current.row {
                    (anchor.row, current.row)
                } else {
                    (current.row, anchor.row)
                };
                let right_col = anchor.col.max(current.col);
                let insert_col = right_col + 1;
                app.block_insert_state = Some(BlockInsertState {
                    mode: BlockInsertMode::Append,
                    rows: (start_row, end_row),
                    insert_col,
                    active_row: start_row,
                    start_col: insert_col,
                });

                app.editor.clear_visual_block_selection();
                app.visual_block_anchor = None;
                app.editor.set_cursor(start_row, insert_col);
                app.vim_mode = VimMode::Insert;
                app.vim.mode = VimModeNew::Insert;
            }
        }
        _ => {}
    }
}

fn handle_vim_command_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            // Cancel command mode
            app.vim.command_buffer.clear();
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Enter => {
            // Execute command
            let cmd = app.vim.command_buffer.clone();
            app.vim.command_buffer.clear();
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();

            if let Some(command) = parse_command(&cmd) {
                execute_vim_command(app, command);
            }
        }
        KeyCode::Backspace => {
            app.vim.command_buffer.pop();
            // If buffer is empty, exit command mode
            if app.vim.command_buffer.is_empty() {
                app.vim.mode = VimModeNew::Normal;
                app.vim.reset_pending();
            }
        }
        KeyCode::Char(c) => {
            app.vim.command_buffer.push(c);
        }
        _ => {}
    }
}

fn handle_vim_search_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    let forward = matches!(app.vim.mode, VimModeNew::Search { forward: true });

    match key.code {
        KeyCode::Esc => {
            app.vim.search_buffer.clear();
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.buffer_search.query.clear();
            app.buffer_search.matches.clear();
            update_editor_search_highlights(app);
        }
        KeyCode::Enter => {
            if !app.vim.search_buffer.is_empty() {
                app.vim.search_pattern = Some(app.vim.search_buffer.clone());
                app.vim.search_direction = if forward {
                    crate::vim::SearchDirection::Forward
                } else {
                    crate::vim::SearchDirection::Backward
                };
                app.buffer_search.query = app.vim.search_buffer.clone();
                app.buffer_search.direction = if forward {
                    crate::app::SearchDirection::Forward
                } else {
                    crate::app::SearchDirection::Backward
                };

                app.perform_buffer_search();

                if !app.buffer_search.matches.is_empty() {
                    if forward {
                        app.buffer_search_next();
                    } else {
                        app.buffer_search_prev();
                    }
                    update_editor_search_highlights(app);
                    app.vim.status_message = None;
                    app.vim.mode = VimModeNew::SearchLocked { forward };
                    app.vim.reset_pending();
                    return;
                } else {
                    app.vim.status_message = Some(format!("Pattern not found: {}", app.vim.search_buffer));
                    app.vim.mode = VimModeNew::Normal;
                    app.vim.reset_pending();
                    return;
                }
            }

            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Backspace => {
            if app.vim.search_buffer.is_empty() {
                app.vim.mode = VimModeNew::Normal;
                app.vim.reset_pending();
                app.buffer_search.query.clear();
                app.buffer_search.matches.clear();
                update_editor_search_highlights(app);
            } else {
                app.vim.search_buffer.pop();
                app.buffer_search.query = app.vim.search_buffer.clone();
                app.perform_buffer_search();
                if !app.buffer_search.matches.is_empty() {
                    app.scroll_to_current_match();
                }
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Char(c) => {
            app.vim.search_buffer.push(c);
            app.buffer_search.query = app.vim.search_buffer.clone();
            app.perform_buffer_search();
            if !app.buffer_search.matches.is_empty() {
                app.scroll_to_current_match();
            }
            update_editor_search_highlights(app);
        }
        _ => {}
    }
}

fn handle_vim_search_locked_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    let forward = matches!(app.vim.mode, VimModeNew::SearchLocked { forward: true });

    match key.code {
        KeyCode::Esc => {
            app.vim.mode = VimModeNew::Search { forward };
        }
        KeyCode::Char('n') => {
            if !app.buffer_search.matches.is_empty() {
                match app.buffer_search.direction {
                    crate::app::SearchDirection::Forward => app.buffer_search_next(),
                    crate::app::SearchDirection::Backward => app.buffer_search_prev(),
                }
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Char('N') => {
            if !app.buffer_search.matches.is_empty() {
                match app.buffer_search.direction {
                    crate::app::SearchDirection::Forward => app.buffer_search_prev(),
                    crate::app::SearchDirection::Backward => app.buffer_search_next(),
                }
                update_editor_search_highlights(app);
            }
        }
        KeyCode::Enter => {
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Char('/') => {
            app.vim.search_buffer.clear();
            app.buffer_search.query.clear();
            app.buffer_search.matches.clear();
            update_editor_search_highlights(app);
            app.vim.mode = VimModeNew::Search { forward: true };
        }
        KeyCode::Char('?') => {
            app.vim.search_buffer.clear();
            app.buffer_search.query.clear();
            app.buffer_search.matches.clear();
            update_editor_search_highlights(app);
            app.vim.mode = VimModeNew::Search { forward: false };
        }
        _ => {
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
            app.buffer_search.query.clear();
            app.buffer_search.matches.clear();
            app.vim.search_buffer.clear();
            update_editor_search_highlights(app);
        }
    }
}

fn execute_vim_command(app: &mut App, command: Command) {
    match command {
        Command::Write => {
            app.save_edit();
        }
        Command::Quit => {
            // Exit edit mode without saving
            app.cancel_edit();
        }
        Command::WriteQuit => {
            app.save_edit();
        }
        Command::ForceQuit => {
            // Force quit without saving
            app.cancel_edit();
        }
        Command::GoToLine(line) => {
            // Go to specific line (1-indexed in vim)
            let target_line = line.saturating_sub(1);
            let total_lines = app.editor.lines().len();
            if target_line < total_lines {
                app.editor.move_cursor(CursorMove::Top);
                for _ in 0..target_line {
                    app.editor.move_cursor(CursorMove::Down);
                }
            }
        }
        Command::Substitute { pattern, replacement, flags } => {
            // Simple substitute implementation
            // First, collect all changes to make
            let lines: Vec<String> = app.editor.lines().iter().map(|s| s.to_string()).collect();
            let mut changes: Vec<(usize, String)> = Vec::new();

            for (row, line) in lines.iter().enumerate() {
                if line.contains(&pattern) {
                    let new_line = if flags.global {
                        line.replace(&pattern, &replacement)
                    } else {
                        line.replacen(&pattern, &replacement, 1)
                    };
                    if new_line != *line {
                        changes.push((row, new_line));
                        if !flags.global {
                            break;
                        }
                    }
                }
            }

            // Apply changes in reverse order to preserve line numbers
            for (row, new_line) in changes.into_iter().rev() {
                // Go to the line
                app.editor.move_cursor(CursorMove::Top);
                for _ in 0..row {
                    app.editor.move_cursor(CursorMove::Down);
                }
                app.editor.move_cursor(CursorMove::Head);
                // Select entire line and delete it
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::End);
                app.editor.cut();
                // Insert the new line content
                for c in new_line.chars() {
                    app.editor.insert_char(c);
                }
            }

            app.update_editor_highlights();
        }
    }
}
