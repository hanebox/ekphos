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
        Style::default().fg(theme.warning)
    } else if is_focused {
        Style::default().fg(theme.primary)
    } else {
        Style::default().fg(theme.border)
    };

    let floating_indicator = if app.floating_cursor_mode { " [FLOAT] " } else { "" };
    let title = app
        .current_note()
        .map(|n| format!(" {}{} ", n.title, floating_indicator))
        .unwrap_or_else(|| format!(" Content{} ", floating_indicator));

    const ZEN_MAX_WIDTH: u16 = 95;

    let inner_area = if app.zen_mode {
        let content_width = area.width.min(ZEN_MAX_WIDTH);
        let x_offset = (area.width.saturating_sub(content_width)) / 2;
        Rect {
            x: area.x + x_offset,
            y: area.y + 1,
            width: content_width,
            height: area.height.saturating_sub(1),
        }
    } else {
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        f.render_widget(block, area);
        inner
    };

    if app.content_items.is_empty() {
        return;
    }

    let cursor = app.content_cursor;
    let available_width = inner_area.width.saturating_sub(4) as usize;
    let max_item_height = inner_area.height.max(1);

    let calc_wrapped_height = |text: &str, prefix_len: usize| -> u16 {
        if text.is_empty() || available_width == 0 {
            return 1;
        }

        let total_len = text.chars().count() + prefix_len;
        let height = ((total_len as f64 / available_width as f64).ceil() as u16).max(1);
        height.min(max_item_height)
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

    if app.buffer_search.active && !app.buffer_search.matches.is_empty() {
        apply_content_search_highlights(f, app, &visible_indices, &chunks);
    }
}

/// Calculate how many characters are removed by inline formatting before a given position
/// This accounts for **bold**, *italic*, `code`, [[wiki links]], and [markdown](links)
fn calc_formatting_shrinkage(text: &str, up_to_pos: usize) -> usize {
    let mut shrinkage = 0usize;
    let mut pos = 0;
    let chars: Vec<char> = text.chars().collect();

    while pos < up_to_pos && pos < chars.len() {
        if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '*' {
            if let Some(end) = find_double_marker(&chars, pos + 2, '*') {
                if end < up_to_pos {
                    shrinkage += 4; 
                } else if pos + 2 < up_to_pos {
                    shrinkage += 2;
                }
                pos = end + 2;
                continue;
            }
        }
        if pos + 1 < chars.len() && chars[pos] == '_' && chars[pos + 1] == '_' {
            if let Some(end) = find_double_marker(&chars, pos + 2, '_') {
                if end < up_to_pos {
                    shrinkage += 4;
                } else if pos + 2 < up_to_pos {
                    shrinkage += 2;
                }
                pos = end + 2;
                continue;
            }
        }
        if chars[pos] == '*' && (pos + 1 >= chars.len() || chars[pos + 1] != '*') {
            if let Some(end) = find_single_marker(&chars, pos + 1, '*') {
                if end < up_to_pos {
                    shrinkage += 2;
                } else if pos + 1 < up_to_pos {
                    shrinkage += 1;
                }
                pos = end + 1;
                continue;
            }
        }
        if chars[pos] == '_' && (pos + 1 >= chars.len() || chars[pos + 1] != '_') {
            if let Some(end) = find_single_marker(&chars, pos + 1, '_') {
                if end < up_to_pos {
                    shrinkage += 2;
                } else if pos + 1 < up_to_pos {
                    shrinkage += 1;
                }
                pos = end + 1;
                continue;
            }
        }
        if chars[pos] == '`' {
            if let Some(end) = find_single_marker(&chars, pos + 1, '`') {
                if end < up_to_pos {
                    shrinkage += 2;
                } else if pos + 1 < up_to_pos {
                    shrinkage += 1;
                }
                pos = end + 1;
                continue;
            }
        }
        if pos + 1 < chars.len() && chars[pos] == '[' && chars[pos + 1] == '[' {
            if let Some(end) = find_wiki_link_end(&chars, pos + 2) {
                if end + 1 < up_to_pos {
                    shrinkage += 4;
                } else if pos + 2 < up_to_pos {
                    shrinkage += 2; 
                }
                pos = end + 2;
                continue;
            }
        }
        if chars[pos] == '[' {
            if let Some((bracket_end, paren_end)) = find_markdown_link(&chars, pos) {
                let url_len = paren_end - bracket_end - 2; 
                if paren_end < up_to_pos {
                    shrinkage += 1 + url_len + 2; 
                } else if bracket_end < up_to_pos {
                    shrinkage += 1; 
                }
                pos = paren_end + 1;
                continue;
            }
        }
        pos += 1;
    }

    shrinkage
}

fn find_double_marker(chars: &[char], start: usize, marker: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == marker && chars[i + 1] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_single_marker(chars: &[char], start: usize, marker: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == marker {
            if marker == '*' || marker == '_' {
                if i + 1 < chars.len() && chars[i + 1] == marker {
                    continue;
                }
            }
            return Some(i);
        }
    }
    None
}

fn find_wiki_link_end(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == ']' {
            return Some(i);
        }
        if chars[i] == '[' || chars[i] == '\n' {
            return None;
        }
        i += 1;
    }
    None
}

fn find_markdown_link(chars: &[char], start: usize) -> Option<(usize, usize)> {
    let mut i = start + 1;
    while i + 1 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == '(' {
            let bracket_end = i;
            let mut j = i + 2;
            while j < chars.len() {
                if chars[j] == ')' {
                    return Some((bracket_end, j));
                }
                if chars[j] == '\n' {
                    return None;
                }
                j += 1;
            }
            return None;
        }
        if chars[i] == '\n' {
            return None;
        }
        i += 1;
    }
    None
}

/// Calculate the adjusted column for a table cell
/// Raw format: "| cell1 | cell2 |"
/// Rendered:   "▶ │ cell1 │ cell2 │" with cells padded to column widths
fn calc_table_adjusted_col(raw_col: usize, cells: &[String], column_widths: &[usize]) -> usize {
    let mut rendered_pos = 3;
    let mut raw_pos = 0;

    for (cell_idx, cell) in cells.iter().enumerate() {
        let col_width = column_widths.get(cell_idx).copied().unwrap_or(3);
        if raw_pos == 0 {
            raw_pos = 1; 
        }

        let raw_cell_start = raw_pos;

        let cell_len = cell.chars().count();
        let raw_cell_end = raw_cell_start + cell_len + 3; // " content |"

        if raw_col >= raw_cell_start && raw_col < raw_cell_end {
            let offset_in_raw_cell = raw_col.saturating_sub(raw_cell_start + 1); // +1 for leading space
            let content_padding = (col_width.saturating_sub(cell_len)) / 2;
            let rendered_content_start = rendered_pos + 1 + content_padding; // +1 for leading space

            return rendered_content_start + offset_in_raw_cell.min(cell_len);
        }

        raw_pos = raw_cell_end;
        rendered_pos += col_width + 2 + 1;
    }

    3 + raw_col
}

fn apply_content_search_highlights(
    f: &mut Frame,
    app: &App,
    visible_indices: &[usize],
    chunks: &[Rect],
) {
    let theme = &app.theme;
    let current_match_idx = app.buffer_search.current_match_index;

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }

        let source_line = app.content_item_source_lines.get(item_idx).copied().unwrap_or(usize::MAX);
        if source_line == usize::MAX {
            continue;
        }

        for (match_idx, m) in app.buffer_search.matches.iter().enumerate() {
            if m.row == source_line {
                let area = chunks[chunk_idx];
                let is_current = match_idx == current_match_idx;
                let highlight_color = if is_current {
                    theme.search.match_current
                } else {
                    theme.search.match_highlight
                };

                // Calculate the rendered position based on content type
                // Different content types transform the raw text differently
                let adjusted_col = match &app.content_items.get(item_idx) {
                    Some(ContentItem::TableRow { cells, column_widths, is_separator, .. }) => {
                        if *is_separator {
                            continue; 
                        }
                        calc_table_adjusted_col(m.start_col, cells, column_widths)
                    }
                    Some(ContentItem::TextLine(line)) => {
                        let line = normalize_whitespace(line);
                        let (rendered_prefix_len, raw_prefix_len, content_text) =
                            if line.starts_with("###### ") {
                                (2, 7, line[7..].to_string())
                            } else if line.starts_with("##### ") {
                                (2, 6, line[6..].to_string())
                            } else if line.starts_with("#### ") {
                                (4, 5, line[5..].to_string())
                            } else if line.starts_with("### ") {
                                (4, 4, line[4..].to_string())
                            } else if line.starts_with("## ") {
                                (4, 3, line[3..].to_string())
                            } else if line.starts_with("# ") {
                                (4, 2, line[2..].to_string())
                            } else if line.starts_with("- ") {
                                (4, 2, line[2..].to_string())
                            } else if line.starts_with("* ") {
                                (4, 2, line[2..].to_string())
                            } else if line.starts_with("> ") {
                                (4, 2, line[2..].to_string())
                            } else {
                                (2, 0, line.to_string())
                            };

                        if m.start_col < raw_prefix_len {
                            continue;
                        }
                        let content_start_col = m.start_col - raw_prefix_len;
                        let formatting_shrinkage = if !content_text.is_empty() {
                            calc_formatting_shrinkage(&content_text, content_start_col)
                        } else {
                            0
                        };
                        rendered_prefix_len + content_start_col - formatting_shrinkage
                    }
                    Some(ContentItem::CodeLine(_)) => {
                        4 + m.start_col
                    }
                    Some(ContentItem::TaskItem { text, .. }) => {
                        if m.start_col < 6 {
                            continue;
                        }
                        let content_start_col = m.start_col - 6;
                        let formatting_shrinkage = calc_formatting_shrinkage(text, content_start_col);
                        6 + content_start_col - formatting_shrinkage
                    }
                    _ => {
                        2 + m.start_col 
                    }
                };

                let start_x = area.x + adjusted_col as u16;
                let match_len = m.end_col - m.start_col;

                for offset in 0..match_len {
                    let x = start_x + offset as u16;
                    if x < area.x + area.width {
                        if let Some(cell) = f.buffer_mut().cell_mut((x, area.y)) {
                            cell.set_bg(highlight_color);
                            cell.set_fg(ratatui::style::Color::Black);
                        }
                    }
                }
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
    let content_theme = &theme.content;

    while let Some((i, c)) = chars.next() {
        // Check for **bold** or *italic*
        if c == '*' {
            if let Some(&(_, '*')) = chars.peek() {
                // Found **, look for closing **
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
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
                        Style::default().fg(content_theme.text).add_modifier(Modifier::BOLD),
                    ));
                    current_start = end + 2;
                } else {
                    // No closing **, treat as regular text
                    current_start = i;
                }
                continue;
            } else {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
                let italic_start = i + 1;
                let mut italic_end = None;

                while let Some((j, ch)) = chars.next() {
                    if ch == '*' {
                        if chars.peek().map(|&(_, c)| c != '*').unwrap_or(true) {
                            italic_end = Some(j);
                            break;
                        }
                    }
                }

                if let Some(end) = italic_end {
                    spans.push(Span::styled(
                        &text[italic_start..end],
                        Style::default().fg(content_theme.text).add_modifier(Modifier::ITALIC),
                    ));
                    current_start = end + 1;
                } else {
                    current_start = i;
                }
                continue;
            }
        }

        // Check for __bold__ or _italic_
        if c == '_' {
            if let Some(&(_, '_')) = chars.peek() {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
                chars.next(); 
                let bold_start = i + 2;
                let mut bold_end = None;

                while let Some((j, ch)) = chars.next() {
                    if ch == '_' {
                        if let Some(&(_, '_')) = chars.peek() {
                            bold_end = Some(j);
                            chars.next(); 
                            break;
                        }
                    }
                }

                if let Some(end) = bold_end {
                    spans.push(Span::styled(
                        &text[bold_start..end],
                        Style::default().fg(content_theme.text).add_modifier(Modifier::BOLD),
                    ));
                    current_start = end + 2;
                } else {
                    current_start = i;
                }
                continue;
            } else {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
                let italic_start = i + 1;
                let mut italic_end = None;

                while let Some((j, ch)) = chars.next() {
                    if ch == '_' {
                        if chars.peek().map(|&(_, c)| c != '_').unwrap_or(true) {
                            italic_end = Some(j);
                            break;
                        }
                    }
                }

                if let Some(end) = italic_end {
                    spans.push(Span::styled(
                        &text[italic_start..end],
                        Style::default().fg(content_theme.text).add_modifier(Modifier::ITALIC),
                    ));
                    current_start = end + 1;
                } else {
                    current_start = i;
                }
                continue;
            }
        }

        // Check for `code`
        if c == '`' {
            if i > current_start {
                spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
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
                    Style::default().fg(content_theme.code).bg(content_theme.code_background),
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
                            spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                        }

                        let is_selected = selected_link == Some(link_index);
                        let is_valid = wiki_link_validator
                            .as_ref()
                            .map(|f| f(target))
                            .unwrap_or(false);

                        let style = if is_selected {
                            Style::default()
                                .fg(theme.background)
                                .bg(theme.warning)
                                .add_modifier(Modifier::BOLD)
                        } else if is_valid {
                            Style::default()
                                .fg(content_theme.link)
                                .add_modifier(Modifier::UNDERLINED)
                        } else {
                            Style::default()
                                .fg(content_theme.link_invalid)
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
                        spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                    }

                    let link_text = &remaining[1..bracket_end];
                    let _link_url = &after_bracket[..paren_end];

                    let is_selected = selected_link == Some(link_index);
                    let style = if is_selected {
                        Style::default()
                            .fg(theme.background)
                            .bg(theme.warning)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(content_theme.link)
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
        spans.push(Span::styled(&text[current_start..], Style::default().fg(content_theme.text)));
    }

    if spans.is_empty() {
        spans.push(Span::styled(text, Style::default().fg(content_theme.text)));
    }

    spans
}

fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    s.width()
}

fn wrap_line_for_cursor<'a>(
    first_line_spans: Vec<Span<'a>>,
    available_width: usize,
    _theme: &Theme,
) -> Vec<Line<'a>> {
    if available_width == 0 {
        return vec![Line::from(first_line_spans)];
    }
    let mut prefix_spans: Vec<Span<'a>> = Vec::new();
    let mut content_spans: Vec<Span<'a>> = Vec::new();
    let mut prefix_width = 0usize;

    for (i, span) in first_line_spans.into_iter().enumerate() {
        let span_text = span.content.to_string();
        let span_width = display_width(&span_text);
        if i == 0 {
            prefix_spans.push(span);
            prefix_width += span_width;
        } else if i == 1 && span_width <= 3 && !span_text.chars().any(|c| c.is_alphanumeric()) {
            prefix_spans.push(span);
            prefix_width += span_width;
        } else {
            content_spans.push(span);
        }
    }

    let content_width: usize = content_spans.iter()
        .map(|s| display_width(&s.content))
        .sum();
    let first_line_available = available_width.saturating_sub(prefix_width);

    if content_width <= first_line_available {
        let mut spans = prefix_spans;
        spans.extend(content_spans);
        return vec![Line::from(spans)];
    }

    let continuation_indent = " ".repeat(prefix_width);
    let continuation_available = available_width.saturating_sub(prefix_width);
    let mut lines: Vec<Line<'a>> = Vec::new();
    let mut current_line_spans: Vec<Span<'a>> = Vec::new();
    let mut current_line_width = 0usize;
    let mut is_first_line = true;

    for span in content_spans {
        let span_style = span.style;
        let span_text = span.content.to_string();
        let mut remaining_text = span_text.as_str();

        while !remaining_text.is_empty() {
            let max_width = if is_first_line { first_line_available } else { continuation_available };
            let available_in_line = max_width.saturating_sub(current_line_width);

            if available_in_line == 0 {
                if is_first_line {
                    let mut line_spans = prefix_spans.clone();
                    line_spans.extend(current_line_spans.drain(..));
                    lines.push(Line::from(line_spans));
                    is_first_line = false;
                } else {
                    let mut line_spans = vec![Span::styled(continuation_indent.clone(), Style::default())];
                    line_spans.extend(current_line_spans.drain(..));
                    lines.push(Line::from(line_spans));
                }
                current_line_width = 0;
                continue;
            }

            let mut fit_chars = 0;
            let mut fit_width = 0;
            for ch in remaining_text.chars() {
                let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                if fit_width + ch_width > available_in_line {
                    break;
                }
                fit_chars += ch.len_utf8();
                fit_width += ch_width;
            }

            if fit_chars == 0 && current_line_width == 0 {
                let ch = remaining_text.chars().next().unwrap();
                fit_chars = ch.len_utf8();
                fit_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
            }

            if fit_chars > 0 {
                let (fitting, rest) = remaining_text.split_at(fit_chars);
                current_line_spans.push(Span::styled(fitting.to_string(), span_style));
                current_line_width += fit_width;
                remaining_text = rest;
            }

            if !remaining_text.is_empty() {
                if is_first_line {
                    let mut line_spans = prefix_spans.clone();
                    line_spans.extend(current_line_spans.drain(..));
                    lines.push(Line::from(line_spans));
                    is_first_line = false;
                } else {
                    let mut line_spans = vec![Span::styled(continuation_indent.clone(), Style::default())];
                    line_spans.extend(current_line_spans.drain(..));
                    lines.push(Line::from(line_spans));
                }
                current_line_width = 0;
            }
        }
    }

    if !current_line_spans.is_empty() {
        if is_first_line {
            let mut line_spans = prefix_spans.clone();
            line_spans.extend(current_line_spans);
            lines.push(Line::from(line_spans));
        } else {
            let mut line_spans = vec![Span::styled(continuation_indent, Style::default())];
            line_spans.extend(current_line_spans);
            lines.push(Line::from(line_spans));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(prefix_spans));
    }

    lines
}

/// Normalize whitespace, replace tabs with spaces and handle special Unicode whitespace
fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => result.push_str("    "),  // Tab to 4 spaces
            '\u{00A0}' => result.push(' '),  // Non-breaking space
            '\u{2000}'..='\u{200B}' => result.push(' '),  // Various Unicode spaces
            '\u{202F}' => result.push(' '),  // Narrow no-break space
            '\u{205F}' => result.push(' '),  // Medium mathematical space
            '\u{3000}' => result.push(' '),  // Ideographic space
            _ => result.push(c),
        }
    }
    result
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
    let line = &normalize_whitespace(line);
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding

    // Check headings from most specific (######) to least specific (#)
    let content_theme = &theme.content;
    let styled_line = if line.starts_with("###### ") {
        // H6: Smallest, italic, subtle
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled(
                line.trim_start_matches("###### "),
                Style::default()
                    .fg(content_theme.text)
                    .add_modifier(Modifier::ITALIC),
            ),
        ])
    } else if line.starts_with("##### ") {
        // H5: Small, muted color
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled(
                line.trim_start_matches("##### "),
                Style::default()
                    .fg(content_theme.heading4)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("#### ") {
        // H4: Small prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("› ", Style::default().fg(content_theme.heading4)),
            Span::styled(
                line.trim_start_matches("#### "),
                Style::default()
                    .fg(content_theme.heading4)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("### ") {
        // H3: Medium prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("▸ ", Style::default().fg(content_theme.heading3)),
            Span::styled(
                line.trim_start_matches("### "),
                Style::default()
                    .fg(content_theme.heading3)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("## ") {
        // H2: Larger prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("■ ", Style::default().fg(content_theme.heading2)),
            Span::styled(
                line.trim_start_matches("## "),
                Style::default()
                    .fg(content_theme.heading2)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("# ") {
        // H1: Largest, most prominent
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("◆ ", Style::default().fg(content_theme.heading1)),
            Span::styled(
                line.trim_start_matches("# ").to_uppercase(),
                Style::default()
                    .fg(content_theme.heading1)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("- ") {
        // Bullet list
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("• ", Style::default().fg(content_theme.list_marker)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("- "), theme, selected, wiki_link_validator));
        Line::from(spans)
    } else if line.starts_with("> ") {
        // Blockquote
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("┃ ", Style::default().fg(content_theme.blockquote)),
            Span::styled(
                line.trim_start_matches("> "),
                Style::default().fg(content_theme.blockquote).add_modifier(Modifier::ITALIC),
            ),
        ])
    } else if line == "---" || line == "***" || line == "___" {
        // Horizontal rule
        let hr_width = available_width.saturating_sub(2);
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("─".repeat(hr_width), Style::default().fg(theme.border)),
        ])
    } else if line.starts_with("* ") {
        // Bullet list (asterisk variant)
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("• ", Style::default().fg(content_theme.list_marker)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("* "), theme, selected, wiki_link_validator));
        Line::from(spans)
    } else {
        // Regular text lines (including numbered lists)
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        ];
        spans.extend(parse_inline_formatting(line, theme, selected, wiki_link_validator));
        Line::from(spans)
    };

    let final_line = if has_link {
        let mut spans = styled_line.spans;
        spans.push(Span::styled(" Open ↗", Style::default().fg(content_theme.link)));
        Line::from(spans)
    } else {
        styled_line
    };

    // Manually handle wrapping so continuation lines have same padding as first line
    let wrapped_lines = wrap_line_for_cursor(final_line.spans, available_width, theme);

    let bg_style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
        let line_area = Rect {
            x: area.x,
            y: area.y.saturating_add(i as u16),
            width: area.width,
            height: 1,
        };
        if line_area.y < area.y + area.height {
            let paragraph = Paragraph::new(wrapped_line.clone()).style(bg_style);
            f.render_widget(paragraph, line_area);
        }
    }
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
    let expanded_line = expand_tabs(line);
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding
    let content_theme = &theme.content;

    let mut spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        Span::styled("│ ", Style::default().fg(theme.border)),
    ];

    if !lang.is_empty() {
        if let Some(hl) = highlighter {
            spans.extend(hl.highlight_line(&expanded_line, lang));
        } else {
            spans.push(Span::styled(expanded_line, Style::default().fg(content_theme.code)));
        }
    } else {
        spans.push(Span::styled(expanded_line, Style::default().fg(content_theme.code)));
    }

    let wrapped_lines = wrap_line_for_cursor(spans, available_width, theme);
    let bg_style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default().bg(content_theme.code_background)
    };

    for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
        let line_area = Rect {
            x: area.x,
            y: area.y.saturating_add(i as u16),
            width: area.width,
            height: 1,
        };
        if line_area.y < area.y + area.height {
            let paragraph = Paragraph::new(wrapped_line.clone()).style(bg_style);
            f.render_widget(paragraph, line_area);
        }
    }
}

fn render_code_fence(f: &mut Frame, theme: &Theme, _lang: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let content_theme = &theme.content;

    let styled_line = Line::from(vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        Span::styled("───", Style::default().fg(theme.border)),
    ]);

    let style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default().bg(content_theme.code_background)
    };

    let paragraph = Paragraph::new(styled_line)
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_task_item(f: &mut Frame, theme: &Theme, text: &str, checked: bool, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let checkbox_color = if checked { theme.success } else { theme.secondary };
    let text_style = if checked {
        Style::default().fg(theme.muted).add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(theme.foreground)
    };
    let expanded_text = expand_tabs(text);
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding

    let spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        Span::styled("[", Style::default().fg(checkbox_color)),
        Span::styled(if checked { "x" } else { " " }, Style::default().fg(checkbox_color).add_modifier(Modifier::BOLD)),
        Span::styled("] ", Style::default().fg(checkbox_color)),
        Span::styled(expanded_text, text_style),
    ];

    let wrapped_lines = wrap_line_for_cursor(spans, available_width, theme);

    let bg_style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
        let line_area = Rect {
            x: area.x,
            y: area.y.saturating_add(i as u16),
            width: area.width,
            height: 1,
        };
        if line_area.y < area.y + area.height {
            let paragraph = Paragraph::new(wrapped_line.clone()).style(bg_style);
            f.render_widget(paragraph, line_area);
        }
    }
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
    let border_color = theme.border;

    let mut spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
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

            let expanded_cell = expand_tabs(cell);
            let width = column_widths.get(i).copied().unwrap_or(expanded_cell.chars().count());
            let cell_content = format!(" {:^width$} ", expanded_cell, width = width);

            let cell_style = if is_header {
                Style::default().fg(theme.info).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.foreground)
            };

            spans.push(Span::styled(cell_content, cell_style));
        }
    }

    spans.push(Span::styled("│", Style::default().fg(border_color)));

    let styled_line = Line::from(spans);

    let style = if is_cursor {
        Style::default().bg(theme.selection)
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
        theme.warning
    } else if is_hovered {
        theme.info
    } else if is_pending {
        theme.secondary
    } else {
        theme.info
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
        let bg = Paragraph::new("").style(Style::default().bg(theme.selection));
        f.render_widget(bg, area);
    }

    f.render_widget(block, area);

    if is_pending || (is_remote && !is_cached && app.current_image.as_ref().map(|s| s.path != path).unwrap_or(true)) {
        let loading = Paragraph::new("  Loading remote image...")
            .style(Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC));
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
            .style(Style::default().fg(theme.error).add_modifier(Modifier::ITALIC));
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

    let expanded_summary = expand_tabs(summary);
    let summary_spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        Span::styled(toggle_indicator, Style::default().fg(theme.info)),
        Span::styled(
            expanded_summary,
            Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
        ),
    ];
    lines.push(Line::from(summary_spans));

    if is_open {
        for content in content_lines {
            let expanded_content = expand_tabs(content);
            let content_spans = vec![
                Span::styled("  ", Style::default()),
                Span::styled("│ ", Style::default().fg(theme.border)),
                Span::styled(expanded_content, Style::default().fg(theme.foreground)),
            ];
            lines.push(Line::from(content_spans));
        }
    }

    let style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(lines)
        .style(style)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}
