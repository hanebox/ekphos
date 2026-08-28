//! Floating toast notification overlay.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, ToastKind};

/// Maximum width of a toast box, including borders.
const MAX_WIDTH: u16 = 48;

/// Render the active toast as a bordered box anchored to the bottom-right,
/// just above the status bar. Called last in the draw pass so it floats on top
/// of everything else. No-op when there is no toast.
pub fn render_toast(f: &mut Frame, app: &App) {
    let Some(toast) = &app.state.toast else { return };
    let theme = &app.state.theme;
    let area = f.area();
    if area.width < 16 || area.height < 5 {
        return;
    }
    let (accent, label) = match toast.kind {
        ToastKind::Error => (theme.error, " Error "),
        ToastKind::Info => (theme.info, " Info "),
        ToastKind::Success => (theme.success, " Done "),
    };
    let max_width = MAX_WIDTH.min(area.width.saturating_sub(4));
    let inner_width = max_width.saturating_sub(2).max(1) as usize;
    let wrapped = wrap_to_width(&toast.message, inner_width);
    let content_width = wrapped.iter().map(|l| UnicodeWidthStr::width(l.as_str())).max().unwrap_or(0).max(label.chars().count());
    let box_width = (content_width as u16 + 2).clamp(12, max_width);
    let box_height = (wrapped.len() as u16 + 2).clamp(3, area.height.saturating_sub(2));
    let toast_area = Rect { x: area.x + area.width.saturating_sub(box_width + 1), y: area.y + area.height.saturating_sub(box_height + 1), width: box_width, height: box_height };
    f.render_widget(Clear, toast_area);
    let lines: Vec<Line> = wrapped.into_iter().map(|l| Line::from(Span::styled(l, Style::default().fg(theme.foreground)))).collect();
    let widget = Paragraph::new(lines).block(Block::default().title(Span::styled(label, Style::default().fg(accent).add_modifier(Modifier::BOLD))).borders(Borders::ALL).border_style(Style::default().fg(accent)).style(Style::default().bg(theme.background_secondary))).alignment(Alignment::Left);
    f.render_widget(widget, toast_area);
}

/// Greedy word-wrap to `width` display columns, honoring explicit newlines.
/// Words longer than `width` are left intact (rare for short error strings).
fn wrap_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        let mut current_width = 0;
        for word in paragraph.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if current.is_empty() {
                current.push_str(word);
                current_width = word_width;
            } else if current_width + 1 + word_width <= width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
                current_width = word_width;
            }
        }
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::wrap_to_width;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn wraps_long_text_within_width() {
        let lines = wrap_to_width("the quick brown fox jumps", 9);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(UnicodeWidthStr::width(line.as_str()) <= 9, "line too wide: {line:?}");
        }
    }

    #[test]
    fn preserves_explicit_newlines() {
        let lines = wrap_to_width("a\nb\nc", 80);
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn short_message_stays_on_one_line() {
        assert_eq!(wrap_to_width("Clipboard: failed", 40), vec!["Clipboard: failed"]);
    }

    #[test]
    fn zero_width_does_not_panic() {
        assert_eq!(wrap_to_width("anything", 0), vec!["anything"]);
    }
}
