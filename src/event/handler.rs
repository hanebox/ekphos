use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::{backend::CrosstermBackend, Terminal};
use tui_textarea::Input;

use crate::app::{App, DialogState, Focus, Mode, VimMode};
use crate::ui;

pub fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

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
    }
}

fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent) {
    // Handle mouse scroll in normal mode only
    if app.mode == Mode::Normal && app.dialog == DialogState::None && !app.show_welcome {
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                match app.focus {
                    Focus::Sidebar => app.next_note(),
                    Focus::Content => {
                        app.next_content_line();
                        app.sync_outline_to_content();
                    }
                    Focus::Outline => app.next_outline(),
                }
            }
            MouseEventKind::ScrollUp => {
                match app.focus {
                    Focus::Sidebar => app.previous_note(),
                    Focus::Content => {
                        app.previous_content_line();
                        app.sync_outline_to_content();
                    }
                    Focus::Outline => app.previous_outline(),
                }
            }
            _ => {}
        }
    }
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
        DialogState::DeleteConfirm => {
            handle_delete_confirm_dialog(app, key);
            return Ok(false);
        }
        DialogState::RenameNote => {
            handle_rename_note_dialog(app, key);
            return Ok(false);
        }
        DialogState::Help => {
            handle_help_dialog(app, key);
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
            let name = app.input_buffer.clone();
            app.create_note(&name);
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

fn handle_help_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.dialog = DialogState::None;
        }
        _ => {}
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
    match key.code {
        KeyCode::Esc => {
            app.clear_search();
        }
        KeyCode::Enter => {
            // Select first filtered note if any
            let visible = app.get_visible_notes();
            if !visible.is_empty() {
                app.selected_note = visible[0].0;
                app.current_image = None;
                app.update_outline();
                app.update_content_items();
            }
            app.search_active = false;
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            app.update_filtered_indices();
        }
        KeyCode::Down => {
            // Navigate within filtered results
            let visible = app.get_visible_notes();
            if !visible.is_empty() {
                let current_pos = visible.iter().position(|(i, _)| *i == app.selected_note).unwrap_or(0);
                let next_pos = (current_pos + 1) % visible.len();
                app.selected_note = visible[next_pos].0;
                app.current_image = None;
                app.update_outline();
                app.update_content_items();
            }
        }
        KeyCode::Up => {
            let visible = app.get_visible_notes();
            if !visible.is_empty() {
                let current_pos = visible.iter().position(|(i, _)| *i == app.selected_note).unwrap_or(0);
                let prev_pos = if current_pos == 0 { visible.len() - 1 } else { current_pos - 1 };
                app.selected_note = visible[prev_pos].0;
                app.current_image = None;
                app.update_outline();
                app.update_content_items();
            }
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
            app.dialog = DialogState::CreateNote;
        }
        KeyCode::Char('d') => {
            if !app.notes.is_empty() {
                app.dialog = DialogState::DeleteConfirm;
            }
        }
        KeyCode::Char('r') => {
            if !app.notes.is_empty() {
                if let Some(note) = app.current_note() {
                    app.input_buffer = note.title.clone();
                }
                app.dialog = DialogState::RenameNote;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            match app.focus {
                Focus::Sidebar => app.next_note(),
                Focus::Outline => app.next_outline(),
                Focus::Content => {
                    app.next_content_line();
                    app.sync_outline_to_content();
                }
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            match app.focus {
                Focus::Sidebar => app.previous_note(),
                Focus::Outline => app.previous_outline(),
                Focus::Content => {
                    app.previous_content_line();
                    app.sync_outline_to_content();
                }
            }
        }
        KeyCode::Enter => {
            match app.focus {
                Focus::Content => app.open_current_image(),
                Focus::Outline => app.jump_to_outline(),
                Focus::Sidebar => {}
            }
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
        _ => {}
    }
    false
}

fn handle_edit_mode(app: &mut App, key: crossterm::event::KeyEvent) {
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
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Insert;
        }
        KeyCode::Char('a') => {
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.textarea.move_cursor(tui_textarea::CursorMove::Forward);
        }
        KeyCode::Char('A') => {
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.textarea.move_cursor(tui_textarea::CursorMove::End);
        }
        KeyCode::Char('I') => {
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.textarea.move_cursor(tui_textarea::CursorMove::Head);
        }
        KeyCode::Char('o') => {
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.textarea.move_cursor(tui_textarea::CursorMove::End);
            app.textarea.insert_newline();
        }
        KeyCode::Char('O') => {
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Insert;
            app.textarea.move_cursor(tui_textarea::CursorMove::Head);
            app.textarea.insert_newline();
            app.textarea.move_cursor(tui_textarea::CursorMove::Up);
        }
        KeyCode::Char('v') => {
            app.vim_mode = VimMode::Visual;
            app.textarea.cancel_selection();
            app.textarea.start_selection();
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('b') => {
            // Start selection, then move - selection shows what will be affected
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::WordBack);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::Down);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::Up);
        }
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('w') => {
            // Start selection, then move - selection shows what will be affected
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::WordForward);
        }
        KeyCode::Char('0') => {
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::Head);
        }
        KeyCode::Char('$') => {
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::End);
        }
        KeyCode::Char('g') => {
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::Top);
        }
        KeyCode::Char('G') => {
            app.highlight_current_word();
            app.textarea.move_cursor(tui_textarea::CursorMove::Bottom);
        }
        KeyCode::Char('x') => {
            app.textarea.cancel_selection();
            app.textarea.delete_char();
        }
        KeyCode::Char('d') => {
            // Delete the highlighted word (selection)
            app.textarea.cut();
        }
        KeyCode::Char('y') => {
            // Yank current selection
            app.textarea.copy();
        }
        KeyCode::Char('p') => {
            app.textarea.cancel_selection();
            app.textarea.paste();
        }
        KeyCode::Char('u') => {
            app.textarea.cancel_selection();
            app.textarea.undo();
        }
        KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
            app.textarea.cancel_selection();
            app.textarea.redo();
        }
        KeyCode::Esc => {
            app.textarea.cancel_selection();
            app.cancel_edit();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.textarea.cancel_selection();
            app.save_edit();
            app.vim_mode = VimMode::Normal;
        }
        _ => {}
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
            let input = Input::from(key);
            app.textarea.input(input);
        }
    }
}

fn handle_vim_visual_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Back);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Down);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Up);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Forward);
        }
        KeyCode::Char('w') => {
            app.textarea.move_cursor(tui_textarea::CursorMove::WordForward);
        }
        KeyCode::Char('b') => {
            app.textarea.move_cursor(tui_textarea::CursorMove::WordBack);
        }
        KeyCode::Char('0') => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Head);
        }
        KeyCode::Char('$') => {
            app.textarea.move_cursor(tui_textarea::CursorMove::End);
        }
        KeyCode::Char('g') => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Top);
        }
        KeyCode::Char('G') => {
            app.textarea.move_cursor(tui_textarea::CursorMove::Bottom);
        }
        KeyCode::Char('y') => {
            app.textarea.copy();
            app.textarea.cancel_selection();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            app.textarea.cut();
            app.vim_mode = VimMode::Normal;
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.textarea.cancel_selection();
            app.save_edit();
            app.vim_mode = VimMode::Normal;
        }
        _ => {}
    }
}
