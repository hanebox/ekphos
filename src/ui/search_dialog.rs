use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;

const DIALOG_WIDTH: u16 = 35;
const DIALOG_HEIGHT: u16 = 3;

pub fn render_search_dialog(f: &mut Frame, app: &App, content_area: Rect) {
    if !app.buffer_search.active {
        return;
    }

    let theme = &app.theme;

    let dialog_x = content_area
        .x
        .saturating_add(content_area.width)
        .saturating_sub(DIALOG_WIDTH + 1);
    let dialog_y = content_area.y + 1;

    let dialog_width = DIALOG_WIDTH.min(content_area.width.saturating_sub(2));
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, DIALOG_HEIGHT);

    f.render_widget(Clear, dialog_area);
    let query = &app.buffer_search.query;
    let cursor = "_";

    let match_count = app.buffer_search.matches.len();
    let current_idx = if match_count > 0 {
        app.buffer_search.current_match_index + 1
    } else {
        0
    };

    let count_text = if match_count > 0 {
        format!("{}/{}", current_idx, match_count)
    } else if !query.is_empty() {
        "0".to_string()
    } else {
        String::new()
    };

    let available_width = (dialog_width as usize).saturating_sub(4 + count_text.len() + 2);

    let display_query = if query.len() > available_width {
        let start = query.len().saturating_sub(available_width);
        format!("...{}", &query[start..])
    } else {
        query.clone()
    };

    let input_line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(&display_query, Style::default().fg(theme.search.input)),
        Span::styled(cursor, Style::default().fg(theme.primary).add_modifier(Modifier::SLOW_BLINK)),
        Span::styled(" ", Style::default()),
    ]);

    let hint_text = if count_text.is_empty() {
        " ↑↓/Tab: nav, Esc: close ".to_string()
    } else {
        format!(" {} ↑↓ ", count_text)
    };

    let border_color = if match_count > 0 {
        theme.success
    } else if !query.is_empty() {
        theme.error
    } else {
        theme.search.border
    };

    let dialog = Paragraph::new(vec![input_line]).block(
        Block::default()
            .title(" Find ")
            .title_bottom(Line::from(Span::styled(
                &hint_text,
                Style::default().fg(theme.search.match_count),
            )).right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(theme.search.background)),
    );

    f.render_widget(dialog, dialog_area);
}
