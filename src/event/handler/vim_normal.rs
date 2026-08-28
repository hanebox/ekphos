use super::*;

pub(super) fn handle_vim_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    app.editor.vim.status_message = None;
    if app.editor.vim.macros.is_recording() && key.code != KeyCode::Char('q') {
        app.editor.vim.macros.record_key(key);
    }
    if let Some(pending) = app.editor.vim.pending_find.take() {
        if let KeyCode::Char(c) = key.code {
            let find = pending.into_find_state(c);
            app.editor.vim.last_find = Some(find);
            execute_find(app, find);
        }
        app.editor.vim.reset_pending();
        return;
    }
    if app.editor.vim.pending_register {
        app.editor.vim.pending_register = false;
        if let KeyCode::Char(c) = key.code {
            if c.is_ascii_alphanumeric() || matches!(c, '"' | '-' | '+' | '*' | '_') {
                app.editor.vim.registers.select(c);
            }
        }
        return;
    }
    if app.editor.vim.awaiting_replace {
        app.editor.vim.awaiting_replace = false;
        if let KeyCode::Char(c) = key.code {
            app.editor.delete_char();
            app.editor.insert_char(c);
            app.editor.move_cursor(CursorMove::Back);
            app.editor.vim.last_change = Some(ekphos_vim::LastChange::ReplaceChar(c));
        }
        app.editor.vim.reset_pending();
        return;
    }
    if let Some(scope) = app.editor.vim.pending_text_object_scope.take() {
        if let KeyCode::Char(c) = key.code {
            if let Some((_, obj)) = TextObject::parse(if scope == TextObjectScope::Inner { 'i' } else { 'a' }, c) {
                execute_text_object(app, scope, obj);
            }
        }
        app.editor.vim.reset_pending();
        return;
    }
    if let Some(pending) = app.editor.vim.pending_macro.take() {
        if let KeyCode::Char(c) = key.code {
            if c.is_ascii_lowercase() {
                match pending {
                    PendingMacro::Record => {
                        if app.editor.vim.macros.is_recording() {
                            app.editor.vim.macros.stop_recording();
                        } else {
                            app.editor.vim.macros.start_recording(c);
                        }
                    }
                    PendingMacro::Play => {
                        if let Some(keys) = app.editor.vim.macros.get_macro(c).cloned() {
                            app.editor.vim.macros.set_last_played(c);
                            let count = app.editor.vim.get_count();
                            for _ in 0..count {
                                for k in &keys {
                                    match app.editor.vim.mode {
                                        VimMode::Insert => handle_vim_insert_mode(app, *k),
                                        VimMode::Replace => handle_vim_replace_mode(app, *k),
                                        VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => handle_vim_visual_mode(app, *k),
                                        _ => handle_vim_normal_mode(app, *k),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        app.editor.vim.reset_pending();
        return;
    }
    if let Some(pending) = app.editor.vim.pending_mark.take() {
        if let KeyCode::Char(c) = key.code {
            match pending {
                PendingMark::Set => {
                    let pos = app.editor.cursor();
                    app.editor.vim.marks.set(c, ekphos_editor::Position::new(pos.0, pos.1));
                }
                PendingMark::GotoExact => {
                    if let Some(pos) = app.editor.vim.marks.get(c) {
                        app.editor.vim.marks.set_last_jump(ekphos_editor::Position::new(app.editor.cursor().0, app.editor.cursor().1));
                        app.editor.move_cursor(CursorMove::GoToLine(pos.row + 1));
                        for _ in 0..pos.col {
                            app.editor.move_cursor(CursorMove::Forward);
                        }
                    }
                }
                PendingMark::GotoLine => {
                    if let Some(pos) = app.editor.vim.marks.get(c) {
                        app.editor.vim.marks.set_last_jump(ekphos_editor::Position::new(app.editor.cursor().0, app.editor.cursor().1));
                        app.editor.move_cursor(CursorMove::GoToLine(pos.row + 1));
                        app.editor.move_cursor(CursorMove::FirstNonBlank);
                    }
                }
            }
        }
        app.editor.vim.reset_pending();
        return;
    }
    if app.editor.vim.pending_g {
        app.editor.vim.pending_g = false;
        match key.code {
            KeyCode::Char('g') => {
                if let Some(op) = app.editor.pending_operator.take() {
                    let target_line = if let Some(count) = app.editor.vim.count.take() { count.saturating_sub(1) } else { 0 };
                    let (current_row, _) = app.editor.cursor();
                    let (start_row, end_row) = if target_line <= current_row { (target_line, current_row) } else { (current_row, target_line) };
                    app.editor.set_cursor(start_row, 0);
                    app.editor.start_selection();
                    app.editor.set_cursor(end_row, 0);
                    app.editor.move_cursor(CursorMove::End);
                    match op {
                        'd' => {
                            app.editor.cut();
                            if start_row < app.editor.line_count() {
                                app.editor.set_cursor(start_row, 0);
                            }
                        }
                        'c' => {
                            app.editor.cut();
                            app.editor.vim.mode = VimMode::Insert;
                            update_cursor_style(app);
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
                } else if let Some(count) = app.editor.vim.count.take() {
                    app.editor.move_cursor(CursorMove::GoToLine(count));
                } else {
                    app.editor.move_cursor(CursorMove::Top);
                }
            }
            KeyCode::Char('e') => {
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    app.editor.move_cursor(CursorMove::WordEndBackward);
                }
            }
            KeyCode::Char('E') => {
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    app.editor.move_cursor(CursorMove::BigWordEndBackward);
                }
            }
            _ => {}
        }
        app.editor.vim.reset_pending();
        return;
    }
    if app.editor.vim.pending_z {
        app.editor.vim.pending_z = false;
        match key.code {
            KeyCode::Char('z') => {
                app.editor.center_cursor();
            }
            KeyCode::Char('t') => {
                app.editor.scroll_cursor_to_top();
            }
            KeyCode::Char('b') => {
                app.editor.scroll_cursor_to_bottom();
            }
            _ => {}
        }
        app.editor.vim.reset_pending();
        return;
    }
    match key.code {
        KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.status_message = Some("Exit edit mode (Esc) to use search".to_string());
        }
        KeyCode::Char(c @ '1'..='9') => {
            let digit = c.to_digit(10).unwrap() as usize;
            app.editor.vim.accumulate_count(digit);
        }
        KeyCode::Char('0') if app.editor.vim.count.is_some() => {
            app.editor.vim.accumulate_count(0);
        }
        KeyCode::Char('"') => {
            app.editor.vim.pending_register = true;
        }
        KeyCode::Char('i') if app.editor.pending_operator.is_some() => {
            app.editor.vim.pending_text_object_scope = Some(TextObjectScope::Inner);
        }
        KeyCode::Char('a') if app.editor.pending_operator.is_some() => {
            app.editor.vim.pending_text_object_scope = Some(TextObjectScope::Around);
        }
        KeyCode::Char('i') => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
        }
        KeyCode::Char('a') => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::Forward);
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
        }
        KeyCode::Char('A') => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
        }
        KeyCode::Char('I') => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::FirstNonBlank);
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
        }
        KeyCode::Char('o') => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.insert_newline();
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
        }
        KeyCode::Char('O') => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.open_line_above();
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
        }
        KeyCode::Char('v') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.editor.vim.mode = VimMode::VisualBlock;
            update_cursor_style(app);
            app.editor.cancel_selection();
            let (row, col) = app.editor.cursor();
            let anchor = Position { row, col };
            app.editor.visual_block_anchor = Some(anchor);
            app.editor.set_visual_block_selection(anchor, anchor);
        }
        KeyCode::Char('v') => {
            app.editor.vim.reset_pending();
            app.editor.vim.mode = VimMode::Visual;
            update_cursor_style(app);
            app.editor.cancel_selection();
            app.editor.start_selection();
            app.editor.set_inclusive_selection(true);
        }
        KeyCode::Char('V') => {
            app.editor.vim.reset_pending();
            app.editor.vim.mode = VimMode::VisualLine;
            update_cursor_style(app);
            let (row, _) = app.editor.cursor();
            app.editor.visual_line_anchor = Some(row);
            app.editor.visual_line_current = Some(row);
            app.editor.set_visual_line_selection(row, row);
        }
        KeyCode::Char('R') => {
            app.editor.vim.reset_pending();
            app.editor.vim.mode = VimMode::Replace;
            update_cursor_style(app);
            app.editor.cancel_selection();
        }
        KeyCode::Char(':') => {
            app.editor.vim.enter_command_mode();
        }
        KeyCode::Char('q') if key.modifiers.is_empty() => {
            if app.editor.vim.macros.is_recording() {
                app.editor.vim.macros.stop_recording();
            } else {
                app.editor.vim.pending_macro = Some(PendingMacro::Record);
            }
        }
        KeyCode::Char('@') => {
            app.editor.vim.pending_macro = Some(PendingMacro::Play);
        }
        KeyCode::Char('m') if key.modifiers.is_empty() => {
            app.editor.vim.pending_mark = Some(PendingMark::Set);
        }
        KeyCode::Char('`') => {
            app.editor.vim.pending_mark = Some(PendingMark::GotoExact);
        }
        KeyCode::Char('\'') => {
            app.editor.vim.pending_mark = Some(PendingMark::GotoLine);
        }
        KeyCode::Char('h') | KeyCode::Left => execute_motion_n(app, CursorMove::Back),
        KeyCode::Char('j') | KeyCode::Down => execute_motion_n(app, CursorMove::Down),
        KeyCode::Char('k') | KeyCode::Up => execute_motion_n(app, CursorMove::Up),
        KeyCode::Char('l') | KeyCode::Right => execute_motion_n(app, CursorMove::Forward),
        KeyCode::Char('b') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::PageUp);
        }
        KeyCode::Char('w') => execute_motion_or_operator(app, CursorMove::WordForward),
        KeyCode::Char('W') => execute_motion_or_operator(app, CursorMove::BigWordForward),
        KeyCode::Char('b') => execute_motion_or_operator(app, CursorMove::WordBack),
        KeyCode::Char('B') => execute_motion_or_operator(app, CursorMove::BigWordBack),
        KeyCode::Char('e') => execute_motion_or_operator(app, CursorMove::WordEndForward),
        KeyCode::Char('E') => execute_motion_or_operator(app, CursorMove::BigWordEndForward),
        KeyCode::Char('0') => execute_motion_or_operator(app, CursorMove::Head),
        KeyCode::Char('^') => execute_motion_or_operator(app, CursorMove::FirstNonBlank),
        KeyCode::Char('$') => execute_motion_or_operator(app, CursorMove::End),
        KeyCode::Char('g') => {
            app.editor.vim.pending_g = true;
        }
        KeyCode::Char('G') => {
            if let Some(op) = app.editor.pending_operator.take() {
                let target_line = if let Some(count) = app.editor.vim.count.take() {
                    count.saturating_sub(1) // Convert to 0-indexed
                } else {
                    app.editor.line_count().saturating_sub(1)
                };
                let (current_row, _) = app.editor.cursor();
                let (start_row, end_row) = if target_line >= current_row { (current_row, target_line) } else { (target_line, current_row) };
                app.editor.set_cursor(start_row, 0);
                app.editor.start_selection();
                app.editor.set_cursor(end_row, 0);
                app.editor.move_cursor(CursorMove::End);
                match op {
                    'd' => {
                        app.editor.cut();
                        if start_row < app.editor.line_count() {
                            app.editor.set_cursor(start_row, 0);
                        }
                    }
                    'c' => {
                        app.editor.cut();
                        app.editor.vim.mode = VimMode::Insert;
                        update_cursor_style(app);
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
            } else if let Some(count) = app.editor.vim.count.take() {
                app.editor.move_cursor(CursorMove::GoToLine(count));
            } else {
                app.editor.move_cursor(CursorMove::Bottom);
            }
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('{') => execute_motion_n(app, CursorMove::ParagraphBack),
        KeyCode::Char('}') => execute_motion_n(app, CursorMove::ParagraphForward),
        KeyCode::Char('H') => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::ScreenTop);
        }
        KeyCode::Char('M') => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::ScreenMiddle);
        }
        KeyCode::Char('L') => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::ScreenBottom);
        }
        KeyCode::Char('z') => {
            app.editor.vim.pending_z = true;
        }
        KeyCode::Char('f') if key.modifiers.is_empty() => {
            app.editor.vim.pending_find = Some(PendingFind::new(true, false));
        }
        KeyCode::Char('F') => {
            app.editor.vim.pending_find = Some(PendingFind::new(false, false));
        }
        KeyCode::Char('t') if key.modifiers.is_empty() => {
            app.editor.vim.pending_find = Some(PendingFind::new(true, true));
        }
        KeyCode::Char('T') => {
            app.editor.vim.pending_find = Some(PendingFind::new(false, true));
        }
        KeyCode::Char(';') => {
            if let Some(find) = app.editor.vim.last_find {
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    execute_find(app, find);
                }
            }
            app.editor.vim.reset_pending();
        }
        KeyCode::Char(',') => {
            if let Some(find) = app.editor.vim.last_find {
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    execute_find(app, find.reversed());
                }
            }
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('%') => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::MatchingBracket);
        }
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::HalfPageUp);
        }
        KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.editor.move_cursor(CursorMove::HalfPageDown);
        }
        KeyCode::Char('f') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.start_buffer_search();
        }
        KeyCode::Char('d') => {
            if app.editor.pending_operator == Some('d') {
                app.editor.pending_operator = None;
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    app.editor.delete_current_line();
                }
                app.editor.vim.last_change = Some(ekphos_vim::LastChange::DeleteLine(count));
                app.editor.vim.reset_pending();
            } else {
                app.editor.pending_operator = Some('d');
            }
        }
        KeyCode::Char('c') => {
            if app.editor.pending_operator == Some('c') {
                app.editor.pending_operator = None;
                app.editor.move_cursor(CursorMove::Head);
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::End);
                app.editor.cut();
                app.editor.vim.mode = VimMode::Insert;
                update_cursor_style(app);
                app.editor.vim.reset_pending();
            } else {
                app.editor.pending_operator = Some('c');
            }
        }
        KeyCode::Char('y') if key.modifiers.is_empty() => {
            if app.editor.pending_operator == Some('y') {
                app.editor.pending_operator = None;
                app.editor.move_cursor(CursorMove::Head);
                app.editor.start_selection();
                app.editor.move_cursor(CursorMove::End);
                app.editor.copy();
                app.editor.cancel_selection();
                app.editor.vim.reset_pending();
            } else {
                app.editor.pending_operator = Some('y');
            }
        }
        KeyCode::Char('>') => {
            if app.editor.pending_operator == Some('>') {
                app.editor.pending_operator = None;
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    app.editor.move_cursor(CursorMove::Head);
                    app.editor.insert_str("    ");
                }
                app.editor.vim.reset_pending();
            } else {
                app.editor.pending_operator = Some('>');
            }
        }
        KeyCode::Char('<') => {
            if app.editor.pending_operator == Some('<') {
                app.editor.pending_operator = None;
                let count = app.editor.vim.get_count();
                for _ in 0..count {
                    app.editor.move_cursor(CursorMove::Head);
                    for _ in 0..4 {
                        let pos = app.editor.cursor();
                        if let Some(line) = app.editor.line(pos.0) {
                            if line.starts_with(' ') || line.starts_with('\t') {
                                app.editor.delete_char();
                            }
                        }
                    }
                }
                app.editor.vim.reset_pending();
            } else {
                app.editor.pending_operator = Some('<');
            }
        }
        KeyCode::Char('x') => {
            let count = app.editor.vim.get_count();
            let mut deleted = 0;
            for _ in 0..count {
                let (row, col) = app.editor.cursor();
                let line_len = app.editor.line(row).map_or(0, |line| line.chars().count());
                if col < line_len {
                    app.editor.delete_char();
                    deleted += 1;
                } else {
                    break;
                }
            }
            if deleted > 0 {
                app.editor.vim.last_change = Some(ekphos_vim::LastChange::DeleteCharForward(deleted));
            }
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('X') => {
            let count = app.editor.vim.get_count();
            for _ in 0..count {
                app.editor.delete_newline();
            }
            app.editor.vim.last_change = Some(ekphos_vim::LastChange::DeleteCharBackward(count));
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers.is_empty() => {
            app.editor.delete_char();
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('S') => {
            app.editor.move_cursor(CursorMove::Head);
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('D') => {
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('C') => {
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.cut();
            app.editor.vim.mode = VimMode::Insert;
            update_cursor_style(app);
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('Y') => {
            app.editor.move_cursor(CursorMove::Head);
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::End);
            app.editor.copy();
            app.editor.cancel_selection();
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('r') if key.modifiers.is_empty() => {
            app.editor.vim.awaiting_replace = true;
        }
        KeyCode::Char('J') => {
            app.editor.move_cursor(CursorMove::End);
            app.editor.delete_char();
            app.editor.insert_char(' ');
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('~') => {
            let pos = app.editor.cursor();
            if let Some(c) = app.editor.line(pos.0).and_then(|line| line.chars().nth(pos.1)) {
                app.editor.delete_char();
                if c.is_uppercase() {
                    app.editor.insert_char(c.to_lowercase().next().unwrap_or(c));
                } else {
                    app.editor.insert_char(c.to_uppercase().next().unwrap_or(c));
                }
            }
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('p') => {
            app.editor.paste_after();
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('P') => {
            app.editor.paste_before();
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('u') if key.modifiers.is_empty() => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.undo();
            app.update_editor_highlights();
        }
        KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.editor.redo();
            app.update_editor_highlights();
        }
        KeyCode::Char('.') => {
            if let Some(change) = app.editor.vim.last_change.clone() {
                repeat_last_change(app, change);
            }
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.editor.vim.reset_pending();
            app.editor.cancel_selection();
            app.save_edit();
            app.editor.vim.mode = VimMode::Normal;
            update_cursor_style(app);
        }
        KeyCode::Esc => {
            app.editor.vim.reset_pending();
            app.editor.pending_operator = None;
            app.editor.cancel_selection();
            if app.has_unsaved_changes() {
                app.state.dialog = DialogState::UnsavedChanges;
            } else {
                app.cancel_edit();
                app.editor.vim.mode = VimMode::Normal;
                update_cursor_style(app);
            }
        }
        KeyCode::Char('/') => {
            app.editor.vim.reset_pending();
            app.editor.vim.search_buffer.clear();
            app.search.buffer_search.query.clear();
            app.search.buffer_search.matches.clear();
            update_editor_search_highlights(app);
            app.editor.vim.mode = VimMode::Search { forward: true };
        }
        KeyCode::Char('?') => {
            app.editor.vim.reset_pending();
            app.editor.vim.search_buffer.clear();
            app.search.buffer_search.query.clear();
            app.search.buffer_search.matches.clear();
            update_editor_search_highlights(app);
            app.editor.vim.mode = VimMode::Search { forward: false };
        }
        KeyCode::Char('n') => {
            app.editor.vim.reset_pending();
            if !app.search.buffer_search.matches.is_empty() {
                match app.search.buffer_search.direction {
                    crate::app::SearchDirection::Forward => app.buffer_search_next(),
                    crate::app::SearchDirection::Backward => app.buffer_search_prev(),
                }
            }
        }
        KeyCode::Char('N') => {
            app.editor.vim.reset_pending();
            if !app.search.buffer_search.matches.is_empty() {
                match app.search.buffer_search.direction {
                    crate::app::SearchDirection::Forward => app.buffer_search_prev(),
                    crate::app::SearchDirection::Backward => app.buffer_search_next(),
                }
            }
        }
        KeyCode::Char('*') => {
            app.editor.vim.reset_pending();
        }
        KeyCode::Char('#') => {
            app.editor.vim.reset_pending();
        }
        _ => {
            app.editor.vim.reset_pending();
            app.editor.pending_operator = None;
        }
    }
}

/// Repeat the last change command (. dot command)
pub(super) fn repeat_last_change(app: &mut App, change: ekphos_vim::LastChange) {
    use ekphos_vim::LastChange;
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
        LastChange::ChangeLine(_, _) | LastChange::YankLine(_) | LastChange::ChangeToEnd(_) | LastChange::SubstituteChar(_) | LastChange::Insert(_, _) | LastChange::ChangeWord(_, _) => {}
    }
}

pub(super) fn execute_motion_n(app: &mut App, movement: CursorMove) {
    let count = app.editor.vim.get_count();
    app.editor.vim.reset_pending();
    app.editor.cancel_selection();
    for _ in 0..count {
        app.editor.move_cursor(movement);
    }
}

pub(super) fn execute_motion_or_operator(app: &mut App, movement: CursorMove) {
    use ekphos_vim::LastChange;
    let count = app.editor.vim.get_count();
    if let Some(op) = app.editor.pending_operator.take() {
        let start_pos = app.editor.cursor();
        let start_row = start_pos.0;
        app.editor.cancel_selection();
        app.editor.start_selection();
        let is_word_forward = matches!(movement, CursorMove::WordForward | CursorMove::BigWordForward);
        if is_word_forward {
            for _ in 0..count {
                let (row, _) = app.editor.cursor();
                let line = app.editor.line(row).map(str::to_owned);
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
                    if let Some(line) = app.editor.line(end_row) {
                        let mut adjusted_col = end_col;
                        while adjusted_col > start_pos.1 && adjusted_col > 0 {
                            if let Some(c) = line.chars().nth(adjusted_col.saturating_sub(1)) {
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
            for _ in 0..count {
                app.editor.move_cursor(movement);
            }
        }
        match op {
            'd' => {
                app.editor.cut();
                match movement {
                    CursorMove::WordForward | CursorMove::BigWordForward => {
                        app.editor.vim.last_change = Some(LastChange::DeleteWordForward(count));
                    }
                    CursorMove::WordBack | CursorMove::BigWordBack => {
                        app.editor.vim.last_change = Some(LastChange::DeleteWordBackward(count));
                    }
                    CursorMove::End => {
                        app.editor.vim.last_change = Some(LastChange::DeleteToEnd);
                    }
                    _ => {}
                }
            }
            'c' => {
                app.editor.cut();
                app.editor.vim.mode = VimMode::Insert;
                update_cursor_style(app);
            }
            'y' => {
                app.editor.copy();
                app.editor.cancel_selection();
            }
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
                        if let Some(line) = app.editor.line(pos.0) {
                            if line.starts_with(' ') || line.starts_with('\t') {
                                app.editor.delete_char();
                            }
                        }
                    }
                }
            }
            _ => {
                app.editor.cancel_selection();
            }
        }
    } else {
        app.editor.cancel_selection();
        for _ in 0..count {
            app.editor.move_cursor(movement);
        }
    }
    app.editor.vim.reset_pending();
}

pub(super) fn execute_find(app: &mut App, find: FindState) {
    let pos = app.editor.cursor();
    let resolved = { app.editor.line(pos.0).and_then(|line| find.find_in_line(line, pos.1).map(|target| (target, line.chars().count()))) };
    let (target_col, line_len) = match resolved {
        Some(v) => v,
        None => return,
    };
    if let Some(op) = app.editor.pending_operator.take() {
        let (sel_start, sel_end) = if find.forward { (pos.1, (target_col + 1).min(line_len)) } else { (target_col, pos.1) };
        app.editor.set_cursor(pos.0, sel_start);
        app.editor.start_selection();
        app.editor.set_inclusive_selection(false);
        app.editor.set_cursor(pos.0, sel_end);
        match op {
            'd' => {
                app.editor.cut();
            }
            'c' => {
                app.editor.cut();
                app.editor.vim.mode = VimMode::Insert;
                update_cursor_style(app);
            }
            'y' => {
                app.editor.copy();
                app.editor.cancel_selection();
                app.editor.set_cursor(pos.0, pos.1);
            }
            _ => {
                app.editor.cancel_selection();
            }
        }
    } else {
        app.editor.set_cursor(pos.0, target_col);
    }
}

pub(super) fn execute_text_object(app: &mut App, scope: TextObjectScope, obj: TextObject) {
    let pos = app.editor.cursor();
    let lines = app.editor.snapshot();
    let cursor_pos = ekphos_editor::Position::new(pos.0, pos.1);
    if let Some((start, end)) = obj.find_bounds_snapshot(scope, &lines, cursor_pos) {
        if let Some(op) = app.editor.pending_operator.take() {
            app.editor.set_cursor(start.row, start.col);
            app.editor.start_selection();
            app.editor.set_cursor(end.row, end.col);
            match op {
                'd' => {
                    app.editor.cut();
                }
                'c' => {
                    app.editor.cut();
                    app.editor.vim.mode = VimMode::Insert;
                    update_cursor_style(app);
                }
                'y' => {
                    app.editor.copy();
                    app.editor.cancel_selection();
                    app.editor.set_cursor(start.row, start.col);
                }
                _ => {
                    app.editor.cancel_selection();
                }
            }
        }
    }
}

/// apply block insert/append text to all lines in the visual block selection
pub(super) fn apply_block_insert(app: &mut App, state: BlockInsertState) {
    let (current_row, current_col) = app.editor.cursor();
    if let Some(line) = app.editor.line(state.active_row) {
        let insert_start = state.start_col;
        let insert_end = current_col;
        if insert_end > insert_start {
            let inserted_text: String = line.chars().skip(insert_start).take(insert_end - insert_start).collect();
            let (start_row, end_row) = state.rows;
            for row in start_row..=end_row {
                if row == state.active_row {
                    continue;
                }
                let line_len = app.editor.line(row).map(|line| line.chars().count()).unwrap_or(0);
                let insert_pos = match state.mode {
                    BlockInsertMode::Insert => state.insert_col.min(line_len),
                    BlockInsertMode::Append => state.insert_col,
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
