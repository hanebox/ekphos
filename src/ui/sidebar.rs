use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Focus, Mode, SidebarItemKind};

pub fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;
    if app.sidebar_collapsed {
        render_collapsed_sidebar(f, app, area);
        return;
    }
    // Split area for search input when search is active
    let (search_area, list_area) = if app.search_active {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    // Render search input if active
    if let Some(search_area) = search_area {
        let has_query = !app.search_query.is_empty();
        let has_results = !app.search_matched_notes.is_empty();
        let border_color = if has_query && !has_results {
            theme.red 
        } else if has_query && has_results {
            theme.green 
        } else {
            theme.yellow 
        };

        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" Search ");

        let search_text = Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(theme.white)),
            Span::styled(&app.search_query, Style::default().fg(theme.foreground)),
            Span::styled("_", Style::default().fg(border_color)),
        ]))
        .block(search_block);

        f.render_widget(search_text, search_area);
    }

    let is_searching = app.search_active && !app.search_query.is_empty();

    let items: Vec<ListItem> = app.sidebar_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = idx == app.selected_sidebar_index;
            let indent = "  ".repeat(item.depth);

            let (icon, style) = match &item.kind {
                SidebarItemKind::Folder { expanded, .. } => {
                    let icon = if *expanded { "▼ " } else { "▶ " };
                    let style = if is_selected {
                        Style::default()
                            .fg(theme.cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.cyan)
                    };
                    (icon, style)
                }
                SidebarItemKind::Note { note_index } => {
                    let icon = "  ";
                    let is_match = is_searching && app.search_matched_notes.contains(note_index);
                    let style = if is_selected {
                        Style::default()
                            .fg(theme.yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if is_match {
                        Style::default()
                            .fg(theme.green)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.foreground)
                    };
                    (icon, style)
                }
            };

            let display = format!("{}{}{}", indent, icon, item.display_name);
            ListItem::new(Line::from(Span::styled(display, style)))
        })
        .collect();

    let border_style = if app.focus == Focus::Sidebar && app.mode == Mode::Normal {
        Style::default().fg(theme.bright_blue)
    } else {
        Style::default().fg(theme.bright_black)
    };

    let title = if is_searching {
        let match_count = app.search_matched_notes.len();
        let total_count = app.notes.len();
        format!(" Found {}/{} ", match_count, total_count)
    } else {
        let note_count = app.sidebar_items
            .iter()
            .filter(|item| matches!(item.kind, SidebarItemKind::Note { .. }))
            .count();
        format!(" Notes ({}) ", note_count)
    };

    let sidebar = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(theme.bright_black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");  

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected_sidebar_index));

    f.render_stateful_widget(sidebar, list_area, &mut list_state);
}

fn render_collapsed_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;

    let border_style = if app.focus == Focus::Sidebar && app.mode == Mode::Normal {
        Style::default().fg(theme.bright_blue)
    } else {
        Style::default().fg(theme.bright_black)
    };

    let note_count = app.sidebar_items
        .iter()
        .filter(|item| matches!(item.kind, SidebarItemKind::Note { .. }))
        .count();

    let mut lines: Vec<Line> = Vec::new();

    let available_height = area.height.saturating_sub(2) as usize; // subtract borders
    let padding_top = available_height / 2;

    for _ in 0..padding_top {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        " ≡",
        Style::default().fg(theme.cyan),
    )));
    lines.push(Line::from(Span::styled(
        format!(" {}", note_count),
        Style::default().fg(theme.foreground),
    )));

    let collapsed = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        );

    f.render_widget(collapsed, area);
}
