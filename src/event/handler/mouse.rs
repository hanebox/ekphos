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
        if !in_content_area && app.canvas_editor_active() && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) && !app.canvas_commit_node_edit() {
            return;
        }
        if !in_content_area && app.active_document_kind() == Some(ekphos_vault::VaultFileKind::Canvas) && matches!(mouse.kind, MouseEventKind::Moved) {
            app.structured.canvas.hovered_node = None;
            app.structured.canvas.hovered_edge = None;
            app.structured.canvas.hovered_resize = None;
        }
        if (in_content_area || app.canvas_interaction_active()) && handle_structured_document_mouse(app, mouse) {
            return;
        }
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

fn handle_structured_document_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) -> bool {
    match app.active_document_kind() {
        Some(ekphos_vault::VaultFileKind::Base) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                if app.structured.base.column_left_rect.is_some_and(|rect| rect.contains(pointer)) {
                    app.base_move_column(-1);
                    app.state.focus = Focus::Content;
                } else if app.structured.base.column_right_rect.is_some_and(|rect| rect.contains(pointer)) {
                    app.base_move_column(1);
                    app.state.focus = Focus::Content;
                } else if let Some((row, _)) = app.structured.base.row_rects.iter().find(|(_, rect)| rect.contains(pointer)).copied() {
                    app.structured.base.selected_row = row;
                    app.state.focus = Focus::Content;
                }
                true
            }
            MouseEventKind::ScrollDown => {
                app.base_move_selection(1);
                true
            }
            MouseEventKind::ScrollUp => {
                app.base_move_selection(-1);
                true
            }
            _ => false,
        },
        Some(ekphos_vault::VaultFileKind::Canvas) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                if matches!(app.structured.canvas.interaction, crate::app::CanvasInteraction::Connecting { .. }) {
                    if let Some((node, rect)) = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).copied() {
                        app.canvas_end_pointer_interaction(Some((node, Some(canvas_side_at(rect, pointer)))));
                        return true;
                    }
                }
                if let Some((side, _)) = app.structured.canvas.handle_rects.iter().find(|(_, rect)| rect.contains(pointer)).copied() {
                    app.canvas_begin_connect(Some(side), Some((mouse.column, mouse.row)));
                    return true;
                }
                if let Some((handle, _)) = app.structured.canvas.resize_rects.iter().find(|(_, rect)| rect.contains(pointer)).copied() {
                    app.canvas_begin_node_resize(app.structured.canvas.selected_node, handle, (mouse.column, mouse.row));
                    return true;
                }
                if let Some(editor_node) = app.structured.canvas.editor.as_ref().map(|editor| editor.node) {
                    if app.canvas_editor_contains(pointer) {
                        app.canvas_edit_place_cursor(pointer);
                        return true;
                    }
                    let inside_node = app.structured.canvas.node_rects.iter().any(|(node, rect)| *node == editor_node && rect.contains(pointer));
                    if inside_node || !app.canvas_commit_node_edit() {
                        return true;
                    }
                }
                if let Some((node, _)) = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).copied() {
                    let double_click = app.structured.canvas.last_click.is_some_and(|(when, previous)| previous == node && when.elapsed() < std::time::Duration::from_millis(400));
                    app.structured.canvas.last_click = Some((std::time::Instant::now(), node));
                    app.canvas_begin_node_drag(node, (mouse.column, mouse.row));
                    if double_click {
                        app.structured.canvas.interaction = crate::app::CanvasInteraction::Idle;
                        app.canvas_activate_selected_node();
                    }
                    return true;
                }
                if let Some((edge, _)) = app.structured.canvas.edge_cells.iter().rev().find(|(_, position)| *position == pointer).copied() {
                    app.canvas_select_edge(edge);
                    return true;
                }
                if app.structured.canvas.view_area.contains(pointer) {
                    app.canvas_begin_pan((mouse.column, mouse.row));
                }
                true
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                app.structured.canvas.hovered_node = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).map(|(node, _)| *node);
                app.canvas_pointer_drag_with_aspect((mouse.column, mouse.row), mouse.modifiers.contains(KeyModifiers::SHIFT));
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                let target = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).map(|(node, rect)| (*node, Some(canvas_side_at(*rect, pointer))));
                app.canvas_end_pointer_interaction(target);
                true
            }
            MouseEventKind::Moved => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                app.structured.canvas.hovered_node = app.structured.canvas.node_rects.iter().rev().find(|(_, rect)| rect.contains(pointer)).map(|(node, _)| *node);
                app.structured.canvas.hovered_edge = if app.structured.canvas.hovered_node.is_none() { app.structured.canvas.edge_cells.iter().rev().find(|(_, position)| *position == pointer).map(|(edge, _)| *edge) } else { None };
                app.structured.canvas.hovered_resize = app.structured.canvas.resize_rects.iter().find(|(_, rect)| rect.contains(pointer)).map(|(handle, _)| (*handle, pointer));
                true
            }
            MouseEventKind::ScrollUp => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                if app.canvas_editor_contains(pointer) {
                    app.canvas_edit_scroll(-3);
                } else {
                    app.canvas_zoom_at(1.1, Some((mouse.column, mouse.row)));
                }
                true
            }
            MouseEventKind::ScrollDown => {
                let pointer = ratatui::layout::Position::new(mouse.column, mouse.row);
                if app.canvas_editor_contains(pointer) {
                    app.canvas_edit_scroll(3);
                } else {
                    app.canvas_zoom_at(1.0 / 1.1, Some((mouse.column, mouse.row)));
                }
                true
            }
            _ => false,
        },
        Some(ekphos_vault::VaultFileKind::Markdown) | None => false,
    }
}

fn canvas_side_at(rect: ratatui::layout::Rect, pointer: ratatui::layout::Position) -> ekphos_canvas::CanvasSide {
    let distances = [
        (pointer.y.saturating_sub(rect.y), ekphos_canvas::CanvasSide::Top),
        (rect.right().saturating_sub(1).saturating_sub(pointer.x), ekphos_canvas::CanvasSide::Right),
        (rect.bottom().saturating_sub(1).saturating_sub(pointer.y), ekphos_canvas::CanvasSide::Bottom),
        (pointer.x.saturating_sub(rect.x), ekphos_canvas::CanvasSide::Left),
    ];
    distances.into_iter().min_by_key(|(distance, _)| *distance).map(|(_, side)| side).unwrap_or(ekphos_canvas::CanvasSide::Right)
}

pub(super) fn handle_paste_event(app: &mut App, text: String) {
    app.state.keymap.reset_pending();
    if app.canvas_editor_active() {
        app.canvas_edit_insert(&text);
        return;
    }
    if app.editor.mode != Mode::Edit {
        return;
    }
    paste_into_editor(app, Some(text));
}

pub(super) fn paste_into_editor(app: &mut App, fallback: Option<String>) {
    app.editor.context_menu_state = ContextMenuState::None;
    app.editor.wiki_autocomplete = WikiAutocompleteState::None;
    if app.state.config.editor.mode == EditingMode::Vim && matches!(app.editor.vim.mode, VimMode::Normal | VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) {
        app.editor.cancel_selection();
        app.editor.vim.mode = VimMode::Insert;
        update_cursor_style(app);
    }
    let paste_text = match clipboard::get_content_as_markdown_from(app.clipboard()) {
        Ok(ClipboardContent::Markdown(md)) => Some(md),
        Ok(ClipboardContent::PlainText(txt)) => Some(txt),
        Ok(ClipboardContent::Empty) => fallback,
        Err(e) => {
            app.show_error_toast(format!("Clipboard: {}", e));
            fallback
        }
    };
    if let Some(paste_text) = paste_text.filter(|text| !text.is_empty()) {
        if paste_text.contains('\n') {
            app.state.needs_full_clear = true;
        }
        app.editor.insert_str(&paste_text);
    } else {
        app.editor.paste();
    }
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
                if app.editor.has_selection() {
                    app.editor.cancel_selection();
                }
                if app.state.config.editor.mode == EditingMode::Vim && app.editor.vim.mode.is_visual() {
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
                let can_start_selection = app.state.config.editor.mode == EditingMode::Standard || app.editor.vim.mode == VimMode::Normal;
                if !app.editor.has_selection() && can_start_selection {
                    if app.state.config.editor.mode == EditingMode::Vim {
                        app.editor.vim.mode = VimMode::Visual;
                        update_cursor_style(app);
                        app.editor.set_inclusive_selection(true);
                    } else {
                        app.editor.set_inclusive_selection(false);
                    }
                    app.editor.start_selection();
                    app.update_editor_block();
                }
                if app.editor.has_selection() {
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
            if app.state.config.editor.mode == EditingMode::Vim {
                app.editor.vim.mode = VimMode::Normal;
                update_cursor_style(app);
            }
        }
        ContextMenuItem::Cut => {
            app.editor.cut();
            if app.state.config.editor.mode == EditingMode::Vim {
                app.editor.vim.mode = VimMode::Normal;
                update_cursor_style(app);
            }
        }
        ContextMenuItem::Paste => {
            paste_into_editor(app, None);
        }
        ContextMenuItem::SelectAll => {
            app.editor.select_all();
            if app.state.config.editor.mode == EditingMode::Vim {
                app.editor.vim.mode = VimMode::Visual;
                update_cursor_style(app);
            }
        }
    }
    app.update_editor_block();
}
