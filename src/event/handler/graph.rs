use super::*;

pub(super) fn zoom_graph(app: &mut App, factor: f32) {
    let old_zoom = app.graph.graph_view.zoom;
    let min_zoom = graph_fit_zoom(app);
    let new_zoom = (old_zoom * factor).clamp(min_zoom, ekphos_graph::GRAPH_MAX_ZOOM);
    if new_zoom <= min_zoom * 1.0001 {
        app.graph.graph_view.zoom = min_zoom;
        center_graph_bounds(app);
        return;
    }
    let (anchor_x, anchor_y) = if let Some(idx) = app.graph.graph_view.selected_node {
        if idx < app.graph.graph_view.nodes.len() {
            let node = &app.graph.graph_view.nodes[idx];
            (node.x + 1.5, node.y + 1.0)
        } else {
            graph_center(app)
        }
    } else {
        graph_center(app)
    };
    let screen_anchor_x = (anchor_x - app.graph.graph_view.viewport_x) * old_zoom;
    let screen_anchor_y = (anchor_y - app.graph.graph_view.viewport_y) * old_zoom;
    app.graph.graph_view.zoom = new_zoom;
    app.graph.graph_view.viewport_x = anchor_x - screen_anchor_x / new_zoom;
    app.graph.graph_view.viewport_y = anchor_y - screen_anchor_y / new_zoom;
}

pub(super) fn graph_bounds(app: &App) -> Option<(f32, f32, f32, f32)> {
    if app.graph.graph_view.nodes.is_empty() {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for node in &app.graph.graph_view.nodes {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x + 3.0);
        max_y = max_y.max(node.y + 3.0);
    }
    Some((min_x, min_y, max_x, max_y))
}

pub(super) fn graph_fit_zoom(app: &App) -> f32 {
    let Some((min_x, min_y, max_x, max_y)) = graph_bounds(app) else {
        return ekphos_graph::fit_zoom_for_bounds(1.0, 1.0, app.graph.graph_view.view_width, app.graph.graph_view.view_height);
    };
    let graph_width = (max_x - min_x).max(3.0);
    let graph_height = (max_y - min_y).max(2.0);
    ekphos_graph::fit_zoom_for_bounds(graph_width, graph_height, app.graph.graph_view.view_width, app.graph.graph_view.view_height)
}

pub(super) fn center_graph_bounds(app: &mut App) {
    let Some((min_x, min_y, max_x, max_y)) = graph_bounds(app) else {
        return;
    };
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;
    app.graph.graph_view.viewport_x = center_x - app.graph.graph_view.view_width / app.graph.graph_view.zoom / 2.0;
    app.graph.graph_view.viewport_y = center_y - app.graph.graph_view.view_height / app.graph.graph_view.zoom / 2.0;
    app.graph.graph_view.needs_center = false;
}

/// Calculate center of all nodes
pub(super) fn graph_center(app: &App) -> (f32, f32) {
    if app.graph.graph_view.nodes.is_empty() {
        return (0.0, 0.0);
    }
    let mut sum_x = 0.0f32;
    let mut sum_y = 0.0f32;
    for node in &app.graph.graph_view.nodes {
        sum_x += node.x;
        sum_y += node.y;
    }
    let n = app.graph.graph_view.nodes.len() as f32;
    (sum_x / n, sum_y / n)
}

/// Fit every node inside the viewport. This is also the zoom-out boundary.
pub(super) fn fit_graph_to_screen(app: &mut App) {
    app.graph.graph_view.zoom = graph_fit_zoom(app);
    center_graph_bounds(app);
}

pub(super) fn handle_graph_view_dialog(app: &mut App, key: crossterm::event::KeyEvent) {
    if app.graph.graph_view.filter_editing {
        match key.code {
            KeyCode::Esc => {
                let previous = app.graph.graph_view.filter_before_edit.clone();
                app.graph.graph_view.filter_draft = previous.clone();
                app.graph.graph_view.filter_editing = false;
                app.update_graph_filter(previous, false);
            }
            KeyCode::Enter => {
                let query = app.graph.graph_view.filter_draft.clone();
                app.graph.graph_view.filter_editing = false;
                app.update_graph_filter(query, false);
                if app.graph.graph_view.selected_node.is_some() {
                    center_on_selected_node(app);
                }
            }
            KeyCode::Backspace => {
                app.graph.graph_view.filter_draft.pop();
                let query = app.graph.graph_view.filter_draft.clone();
                app.update_graph_filter(query, false);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.graph.graph_view.filter_draft.clear();
                app.update_graph_filter(String::new(), false);
            }
            KeyCode::Char(ch) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                app.graph.graph_view.filter_draft.push(ch);
                let query = app.graph.graph_view.filter_draft.clone();
                app.update_graph_filter(query, false);
            }
            _ => {}
        }
        return;
    }
    if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
        if let Some(node_idx) = app.graph.graph_view.selected_node {
            if node_idx < app.graph.graph_view.nodes.len() {
                let move_amount = 2.0;
                match key.code {
                    KeyCode::Char('h') => {
                        app.graph.graph_view.nodes[node_idx].x -= move_amount;
                        return;
                    }
                    KeyCode::Char('j') => {
                        app.graph.graph_view.nodes[node_idx].y += move_amount;
                        return;
                    }
                    KeyCode::Char('k') => {
                        app.graph.graph_view.nodes[node_idx].y -= move_amount;
                        return;
                    }
                    KeyCode::Char('l') => {
                        app.graph.graph_view.nodes[node_idx].x += move_amount;
                        return;
                    }
                    _ => {}
                }
            }
        }
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_graph_view();
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
            open_selected_graph_node(app);
        }
        KeyCode::Char('H') => {
            app.graph.graph_view.viewport_x -= 10.0;
        }
        KeyCode::Char('J') => {
            app.graph.graph_view.viewport_y += 5.0;
        }
        KeyCode::Char('K') => {
            app.graph.graph_view.viewport_y -= 5.0;
        }
        KeyCode::Char('L') => {
            app.graph.graph_view.viewport_x += 10.0;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            zoom_graph(app, 1.25);
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            zoom_graph(app, 1.0 / 1.25);
        }
        KeyCode::Char('f') => {
            fit_graph_to_screen(app);
        }
        KeyCode::Char('0') => {
            let fit_zoom = graph_fit_zoom(app);
            app.graph.graph_view.zoom = 1.0f32.clamp(fit_zoom, ekphos_graph::GRAPH_MAX_ZOOM);
            if app.graph.graph_view.zoom <= fit_zoom * 1.0001 {
                center_graph_bounds(app);
            } else {
                center_on_selected_node(app);
            }
        }
        KeyCode::Char('g') => {
            if !app.graph.graph_view.nodes.is_empty() {
                select_graph_node(app, 0, true);
            }
        }
        KeyCode::Char('G') => {
            if !app.graph.graph_view.nodes.is_empty() {
                select_graph_node(app, app.graph.graph_view.nodes.len() - 1, true);
            }
        }
        KeyCode::Char('u') => {
            app.graph.graph_view.selected_node = None;
            app.graph.graph_view.selected_note_index = None;
        }
        KeyCode::Char('v') => app.toggle_graph_mode(),
        KeyCode::Char('[') => app.change_graph_depth(-1),
        KeyCode::Char(']') => app.change_graph_depth(1),
        KeyCode::Char('d') => app.cycle_graph_link_scope(),
        KeyCode::Char('/') => {
            app.graph.graph_view.filter_before_edit = app.graph.graph_view.filter_query.clone();
            app.graph.graph_view.filter_draft = app.graph.graph_view.filter_query.clone();
            app.graph.graph_view.filter_editing = true;
        }
        KeyCode::Char('o') => app.toggle_graph_orphans(),
        KeyCode::Char('r') => app.reset_graph_view(),
        KeyCode::Char('?') => app.graph.graph_view.help_visible = !app.graph.graph_view.help_visible,
        KeyCode::Char(' ') => app.reroot_graph_on_selected(),
        KeyCode::Char('n') => cycle_graph_match(app, 1),
        KeyCode::Char('N') => cycle_graph_match(app, -1),
        KeyCode::Tab => {
            app.toggle_graph_mode();
        }
        _ => {}
    }
}

pub(super) fn open_selected_graph_node(app: &mut App) {
    let Some(node_idx) = app.graph.graph_view.selected_node else {
        return;
    };
    let Some(node) = app.graph.graph_view.nodes.get(node_idx) else {
        return;
    };
    let note_id = node.note_id;
    let Some(note_idx) = app.note_index_for_id(note_id) else {
        return;
    };
    if app.navigate_to_note(note_idx) {
        app.close_graph_view();
        app.state.focus = Focus::Content;
    }
}

pub(super) fn select_graph_node(app: &mut App, idx: usize, center: bool) {
    if let Some(note_id) = app.graph.graph_view.nodes.get(idx).map(|node| node.note_id) {
        app.graph.graph_view.selected_node = Some(idx);
        app.graph.graph_view.selected_note_index = app.note_index_for_id(note_id);
        if center {
            center_on_selected_node(app);
        }
    }
}

pub(super) fn cycle_graph_match(app: &mut App, delta: isize) {
    let skip_context_root = app.graph.graph_view.mode == ekphos_graph::GraphMode::Local && !app.graph.graph_view.filter_query.trim().is_empty();
    let candidates: Vec<_> = app.graph.graph_view.nodes.iter().enumerate().filter_map(|(idx, node)| (!skip_context_root || node.depth != 0).then_some(idx)).collect();
    if candidates.is_empty() {
        return;
    }
    let next = app.graph.graph_view.selected_node.and_then(|selected| candidates.iter().position(|&idx| idx == selected)).map(|position| (position as isize + delta).rem_euclid(candidates.len() as isize) as usize).unwrap_or_else(|| usize::from(delta < 0) * (candidates.len() - 1));
    select_graph_node(app, candidates[next], true);
}

#[derive(Debug, Clone, Copy)]
enum GraphDirection {
    Left,
    Right,
    Up,
    Down,
}
fn navigate_graph_node(app: &mut App, direction: GraphDirection) {
    if app.graph.graph_view.nodes.is_empty() {
        return;
    }
    let current = app.graph.graph_view.selected_node.unwrap_or(0);
    if current >= app.graph.graph_view.nodes.len() {
        app.graph.graph_view.selected_node = Some(0);
        return;
    }
    let current_node = &app.graph.graph_view.nodes[current];
    let current_x = current_node.x;
    let current_y = current_node.y;
    let mut best_idx = None;
    let mut best_dist = f32::MAX;
    for (idx, node) in app.graph.graph_view.nodes.iter().enumerate() {
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
        select_graph_node(app, idx, true);
    }
}

pub(super) fn center_on_selected_node(app: &mut App) {
    app.graph.graph_view.needs_center = true;
}

pub(super) fn handle_graph_view_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    let mouse_x = mouse.column;
    let mouse_y = mouse.row;
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(idx) = find_node_at_position(app, mouse_x, mouse_y) {
                let double_click = app.graph.graph_view.last_click.map(|(when, previous)| previous == idx && when.elapsed() < std::time::Duration::from_millis(400)).unwrap_or(false);
                select_graph_node(app, idx, false);
                app.graph.graph_view.last_click = Some((std::time::Instant::now(), idx));
                if double_click {
                    open_selected_graph_node(app);
                    return;
                }
                if app.graph.graph_view.zoom < 0.055 {
                    let overview_zoom = (0.12 / app.graph.graph_view.zoom).clamp(2.0, 64.0);
                    zoom_graph(app, overview_zoom);
                    center_on_selected_node(app);
                    app.graph.graph_view.dragging_node = None;
                    app.graph.graph_view.is_panning = false;
                    return;
                }
                app.graph.graph_view.dragging_node = Some(idx);
                app.graph.graph_view.drag_start = Some((mouse_x, mouse_y));
                app.graph.graph_view.is_panning = false;
            } else {
                app.graph.graph_view.dragging_node = None;
                app.graph.graph_view.is_panning = true;
                app.graph.graph_view.drag_start = Some((mouse_x, mouse_y));
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            app.graph.graph_view.is_panning = false;
            app.graph.graph_view.dragging_node = None;
            app.graph.graph_view.drag_start = None;
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((start_x, start_y)) = app.graph.graph_view.drag_start {
                let dx = mouse_x as f32 - start_x as f32;
                let dy = mouse_y as f32 - start_y as f32;
                if let Some(node_idx) = app.graph.graph_view.dragging_node {
                    if node_idx < app.graph.graph_view.nodes.len() {
                        app.graph.graph_view.nodes[node_idx].x += dx / app.graph.graph_view.zoom;
                        app.graph.graph_view.nodes[node_idx].y += dy / app.graph.graph_view.zoom;
                        app.graph.graph_view.nodes[node_idx].home_x = app.graph.graph_view.nodes[node_idx].x;
                        app.graph.graph_view.nodes[node_idx].home_y = app.graph.graph_view.nodes[node_idx].y;
                    }
                } else if app.graph.graph_view.is_panning {
                    app.graph.graph_view.viewport_x -= dx / app.graph.graph_view.zoom;
                    app.graph.graph_view.viewport_y -= dy / app.graph.graph_view.zoom;
                }
                app.graph.graph_view.drag_start = Some((mouse_x, mouse_y));
            }
        }
        MouseEventKind::ScrollUp => {
            zoom_graph(app, 1.15);
        }
        MouseEventKind::ScrollDown => {
            zoom_graph(app, 1.0 / 1.15);
        }
        _ => {}
    }
}

pub(super) fn find_node_at_position(app: &App, mouse_x: u16, mouse_y: u16) -> Option<usize> {
    const NODE_WIDTH: i32 = 3;
    const NODE_HEIGHT: i32 = 2;
    let vx = app.graph.graph_view.viewport_x;
    let vy = app.graph.graph_view.viewport_y;
    let zoom = app.graph.graph_view.zoom;
    let area = app.graph.graph_view.graph_area;
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let mut best: Option<(usize, usize)> = None;
    for (idx, node) in app.graph.graph_view.nodes.iter().enumerate() {
        let screen_x = ((node.x - vx) * zoom + area.x as f32).round() as i32;
        let screen_y = ((node.y - vy) * zoom + area.y as f32).round() as i32;
        let radius_x = if zoom < 0.20 { 1 } else { NODE_WIDTH };
        let radius_y = if zoom < 0.20 { 1 } else { NODE_HEIGHT };
        if mouse_x as i32 >= screen_x - 1 && mouse_x as i32 <= screen_x + radius_x && mouse_y as i32 >= screen_y - 1 && mouse_y as i32 <= screen_y + radius_y {
            let priority = usize::from(app.graph.graph_view.selected_node == Some(idx)).saturating_mul(usize::MAX / 2).saturating_add(node.degree());
            if best.map(|(_, current)| priority > current).unwrap_or(true) {
                best = Some((idx, priority));
            }
        }
    }
    best.map(|(idx, _)| idx)
}
