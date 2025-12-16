use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{App, Focus, Mode};

pub fn render_outline(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;
    let items: Vec<ListItem> = app
        .outline
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.level.saturating_sub(1));
            let prefix = match item.level {
                1 => "# ",
                2 => "## ",
                3 => "### ",
                _ => "",
            };
            let style = match item.level {
                1 => Style::default().fg(theme.blue).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(theme.green),
                3 => Style::default().fg(theme.yellow),
                _ => Style::default().fg(theme.text),
            };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}{}", indent, prefix, item.title),
                style,
            )))
        })
        .collect();

    let border_style = if app.focus == Focus::Outline && app.mode == Mode::Normal {
        Style::default().fg(theme.lavender)
    } else {
        Style::default().fg(theme.surface1)
    };

    let outline = List::new(items)
        .block(
            Block::default()
                .title(" Outline ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(theme.surface0)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(outline, area, &mut app.outline_state);
}
