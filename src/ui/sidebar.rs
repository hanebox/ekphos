use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Focus, Mode};

pub fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;

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
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.peach))
            .title(" Search ");

        let search_text = Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(theme.overlay0)),
            Span::styled(&app.search_query, Style::default().fg(theme.text)),
            Span::styled("_", Style::default().fg(theme.peach)),
        ]))
        .block(search_block);

        f.render_widget(search_text, search_area);
    }

    // Get visible notes (filtered or all)
    let visible_notes = app.get_visible_notes();

    let items: Vec<ListItem> = visible_notes
        .iter()
        .map(|(original_idx, note)| {
            let style = if *original_idx == app.selected_note {
                Style::default()
                    .fg(theme.yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            ListItem::new(Line::from(Span::styled(&note.title, style)))
        })
        .collect();

    let border_style = if app.focus == Focus::Sidebar && app.mode == Mode::Normal {
        Style::default().fg(theme.lavender)
    } else {
        Style::default().fg(theme.surface1)
    };

    let title = if app.search_active && !app.search_query.is_empty() {
        format!(" Notes ({}) ", visible_notes.len())
    } else {
        " Notes ".to_string()
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
                .bg(theme.surface0)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    // Update list state selection based on visible notes
    let mut list_state = ListState::default();
    if let Some(pos) = visible_notes.iter().position(|(i, _)| *i == app.selected_note) {
        list_state.select(Some(pos));
    }

    f.render_stateful_widget(sidebar, list_area, &mut list_state);
}
