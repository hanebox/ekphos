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
    if app.graph_view.dirty && !app.graph_view.nodes.is_empty() {
        apply_force_directed_layout(
            &mut app.graph_view.nodes,
            &app.graph_view.edges,
            inner.width as f32,
            inner.height as f32,
        );
        if let Some(selected_idx) = app.graph_view.selected_node {
            if selected_idx < app.graph_view.nodes.len() {
                let node = &app.graph_view.nodes[selected_idx];
                let node_center_x = node.x + (node.width as f32 / 2.0);
                let node_center_y = node.y + (NODE_HEIGHT as f32 / 2.0);
                app.graph_view.viewport_x = node_center_x - (inner.width as f32 / 2.0);
                app.graph_view.viewport_y = node_center_y - (inner.height as f32 / 2.0);
            } else {
                app.graph_view.viewport_x = 0.0;
                app.graph_view.viewport_y = 0.0;
            }
        } else {
            app.graph_view.viewport_x = 0.0;
            app.graph_view.viewport_y = 0.0;
        }
        app.graph_view.dirty = false;
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

    // Draw edges first (below nodes) using orthogonal lines
    for edge in &app.graph_view.edges {
        if edge.from >= app.graph_view.nodes.len() || edge.to >= app.graph_view.nodes.len() {
            continue;
        }

        let from_node = &app.graph_view.nodes[edge.from];
        let to_node = &app.graph_view.nodes[edge.to];

        let from_node_x = ((from_node.x - vx) * zoom + inner.x as f32) as i32;
        let from_node_y = ((from_node.y - vy) * zoom + inner.y as f32) as i32;
        let to_node_x = ((to_node.x - vx) * zoom + inner.x as f32) as i32;
        let to_node_y = ((to_node.y - vy) * zoom + inner.y as f32) as i32;

        let (from_x, from_y, to_x, to_y) = calculate_connection_points(
            from_node_x, from_node_y, from_node.width as i32,
            to_node_x, to_node_y, to_node.width as i32,
            NODE_HEIGHT as i32,
        );

        let is_selected_edge = app.graph_view.selected_node
            .map(|sel| edge.from == sel || edge.to == sel)
            .unwrap_or(false);

        let edge_color = if is_selected_edge {
            theme.primary
        } else {
            theme.muted
        };

        draw_orthogonal_edge(buf, from_x, from_y, to_x, to_y, edge_color, inner);
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
        render_node(buf, node, screen_x, screen_y, is_selected, theme, inner);
    }

    render_help_bar(f, app, area);
}

/// Calculate optimal connection points between two nodes
/// All connections use the CENTER of each side:
/// - Top/Bottom: center-x of the node
/// - Left/Right: center-y of the node (middle row)
fn calculate_connection_points(
    from_x: i32, from_y: i32, from_w: i32,
    to_x: i32, to_y: i32, to_w: i32,
    node_h: i32,
) -> (i32, i32, i32, i32) {
    let from_center_x = from_x + from_w / 2;
    let to_center_x = to_x + to_w / 2;
    let from_center_y = from_y + node_h / 2;
    let to_center_y = to_y + node_h / 2;

    let dy = to_y - from_y;

    if dy > 0 {
        (from_center_x, from_y + node_h - 1, to_center_x, to_y)
    } else if dy < 0 {
        (from_center_x, from_y, to_center_x, to_y + node_h - 1)
    } else {
        let dx = to_x - from_x;
        if dx > 0 {
            (from_x + from_w - 1, from_center_y, to_x, to_center_y)
        } else {
            (from_x, from_center_y, to_x + to_w - 1, to_center_y)
        }
    }
}

fn render_node(
    buf: &mut Buffer,
    node: &crate::app::GraphNode,
    screen_x: i32,
    screen_y: i32,
    is_selected: bool,
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

    let border_color = if is_selected {
        theme.primary
    } else {
        theme.border
    };

    let bg_color = if is_selected {
        theme.selection
    } else {
        theme.dialog.background
    };

    let text_color = if is_selected {
        theme.foreground
    } else {
        theme.dialog.text
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

fn draw_orthogonal_edge(
    buf: &mut Buffer,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: ratatui::style::Color,
    clip: Rect,
) {
    if (x0 - x1).abs() <= 1 && (y0 - y1).abs() <= 1 {
        return;
    }

    let (adj_y0, adj_y1) = if y0 < y1 {
        (y0 + 1, y1 - 1) 
    } else if y0 > y1 {
        (y0 - 1, y1 + 1) 
    } else {
        (y0, y1) 
    };

    if x0 == x1 {
        if adj_y0 <= adj_y1 {
            for y in adj_y0..=adj_y1 {
                set_line_char(buf, x0, y, '│', color, clip);
            }
        } else {
            for y in adj_y1..=adj_y0 {
                set_line_char(buf, x0, y, '│', color, clip);
            }
        }
        return;
    }

    if y0 == y1 {
        let wrap_y = y0 + 2; 
        let going_right = x1 > x0;

        let adj_x0 = if going_right { x0 + 1 } else { x0 - 1 };
        let adj_x1 = if going_right { x1 - 1 } else { x1 + 1 };
        for y in y0..wrap_y {
            set_line_char(buf, adj_x0, y, '│', color, clip);
            set_line_char(buf, adj_x1, y, '│', color, clip);
        }

        let (h_min, h_max) = if adj_x0 < adj_x1 { (adj_x0 + 1, adj_x1 - 1) } else { (adj_x1 + 1, adj_x0 - 1) };
        for x in h_min..=h_max {
            set_line_char(buf, x, wrap_y, '─', color, clip);
        }

        set_line_char(buf, adj_x0, wrap_y, if going_right { '└' } else { '┘' }, color, clip);
        set_line_char(buf, adj_x1, wrap_y, if going_right { '┘' } else { '└' }, color, clip);
        return;
    }

    let mid_y = (y0 + y1) / 2;
    let going_right = x1 > x0;
    let going_down = y1 > y0;

    if going_down {
        for y in adj_y0..mid_y {
            set_line_char(buf, x0, y, '│', color, clip);
        }
    } else {
        for y in (mid_y + 1)..=adj_y0 {
            set_line_char(buf, x0, y, '│', color, clip);
        }
    }

    let (h_min, h_max) = if x0 < x1 { (x0 + 1, x1 - 1) } else { (x1 + 1, x0 - 1) };
    for x in h_min..=h_max {
        set_line_char(buf, x, mid_y, '─', color, clip);
    }

    if going_down {
        for y in (mid_y + 1)..=adj_y1 {
            set_line_char(buf, x1, y, '│', color, clip);
        }
    } else {
        for y in adj_y1..mid_y {
            set_line_char(buf, x1, y, '│', color, clip);
        }
    }

    let corner1 = if going_down {
        if going_right { '└' } else { '┘' }
    } else {
        if going_right { '┌' } else { '┐' }
    };
    set_line_char(buf, x0, mid_y, corner1, color, clip);

    let corner2 = if going_down {
        if going_right { '┐' } else { '┌' }
    } else {
        if going_right { '┘' } else { '└' }
    };
    set_line_char(buf, x1, mid_y, corner2, color, clip);
}

fn draw_vertical_line(
    buf: &mut Buffer,
    x: i32,
    y_start: i32,
    y_end: i32,
    color: ratatui::style::Color,
    clip: Rect,
) {
    let (y_min, y_max) = if y_start <= y_end { (y_start, y_end) } else { (y_end, y_start) };

    for y in y_min..=y_max {
        set_line_char(buf, x, y, '│', color, clip);
    }
}

fn set_line_char(
    buf: &mut Buffer,
    x: i32,
    y: i32,
    ch: char,
    color: ratatui::style::Color,
    clip: Rect,
) {
    if x >= clip.x as i32
        && x < (clip.x + clip.width) as i32
        && y >= clip.y as i32
        && y < (clip.y + clip.height) as i32
    {
        if let Some(cell) = buf.cell_mut((x as u16, y as u16)) {
            let current = cell.symbol();
            if current == " " {
                cell.set_char(ch);
                cell.set_fg(color);
            } else if is_line_char(current) {
                let merged = merge_line_chars(current.chars().next().unwrap_or(' '), ch);
                cell.set_char(merged);
                cell.set_fg(color);
            }
        }
    }
}

fn is_line_char(s: &str) -> bool {
    matches!(s, "─" | "│" | "┌" | "┐" | "└" | "┘" | "├" | "┤" | "┬" | "┴" | "┼" | "·")
}

fn merge_line_chars(existing: char, new: char) -> char {
    let (e_up, e_down, e_left, e_right) = char_directions(existing);
    let (n_up, n_down, n_left, n_right) = char_directions(new);

    let up = e_up || n_up;
    let down = e_down || n_down;
    let left = e_left || n_left;
    let right = e_right || n_right;

    match (up, down, left, right) {
        (true, true, true, true) => '┼',
        (true, true, true, false) => '┤',
        (true, true, false, true) => '├',
        (true, false, true, true) => '┴',
        (false, true, true, true) => '┬',
        (true, true, false, false) => '│',
        (false, false, true, true) => '─',
        (true, false, true, false) => '┘',
        (true, false, false, true) => '└',
        (false, true, true, false) => '┐',
        (false, true, false, true) => '┌',
        (true, false, false, false) => '│',
        (false, true, false, false) => '│',
        (false, false, true, false) => '─',
        (false, false, false, true) => '─',
        _ => new,
    }
}

fn char_directions(ch: char) -> (bool, bool, bool, bool) {
    match ch {
        '│' => (true, true, false, false),
        '─' => (false, false, true, true),
        '┌' => (false, true, false, true),
        '┐' => (false, true, true, false),
        '└' => (true, false, false, true),
        '┘' => (true, false, true, false),
        '├' => (true, true, false, true),
        '┤' => (true, true, true, false),
        '┬' => (false, true, true, true),
        '┴' => (true, false, true, true),
        '┼' => (true, true, true, true),
        _ => (false, false, false, false),
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
