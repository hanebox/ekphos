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
    if let ContextMenuState::Open { x, y, selected_index } = app.editor.context_menu_state {
        let items = ContextMenuItem::all();
        let menu_height = items.len() as u16 + 2; // +2 for borders
        let frame_area = f.area();
        let menu_x = if x + MENU_WIDTH > frame_area.width { frame_area.width.saturating_sub(MENU_WIDTH) } else { x };
        let menu_y = if y + menu_height > frame_area.height { frame_area.height.saturating_sub(menu_height) } else { y };
        let menu_area = Rect::new(menu_x, menu_y, MENU_WIDTH, menu_height);
        f.render_widget(Clear, menu_area);
        let lines: Vec<Line> = items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let label = item.label();
                let style = if idx == selected_index { Style::default().fg(app.state.theme.background).bg(app.state.theme.primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(app.state.theme.foreground) };
                Line::from(Span::styled(format!(" {} ", label), style))
            })
            .collect();
        let menu = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(app.state.theme.border)).style(Style::default().bg(app.state.theme.background_secondary)));
        f.render_widget(menu, menu_area);
    }
}
