//! Local-first graph rendering with terminal-aware levels of detail.

use std::collections::{HashMap, HashSet};

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::config::Theme;
use ekphos_graph::{fit_zoom_for_bounds, GraphIndexNode, GraphMode, GraphNode, GraphRelation};

const NODE_WIDTH: u16 = 3;
const NODE_HEIGHT: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailLevel {
    Overview,
    Compact,
    Detail,
}

#[derive(Debug, Clone, Copy)]
struct OverviewBin {
    representative: usize,
    count: usize,
    selected: bool,
    root: bool,
    connected: bool,
}

pub fn render_graph_view(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.theme.clone();
    frame.render_widget(Clear, area);

    let title = graph_title(app);
    let block = Block::default()
        .title(Line::from(title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog.border))
        .style(Style::default().bg(theme.dialog.background));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height < 3 {
        app.graph_view.graph_area = Rect::default();
        app.graph_view.view_width = 0.0;
        app.graph_view.view_height = 0.0;
        return;
    }

    let has_top_status = inner.height >= 7;
    let top_rows = u16::from(has_top_status);
    let bottom_rows = if inner.height >= 6 { 2 } else { 1 };
    let graph_area = Rect::new(inner.x, inner.y + top_rows, inner.width, inner.height.saturating_sub(top_rows + bottom_rows));
    app.graph_view.graph_area = graph_area;
    app.graph_view.view_width = graph_area.width as f32;
    app.graph_view.view_height = graph_area.height as f32;

    if has_top_status {
        render_top_status(frame, app, Rect::new(inner.x, inner.y, inner.width, 1));
    }

    if app.graph_view.dirty && !app.graph_view.nodes.is_empty() {
        fit_graph_to_area(app, graph_area);
        app.graph_view.dirty = false;
    }
    if app.graph_view.needs_center {
        center_selected(app, graph_area);
        app.graph_view.needs_center = false;
    }

    if app.graph_view.nodes.is_empty() {
        let message = if app.graph_view.index_pending {
            "Indexing note connections…"
        } else if !app.graph_view.filter_query.is_empty() {
            "No notes match this graph filter"
        } else {
            "No notes to display"
        };
        render_centered_message(frame, graph_area, message, theme.muted);
    } else {
        render_graph(frame.buffer_mut(), app, graph_area);
    }

    render_bottom_status(frame, app, inner, bottom_rows);
    if app.graph_view.help_visible {
        render_help_overlay(frame, app, area);
    }
}

fn graph_title(app: &App) -> Vec<Span<'static>> {
    let theme = &app.theme;
    let mut spans = vec![
        Span::styled(" GRAPH ", Style::default().fg(theme.dialog.title).add_modifier(Modifier::BOLD)),
        Span::styled(app.graph_view.mode.label(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
    ];
    if app.graph_view.mode == GraphMode::Local {
        spans.extend([
            Span::styled(format!("  depth {}", app.graph_view.depth), Style::default().fg(theme.dialog.text)),
            Span::styled(format!("  {}", app.graph_view.link_scope.label()), Style::default().fg(theme.info)),
        ]);
    }
    spans.push(Span::styled(
        format!("  {} notes · {} links ", app.graph_view.nodes.len(), app.graph_view.edges.len()),
        Style::default().fg(theme.muted),
    ));
    spans
}

fn render_top_status(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let line = if app.graph_view.filter_editing {
        Line::from(vec![
            Span::styled(" Filter › ", Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            Span::styled(app.graph_view.filter_draft.clone(), Style::default().fg(theme.search.input)),
            Span::styled("▏", Style::default().fg(theme.primary)),
            Span::styled("  Enter apply · Esc cancel", Style::default().fg(theme.muted)),
        ])
    } else if let Some(selected) = app.graph_view.selected_node.and_then(|idx| app.graph_view.nodes.get(idx)) {
        let metadata = graph_node_metadata(app, selected);
        Line::from(vec![
            Span::styled(
                format!(" {} ", metadata.map_or("", |node| node.title.as_str())),
                Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", metadata.map_or("", |node| node.path.as_str())),
                Style::default().fg(theme.muted),
            ),
            Span::styled(format!("←{}  →{}", selected.in_degree, selected.out_degree), Style::default().fg(theme.info)),
        ])
    } else {
        Line::from(Span::styled(
            format!(" {} total notes · {} total links", app.graph_view.total_nodes, app.graph_view.total_edges),
            Style::default().fg(theme.muted),
        ))
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn render_bottom_status(frame: &mut Frame, app: &App, inner: Rect, rows: u16) {
    let theme = &app.theme;
    let status_y = inner.y + inner.height.saturating_sub(rows);
    let mut status = Vec::new();
    status.push(Span::styled(format!(" {:.2}×  ", app.graph_view.zoom), Style::default().fg(theme.muted)));
    if app.graph_view.layout_pending {
        status.push(Span::styled(" ◌ refining global layout  ", Style::default().fg(theme.info)));
    }
    if !app.graph_view.filter_query.is_empty() {
        status.push(Span::styled(format!(" /{}  ", app.graph_view.filter_query), Style::default().fg(theme.warning)));
    }
    if app.graph_view.mode == GraphMode::Global {
        status.push(Span::styled(
            if app.graph_view.show_orphans { " orphans on" } else { " orphans off" },
            Style::default().fg(theme.muted),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(status)), Rect::new(inner.x, status_y, inner.width, 1));

    if rows >= 2 {
        let help = Line::from(vec![
            key("Enter", theme),
            hint(" open  ", theme),
            key("v", theme),
            hint(" view  ", theme),
            key("[/]", theme),
            hint(" depth  ", theme),
            key("d", theme),
            hint(" direction  ", theme),
            key("/", theme),
            hint(" filter  ", theme),
            key("Space", theme),
            hint(" focus  ", theme),
            key("?", theme),
            hint(" help  ", theme),
            key("Esc", theme),
            hint(" close", theme),
        ]);
        frame.render_widget(Paragraph::new(help), Rect::new(inner.x, status_y + 1, inner.width, 1));
    }
}

fn key(text: &'static str, theme: &Theme) -> Span<'static> {
    Span::styled(text, Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))
}

fn hint(text: &'static str, theme: &Theme) -> Span<'static> {
    Span::styled(text, Style::default().fg(theme.muted))
}

fn render_graph(buffer: &mut Buffer, app: &App, area: Rect) {
    let visible_nodes = visible_node_count(app, area);
    let level = detail_level(app, area, visible_nodes);
    let labels = label_budget(level, visible_nodes, area);
    let selected = app.graph_view.selected_node;
    let connected = connected_nodes(app, selected);

    render_edges(buffer, app, area, level, selected);
    match level {
        DetailLevel::Overview => render_overview_nodes(buffer, app, area, &connected),
        DetailLevel::Compact => render_compact_nodes(buffer, app, area, &connected),
        DetailLevel::Detail => render_detail_nodes(buffer, app, area, &connected, labels),
    }
}

fn visible_node_count(app: &App, area: Rect) -> usize {
    app.graph_view
        .nodes
        .iter()
        .filter(|node| {
            let (x, y) = screen_position(app, area, node);
            contains(area, x, y)
        })
        .count()
}

fn detail_level(app: &App, area: Rect, visible_nodes: usize) -> DetailLevel {
    let screen_cells = (area.width as usize).saturating_mul(area.height as usize).max(1);
    let density = visible_nodes as f32 / screen_cells as f32;
    if app.graph_view.zoom >= 0.20 && density < 0.35 {
        DetailLevel::Detail
    } else if app.graph_view.zoom >= 0.055 && density < 1.5 {
        DetailLevel::Compact
    } else {
        DetailLevel::Overview
    }
}

/// Labels are a progressive enhancement. Dense graph views deliberately show
/// only nodes: the selected note is always named in the status row, and hiding
/// labels avoids terminal-cell contention and illegible text fragments.
fn label_budget(level: DetailLevel, visible_nodes: usize, area: Rect) -> usize {
    if level != DetailLevel::Detail || visible_nodes == 0 {
        return 0;
    }
    let screen_cells = (area.width as usize).saturating_mul(area.height as usize).max(1);
    let comfortable_node_limit = (screen_cells / 24).max(8);
    if visible_nodes > comfortable_node_limit {
        return 0;
    }
    visible_nodes.min((screen_cells / 64).clamp(1, 48))
}

fn connected_nodes(app: &App, selected: Option<usize>) -> HashSet<usize> {
    let mut connected = HashSet::new();
    if let Some(selected) = selected {
        connected.insert(selected);
        for edge in &app.graph_view.edges {
            let from = edge.from_index();
            let to = edge.to_index();
            if from == selected {
                connected.insert(to);
            }
            if to == selected {
                connected.insert(from);
            }
        }
    }
    connected
}

fn screen_position(app: &App, area: Rect, node: &GraphNode) -> (i32, i32) {
    (
        ((node.x - app.graph_view.viewport_x) * app.graph_view.zoom + area.x as f32).round() as i32,
        ((node.y - app.graph_view.viewport_y) * app.graph_view.zoom + area.y as f32).round() as i32,
    )
}

fn render_edges(buffer: &mut Buffer, app: &App, area: Rect, level: DetailLevel, selected: Option<usize>) {
    let screen_cells = (area.width as usize).saturating_mul(area.height as usize).max(1);
    let (edge_budget, selected_budget) = match level {
        DetailLevel::Overview => ((screen_cells / 10).max(32), (screen_cells / 3).max(64)),
        DetailLevel::Compact => ((screen_cells / 2).max(64), screen_cells.max(96)),
        DetailLevel::Detail => (screen_cells.saturating_mul(2).max(128), screen_cells.saturating_mul(2).max(128)),
    };
    let selected_count = selected
        .map(|idx| {
            app.graph_view
                .edges
                .iter()
                .filter(|edge| edge.from_index() == idx || edge.to_index() == idx)
                .count()
        })
        .unwrap_or(0);
    let normal_count = app.graph_view.edges.len().saturating_sub(selected_count);
    let mut seen_normal = HashSet::with_capacity(edge_budget);
    let mut seen_selected = HashSet::with_capacity(selected_budget);

    // Ordinary links form the base layer; the focused trace is painted last so
    // crossings cannot erase it. Even a 10k-link hub remains budget-bounded.
    for selected_pass in [false, true] {
        let pass_count = if selected_pass { selected_count } else { normal_count };
        let budget = if selected_pass { selected_budget } else { edge_budget };
        let stride = pass_count.div_ceil(budget.max(1)).max(1);
        let mut pass_index = 0usize;
        let mut drawn = 0usize;
        for edge in &app.graph_view.edges {
            let from_index = edge.from_index();
            let to_index = edge.to_index();
            let is_selected = selected.map(|idx| from_index == idx || to_index == idx).unwrap_or(false);
            if is_selected != selected_pass || from_index >= app.graph_view.nodes.len() || to_index >= app.graph_view.nodes.len() {
                continue;
            }
            let sample_index = pass_index;
            pass_index += 1;
            if sample_index % stride != 0 || drawn >= budget {
                continue;
            }
            let from = screen_position(app, area, &app.graph_view.nodes[from_index]);
            let to = screen_position(app, area, &app.graph_view.nodes[to_index]);
            if from == to {
                continue;
            }
            let key = if from <= to { (from, to) } else { (to, from) };
            let first_at_screen_position = if is_selected { seen_selected.insert(key) } else { seen_normal.insert(key) };
            if !first_at_screen_position {
                continue;
            }
            let color = if is_selected { app.theme.primary } else { app.theme.border };
            draw_clipped_line(buffer, from, to, area, color, is_selected);
            drawn += 1;
            if level == DetailLevel::Detail && (is_selected || app.graph_view.nodes.len() < 300) {
                draw_arrow(buffer, from, to, area, color, edge.bidirectional);
            }
        }
    }
}

fn render_overview_nodes(buffer: &mut Buffer, app: &App, area: Rect, connected: &HashSet<usize>) {
    let mut bins: HashMap<(i32, i32), OverviewBin> = HashMap::new();
    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        let position = screen_position(app, area, node);
        if contains(area, position.0, position.1) {
            let selected = app.graph_view.selected_node == Some(idx);
            let root = node.relation == GraphRelation::Root;
            let is_connected = connected.contains(&idx);
            bins.entry(position)
                .and_modify(|bin| {
                    bin.count += 1;
                    bin.selected |= selected;
                    bin.root |= root;
                    bin.connected |= is_connected;
                    if overview_priority(app, idx, connected) > overview_priority(app, bin.representative, connected) {
                        bin.representative = idx;
                    }
                })
                .or_insert(OverviewBin {
                    representative: idx,
                    count: 1,
                    selected,
                    root,
                    connected: is_connected,
                });
        }
    }
    for ((x, y), bin) in bins {
        let glyph = overview_glyph(bin);
        let color = if bin.selected || bin.connected {
            app.theme.primary
        } else if bin.root {
            app.theme.warning
        } else {
            relation_color(&app.graph_view.nodes[bin.representative], &app.theme, false)
        };
        put(buffer, x, y, glyph, color, area);
    }
}

fn overview_glyph(bin: OverviewBin) -> char {
    if bin.selected {
        '◆'
    } else if bin.root {
        '◇'
    } else if bin.count >= 4 {
        '●'
    } else {
        '•'
    }
}

fn overview_priority(app: &App, idx: usize, connected: &HashSet<usize>) -> usize {
    let node = &app.graph_view.nodes[idx];
    usize::from(app.graph_view.selected_node == Some(idx))
        .saturating_mul(usize::MAX / 2)
        .saturating_add(usize::from(node.relation == GraphRelation::Root).saturating_mul(usize::MAX / 4))
        .saturating_add(usize::from(connected.contains(&idx)).saturating_mul(usize::MAX / 8))
        .saturating_add(node.degree())
}

fn render_compact_nodes(buffer: &mut Buffer, app: &App, area: Rect, connected: &HashSet<usize>) {
    let mut bins: HashMap<(i32, i32), OverviewBin> = HashMap::new();
    for (idx, node) in app.graph_view.nodes.iter().enumerate() {
        let position = screen_position(app, area, node);
        if contains(area, position.0, position.1) {
            let selected = app.graph_view.selected_node == Some(idx);
            let root = node.relation == GraphRelation::Root;
            let is_connected = connected.contains(&idx);
            bins.entry(position)
                .and_modify(|bin| {
                    bin.count += 1;
                    bin.selected |= selected;
                    bin.root |= root;
                    bin.connected |= is_connected;
                    if overview_priority(app, idx, connected) > overview_priority(app, bin.representative, connected) {
                        bin.representative = idx;
                    }
                })
                .or_insert(OverviewBin {
                    representative: idx,
                    count: 1,
                    selected,
                    root,
                    connected: is_connected,
                });
        }
    }

    for ((x, y), bin) in bins {
        let node = &app.graph_view.nodes[bin.representative];
        let dimmed = app.graph_view.selected_node.is_some() && !bin.connected;
        let color = if bin.connected {
            app.theme.primary
        } else {
            relation_color(node, &app.theme, dimmed)
        };
        let glyph = if bin.selected {
            '◆'
        } else if bin.root {
            '◇'
        } else if bin.count >= 3 || node.degree() >= 8 {
            '●'
        } else {
            '•'
        };
        put(buffer, x, y, glyph, color, area);
    }
}

fn render_detail_nodes(buffer: &mut Buffer, app: &App, area: Rect, connected: &HashSet<usize>, label_budget: usize) {
    let mut occupied = HashSet::new();
    let mut visible = Vec::new();
    // Two linear passes keep the focused node on top without sorting the full
    // vault every frame.
    for selected_pass in [false, true] {
        for (idx, node) in app.graph_view.nodes.iter().enumerate() {
            let selected = app.graph_view.selected_node == Some(idx);
            if selected != selected_pass {
                continue;
            }
            let (x, y) = screen_position(app, area, node);
            if x < area.x as i32 - 3 || x >= area.right() as i32 || y < area.y as i32 - 2 || y >= area.bottom() as i32 {
                continue;
            }
            let dimmed = app.graph_view.selected_node.is_some() && !connected.contains(&idx);
            let color = if connected.contains(&idx) {
                app.theme.primary
            } else {
                relation_color(node, &app.theme, dimmed)
            };
            draw_box_node(buffer, x, y, color, selected, node.relation == GraphRelation::Root, area);
            for px in x..x + NODE_WIDTH as i32 {
                for py in y..y + NODE_HEIGHT as i32 {
                    if contains(area, px, py) {
                        occupied.insert((px as u16, py as u16));
                    }
                }
            }
            visible.push(idx);
        }
    }

    if label_budget == 0 {
        return;
    }

    // Place text only after every node has claimed its cells. This prevents a
    // later node from slicing a title into the visual glitches seen in dense
    // views. Focus, roots, connected notes, and hubs receive first priority.
    visible.sort_by_key(|idx| std::cmp::Reverse(overview_priority(app, *idx, connected)));
    let mut placed = 0usize;
    for idx in visible {
        if placed >= label_budget {
            break;
        }
        let node = &app.graph_view.nodes[idx];
        let dimmed = app.graph_view.selected_node.is_some() && !connected.contains(&idx);
        if dimmed {
            continue;
        }
        let (x, y) = screen_position(app, area, node);
        let color = if connected.contains(&idx) {
            app.theme.primary
        } else {
            relation_color(node, &app.theme, false)
        };
        let title = graph_node_metadata(app, node).map_or("", |metadata| metadata.title.as_str());
        if place_label(buffer, title, x + 1, y + 1, color, area, &mut occupied) {
            placed += 1;
        }
    }
}

fn relation_color(node: &GraphNode, theme: &Theme, dimmed: bool) -> Color {
    if dimmed {
        return theme.muted;
    }
    match node.relation {
        GraphRelation::Root => theme.warning,
        GraphRelation::Incoming => theme.secondary,
        GraphRelation::Outgoing => theme.info,
        GraphRelation::Bidirectional => theme.success,
        GraphRelation::Neutral => theme.foreground,
    }
}

fn draw_box_node(buffer: &mut Buffer, x: i32, y: i32, color: Color, selected: bool, root: bool, area: Rect) {
    let middle = if selected {
        '●'
    } else if root {
        '◆'
    } else {
        '─'
    };
    for (dy, chars) in [(0, ['╭', middle, '╮']), (1, ['╰', '─', '╯'])] {
        for (dx, ch) in chars.into_iter().enumerate() {
            put(buffer, x + dx as i32, y + dy, ch, color, area);
        }
    }
}

fn place_label(buffer: &mut Buffer, title: &str, anchor_x: i32, anchor_y: i32, color: Color, area: Rect, occupied: &mut HashSet<(u16, u16)>) -> bool {
    const MAX_WIDTH: usize = 28;
    let full_width = title.width();
    let truncated = full_width > MAX_WIDTH;
    let content_limit = if truncated { MAX_WIDTH - 3 } else { MAX_WIDTH };
    let content_width = title
        .chars()
        .map(|ch| ch.width().unwrap_or(1))
        .scan(0usize, |used, width| {
            if *used + width > content_limit {
                None
            } else {
                *used += width;
                Some(width)
            }
        })
        .sum::<usize>();
    let width = (content_width + usize::from(truncated) * 3) as i32;
    let candidates = [
        (anchor_x - width / 2, anchor_y + 2),
        (anchor_x - width / 2, anchor_y - 2),
        (anchor_x + 2, anchor_y),
        (anchor_x - width - 2, anchor_y),
    ];
    let Some((x, y)) = candidates.into_iter().find(|(x, y)| {
        *y >= area.y as i32
            && *y < area.bottom() as i32
            && *x >= area.x as i32
            && *x + width <= area.right() as i32
            && (0..width).all(|offset| !occupied.contains(&((*x + offset) as u16, *y as u16)))
    }) else {
        return false;
    };

    let mut offset = 0i32;
    for ch in title.chars() {
        let char_width = ch.width().unwrap_or(1) as i32;
        if offset + char_width > content_limit as i32 {
            break;
        }
        put(buffer, x + offset, y, ch, color, area);
        for cell in 0..char_width {
            occupied.insert(((x + offset + cell) as u16, y as u16));
        }
        offset += char_width;
    }
    if truncated {
        for _ in 0..3 {
            put(buffer, x + offset, y, '.', color, area);
            occupied.insert(((x + offset) as u16, y as u16));
            offset += 1;
        }
    }
    true
}

fn graph_node_metadata<'a>(app: &'a App, node: &GraphNode) -> Option<&'a GraphIndexNode> {
    app.graph_index.as_ref()?.metadata_for_note(node.note_id)
}

fn draw_arrow(buffer: &mut Buffer, from: (i32, i32), to: (i32, i32), area: Rect, color: Color, bidirectional: bool) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let arrow = if bidirectional {
        '◆'
    } else if dx.abs() >= dy.abs() {
        if dx >= 0 {
            '›'
        } else {
            '‹'
        }
    } else if dy >= 0 {
        '⌄'
    } else {
        '⌃'
    };
    let position = if dx.abs() >= dy.abs() {
        (to.0 - dx.signum() * 2, to.1)
    } else {
        (to.0, to.1 - dy.signum() * 2)
    };
    put(buffer, position.0, position.1, arrow, color, area);
}

fn draw_clipped_line(buffer: &mut Buffer, from: (i32, i32), to: (i32, i32), area: Rect, color: Color, overwrite: bool) {
    let Some(((mut x0, mut y0), (x1, y1))) = clip_line(from, to, area) else {
        return;
    };
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx - dy;
    loop {
        if let Some(cell) = buffer.cell_mut((x0 as u16, y0 as u16)) {
            if overwrite || cell.symbol() == " " || cell.symbol() == "·" {
                cell.set_char('·');
                cell.set_fg(color);
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let twice = 2 * error;
        if twice > -dy {
            error -= dy;
            x0 += sx;
        }
        if twice < dx {
            error += dx;
            y0 += sy;
        }
    }
}

/// Liang-Barsky clipping bounds work to the visible terminal rectangle before
/// Bresenham traversal, so a far-away edge cannot walk thousands of cells.
fn clip_line(from: (i32, i32), to: (i32, i32), area: Rect) -> Option<((i32, i32), (i32, i32))> {
    let (x0, y0, x1, y1) = (from.0 as f32, from.1 as f32, to.0 as f32, to.1 as f32);
    let dx = x1 - x0;
    let dy = y1 - y0;
    let bounds = [
        (-dx, x0 - area.x as f32),
        (dx, area.right().saturating_sub(1) as f32 - x0),
        (-dy, y0 - area.y as f32),
        (dy, area.bottom().saturating_sub(1) as f32 - y0),
    ];
    let (mut enter, mut exit) = (0.0f32, 1.0f32);
    for (p, q) in bounds {
        if p.abs() < f32::EPSILON {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            enter = enter.max(ratio);
        } else {
            exit = exit.min(ratio);
        }
        if enter > exit {
            return None;
        }
    }
    Some((
        ((x0 + enter * dx).round() as i32, (y0 + enter * dy).round() as i32),
        ((x0 + exit * dx).round() as i32, (y0 + exit * dy).round() as i32),
    ))
}

fn put(buffer: &mut Buffer, x: i32, y: i32, ch: char, color: Color, area: Rect) {
    if contains(area, x, y) {
        if let Some(cell) = buffer.cell_mut((x as u16, y as u16)) {
            cell.set_char(ch);
            cell.set_fg(color);
        }
    }
}

fn contains(area: Rect, x: i32, y: i32) -> bool {
    x >= area.x as i32 && x < area.right() as i32 && y >= area.y as i32 && y < area.bottom() as i32
}

fn graph_bounds(nodes: &[GraphNode]) -> (f32, f32, f32, f32) {
    if nodes.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    nodes.iter().fold(
        (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        |(min_x, min_y, max_x, max_y), node| (min_x.min(node.x), min_y.min(node.y), max_x.max(node.x + 3.0), max_y.max(node.y + 3.0)),
    )
}

fn fit_graph_to_area(app: &mut App, area: Rect) {
    let (min_x, min_y, max_x, max_y) = graph_bounds(&app.graph_view.nodes);
    let graph_width = (max_x - min_x).max(3.0);
    let graph_height = (max_y - min_y).max(2.0);
    app.graph_view.zoom = fit_zoom_for_bounds(graph_width, graph_height, area.width as f32, area.height as f32);
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;
    app.graph_view.viewport_x = center_x - area.width as f32 / app.graph_view.zoom / 2.0;
    app.graph_view.viewport_y = center_y - area.height as f32 / app.graph_view.zoom / 2.0;
}

fn center_selected(app: &mut App, area: Rect) {
    let Some(node) = app.graph_view.selected_node.and_then(|idx| app.graph_view.nodes.get(idx)) else {
        return;
    };
    app.graph_view.viewport_x = node.x - area.width as f32 / app.graph_view.zoom / 2.0;
    app.graph_view.viewport_y = node.y - area.height as f32 / app.graph_view.zoom / 2.0;
}

fn render_centered_message(frame: &mut Frame, area: Rect, message: &str, color: Color) {
    let y = area.y + area.height / 2;
    frame.render_widget(
        Paragraph::new(message).alignment(Alignment::Center).style(Style::default().fg(color)),
        Rect::new(area.x, y, area.width, 1),
    );
}

fn render_help_overlay(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(2).min(72);
    let height = area.height.saturating_sub(2).min(22);
    if width < 4 || height < 4 {
        return;
    }
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let text = "EXPLORE\n\n  hjkl / arrows   select spatial neighbor\n  HJKL             pan camera\n  + / -             bounded zoom\n  f / 0             fit all, center\n  Enter             open selected note\n  Space             make selected note the Local root\n\nSHAPE THE VIEW\n\n  v / Tab           Local ↔ Global\n  [ / ]             Local depth (1–5)\n  d                 all → incoming → outgoing\n  /                 title, path, #tag filter\n  n / N             next / previous match\n  o                 toggle Global orphans\n  r                 reset graph controls\n\n  ?                 close this help";
    let block = Block::default()
        .title(" Graph controls ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.primary))
        .style(Style::default().bg(app.theme.dialog.background));
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(app.theme.dialog.text)),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_far_edges_to_viewport() {
        let area = Rect::new(10, 5, 20, 10);
        assert_eq!(clip_line((-10_000, 8), (10_000, 8), area), Some(((10, 8), (29, 8))));
        assert_eq!(clip_line((-10, -10), (-1, -1), area), None);
    }

    #[test]
    fn bounds_are_finite_for_single_node() {
        let mut node = GraphNode {
            note_id: ekphos_core::NoteId::new(0),
            x: 2.0,
            y: 3.0,
            home_x: 2.0,
            home_y: 3.0,
            depth: 0,
            relation: GraphRelation::Root,
            in_degree: 0,
            out_degree: 0,
        };
        assert_eq!(graph_bounds(std::slice::from_mut(&mut node)), (2.0, 3.0, 5.0, 6.0));
    }

    #[test]
    fn overview_nodes_remain_round_and_distinct_from_edges() {
        let bin = OverviewBin {
            representative: 0,
            count: 1,
            selected: false,
            root: false,
            connected: false,
        };
        assert_eq!(overview_glyph(bin), '•');
        assert_ne!(overview_glyph(bin), '·');
        assert_eq!(overview_glyph(OverviewBin { count: 8, ..bin }), '●');
    }

    #[test]
    fn dense_views_suppress_labels() {
        let area = Rect::new(0, 0, 120, 40);
        assert_eq!(label_budget(DetailLevel::Overview, 1, area), 0);
        assert_eq!(label_budget(DetailLevel::Compact, 1, area), 0);
        assert_eq!(label_budget(DetailLevel::Detail, 201, area), 0);
        assert_eq!(label_budget(DetailLevel::Detail, 20, area), 20);
    }

    #[test]
    fn looked_up_unicode_labels_are_truncated_without_projection_strings() {
        let area = Rect::new(0, 0, 40, 4);
        let mut buffer = Buffer::empty(area);
        let mut occupied = HashSet::new();
        assert!(place_label(
            &mut buffer,
            "日本語のとても長いノートタイトルです追加",
            1,
            1,
            Color::White,
            area,
            &mut occupied,
        ));
        assert_eq!(occupied.len(), 27);
        assert_eq!(buffer.cell((27, 1)).map(|cell| cell.symbol()), Some("."));
        assert_eq!(buffer.cell((28, 1)).map(|cell| cell.symbol()), Some("."));
        assert_eq!(buffer.cell((29, 1)).map(|cell| cell.symbol()), Some("."));
    }

    #[test]
    fn focused_trace_wins_at_edge_crossings() {
        let area = Rect::new(0, 0, 9, 5);
        let mut buffer = Buffer::empty(area);
        draw_clipped_line(&mut buffer, (0, 2), (8, 2), area, Color::DarkGray, false);
        draw_clipped_line(&mut buffer, (4, 0), (4, 4), area, Color::Yellow, true);

        let crossing = buffer.cell((4, 2)).expect("crossing cell");
        assert_eq!(crossing.symbol(), "·");
        assert_eq!(crossing.fg, Color::Yellow);
    }
}
