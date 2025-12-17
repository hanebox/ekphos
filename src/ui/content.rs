use std::path::PathBuf;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use ratatui_image::StatefulImage;

use crate::app::{App, ContentItem, Focus, ImageState, Mode};
use crate::theme::Theme;

pub fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focus == Focus::Content && app.mode == Mode::Normal;
    let theme = &app.theme;

    let border_style = if is_focused {
        Style::default().fg(theme.bright_blue)
    } else {
        Style::default().fg(theme.bright_black)
    };

    let title = app
        .current_note()
        .map(|n| format!(" {} ", n.title))
        .unwrap_or_else(|| " Content ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if app.content_items.is_empty() {
        return;
    }

    let cursor = app.content_cursor;
    let available_width = inner_area.width.saturating_sub(4) as usize;

    // calculate wrapped text row height
    let calc_wrapped_height = |text: &str, prefix_len: usize| -> u16 {
        if text.is_empty() || available_width == 0 {
            return 1;
        }

        let total_len = text.chars().count() + prefix_len;

        ((total_len as f64 / available_width as f64).ceil() as u16).max(1)
    };

    // Helper to get item height
    let get_item_height = |item: &ContentItem| -> u16 {
        match item {
            ContentItem::TextLine(line) => calc_wrapped_height(line, 4),
            ContentItem::Image(_) => 8u16,
            ContentItem::CodeLine(line) => calc_wrapped_height(line, 6),
            ContentItem::CodeFence(_) => 1u16,
            ContentItem::TaskItem { text, .. } => calc_wrapped_height(text, 6),
        }
    };

    // Calculate scroll offset accounting for variable item heights
    // We need to ensure the cursor line is visible
    let mut scroll_offset = 0;
    let mut cumulative_height = 0u16;

    for (i, item) in app.content_items.iter().enumerate() {
        let item_height = get_item_height(item);
        if i <= cursor {
            cumulative_height += item_height;
        }
        if i == cursor {
            break;
        }
    }

    // If cursor would be below visible area, find scroll offset
    if cumulative_height > inner_area.height {
        let mut height_so_far = 0u16;
        for (i, item) in app.content_items.iter().enumerate() {
            if i > cursor {
                break;
            }
            let item_height = get_item_height(item);
            height_so_far += item_height;
            if cumulative_height - height_so_far <= inner_area.height {
                scroll_offset = i + 1;
                break;
            }
        }
    }

    let mut constraints: Vec<Constraint> = Vec::new();
    let mut visible_indices: Vec<usize> = Vec::new();
    let mut total_height = 0u16;

    for (i, item) in app.content_items.iter().enumerate().skip(scroll_offset) {
        if total_height >= inner_area.height {
            break;
        }
        let item_height = get_item_height(item);
        constraints.push(Constraint::Length(item_height));
        visible_indices.push(i);
        total_height += item_height;
    }

    if constraints.is_empty() {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }
        let is_cursor_line = item_idx == cursor && is_focused;

        // Clone the item data to avoid borrow conflicts
        let item_clone = app.content_items[item_idx].clone();

        match item_clone {
            ContentItem::TextLine(line) => {
                render_content_line(f, &app.theme, &line, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::Image(path) => {
                render_inline_image_with_cursor(f, app, &path, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeLine(line) => {
                render_code_line(f, &app.theme, &line, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeFence(lang) => {
                render_code_fence(f, &app.theme, &lang, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TaskItem { text, checked, .. } => {
                render_task_item(f, &app.theme, &text, checked, chunks[chunk_idx], is_cursor_line);
            }
        }
    }
}

fn parse_inline_formatting<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut current_start = 0;

    while let Some((i, c)) = chars.next() {
        // Check for **bold**
        if c == '*' {
            if let Some(&(_, '*')) = chars.peek() {
                // Found **, look for closing **
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(theme.foreground)));
                }
                chars.next(); // consume second *
                let bold_start = i + 2;
                let mut bold_end = None;

                while let Some((j, ch)) = chars.next() {
                    if ch == '*' {
                        if let Some(&(_, '*')) = chars.peek() {
                            bold_end = Some(j);
                            chars.next(); // consume second *
                            break;
                        }
                    }
                }

                if let Some(end) = bold_end {
                    spans.push(Span::styled(
                        &text[bold_start..end],
                        Style::default().fg(theme.foreground).add_modifier(Modifier::BOLD),
                    ));
                    current_start = end + 2;
                } else {
                    // No closing **, treat as regular text
                    current_start = i;
                }
                continue;
            }
        }

        // Check for `code`
        if c == '`' {
            if i > current_start {
                spans.push(Span::styled(&text[current_start..i], Style::default().fg(theme.foreground)));
            }
            let code_start = i + 1;
            let mut code_end = None;

            while let Some((j, ch)) = chars.next() {
                if ch == '`' {
                    code_end = Some(j);
                    break;
                }
            }

            if let Some(end) = code_end {
                spans.push(Span::styled(
                    &text[code_start..end],
                    Style::default().fg(theme.green).bg(theme.black),
                ));
                current_start = end + 1;
            } else {
                // No closing `, treat as regular text
                current_start = i;
            }
            continue;
        }
    }

    // Add remaining text
    if current_start < text.len() {
        spans.push(Span::styled(&text[current_start..], Style::default().fg(theme.foreground)));
    }

    if spans.is_empty() {
        spans.push(Span::styled(text, Style::default().fg(theme.foreground)));
    }

    spans
}

fn render_content_line(f: &mut Frame, theme: &Theme, line: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    // Check headings from most specific (######) to least specific (#)
    let styled_line = if line.starts_with("###### ") {
        // H6: Smallest, italic, subtle
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled(
                line.trim_start_matches("###### "),
                Style::default()
                    .fg(theme.white)
                    .add_modifier(Modifier::ITALIC),
            ),
        ])
    } else if line.starts_with("##### ") {
        // H5: Small, muted color
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled(
                line.trim_start_matches("##### "),
                Style::default()
                    .fg(theme.cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("#### ") {
        // H4: Small prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("› ", Style::default().fg(theme.magenta)),
            Span::styled(
                line.trim_start_matches("#### "),
                Style::default()
                    .fg(theme.magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("### ") {
        // H3: Medium prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("▸ ", Style::default().fg(theme.yellow)),
            Span::styled(
                line.trim_start_matches("### "),
                Style::default()
                    .fg(theme.yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("## ") {
        // H2: Larger prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("■ ", Style::default().fg(theme.green)),
            Span::styled(
                line.trim_start_matches("## "),
                Style::default()
                    .fg(theme.green)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("# ") {
        // H1: Largest, most prominent
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("◆ ", Style::default().fg(theme.blue)),
            Span::styled(
                line.trim_start_matches("# ").to_uppercase(),
                Style::default()
                    .fg(theme.blue)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("- ") {
        // Bullet list
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("• ", Style::default().fg(theme.magenta)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("- "), theme));
        Line::from(spans)
    } else if line.starts_with("> ") {
        // Blockquote
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("┃ ", Style::default().fg(theme.bright_black)),
            Span::styled(
                line.trim_start_matches("> "),
                Style::default().fg(theme.white).add_modifier(Modifier::ITALIC),
            ),
        ])
    } else if line == "---" || line == "***" || line == "___" {
        // Horizontal rule
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("─".repeat(40), Style::default().fg(theme.bright_black)),
        ])
    } else if line.starts_with("* ") {
        // Bullet list (asterisk variant)
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("• ", Style::default().fg(theme.magenta)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("* "), theme));
        Line::from(spans)
    } else {
        // Regular text lines (including numbered lists)
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        ];
        spans.extend(parse_inline_formatting(line, theme));
        Line::from(spans)
    };

    let style = if is_cursor {
        Style::default().bg(theme.bright_black)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(styled_line)
        .style(style)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn render_code_line(f: &mut Frame, theme: &Theme, line: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let styled_line = Line::from(vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        Span::styled("│ ", Style::default().fg(theme.bright_black)),
        Span::styled(line, Style::default().fg(theme.green)),
    ]);

    let style = if is_cursor {
        Style::default().bg(theme.bright_black)
    } else {
        Style::default().bg(theme.black)
    };

    let paragraph = Paragraph::new(styled_line)
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_code_fence(f: &mut Frame, theme: &Theme, _lang: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let styled_line = Line::from(vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        Span::styled("───", Style::default().fg(theme.bright_black)),
    ]);

    let style = if is_cursor {
        Style::default().bg(theme.bright_black)
    } else {
        Style::default().bg(theme.black)
    };

    let paragraph = Paragraph::new(styled_line)
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_task_item(f: &mut Frame, theme: &Theme, text: &str, checked: bool, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let checkbox_color = if checked { theme.green } else { theme.magenta };
    let text_style = if checked {
        Style::default().fg(theme.bright_black).add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(theme.foreground)
    };

    let styled_line = Line::from(vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        Span::styled("[", Style::default().fg(checkbox_color)),
        Span::styled(if checked { "x" } else { " " }, Style::default().fg(checkbox_color).add_modifier(Modifier::BOLD)),
        Span::styled("] ", Style::default().fg(checkbox_color)),
        Span::styled(text, text_style),
    ]);

    let style = if is_cursor {
        Style::default().bg(theme.bright_black)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(styled_line)
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_inline_image_with_cursor(f: &mut Frame, app: &mut App, path: &str, area: Rect, is_cursor: bool) {
    // Check if we need to load a new image
    let need_load = match &app.current_image {
        Some(state) => state.path != path,
        None => true,
    };

    if need_load {
        // Load image from cache or disk
        let img = if let Some(img) = app.image_cache.get(path) {
            Some(img.clone())
        } else {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() {
                if let Ok(img) = image::open(&path_buf) {
                    app.image_cache.insert(path.to_string(), img.clone());
                    Some(img)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let (Some(img), Some(picker)) = (img, &mut app.picker) {
            let protocol = picker.new_resize_protocol(img);
            app.current_image = Some(ImageState {
                image: protocol,
                path: path.to_string(),
            });
        }
    }

    // Create a bordered area for the image with cursor indicator
    let theme = &app.theme;
    let border_color = if is_cursor {
        theme.yellow
    } else {
        theme.cyan
    };

    let title = if is_cursor {
        format!(" Image: {} [Enter/o to open] ", path)
    } else {
        format!(" Image: {} ", path)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);

    // Add background highlight when cursor is on image
    if is_cursor {
        let bg = Paragraph::new("").style(Style::default().bg(theme.bright_black));
        f.render_widget(bg, area);
    }

    f.render_widget(block, area);

    if let Some(state) = &mut app.current_image {
        if state.path == path {
            let image_widget = StatefulImage::new(None);
            f.render_stateful_widget(image_widget, inner_area, &mut state.image);
        }
    } else {
        // Show placeholder if image couldn't be loaded
        let placeholder = Paragraph::new("  [Image not found]")
            .style(Style::default().fg(theme.red).add_modifier(Modifier::ITALIC));
        f.render_widget(placeholder, inner_area);
    }
}
