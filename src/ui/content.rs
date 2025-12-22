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
use crate::config::Theme;

pub fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focus == Focus::Content && app.mode == Mode::Normal;
    let theme = &app.theme;

    let border_style = if app.floating_cursor_mode {
        Style::default().fg(theme.yellow)
    } else if is_focused {
        Style::default().fg(theme.bright_blue)
    } else {
        Style::default().fg(theme.bright_black)
    };

    let floating_indicator = if app.floating_cursor_mode { " [FLOAT] " } else { "" };
    let title = app
        .current_note()
        .map(|n| format!(" {}{} ", n.title, floating_indicator))
        .unwrap_or_else(|| format!(" Content{} ", floating_indicator));

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

    let details_states = &app.details_open_states;
    let get_item_height = |item: &ContentItem| -> u16 {
        match item {
            ContentItem::TextLine(line) => calc_wrapped_height(line, 4),
            ContentItem::Image(_) => 8u16,
            ContentItem::CodeLine(line) => calc_wrapped_height(line, 6),
            ContentItem::CodeFence(_) => 1u16,
            ContentItem::TaskItem { text, .. } => calc_wrapped_height(text, 6),
            ContentItem::TableRow { .. } => 1u16,
            ContentItem::Details { content_lines, id, .. } => {
                let is_open = details_states.get(id).copied().unwrap_or(false);
                if is_open {
                    1 + content_lines.len() as u16
                } else {
                    1u16 
                }
            }
        }
    };

    let scroll_offset = if app.floating_cursor_mode {
        // FLOATING MODE: cursor moves freely, view only scrolls when cursor goes out of bounds
        let base_offset = if app.content_scroll_offset > 0 {
            app.content_scroll_offset.saturating_sub(1)
        } else {
            0
        };

        let mut height_from_offset = 0u16;
        let mut last_visible_idx = base_offset;
        for (i, item) in app.content_items.iter().enumerate().skip(base_offset) {
            let item_height = get_item_height(item);
            if height_from_offset + item_height > inner_area.height {
                break;
            }
            height_from_offset += item_height;
            last_visible_idx = i;
        }

        if cursor < base_offset {
            app.content_scroll_offset = cursor + 1;
            cursor
        } else if cursor > last_visible_idx {
            let mut cumulative_height = 0u16;
            for (i, item) in app.content_items.iter().enumerate() {
                if i <= cursor {
                    cumulative_height += get_item_height(item);
                }
                if i == cursor {
                    break;
                }
            }

            let mut new_offset = 0;
            let mut height_so_far = 0u16;
            for (i, item) in app.content_items.iter().enumerate() {
                if i > cursor {
                    break;
                }
                height_so_far += get_item_height(item);
                if cumulative_height - height_so_far <= inner_area.height {
                    new_offset = i + 1;
                    break;
                }
            }
            app.content_scroll_offset = new_offset + 1;
            new_offset
        } else {
            base_offset
        }
    } else {
        // NORMAL MODE: cursor moves freely in first page, then stays at bottom

        let mut first_page_height = 0u16;
        let mut first_page_last_idx = 0;
        for (i, item) in app.content_items.iter().enumerate() {
            let item_height = get_item_height(item);
            if first_page_height + item_height > inner_area.height {
                break;
            }
            first_page_height += item_height;
            first_page_last_idx = i;
        }

        if cursor <= first_page_last_idx {
            app.content_scroll_offset = 1;
            0
        } else {
            let mut height_from_cursor = 0u16;
            let mut first_visible_idx = cursor;

            for i in (0..=cursor).rev() {
                let item_height = get_item_height(&app.content_items[i]);
                if height_from_cursor + item_height > inner_area.height {
                    break;
                }
                height_from_cursor += item_height;
                first_visible_idx = i;
            }

            app.content_scroll_offset = first_visible_idx + 1;
            first_visible_idx
        }
    };

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
        app.content_area = inner_area;
        app.content_item_rects.clear();
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    app.content_area = inner_area;
    app.content_item_rects.clear();
    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx < chunks.len() {
            app.content_item_rects.push((item_idx, chunks[chunk_idx]));
        }
    }

    let mut current_lang = String::new();
    if let Some(&first_visible_idx) = visible_indices.first() {
        for i in (0..first_visible_idx).rev() {
            if let ContentItem::CodeFence(lang) = &app.content_items[i] {
                if !lang.is_empty() {
                    current_lang = lang.clone();
                }
                break;
            }
        }
    }

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }
        let is_cursor_line = item_idx == cursor && is_focused;
        let is_hovered = app.mouse_hover_item == Some(item_idx);

        // Clone the item data to avoid borrow conflicts
        let item_clone = app.content_items[item_idx].clone();

        match item_clone {
            ContentItem::TextLine(line) => {
                let has_regular_link = app.item_link_at(item_idx).is_some();
                let has_wiki_link = !app.item_wiki_links_at(item_idx).is_empty();
                let has_link = (is_cursor_line || is_hovered) && (has_regular_link || has_wiki_link);
                let selected_link = if is_cursor_line { app.selected_link_index } else { 0 };
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                render_content_line(f, &app.theme, &line, chunks[chunk_idx], is_cursor_line, has_link, selected_link, Some(wiki_validator));
            }
            ContentItem::Image(path) => {
                render_inline_image_with_cursor(f, app, &path, chunks[chunk_idx], is_cursor_line, is_hovered);
            }
            ContentItem::CodeLine(line) => {
                app.ensure_highlighter();
                render_code_line(f, &app.theme, app.get_highlighter(), &line, &current_lang, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeFence(lang) => {
                current_lang = lang.clone();
                render_code_fence(f, &app.theme, &lang, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TaskItem { text, checked, .. } => {
                render_task_item(f, &app.theme, &text, checked, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TableRow { cells, is_separator, is_header, column_widths } => {
                render_table_row(f, &app.theme, &cells, is_separator, is_header, &column_widths, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::Details { summary, content_lines, id } => {
                let is_open = app.details_open_states.get(&id).copied().unwrap_or(false);
                render_details(f, &app.theme, &summary, &content_lines, is_open, chunks[chunk_idx], is_cursor_line);
            }
        }
    }
}

fn parse_inline_formatting<'a, F>(
    text: &'a str,
    theme: &Theme,
    selected_link: Option<usize>,
    wiki_link_validator: Option<F>,
) -> Vec<Span<'a>>
where
    F: Fn(&str) -> bool,
{
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut current_start = 0;
    let mut link_index = 0;

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

        // Check for [[wiki link]]
        if c == '[' {
            let remaining = &text[i..];
            if remaining.starts_with("[[") {
                if let Some(close_pos) = remaining[2..].find("]]") {
                    let target = &remaining[2..2 + close_pos];
                    if !target.is_empty() && !target.contains('[') && !target.contains(']') {
                        if i > current_start {
                            spans.push(Span::styled(&text[current_start..i], Style::default().fg(theme.foreground)));
                        }

                        let is_selected = selected_link == Some(link_index);
                        let is_valid = wiki_link_validator
                            .as_ref()
                            .map(|f| f(target))
                            .unwrap_or(false);

                        let style = if is_selected {
                            Style::default()
                                .fg(theme.black)
                                .bg(theme.yellow)
                                .add_modifier(Modifier::BOLD)
                        } else if is_valid {
                            Style::default()
                                .fg(theme.cyan)
                                .add_modifier(Modifier::UNDERLINED)
                        } else {
                            Style::default()
                                .fg(theme.red)
                                .add_modifier(Modifier::UNDERLINED)
                        };

                        spans.push(Span::styled(target.to_string(), style));
                        link_index += 1;

                        let total_link_len = 2 + close_pos + 2; // [[target]]
                        for _ in 0..total_link_len - 1 {
                            chars.next();
                        }
                        current_start = i + total_link_len;
                        continue;
                    }
                }
            }

            if let Some(bracket_end) = remaining.find("](") {
                let after_bracket = &remaining[bracket_end + 2..];
                if let Some(paren_end) = after_bracket.find(')') {
                    if i > current_start {
                        spans.push(Span::styled(&text[current_start..i], Style::default().fg(theme.foreground)));
                    }

                    let link_text = &remaining[1..bracket_end];
                    let _link_url = &after_bracket[..paren_end];

                    let is_selected = selected_link == Some(link_index);
                    let style = if is_selected {
                        Style::default()
                            .fg(theme.black)
                            .bg(theme.yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(theme.cyan)
                            .add_modifier(Modifier::UNDERLINED)
                    };

                    spans.push(Span::styled(link_text, style));
                    link_index += 1;

                    let total_link_len = bracket_end + 2 + paren_end + 1; // [text](url)
                    for _ in 0..total_link_len - 1 {
                        chars.next();
                    }
                    current_start = i + total_link_len;
                    continue;
                }
            }
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

fn render_content_line<F>(
    f: &mut Frame,
    theme: &Theme,
    line: &str,
    area: Rect,
    is_cursor: bool,
    has_link: bool,
    selected_link: usize,
    wiki_link_validator: Option<F>,
) where
    F: Fn(&str) -> bool,
{
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
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("• ", Style::default().fg(theme.magenta)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("- "), theme, selected, wiki_link_validator));
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
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
            Span::styled("• ", Style::default().fg(theme.magenta)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("* "), theme, selected, wiki_link_validator));
        Line::from(spans)
    } else {
        // Regular text lines (including numbered lists)
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        ];
        spans.extend(parse_inline_formatting(line, theme, selected, wiki_link_validator));
        Line::from(spans)
    };

    let final_line = if has_link {
        let mut spans = styled_line.spans;
        spans.push(Span::styled(" Open ↗", Style::default().fg(theme.cyan)));
        Line::from(spans)
    } else {
        styled_line
    };

    let style = if is_cursor {
        Style::default().bg(theme.bright_black)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(final_line)
        .style(style)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn render_code_line(
    f: &mut Frame,
    theme: &Theme,
    highlighter: Option<&crate::highlight::Highlighter>,
    line: &str,
    lang: &str,
    area: Rect,
    is_cursor: bool,
) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let mut spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        Span::styled("│ ", Style::default().fg(theme.bright_black)),
    ];

    if !lang.is_empty() {
        if let Some(hl) = highlighter {
            spans.extend(hl.highlight_line(line, lang));
        } else {
            spans.push(Span::styled(line.to_string(), Style::default().fg(theme.green)));
        }
    } else {
        spans.push(Span::styled(line.to_string(), Style::default().fg(theme.green)));
    }

    let styled_line = Line::from(spans);

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

fn render_table_row(
    f: &mut Frame,
    theme: &Theme,
    cells: &[String],
    is_separator: bool,
    is_header: bool,
    column_widths: &[usize],
    area: Rect,
    is_cursor: bool,
) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let border_color = theme.bright_black;

    let mut spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        Span::styled("│", Style::default().fg(border_color)),
    ];

    if is_separator {
        for (i, &width) in column_widths.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("┼", Style::default().fg(border_color)));
            }
            let dashes = "─".repeat(width + 2);
            spans.push(Span::styled(dashes, Style::default().fg(border_color)));
        }
    } else {
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("│", Style::default().fg(border_color)));
            }

            let width = column_widths.get(i).copied().unwrap_or(cell.chars().count());
            let cell_content = format!(" {:^width$} ", cell, width = width);

            let cell_style = if is_header {
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.foreground)
            };

            spans.push(Span::styled(cell_content, cell_style));
        }
    }

    spans.push(Span::styled("│", Style::default().fg(border_color)));

    let styled_line = Line::from(spans);

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

fn render_inline_image_with_cursor(f: &mut Frame, app: &mut App, path: &str, area: Rect, is_cursor: bool, is_hovered: bool) {
    let is_remote = path.starts_with("http://") || path.starts_with("https://");
    let is_pending = is_remote && app.is_image_pending(path);
    let is_cached = app.image_cache.contains_key(path);

    // Check if we need to load a new image
    let need_load = match &app.current_image {
        Some(state) => state.path != path,
        None => true,
    };

    if need_load {
        // Load image from cache, disk, or trigger async fetch for remote
        let img = if let Some(img) = app.image_cache.get(path) {
            Some(img.clone())
        } else if is_remote {
            if !is_pending {
                app.start_remote_image_fetch(path);
            }
            None
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
    let show_hint = is_cursor || is_hovered;
    let border_color = if is_cursor {
        theme.yellow
    } else if is_hovered {
        theme.cyan
    } else if is_pending {
        theme.magenta
    } else {
        theme.cyan
    };

    let title = if is_pending {
        format!(" Loading: {} ", path)
    } else if show_hint {
        format!(" Image: {} Open  ↗ ", path)
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

    if is_pending || (is_remote && !is_cached && app.current_image.as_ref().map(|s| s.path != path).unwrap_or(true)) {
        let loading = Paragraph::new("  Loading remote image...")
            .style(Style::default().fg(theme.magenta).add_modifier(Modifier::ITALIC));
        f.render_widget(loading, inner_area);
        return;
    }

    if let Some(state) = &mut app.current_image {
        if state.path == path {
            let image_widget = StatefulImage::new(None);
            f.render_stateful_widget(image_widget, inner_area, &mut state.image);
        }
    } else if !is_remote {
        let placeholder = Paragraph::new("  [Image not found]")
            .style(Style::default().fg(theme.red).add_modifier(Modifier::ITALIC));
        f.render_widget(placeholder, inner_area);
    }
}

fn render_details(
    f: &mut Frame,
    theme: &Theme,
    summary: &str,
    content_lines: &[String],
    is_open: bool,
    area: Rect,
    is_cursor: bool,
) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let toggle_indicator = if is_open { "▼ " } else { "▶ " };

    let mut lines: Vec<Line> = Vec::new();

    let summary_spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.yellow)),
        Span::styled(toggle_indicator, Style::default().fg(theme.cyan)),
        Span::styled(
            summary,
            Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
        ),
    ];
    lines.push(Line::from(summary_spans));

    if is_open {
        for content in content_lines {
            let content_spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("│ ", Style::default().fg(theme.bright_black)),
                Span::styled(content, Style::default().fg(theme.foreground)),
            ];
            lines.push(Line::from(content_spans));
        }
    }

    let style = if is_cursor {
        Style::default().bg(theme.bright_black)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(lines)
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}
