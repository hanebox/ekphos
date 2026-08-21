use super::*;

pub(super) fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent) {
    app.keymap.reset_pending();
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

    // Handle search picker mouse events
    if !matches!(app.search_picker, SearchPickerState::Closed) {
        if app.is_inside_search_picker(mouse_x, mouse_y) {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    app.search_picker_scroll_up();
                    return;
                }
                MouseEventKind::ScrollDown => {
                    app.search_picker_scroll_down();
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    match app.search_picker_click(mouse_x, mouse_y) {
                        2 => {
                            // Double-click: select and confirm
                            app.select_search_picker_result();
                        }
                        1 => {
                            // Single click: just select
                        }
                        _ => {}
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    return;
                }
                _ => {}
            }
        } else if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            // Click outside closes the picker
            app.close_search_picker();
            return;
        }
        return;
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
                    let hovered_inline_image = app.inline_image_rects.iter().find(|image| {
                        mouse_x >= image.rect.x
                            && mouse_x < image.rect.x + image.rect.width
                            && mouse_y >= image.rect.y
                            && mouse_y < image.rect.y + image.rect.height
                    });
                    app.mouse_hover_inline_image = hovered_inline_image.map(|image| (image.item_index, image.selection_index));

                    let hovered_item = app
                        .content_item_rects
                        .iter()
                        .find(|(_, rect)| mouse_y >= rect.y && mouse_y < rect.y + rect.height)
                        .map(|(idx, _)| *idx);

                    if let Some(idx) = hovered_item {
                        if app.mouse_hover_inline_image.is_some() || app.item_has_link_at(idx) || app.item_is_image_at(idx).is_some() {
                            app.mouse_hover_item = Some(idx);
                        } else {
                            app.mouse_hover_item = None;
                        }
                    } else {
                        app.mouse_hover_item = None;
                    }
                } else {
                    app.mouse_hover_item = None;
                    app.mouse_hover_inline_image = None;
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
                        let item_info = app
                            .sidebar_items
                            .get(clicked_index)
                            .map(|item| match &item.kind {
                                SidebarItemKind::Folder(folder) => Some((true, folder.path.clone(), 0)),
                                SidebarItemKind::Note { note_id } => Some((false, std::path::PathBuf::new(), app.note_index_for_id(*note_id)?)),
                            })
                            .flatten();

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
                    let clicked_inline_image = app
                        .inline_image_rects
                        .iter()
                        .find(|image| {
                            mouse_x >= image.rect.x
                                && mouse_x < image.rect.x + image.rect.width
                                && mouse_y >= image.rect.y
                                && mouse_y < image.rect.y + image.rect.height
                        })
                        .cloned();

                    if let Some(image) = clicked_inline_image {
                        app.focus = Focus::Content;
                        app.content_cursor = image.item_index;
                        app.selected_link_index = image.selection_index;
                        app.open_path_or_url(&image.path);
                        return;
                    }

                    let clicked_item = app
                        .content_item_rects
                        .iter()
                        .find(|(_, rect)| mouse_y >= rect.y && mouse_y < rect.y + rect.height)
                        .copied();

                    if let Some((idx, item_rect)) = clicked_item {
                        if app.is_content_item_visible(idx) {
                            app.content_cursor = idx;
                            app.selected_link_index = 0;
                        }

                        let clicked_rendered_col = crate::ui::content_item_click_col(app, idx, item_rect, mouse_x, mouse_y);

                        if mouse_y == item_rect.y && app.is_click_on_task_checkbox(idx, mouse_x, app.content_area.x) {
                            app.toggle_task_at(idx);
                        } else if let Some(url) = clicked_rendered_col.and_then(|col| app.find_clicked_link_at_col(idx, col)) {
                            app.open_link(&url);
                        } else if let Some(wiki_link) = clicked_rendered_col.and_then(|col| app.find_clicked_wiki_link_at_col(idx, col)) {
                            if wiki_link.is_valid {
                                app.navigate_to_wiki_link_with_heading(&wiki_link.target, wiki_link.heading.as_deref());
                            } else {
                                app.pending_wiki_target = Some(wiki_link.target);
                                app.dialog = DialogState::CreateWikiNote;
                            }
                        } else if let Some(path) = app.item_is_image_at(idx) {
                            let normalized = crate::app::normalize_image_destination(path);
                            let is_url = normalized.starts_with("http://") || normalized.starts_with("https://");
                            let open_path = if is_url {
                                Some(normalized)
                            } else {
                                app.resolve_image_path(path).map(|p| p.to_string_lossy().to_string())
                            };
                            if let Some(open_path) = open_path {
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(&open_path).spawn();
                                #[cfg(any(target_os = "android", target_os = "freebsd", target_os = "linux"))]
                                let _ = std::process::Command::new("xdg-open").arg(&open_path).spawn();
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("cmd").args(["/c", "start", "", &open_path]).spawn();
                            }
                        } else if app.item_is_details_at(idx) {
                            app.toggle_details_at(idx);
                        } else if app.is_heading_at(idx) {
                            app.toggle_heading_fold_at(idx);
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => match app.focus {
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
            },
            MouseEventKind::ScrollUp => match app.focus {
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
            },
            _ => {}
        }
    }
}

pub(super) fn handle_paste_event(app: &mut App, text: String) {
    app.keymap.reset_pending();
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
        update_cursor_style(app);
    }

    // Try to get html from clipboard and convert to Markdown
    // falls back to plain text if html not available or conversion fails
    let paste_text = match clipboard::get_content_as_markdown_from(app.clipboard()) {
        Ok(ClipboardContent::Markdown(md)) => md,
        Ok(ClipboardContent::PlainText(txt)) => txt,
        Ok(ClipboardContent::Empty) => text.clone(),
        Err(e) => {
            // The terminal already handed us the pasted text, so fall back to it
            // and surface the clipboard failure as a toast (never to stdout).
            app.show_error_toast(format!("Clipboard: {}", e));
            text.clone()
        }
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

pub(super) fn handle_edit_mode_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
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
                    update_cursor_style(app);
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
                    update_cursor_style(app);
                    app.editor.start_selection();
                    app.editor.set_inclusive_selection(true);
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
            }
            constrain_cursor_to_viewport(app);
        }

        MouseEventKind::ScrollDown => {
            let line_count = app.editor.line_count();
            let max_scroll = line_count.saturating_sub(1);

            if app.editor_scroll_top < max_scroll {
                app.editor_scroll_top = (app.editor_scroll_top + 3).min(max_scroll);
                app.editor.set_scroll_offset(app.editor_scroll_top);
            }
            constrain_cursor_to_viewport(app);
        }

        _ => {}
    }
}

pub(super) fn handle_auto_scroll(app: &mut App, mouse_y: u16) {
    let direction = app.get_auto_scroll_direction(mouse_y);
    if direction == 0 {
        return;
    }

    perform_auto_scroll(app, direction);
}

/// Continuous auto-scroll when mouse is held near edges (called from main loop)
pub(super) fn handle_continuous_auto_scroll(app: &mut App) {
    let direction = app.get_auto_scroll_direction(app.last_mouse_y);
    if direction == 0 {
        return;
    }

    perform_auto_scroll(app, direction);
}

/// Perform the actual scrolling in the given direction
pub(super) fn perform_auto_scroll(app: &mut App, direction: i8) {
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
pub(super) fn move_editor_cursor_to(app: &mut App, target_row: usize, target_col: usize) {
    app.editor.set_cursor_no_scroll(target_row, target_col);
}

pub(super) fn constrain_cursor_to_viewport(app: &mut App) {
    let view_height = app.editor_view_height;
    if view_height == 0 {
        return;
    }

    let (cursor_row, cursor_col) = app.editor.cursor();
    let line_count = app.editor.line_count();
    let max_row = line_count.saturating_sub(1);
    let viewport_top = app.editor_scroll_top;
    let viewport_bottom = (app.editor_scroll_top + view_height.saturating_sub(1)).min(max_row);

    let clamped_row = if cursor_row < viewport_top {
        viewport_top
    } else if cursor_row > viewport_bottom {
        viewport_bottom
    } else {
        cursor_row
    };

    let scrolloff = app.config.editor.scrolloff as usize;
    let effective_scrolloff = scrolloff.min(view_height / 2);

    let final_row = if effective_scrolloff > 0 && clamped_row == cursor_row {
        let scrolloff_top = viewport_top + effective_scrolloff;
        let scrolloff_bottom = viewport_bottom.saturating_sub(effective_scrolloff);

        if cursor_row < scrolloff_top {
            scrolloff_top.min(max_row).min(viewport_bottom)
        } else if cursor_row > scrolloff_bottom {
            scrolloff_bottom.max(viewport_top)
        } else {
            cursor_row
        }
    } else {
        clamped_row
    };

    app.editor.set_cursor_no_scroll(final_row, cursor_col);
}

// ==================== Context Menu Helpers ====================

const MENU_WIDTH: u16 = 14;

pub(super) fn get_context_menu_click(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<ContextMenuItem> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2; // +2 for borders

    // Check if click is within menu bounds
    if mouse_x >= menu_x && mouse_x < menu_x + MENU_WIDTH && mouse_y >= menu_y && mouse_y < menu_y + menu_height {
        let relative_y = mouse_y.saturating_sub(menu_y).saturating_sub(1); // -1 for top border
        let index = relative_y as usize;
        if index < items.len() {
            return Some(items[index]);
        }
    }

    None
}

pub(super) fn get_context_menu_hover_index(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<usize> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2;

    if mouse_x >= menu_x && mouse_x < menu_x + MENU_WIDTH && mouse_y > menu_y && mouse_y < menu_y + menu_height - 1 {
        let index = (mouse_y - menu_y - 1) as usize;
        if index < items.len() {
            return Some(index);
        }
    }
    None
}

pub(super) fn execute_context_menu_action(app: &mut App, action: ContextMenuItem) {
    match action {
        ContextMenuItem::Copy => {
            app.editor.copy();
            app.editor.cancel_selection();
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
        }
        ContextMenuItem::Cut => {
            app.editor.cut();
            app.vim_mode = VimMode::Normal;
            update_cursor_style(app);
        }
        ContextMenuItem::Paste => {
            app.editor.paste();
        }
        ContextMenuItem::SelectAll => {
            app.editor.move_cursor(CursorMove::Top);
            app.editor.start_selection();
            app.editor.move_cursor(CursorMove::Bottom);
            app.editor.set_inclusive_selection(true);
            app.vim_mode = VimMode::Visual;
            update_cursor_style(app);
        }
    }
    app.update_editor_block();
}
