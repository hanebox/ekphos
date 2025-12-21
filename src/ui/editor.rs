use ratatui::{
    layout::Rect,
    style::Style,
    widgets::Paragraph,
    Frame,
};

use crate::app::App;

pub fn render_editor(f: &mut Frame, app: &mut App, area: Rect) {
    // Store editor area for mouse coordinate translation
    app.editor_area = area;

    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;

    // Update editor view dimensions and scroll
    app.editor.set_view_size(inner_width, inner_height);
    app.update_editor_scroll(inner_height);

    f.render_widget(&app.editor, area);

    // Only show overflow indicators when line wrap is disabled
    if !app.editor.line_wrap_enabled() {
        let theme = &app.theme;
        let (cursor_row, _cursor_col) = app.editor.cursor();
        let scroll_top = app.editor_scroll_top;

        // Get overflow info from editor's horizontal scroll tracking
        let (has_left_overflow, has_right_overflow) = app.editor.get_overflow_info();

        // Render overflow indicators on the cursor line
        if cursor_row >= scroll_top && cursor_row < scroll_top + inner_height {
            let y = area.y + 1 + (cursor_row - scroll_top) as u16;

            if has_left_overflow {
                let indicator = Paragraph::new("«│")
                    .style(Style::default().fg(theme.yellow));
                f.render_widget(indicator, Rect::new(area.x + 1, y, 2, 1));
            }

            if has_right_overflow {
                let indicator = Paragraph::new("│»")
                    .style(Style::default().fg(theme.yellow));
                f.render_widget(indicator, Rect::new(area.x + area.width - 3, y, 2, 1));
            }
        }
    }
}
