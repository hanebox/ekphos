use ratatui::{
    layout::Rect,
    style::Style,
    widgets::Paragraph,
    Frame,
};

use crate::app::App;

pub fn render_editor(f: &mut Frame, app: &mut App, area: Rect) {
    f.render_widget(&app.textarea, area);

    let theme = &app.theme;
    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    let (cursor_row, cursor_col) = app.textarea.cursor();
    let lines = app.textarea.lines();
    let scroll_top = cursor_row.saturating_sub(inner_height.saturating_sub(1));

    for (i, line) in lines.iter().enumerate().skip(scroll_top).take(inner_height) {
        let line_len = line.chars().count();
        let y = area.y + 1 + (i - scroll_top) as u16;

        if y >= area.y + area.height - 1 {
            continue;
        }

        let is_cursor_line = i == cursor_row;
        let h_scroll = if is_cursor_line && cursor_col > inner_width.saturating_sub(1) {
            cursor_col.saturating_sub(inner_width.saturating_sub(1))
        } else {
            0
        };

        let has_left_overflow = h_scroll > 0;
        let has_right_overflow = line_len > h_scroll + inner_width;

        if has_left_overflow {
            let indicator = Paragraph::new("«│")
                .style(Style::default().fg(theme.peach));
            f.render_widget(indicator, Rect::new(area.x + 1, y, 2, 1));
        }

        if has_right_overflow {
            let indicator = Paragraph::new("│»")
                .style(Style::default().fg(theme.peach));
            f.render_widget(indicator, Rect::new(area.x + area.width - 3, y, 2, 1));
        }
    }
}
