//! Graph View rendering for wiki link visualization
//! Uses orthogonal (horizontal/vertical only) lines for clean tree-like structure

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::graph::apply_force_directed_layout;

const NODE_HEIGHT: u16 = 3;

pub fn render_graph_view(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let theme = &app.theme;
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Graph View ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog.border))
        .style(Style::default().bg(theme.dialog.background));

    let inner = block.inner(area);
    f.render_widget(block, area);

    app.graph_view.view_width = inner.width as f32;
    app.graph_view.view_height = inner.height as f32;

    if app.graph_view.dirty && !app.graph_view.nodes.is_empty() {
        apply_force_directed_layout(
            &mut app.graph_view.nodes,
            &app.graph_view.edges,
            inner.width as f32,
            inner.height as f32,
        );
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for node in &app.graph_view.nodes {
            min_x = min_x.min(node.x);
            min_y = min_y.min(node.y);
            max_x = max_x.max(node.x + node.width as f32);
            max_y = max_y.max(node.y + NODE_HEIGHT as f32);
        }
        let graph_width = max_x - min_x;
        let graph_height = max_y - min_y;
        let padding = 10.0;

        let zoom_x = (inner.width as f32 - padding * 2.0) / graph_width.max(1.0);
        let zoom_y = (inner.height as f32 - padding * 2.0) / graph_height.max(1.0);
        let fit_zoom = zoom_x.min(zoom_y).min(0.7).max(0.3); // Clamp between 0.3 and 0.7
        app.graph_view.zoom = fit_zoom;

        let graph_center_x = (min_x + max_x) / 2.0;
        let graph_center_y = (min_y + max_y) / 2.0;
        app.graph_view.viewport_x = graph_center_x - (inner.width as f32 / fit_zoom / 2.0);
        app.graph_view.viewport_y = graph_center_y - (inner.height as f32 / fit_zoom / 2.0);

        app.graph_view.dirty = false;
    }

    if app.graph_view.needs_center {
        if let Some(selected_idx) = app.graph_view.selected_node {
            if selected_idx < app.graph_view.nodes.len() {
                let node = &app.graph_view.nodes[selected_idx];
                let zoom = app.graph_view.zoom;
                let node_center_x = node.x + (node.width as f32 / 2.0);
                let node_center_y = node.y + (NODE_HEIGHT as f32 / 2.0);
                app.graph_view.viewport_x = node_center_x - (inner.width as f32 / zoom / 2.0);
                app.graph_view.viewport_y = node_center_y - (inner.height as f32 / zoom / 2.0);
            }
        }
        app.graph_view.needs_center = false;
    }

    if app.graph_view.nodes.is_empty() {
        let empty_msg = Paragraph::new("No notes to display")
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Center);
        let msg_area = Rect {
            x: inner.x,
            y: inner.y + inner.height / 2,
            width: inner.width,
            height: 1,
        };
        f.render_widget(empty_msg, msg_area);
        render_help_bar(f, app, area);
        return;
    }

    let vx = app.graph_view.viewport_x;
    let vy = app.graph_view.viewport_y;
    let zoom = app.graph_view.zoom;
    let buf = f.buffer_mut();

    // Build set of connected nodes for dimming effect
    let connected_nodes: std::collections::HashSet<usize> = if let Some(selected) = app.graph_view.selected_node {
        let mut connected = std::collections::HashSet::new();
        connected.insert(selected);
        for edge in &app.graph_view.edges {
            if edge.from == selected {
                connected.insert(edge.to);
            } else if edge.to == selected {
                connected.insert(edge.from);
            }
        }
        connected
    } else {
        std::collections::HashSet::new()
    };
    let has_selection = app.graph_view.selected_node.is_some();

    // Draw edges first (below nodes) using straight lines from node centers
    for edge in &app.graph_view.edges {
        if edge.from >= app.graph_view.nodes.len() || edge.to >= app.graph_view.nodes.len() {
            continue;
        }

        let from_node = &app.graph_view.nodes[edge.from];
        let to_node = &app.graph_view.nodes[edge.to];

        let from_screen_x = ((from_node.x - vx) * zoom + inner.x as f32) as i32;
        let from_screen_y = ((from_node.y - vy) * zoom + inner.y as f32) as i32;
        let to_screen_x = ((to_node.x - vx) * zoom + inner.x as f32) as i32;
        let to_screen_y = ((to_node.y - vy) * zoom + inner.y as f32) as i32;
        let from_center_x = from_screen_x + from_node.width as i32 / 2;
        let from_center_y = from_screen_y + NODE_HEIGHT as i32 / 2;
        let to_center_x = to_screen_x + to_node.width as i32 / 2;
        let to_center_y = to_screen_y + NODE_HEIGHT as i32 / 2;

        let is_selected_edge = app.graph_view.selected_node
            .map(|sel| edge.from == sel || edge.to == sel)
            .unwrap_or(false);

        let edge_color = if is_selected_edge {
            theme.primary
        } else {
            theme.muted
        };

        draw_line(buf, from_center_x, from_center_y, to_center_x, to_center_y, edge_color, inner);
    }

    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        let screen_x = ((node.x - vx) * zoom + inner.x as f32) as i32;
        let screen_y = ((node.y - vy) * zoom + inner.y as f32) as i32;
        let node_width = node.width as i32;

        if screen_x < (inner.x as i32 - node_width)
            || screen_x >= (inner.x + inner.width) as i32
            || screen_y < (inner.y as i32 - NODE_HEIGHT as i32)
            || screen_y >= (inner.y + inner.height) as i32
        {
            continue;
        }

        let is_selected = app.graph_view.selected_node == Some(idx);
        let is_dimmed = has_selection && !connected_nodes.contains(&idx);
        render_node(buf, node, screen_x, screen_y, is_selected, is_dimmed, theme, inner);
    }

    render_help_bar(f, app, area);
}

/// Draw a straight line between two points using Bresenham's algorithm
fn draw_line(
    buf: &mut Buffer,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: ratatui::style::Color,
    clip: Rect,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;
    let line_char = '·';

    loop {
        if x >= clip.x as i32
            && x < (clip.x + clip.width) as i32
            && y >= clip.y as i32
            && y < (clip.y + clip.height) as i32
        {
            if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
                let current = cell.symbol();
                if current == " " || current == "·" {
                    cell.set_char(line_char);
                    cell.set_fg(color);
                }
            }
        }

        if x == x1 && y == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

fn render_node(
    buf: &mut Buffer,
    node: &crate::app::GraphNode,
    screen_x: i32,
    screen_y: i32,
    is_selected: bool,
    is_dimmed: bool,
    theme: &crate::config::Theme,
    clip: Rect,
) {
    let node_width = node.width as i32;

    let x = screen_x.max(clip.x as i32) as u16;
    let y = screen_y.max(clip.y as i32) as u16;
    let right = ((screen_x + node_width) as u16).min(clip.x + clip.width);
    let bottom = ((screen_y + NODE_HEIGHT as i32) as u16).min(clip.y + clip.height);

    if x >= right || y >= bottom {
        return;
    }

    let (border_color, bg_color, text_color) = if is_selected {
        (theme.primary, theme.selection, theme.foreground)
    } else if is_dimmed {
        (theme.muted, theme.dialog.background, theme.muted)
    } else {
        (theme.border, theme.dialog.background, theme.dialog.text)
    };

    for row in y..bottom {
        for col in x..right {
            if let Some(cell) = buf.cell_mut((col, row)) {
                let rel_x = col as i32 - screen_x;
                let rel_y = row as i32 - screen_y;

                let ch = if rel_y == 0 {
                    if rel_x == 0 {
                        '┌'
                    } else if rel_x == node_width - 1 {
                        '┐'
                    } else {
                        '─'
                    }
                } else if rel_y == NODE_HEIGHT as i32 - 1 {
                    if rel_x == 0 {
                        '└'
                    } else if rel_x == node_width - 1 {
                        '┘'
                    } else {
                        '─'
                    }
                } else if rel_x == 0 || rel_x == node_width - 1 {
                    '│'
                } else {
                    ' '
                };

                cell.set_char(ch);
                cell.set_fg(border_color);
                cell.set_bg(bg_color);
            }
        }
    }

    let title_y = screen_y + 1;
    if title_y >= clip.y as i32 && title_y < (clip.y + clip.height) as i32 {
        let display_title = &node.title;
        let display_len = display_title.chars().count();
        let inner_width = (node_width - 2) as usize;
        let padding = (inner_width.saturating_sub(display_len)) / 2;
        let title_x = screen_x + 1 + padding as i32;

        for (i, ch) in display_title.chars().enumerate() {
            let col = title_x + i as i32;
            if col >= clip.x as i32 && col < (clip.x + clip.width) as i32 && col > screen_x && col < screen_x + node_width - 1 {
                if let Some(cell) = buf.cell_mut((col as u16, title_y as u16)) {
                    cell.set_char(ch);
                    cell.set_fg(text_color);
                    cell.set_bg(bg_color);
                }
            }
        }
    }
}

fn render_help_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;

    let hint = Line::from(vec![
        Span::styled("hjkl", Style::default().fg(theme.warning)),
        Span::styled(": select  ", Style::default().fg(theme.muted)),
        Span::styled("HJKL", Style::default().fg(theme.warning)),
        Span::styled(": pan  ", Style::default().fg(theme.muted)),
        Span::styled("C-hjkl", Style::default().fg(theme.warning)),
        Span::styled(": move node  ", Style::default().fg(theme.muted)),
        Span::styled("Enter", Style::default().fg(theme.warning)),
        Span::styled(": open  ", Style::default().fg(theme.muted)),
        Span::styled("+/-", Style::default().fg(theme.warning)),
        Span::styled(": zoom  ", Style::default().fg(theme.muted)),
        Span::styled("Esc", Style::default().fg(theme.warning)),
        Span::styled(": close", Style::default().fg(theme.muted)),
    ]);

    let hint_area = Rect::new(area.x + 2, area.y + area.height - 2, area.width.saturating_sub(4), 1);
    f.render_widget(Paragraph::new(hint), hint_area);
}
