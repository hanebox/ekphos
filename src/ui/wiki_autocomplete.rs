use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, WikiAutocompleteMode, WikiAutocompleteState};

const POPUP_WIDTH: u16 = 45;
const POPUP_MAX_VISIBLE_ITEMS: usize = 5;
const POPUP_MAX_HEIGHT: u16 = (POPUP_MAX_VISIBLE_ITEMS as u16) + 2;

pub fn render_wiki_autocomplete(f: &mut Frame, app: &App) {
    if let WikiAutocompleteState::Open {
        query,
        suggestions,
        selected_index,
        mode,
        target_note,
        ..
    } = &app.wiki_autocomplete
    {
        let theme = &app.theme;
        let area = f.area();

        let (cursor_row, cursor_col) = app.editor.cursor();
        let editor_area = app.editor_area;
        let cursor_screen_y = editor_area.y + 1 + (cursor_row.saturating_sub(app.editor_scroll_top)) as u16;
        let cursor_screen_x = editor_area.x + 1 + cursor_col as u16;

        let is_alias_mode = *mode == WikiAutocompleteMode::Alias;

        let visible_items = if is_alias_mode {
            1 
        } else {
            suggestions.len().min(POPUP_MAX_VISIBLE_ITEMS)
        };
        let popup_height = (visible_items as u16 + 2).min(POPUP_MAX_HEIGHT);
        let popup_width = POPUP_WIDTH.min(area.width.saturating_sub(2));

        let popup_y = if cursor_screen_y + popup_height + 1 <= area.height {
            cursor_screen_y + 1
        } else {
            cursor_screen_y.saturating_sub(popup_height + 1)
        };

        let popup_x = cursor_screen_x.min(area.width.saturating_sub(popup_width + 1));

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        f.render_widget(Clear, popup_area);

        let visible_count = POPUP_MAX_VISIBLE_ITEMS;
        let scroll_offset = if *selected_index >= visible_count {
            selected_index - visible_count + 1
        } else {
            0
        };

        let max_name_width = (popup_width as usize).saturating_sub(8);

        let lines: Vec<Line> = if is_alias_mode {
            let hint_text = if query.is_empty() {
                "Type display text..."
            } else {
                query.as_str()
            };
            vec![Line::from(vec![
                Span::raw(" "),
                Span::styled(hint_text, Style::default().fg(theme.muted)),
            ])]
        } else {
            suggestions
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(visible_count)
                .map(|(idx, suggestion)| {
                    let prefix = if suggestion.is_folder { "dir: " } else { "" };
                    let prefix_len = prefix.len();
                    let is_selected = idx == *selected_index;

                    // Truncate display name if too long (use chars for Unicode safety)
                    let display_name = if suggestion.display_name.chars().count() > max_name_width {
                        let truncated: String = suggestion
                            .display_name
                            .chars()
                            .take(max_name_width.saturating_sub(1))
                            .collect();
                        format!("{}…", truncated)
                    } else {
                        suggestion.display_name.clone()
                    };

                    let style = if is_selected {
                        Style::default()
                            .fg(theme.background)
                            .bg(theme.primary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.foreground)
                    };

                    let prefix_style = if is_selected {
                        style
                    } else {
                        Style::default().fg(theme.warning)
                    };

                    if is_selected {
                        let content_width = (popup_width as usize).saturating_sub(2);
                        let used_width = 1 + prefix_len + display_name.chars().count();
                        let padding_right = " ".repeat(content_width.saturating_sub(used_width));
                        Line::from(vec![
                            Span::styled(" ".to_string(), style),
                            Span::styled(prefix.to_string(), prefix_style),
                            Span::styled(display_name, style),
                            Span::styled(padding_right, style),
                        ])
                    } else {
                        Line::from(vec![
                            Span::raw(" "),
                            Span::styled(prefix.to_string(), prefix_style),
                            Span::styled(display_name, style),
                        ])
                    }
                })
                .collect()
        };

        let title = match mode {
            WikiAutocompleteMode::Note => {
                if query.is_empty() {
                    " Wiki Link ".to_string()
                } else {
                    format!(" [[{} ", query)
                }
            }
            WikiAutocompleteMode::Heading => {
                let note = target_note.as_ref().map(|s| s.as_str()).unwrap_or("");
                if query.is_empty() {
                    format!(" [[{}# ", note)
                } else {
                    format!(" [[{}#{} ", note, query)
                }
            }
            WikiAutocompleteMode::Alias => {
                let target = target_note.as_ref().map(|s| s.as_str()).unwrap_or("");
                if query.is_empty() {
                    format!(" [[{}| ", target)
                } else {
                    format!(" [[{}|{} ", target, query)
                }
            }
        };

        let hint = match mode {
            WikiAutocompleteMode::Alias => " Enter to close ".to_string(),
            _ if !suggestions.is_empty() => format!(" {}/{} ", selected_index + 1, suggestions.len()),
            _ => " No matches ".to_string(),
        };

        let popup = Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .title_bottom(Line::from(hint).right_aligned())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.info))
                .style(Style::default().bg(theme.background_secondary)),
        );

        f.render_widget(popup, popup_area);
    }
}
