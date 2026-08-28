use super::*;

pub(super) fn handle_mouse_event(app: &mut App, mouse: crossterm::event::MouseEvent) {
    app.state.keymap.reset_pending();
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;
    if let ContextMenuState::Open { x, y, selected_index: _ } = app.editor.context_menu_state {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = get_context_menu_click(mouse_x, mouse_y, x, y) {
                    execute_context_menu_action(app, action);
                }
                app.editor.context_menu_state = ContextMenuState::None;
                return;
            }
            MouseEventKind::Moved => {
                if let Some(new_idx) = get_context_menu_hover_index(mouse_x, mouse_y, x, y) {
                    app.editor.context_menu_state = ContextMenuState::Open { x, y, selected_index: new_idx };
                }
                return;
            }
            _ => {
                if matches!(mouse.kind, MouseEventKind::Down(_)) {
                    app.editor.context_menu_state = ContextMenuState::None;
                }
                return;
            }
        }
    }
    if !matches!(app.search.search_picker, SearchPickerState::Closed) {
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
                            app.select_search_picker_result();
                        }
                        1 => {}
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
            app.close_search_picker();
            return;
        }
        return;
    }
    if app.state.dialog == DialogState::GraphView {
        handle_graph_view_mouse(app, mouse);
        return;
    }
    if app.editor.mode == Mode::Edit {
        handle_edit_mode_mouse(app, mouse);
        return;
    }
    if app.editor.mode == Mode::Normal && app.state.dialog == DialogState::None && !app.state.show_welcome {
        let in_content_area = mouse_x >= app.state.content_area.x && mouse_x < app.state.content_area.x + app.state.content_area.width && mouse_y >= app.state.content_area.y && mouse_y < app.state.content_area.y + app.state.content_area.height;
        match mouse.kind {
            MouseEventKind::Moved => {
                if in_content_area {
                    let hovered_inline_image = app.state.inline_image_rects.iter().find(|image| mouse_x >= image.rect.x && mouse_x < image.rect.x + image.rect.width && mouse_y >= image.rect.y && mouse_y < image.rect.y + image.rect.height);
                    app.state.mouse_hover_inline_image = hovered_inline_image.map(|image| (image.item_index, image.selection_index));
                    let hovered_item = app.state.content_item_rects.iter().find(|(_, rect)| mouse_y >= rect.y && mouse_y < rect.y + rect.height).map(|(idx, _)| *idx);
                    if let Some(idx) = hovered_item {
                        if app.state.mouse_hover_inline_image.is_some() || app.item_has_link_at(idx) || app.item_is_image_at(idx).is_some() {
                            app.state.mouse_hover_item = Some(idx);
                        } else {
                            app.state.mouse_hover_item = None;
                        }
                    } else {
                        app.state.mouse_hover_item = None;
                    }
                } else {
                    app.state.mouse_hover_item = None;
                    app.state.mouse_hover_inline_image = None;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let in_sidebar_area = app.state.sidebar_area.width > 0 && mouse_x >= app.state.sidebar_area.x && mouse_x < app.state.sidebar_area.x + app.state.sidebar_area.width && mouse_y >= app.state.sidebar_area.y && mouse_y < app.state.sidebar_area.y + app.state.sidebar_area.height;
                let in_outline_area = app.state.outline_area.width > 0 && mouse_x >= app.state.outline_area.x && mouse_x < app.state.outline_area.x + app.state.outline_area.width && mouse_y >= app.state.outline_area.y && mouse_y < app.state.outline_area.y + app.state.outline_area.height;
                if in_sidebar_area {
                    let inner_y = mouse_y.saturating_sub(app.state.sidebar_area.y + 1); // +1 for top border
                    let clicked_index = inner_y as usize;
                    if clicked_index < app.vault.sidebar_items.len() {
                        app.vault.selected_sidebar_index = clicked_index;
                        app.state.focus = Focus::Sidebar;
                        execute_app_command(app, AppCommand::Activate);
                    }
                } else if in_outline_area {
                    let inner_y = mouse_y.saturating_sub(app.state.outline_area.y + 1); // +1 for top border
                    let clicked_index = inner_y as usize;
                    if clicked_index < app.document.outline.len() {
                        app.document.outline_state.select(Some(clicked_index));
                        app.state.focus = Focus::Outline;
                        execute_app_command(app, AppCommand::Activate);
                    }
                } else if in_content_area {
                    let clicked_inline_image = app.state.inline_image_rects.iter().find(|image| mouse_x >= image.rect.x && mouse_x < image.rect.x + image.rect.width && mouse_y >= image.rect.y && mouse_y < image.rect.y + image.rect.height).cloned();
                    if let Some(image) = clicked_inline_image {
                        app.state.focus = Focus::Content;
                        app.document.content_cursor = image.item_index;
                        app.document.selected_link_index = image.selection_index;
                        open_selected_content_target(app);
                        return;
                    }
                    let clicked_item = app.state.content_item_rects.iter().find(|(_, rect)| mouse_y >= rect.y && mouse_y < rect.y + rect.height).copied();
                    if let Some((idx, item_rect)) = clicked_item {
                        if app.is_content_item_visible(idx) {
                            app.document.content_cursor = idx;
                            app.document.selected_link_index = 0;
                        }
                        let clicked_rendered_col = crate::ui::content_item_click_col(app, idx, item_rect, mouse_x, mouse_y);
                        if mouse_y == item_rect.y && app.is_click_on_task_checkbox(idx, mouse_x, app.state.content_area.x) {
                            app.toggle_task_at(idx);
                        } else if let Some(url) = clicked_rendered_col.and_then(|col| app.find_clicked_link_at_col(idx, col)) {
                            app.open_link(&url);
                        } else if let Some(wiki_link) = clicked_rendered_col.and_then(|col| app.find_clicked_wiki_link_at_col(idx, col)) {
                            if wiki_link.is_valid {
                                app.navigate_to_wiki_link_with_heading(&wiki_link.target, wiki_link.heading.as_deref());
                            } else {
                                app.editor.pending_wiki_target = Some(wiki_link.target);
                                app.state.dialog = DialogState::CreateWikiNote;
                            }
                        } else if let Some(path) = app.item_is_image_at(idx) {
                            app.open_path_or_url(path);
                        } else if app.item_is_details_at(idx) {
                            app.toggle_details_at(idx);
                        } else if app.is_heading_at(idx) {
                            app.toggle_heading_fold_at(idx);
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                execute_app_command(app, AppCommand::MoveDown);
            }
            MouseEventKind::ScrollUp => {
                execute_app_command(app, AppCommand::MoveUp);
            }
            _ => {}
        }
    }
}

pub(super) fn handle_paste_event(app: &mut App, text: String) {
    app.state.keymap.reset_pending();
    if app.editor.mode != Mode::Edit {
        return;
    }
    app.editor.context_menu_state = ContextMenuState::None;
    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
    if app.editor.vim.mode == VimMode::Normal || app.editor.vim.mode == VimMode::Visual {
        app.editor.cancel_selection();
        app.editor.vim.mode = VimMode::Insert;
        update_cursor_style(app);
    }
    let paste_text = match clipboard::get_content_as_markdown_from(app.clipboard()) {
        Ok(ClipboardContent::Markdown(md)) => md,
        Ok(ClipboardContent::PlainText(txt)) => txt,
        Ok(ClipboardContent::Empty) => text.clone(),
        Err(e) => {
            app.show_error_toast(format!("Clipboard: {}", e));
            text.clone()
        }
    };
    if paste_text.contains('\n') {
        app.state.needs_full_clear = true;
    }
    app.editor.insert_str(&paste_text);
    app.update_editor_highlights();
    app.update_editor_block();
    if let Some(view_height) = app.editor.editor_view_height.checked_sub(2) {
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
            app.editor.context_menu_state = ContextMenuState::None;
            if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                let line_count = app.editor.line_count();
                let row = row.min(line_count.saturating_sub(1));
                let line_len = app.editor.line(row).map(|line| line.chars().count()).unwrap_or(0);
                let col = col.min(line_len);
                if app.editor.vim.mode == VimMode::Visual {
                    app.editor.cancel_selection();
                    app.editor.vim.mode = VimMode::Normal;
                    update_cursor_style(app);
                }
                move_editor_cursor_to(app, row, col);
                app.editor.mouse_button_held = true;
                app.editor.mouse_drag_start = Some((row as u16, col as u16));
                app.editor.last_mouse_y = mouse_y; // Initialize to prevent stale auto-scroll
                app.update_editor_block();
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            app.editor.context_menu_state = ContextMenuState::Open { x: mouse_x, y: mouse_y, selected_index: 0 };
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.editor.mouse_button_held = false;
            app.editor.mouse_drag_start = None;
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if app.editor.mouse_button_held {
                app.editor.last_mouse_y = mouse_y;
                if app.editor.vim.mode == VimMode::Normal {
                    app.editor.vim.mode = VimMode::Visual;
                    update_cursor_style(app);
                    app.editor.start_selection();
                    app.editor.set_inclusive_selection(true);
                    app.update_editor_block();
                }
                if app.editor.vim.mode == VimMode::Visual {
                    handle_auto_scroll(app, mouse_y);
                }
                if let Some((row, col)) = app.screen_to_editor_coords(mouse_x, mouse_y) {
                    let line_count = app.editor.line_count();
                    let row = row.min(line_count.saturating_sub(1));
                    let line_len = app.editor.line(row).map(|line| line.chars().count()).unwrap_or(0);
                    let col = col.min(line_len);
                    move_editor_cursor_to(app, row, col);
                }
            }
        }
        MouseEventKind::ScrollUp => {
            if app.editor.editor_scroll_top > 0 {
                app.editor.editor_scroll_top = app.editor.editor_scroll_top.saturating_sub(3);
                app.editor.sync_scroll_offset();
            }
            constrain_cursor_to_viewport(app);
        }
        MouseEventKind::ScrollDown => {
            let line_count = app.editor.line_count();
            let max_scroll = line_count.saturating_sub(1);
            if app.editor.editor_scroll_top < max_scroll {
                app.editor.editor_scroll_top = (app.editor.editor_scroll_top + 3).min(max_scroll);
                app.editor.sync_scroll_offset();
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
    let direction = app.get_auto_scroll_direction(app.editor.last_mouse_y);
    if direction == 0 {
        return;
    }
    perform_auto_scroll(app, direction);
}

/// Perform the actual scrolling in the given direction
pub(super) fn perform_auto_scroll(app: &mut App, direction: i8) {
    if direction < 0 {
        if app.editor.editor_scroll_top > 0 {
            app.editor.editor_scroll_top = app.editor.editor_scroll_top.saturating_sub(1);
            app.editor.sync_scroll_offset();
            app.editor.move_cursor(CursorMove::Up);
        }
    } else {
        let max_scroll = app.editor.line_count().saturating_sub(app.editor.editor_view_height);
        if app.editor.editor_scroll_top < max_scroll {
            app.editor.editor_scroll_top += 1;
            app.editor.sync_scroll_offset();
            app.editor.move_cursor(CursorMove::Down);
        }
    }
}

/// Move editor cursor to specific row/col position
pub(super) fn move_editor_cursor_to(app: &mut App, target_row: usize, target_col: usize) {
    app.editor.set_cursor_no_scroll(target_row, target_col);
}

pub(super) fn constrain_cursor_to_viewport(app: &mut App) {
    let view_height = app.editor.editor_view_height;
    if view_height == 0 {
        return;
    }
    let (cursor_row, cursor_col) = app.editor.cursor();
    let line_count = app.editor.line_count();
    let max_row = line_count.saturating_sub(1);
    let viewport_top = app.editor.editor_scroll_top;
    let viewport_bottom = (app.editor.editor_scroll_top + view_height.saturating_sub(1)).min(max_row);
    let clamped_row = if cursor_row < viewport_top {
        viewport_top
    } else if cursor_row > viewport_bottom {
        viewport_bottom
    } else {
        cursor_row
    };
    let scrolloff = app.state.config.editor.scrolloff as usize;
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

const MENU_WIDTH: u16 = 14;

pub(super) fn get_context_menu_click(mouse_x: u16, mouse_y: u16, menu_x: u16, menu_y: u16) -> Option<ContextMenuItem> {
    let items = ContextMenuItem::all();
    let menu_height = items.len() as u16 + 2; // +2 for borders
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
            app.editor.vim.mode = VimMode::Normal;
            update_cursor_style(app);
        }
        ContextMenuItem::Cut => {
            app.editor.cut();
            app.editor.vim.mode = VimMode::Normal;
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
            app.editor.vim.mode = VimMode::Visual;
            update_cursor_style(app);
        }
    }
    app.update_editor_block();
}
