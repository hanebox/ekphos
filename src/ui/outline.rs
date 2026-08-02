use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use crate::app::{App, Focus, Mode};

fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

pub fn render_outline(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;
    let outline_theme = &theme.outline;
    if app.is_outline_minimized() {
        render_collapsed_outline(f, app, area);
        return;
    }

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
                1 => Style::default().fg(outline_theme.heading1).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(outline_theme.heading2),
                3 => Style::default().fg(outline_theme.heading3),
                _ => Style::default().fg(outline_theme.heading4),
            };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}{}", indent, prefix, expand_tabs(&item.title)),
                style,
            )))
        })
        .collect();

    let border_style = if app.focus == Focus::Outline && app.mode == Mode::Normal {
        Style::default().fg(theme.primary)
    } else {
        Style::default().fg(theme.border)
    };

    let mut outline = List::new(items)
        .block(
            Block::default()
                .title(" Outline ")
                .borders(Borders::ALL)
                .border_style(border_style),
        );

    if app.mode != Mode::Edit {
        outline = outline
            .highlight_style(
                Style::default()
                    .bg(theme.selection)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");
    }

    app.outline_area = area;

    f.render_stateful_widget(outline, area, &mut app.outline_state);
}

fn render_collapsed_outline(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;
    let outline_theme = &theme.outline;
    let in_edit_mode = app.mode == Mode::Edit;

    let items: Vec<ListItem> = app
        .outline
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            // Hide selection indicator in edit mode
            let is_selected = !in_edit_mode && app.outline_state.selected() == Some(idx);

            let symbol = match item.level {
                1 => "◆",  // H1
                2 => "■",  // H2
                3 => "▸",  // H3
                _ => "›",  // H4+
            };

            let style = match item.level {
                1 => Style::default().fg(outline_theme.heading1).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(outline_theme.heading2),
                3 => Style::default().fg(outline_theme.heading3),
                _ => Style::default().fg(outline_theme.heading4),
            };

            let display = if is_selected {
                format!("▶{}", symbol)
            } else {
                format!(" {}", symbol)
            };

            ListItem::new(Line::from(Span::styled(display, style)))
        })
        .collect();

    let border_style = if app.focus == Focus::Outline && app.mode == Mode::Normal {
        Style::default().fg(theme.primary)
    } else {
        Style::default().fg(theme.border)
    };

    let mut outline = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        );

    if !in_edit_mode {
        outline = outline.highlight_style(
            Style::default()
                .bg(theme.selection)
                .add_modifier(Modifier::BOLD),
        );
    }
    app.outline_area = area;

    f.render_stateful_widget(outline, area, &mut app.outline_state);
}
