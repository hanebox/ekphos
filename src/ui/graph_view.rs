//! Graph View rendering for wiki link visualization
//! Uses square nodes with floating text labels below

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::graph::apply_force_directed_layout;

// Node is a small square: 3 wide, 2 tall (looks square in terminal)
const NODE_WIDTH: u16 = 3;
const NODE_HEIGHT: u16 = 2;
const LABEL_OFFSET: i32 = 1;  // Gap between node and label

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

        // Start zoomed out to see more of the graph
        app.graph_view.zoom = 0.5;

        // Center viewport on selected node if any, otherwise graph center
        let (center_x, center_y) = if let Some(idx) = app.graph_view.selected_node {
            if idx < app.graph_view.nodes.len() {
                let node = &app.graph_view.nodes[idx];
                (node.x + NODE_WIDTH as f32 / 2.0, node.y + NODE_HEIGHT as f32 / 2.0)
            } else {
                graph_center(&app.graph_view.nodes)
            }
        } else {
            graph_center(&app.graph_view.nodes)
        };

        let zoom = app.graph_view.zoom;
        app.graph_view.viewport_x = center_x - (inner.width as f32 / zoom / 2.0);
        app.graph_view.viewport_y = center_y - (inner.height as f32 / zoom / 2.0);

        app.graph_view.dirty = false;
    }

    if app.graph_view.needs_center {
        if let Some(selected_idx) = app.graph_view.selected_node {
            if selected_idx < app.graph_view.nodes.len() {
                let node = &app.graph_view.nodes[selected_idx];
                let zoom = app.graph_view.zoom;
                let node_center_x = node.x + (NODE_WIDTH as f32 / 2.0);
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

    // Layer 1: Draw dimmed edges first (not connected to selected node)
    for edge in &app.graph_view.edges {
        if edge.from >= app.graph_view.nodes.len() || edge.to >= app.graph_view.nodes.len() {
            continue;
        }

        let is_selected_edge = app.graph_view.selected_node
            .map(|sel| edge.from == sel || edge.to == sel)
            .unwrap_or(false);

        if is_selected_edge {
            continue; // Draw these later on top
        }

        let from_node = &app.graph_view.nodes[edge.from];
        let to_node = &app.graph_view.nodes[edge.to];

        let from_screen_x = ((from_node.x - vx) * zoom + inner.x as f32) as i32;
        let from_screen_y = ((from_node.y - vy) * zoom + inner.y as f32) as i32;
        let to_screen_x = ((to_node.x - vx) * zoom + inner.x as f32) as i32;
        let to_screen_y = ((to_node.y - vy) * zoom + inner.y as f32) as i32;
        let from_center_x = from_screen_x + NODE_WIDTH as i32 / 2;
        let from_center_y = from_screen_y + NODE_HEIGHT as i32 / 2;
        let to_center_x = to_screen_x + NODE_WIDTH as i32 / 2;
        let to_center_y = to_screen_y + NODE_HEIGHT as i32 / 2;

        // Dimmed edge color when there's a selection
        let edge_color = if has_selection {
            theme.muted
        } else {
            theme.border
        };

        draw_line(buf, from_center_x, from_center_y, to_center_x, to_center_y, edge_color, inner, false);
    }

    // Layer 2: Draw dimmed nodes (not connected to selected)
    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        let is_dimmed = has_selection && !connected_nodes.contains(&idx);
        if !is_dimmed {
            continue; // Draw connected nodes later on top
        }

        let screen_x = ((node.x - vx) * zoom + inner.x as f32) as i32;
        let screen_y = ((node.y - vy) * zoom + inner.y as f32) as i32;

        if screen_x < (inner.x as i32 - NODE_WIDTH as i32)
            || screen_x >= (inner.x + inner.width) as i32
            || screen_y < (inner.y as i32 - NODE_HEIGHT as i32)
            || screen_y >= (inner.y + inner.height) as i32
        {
            continue;
        }

        render_node(buf, node, screen_x, screen_y, false, true, theme, inner);
    }

    // Layer 3: Draw highlighted edges (connected to selected node) on top
    for edge in &app.graph_view.edges {
        if edge.from >= app.graph_view.nodes.len() || edge.to >= app.graph_view.nodes.len() {
            continue;
        }

        let is_selected_edge = app.graph_view.selected_node
            .map(|sel| edge.from == sel || edge.to == sel)
            .unwrap_or(false);

        if !is_selected_edge {
            continue; // Already drawn
        }

        let from_node = &app.graph_view.nodes[edge.from];
        let to_node = &app.graph_view.nodes[edge.to];

        let from_screen_x = ((from_node.x - vx) * zoom + inner.x as f32) as i32;
        let from_screen_y = ((from_node.y - vy) * zoom + inner.y as f32) as i32;
        let to_screen_x = ((to_node.x - vx) * zoom + inner.x as f32) as i32;
        let to_screen_y = ((to_node.y - vy) * zoom + inner.y as f32) as i32;
        let from_center_x = from_screen_x + NODE_WIDTH as i32 / 2;
        let from_center_y = from_screen_y + NODE_HEIGHT as i32 / 2;
        let to_center_x = to_screen_x + NODE_WIDTH as i32 / 2;
        let to_center_y = to_screen_y + NODE_HEIGHT as i32 / 2;

        draw_line(buf, from_center_x, from_center_y, to_center_x, to_center_y, theme.primary, inner, true);
    }

    // Layer 4: Draw connected and selected nodes on top
    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        let is_dimmed = has_selection && !connected_nodes.contains(&idx);
        if is_dimmed {
            continue; // Already drawn
        }

        let screen_x = ((node.x - vx) * zoom + inner.x as f32) as i32;
        let screen_y = ((node.y - vy) * zoom + inner.y as f32) as i32;

        if screen_x < (inner.x as i32 - NODE_WIDTH as i32)
            || screen_x >= (inner.x + inner.width) as i32
            || screen_y < (inner.y as i32 - NODE_HEIGHT as i32)
            || screen_y >= (inner.y + inner.height) as i32
        {
            continue;
        }

        let is_selected = app.graph_view.selected_node == Some(idx);
        render_node(buf, node, screen_x, screen_y, is_selected, false, theme, inner);
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
    force_overwrite: bool,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;

    // Determine line character based on overall direction
    // Use simpler dot character for cleaner appearance
    let line_char = if dx == 0 {
        '│' 
    } else if dy == 0 {
        '─' 
    } else {
        '·' 
    };

    loop {
        if x >= clip.x as i32
            && x < (clip.x + clip.width) as i32
            && y >= clip.y as i32
            && y < (clip.y + clip.height) as i32
        {
            if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
                let current = cell.symbol();
                // For highlighted edges, overwrite more aggressively
                if force_overwrite || current == " " || current == "·" || current == "─" || current == "│" {
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
    // Determine colors
    let (node_color, text_color) = if is_selected {
        (theme.primary, theme.primary)
    } else if is_dimmed {
        (theme.muted, theme.muted)
    } else {
        (theme.foreground, theme.dialog.text)
    };

    // Selected nodes: square with dot on top ╭●╮
    // Regular nodes: plain square ╭─╮
    // Both are 2 rows tall (looks square in terminal):
    // ╭●╮ or ╭─╮
    // ╰─╯    ╰─╯
    let node_height = 2;
    let top_chars = if is_selected {
        ['╭', '●', '╮']
    } else {
        ['╭', '─', '╮']
    };

    // Row 0: top edge
    for dx in 0..NODE_WIDTH as i32 {
        let px = screen_x + dx;
        let py = screen_y;
        if px >= clip.x as i32 && px < (clip.x + clip.width) as i32
            && py >= clip.y as i32 && py < (clip.y + clip.height) as i32 {
            if let Some(cell) = buf.cell_mut((px as u16, py as u16)) {
                cell.set_char(top_chars[dx as usize]);
                cell.set_fg(node_color);
            }
        }
    }
    // Row 1: ╰─╯
    let bot_chars = ['╰', '─', '╯'];
    for dx in 0..NODE_WIDTH as i32 {
        let px = screen_x + dx;
        let py = screen_y + 1;
        if px >= clip.x as i32 && px < (clip.x + clip.width) as i32
            && py >= clip.y as i32 && py < (clip.y + clip.height) as i32 {
            if let Some(cell) = buf.cell_mut((px as u16, py as u16)) {
                cell.set_char(bot_chars[dx as usize]);
                cell.set_fg(node_color);
            }
        }
    }

    // Draw floating label below the node (centered)
    let label_y = screen_y + node_height + LABEL_OFFSET;
    if label_y >= clip.y as i32 && label_y < (clip.y + clip.height) as i32 {
        let display_title = &node.title;
        let display_len = display_title.width();

        // Center the label under the node
        let label_x = screen_x + (NODE_WIDTH as i32 / 2) - (display_len as i32 / 2);

        // Track display column position for proper CJK character rendering
        let mut col_offset = 0i32;
        for ch in display_title.chars() {
            let ch_width = ch.width().unwrap_or(1);
            let col = label_x + col_offset;
            if col >= clip.x as i32 && col < (clip.x + clip.width) as i32 {
                if let Some(cell) = buf.cell_mut((col as u16, label_y as u16)) {
                    cell.set_char(ch);
                    cell.set_fg(text_color);
                }
            }
            col_offset += ch_width as i32;
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

/// Calculate center of all nodes
fn graph_center(nodes: &[crate::app::GraphNode]) -> (f32, f32) {
    if nodes.is_empty() {
        return (0.0, 0.0);
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for node in nodes {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x + NODE_WIDTH as f32);
        max_y = max_y.max(node.y + NODE_HEIGHT as f32);
    }
    ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
}
