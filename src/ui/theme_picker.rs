use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::ThemePicker;
use crate::config::Theme;

const MAX_VISIBLE: usize = 12;
const MIN_WIDTH: u16 = 40;
const MAX_WIDTH: u16 = 60;
const TAG_WIDTH: usize = 8; // width of the widest tag ("official")

/// Centered theme selector modal. The highlighted theme is already applied to
/// the live UI (see `App::preview_selected_theme`), so this popup is just the
/// list of names; the rest of the screen *is* the preview.
pub struct ThemePickerView<'a> {
    pub theme: &'a Theme,
    pub picker: &'a ThemePicker,
}

pub fn render_theme_picker(f: &mut Frame, view: ThemePickerView<'_>) -> Option<usize> {
    let picker = view.picker;
    let len = picker.themes.len();
    if len == 0 {
        return None;
    }
    let selected = picker.selected.min(len - 1);
    let visible = len.min(MAX_VISIBLE);
    let mut scroll = picker.scroll_offset;
    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll + visible {
        scroll = selected + 1 - visible;
    }
    let theme = view.theme;
    let area = f.area();
    let longest = picker.themes.iter().map(|t| t.name.chars().count()).max().unwrap_or(0) as u16;
    let content_width = longest + 2 + 1 + TAG_WIDTH as u16 + 2;
    let popup_width = content_width.clamp(MIN_WIDTH, MAX_WIDTH).min(area.width.saturating_sub(4));
    let popup_height = (visible as u16 + 4).min(area.height.saturating_sub(4));
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);
    f.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(Line::from(Span::styled(" Themes ", Style::default().fg(theme.dialog.title).add_modifier(Modifier::BOLD))))
        .title_bottom(Line::from(Span::styled(" ↑↓ preview · ⏎ apply · esc cancel ", Style::default().fg(theme.muted))).right_aligned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.dialog.border))
        .style(Style::default().bg(theme.dialog.background));
    f.render_widget(block, popup_area);
    let inner_w = popup_width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::with_capacity(visible + 2);
    lines.push(Line::from("")); // top padding
    for (row, entry) in picker.themes.iter().enumerate().skip(scroll).take(visible) {
        let is_sel = row == selected;
        let marker = if is_sel { "▶ " } else { "  " };
        let tag = if entry.bundled { "official" } else { "custom" };
        let used = marker.chars().count() + entry.name.chars().count() + tag.chars().count();
        let gap = inner_w.saturating_sub(used).max(1);
        let name_style = if is_sel { Style::default().fg(theme.dialog.title).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.dialog.text) };
        let tag_style = Style::default().fg(if entry.bundled { theme.info } else { theme.muted });
        let line_style = if is_sel { Style::default().bg(theme.selection) } else { Style::default() };
        let line = Line::from(vec![Span::styled(marker, Style::default().fg(theme.dialog.title)), Span::styled(entry.name.clone(), name_style), Span::raw(" ".repeat(gap)), Span::styled(tag, tag_style)]).style(line_style);
        lines.push(line);
    }
    lines.push(Line::from("")); // bottom padding
    let inner = Rect::new(popup_area.x + 1, popup_area.y + 1, popup_area.width.saturating_sub(2), popup_area.height.saturating_sub(2));
    f.render_widget(Paragraph::new(lines), inner);
    Some(scroll)
}
