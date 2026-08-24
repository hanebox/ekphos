use super::*;

pub(super) fn handle_vim_insert_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.vim.macros.is_recording() {
        app.vim.macros.record_key(key);
    }

    match key.code {
        KeyCode::Esc => {
            if let Some(state) = app.block_insert_state.take() {
                apply_block_insert(app, state);
            }
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            if let Some(state) = app.block_insert_state.take() {
                apply_block_insert(app, state);
            }
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
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
                if let Some(line) = app.editor.line(row) {
                    if col >= 2 {
                        if line.chars().nth(col.saturating_sub(2)) == Some('[') && line.chars().nth(col.saturating_sub(1)) == Some('[') {
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
                app.update_editor_highlights_incremental();

                let should_detect = matches!(key.code, KeyCode::Char(_))
                    || (matches!(key.code, KeyCode::Backspace) && matches!(app.wiki_autocomplete, WikiAutocompleteState::Open { .. }));
                if should_detect {
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

pub(super) fn handle_vim_replace_mode(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.vim.macros.is_recording() {
        app.vim.macros.record_key(key);
    }

    match key.code {
        KeyCode::Esc => {
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
            app.vim.mode = VimModeNew::Normal;
            app.vim.reset_pending();
        }
        KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
            app.save_edit();
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
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
            if let Some(line) = app.editor.line(row) {
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

pub(super) fn handle_vim_visual_mode(app: &mut App, key: crossterm::event::KeyEvent) {
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
            update_cursor_style(app);
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
                    let line_count = app.editor.line_count();
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
                let line_count = app.editor.line_count();
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
            update_cursor_style(app);
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
            update_cursor_style(app);
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
            update_cursor_style(app);
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
            update_cursor_style(app);
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
                let current = Position {
                    row: current_row,
                    col: current_col,
                };

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
                update_cursor_style(app);
                app.vim.mode = VimModeNew::Insert;
            }
        }
        KeyCode::Char('A') if app.vim_mode == VimMode::VisualBlock => {
            if let Some(anchor) = app.visual_block_anchor {
                let (current_row, current_col) = app.editor.cursor();
                let current = Position {
                    row: current_row,
                    col: current_col,
                };
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
                update_cursor_style(app);
                app.vim.mode = VimModeNew::Insert;
            }
        }
        _ => {}
    }
}

pub(super) fn handle_vim_command_mode(app: &mut App, key: crossterm::event::KeyEvent) {
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

pub(super) fn handle_vim_search_mode(app: &mut App, key: crossterm::event::KeyEvent) {
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
                    ekphos_vim::SearchDirection::Forward
                } else {
                    ekphos_vim::SearchDirection::Backward
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

pub(super) fn handle_vim_search_locked_mode(app: &mut App, key: crossterm::event::KeyEvent) {
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

pub(super) fn execute_vim_command(app: &mut App, command: Command) {
    match command {
        Command::Write => {
            app.save_edit();
        }
        Command::Quit => {
            if app.has_unsaved_changes() {
                app.dialog = DialogState::UnsavedChanges;
            } else {
                app.cancel_edit();
            }
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
            let total_lines = app.editor.line_count();
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
            let lines = app.editor.snapshot();
            let mut changes: Vec<(usize, String)> = Vec::new();

            for (row, line) in lines.iter_lines().enumerate() {
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
