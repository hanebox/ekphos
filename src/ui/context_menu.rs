use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, ContextMenuItem, ContextMenuState};

const MENU_WIDTH: u16 = 14;

pub fn render_context_menu(f: &mut Frame, app: &App) {
    if let ContextMenuState::Open { x, y, selected_index } = app.context_menu_state {
        let items = ContextMenuItem::all();
        let menu_height = items.len() as u16 + 2; // +2 for borders

        // Adjust position to keep menu on screen
        let frame_area = f.area();
        let menu_x = if x + MENU_WIDTH > frame_area.width {
            frame_area.width.saturating_sub(MENU_WIDTH)
        } else {
            x
        };
        let menu_y = if y + menu_height > frame_area.height {
            frame_area.height.saturating_sub(menu_height)
        } else {
            y
        };

        let menu_area = Rect::new(menu_x, menu_y, MENU_WIDTH, menu_height);

        // Clear the area behind the menu
        f.render_widget(Clear, menu_area);

        // Build menu content
        let lines: Vec<Line> = items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let label = item.label();
                let style = if idx == selected_index {
                    Style::default()
                        .fg(app.theme.background)
                        .bg(app.theme.blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.theme.foreground)
                };
                Line::from(Span::styled(format!(" {} ", label), style))
            })
            .collect();

        let menu = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.theme.bright_black))
                .style(Style::default().bg(app.theme.black)),
        );

        f.render_widget(menu, menu_area);
    }
}
