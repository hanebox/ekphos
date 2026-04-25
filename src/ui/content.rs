use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use ratatui_image::StatefulImage;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, ContentItem, DialogState, Focus, ImageState, Mode};
use crate::config::Theme;

const INLINE_THUMBNAIL_HEIGHT: u16 = 4;

fn is_inside_inline_code(text: &str, position: usize) -> bool {
    let before = &text[..position];
    let mut inside_code = false;
    for c in before.chars() {
        if c == '`' {
            inside_code = !inside_code;
        }
    }
    inside_code
}

fn extract_inline_images(text: &str) -> Vec<String> {
    let mut images = Vec::new();
    let mut search_start = 0;

    while search_start < text.len() {
        let remaining = &text[search_start..];
        if let Some(img_pos) = remaining.find("![") {
            let abs_img_pos = search_start + img_pos;

            // skip double-bang images they don't get thumbnails
            if abs_img_pos > 0 && text.as_bytes().get(abs_img_pos - 1) == Some(&b'!') {
                search_start = abs_img_pos + 2;
                continue;
            }
            if is_inside_inline_code(text, abs_img_pos) {
                search_start = abs_img_pos + 2;
                continue;
            }

            let from_img = &text[abs_img_pos..];

            if let Some(bracket_end) = from_img[1..].find("](") {
                let after_bracket = &from_img[1 + bracket_end + 2..];
                if let Some(paren_end) = after_bracket.find(')') {
                    let url = &after_bracket[..paren_end];
                    if !url.is_empty() {
                        images.push(url.to_string());
                    }
                    search_start = abs_img_pos + 1 + bracket_end + 2 + paren_end + 1;
                    continue;
                }
            }
            search_start = abs_img_pos + 2;
        } else {
            break;
        }
    }

    images
}

pub fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focus == Focus::Content && app.mode == Mode::Normal;
    // Skip rendering images when dialog is active to prevent terminal graphics artifacts
    let skip_images = app.dialog != DialogState::None || app.show_welcome;
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
        if app.floating_cursor_mode {
            let status_area = Rect {
                x: area.x + x_offset,
                y: area.y,
                width: content_width,
                height: 1,
            };
            render_zen_content_status_line(f, theme, status_area);
        }

        let y_offset = if app.floating_cursor_mode { 2 } else { 1 };
        Rect {
            x: area.x + x_offset,
            y: area.y + y_offset,
            width: content_width,
            height: area.height.saturating_sub(y_offset),
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
    app.editor_area = if app.zen_mode { inner_area } else { area };

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

        let content_width = available_width.saturating_sub(prefix_len);
        if content_width == 0 {
            return 1;
        }

        let mut lines = 1u16;
        let mut current_line_width = 0usize;

        for word in text.split_whitespace() {
            let word_width = unicode_width::UnicodeWidthStr::width(word);

            if current_line_width == 0 {
                if word_width > content_width {
                    lines += ((word_width - 1) / content_width) as u16;
                }
                current_line_width = word_width;
            } else if current_line_width + 1 + word_width <= content_width {
                current_line_width += 1 + word_width;
            } else {
                lines += 1;
                if word_width > content_width {
                    lines += ((word_width - 1) / content_width) as u16;
                }
                current_line_width = word_width.min(content_width);
            }
        }

        lines.min(max_item_height)
    };

    let details_states = &app.details_open_states;
    let get_item_height = |item: &ContentItem| -> u16 {
        match item {
            ContentItem::TextLine(line) => {
                let base_height = calc_wrapped_height(line, 4);
                let inline_images = extract_inline_images(line);
                if inline_images.is_empty() {
                    base_height
                } else {
                    base_height + (inline_images.len() as u16 * INLINE_THUMBNAIL_HEIGHT)
                }
            }
            ContentItem::Image(_) => 8u16,
            ContentItem::CodeLine(line) => calc_wrapped_height(line, 6),
            ContentItem::CodeFence(_) => 1u16,
            ContentItem::TaskItem { text, .. } => {
                let base_height = calc_wrapped_height(text, 6);
                let inline_images = extract_inline_images(text);
                if inline_images.is_empty() {
                    base_height
                } else {
                    base_height + (inline_images.len() as u16 * INLINE_THUMBNAIL_HEIGHT)
                }
            }
            ContentItem::TableRow { cells, is_separator, column_widths, .. } => {
                if *is_separator {
                    1u16
                } else {
                    // Budget must match render_table_row exactly. render uses area.width
                    // (= inner_area.width after chunk split), not `available_width`, which
                    // carries a 4-char list-prefix margin that tables don't need.
                    let n = column_widths.len();
                    let overhead = 3 + 3 * n;
                    let budget = (inner_area.width as usize).saturating_sub(overhead);
                    let capped = cap_column_widths(column_widths, budget);
                    let text_color = theme.content.text;
                    let row_lines = cells.iter().enumerate().map(|(i, cell)| {
                        let w = capped.get(i).copied().unwrap_or(0);
                        let expanded = expand_tabs(cell);
                        let spans = parse_inline_formatting::<fn(&str) -> bool>(&expanded, theme, None, None);
                        distribute_spans_across_lines(spans, w, text_color).len()
                    }).max().unwrap_or(1).max(1);
                    (row_lines as u16).min(max_item_height)
                }
            }
            ContentItem::Details { content_lines, id, .. } => {
                let is_open = details_states.get(id).copied().unwrap_or(false);
                if is_open {
                    1 + content_lines.len() as u16
                } else {
                    1u16
                }
            }
            ContentItem::FrontmatterLine { .. } => 1u16,
            ContentItem::FrontmatterDelimiter { .. } => 1u16,
            ContentItem::TagBadges { .. } => 2u16, // 1 line padding + 1 line for tags
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
            if !app.is_content_item_visible(i) {
                continue;
            }
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
                if !app.is_content_item_visible(i) {
                    continue;
                }
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
                if !app.is_content_item_visible(i) {
                    continue;
                }
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
            if !app.is_content_item_visible(i) {
                continue;
            }
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
                if !app.is_content_item_visible(i) {
                    continue;
                }
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
        // Skip items hidden by folded headings
        if !app.is_content_item_visible(i) {
            continue;
        }
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

    // Pre-compute code block highlights for proper syntax state tracking
    let code_block_highlights: std::collections::HashMap<usize, Vec<Span<'static>>> = {
        app.ensure_highlighter();
        let highlighter = app.get_highlighter();
        let mut highlights = std::collections::HashMap::new();

        if let Some(hl) = highlighter {
            let mut block_start: Option<(usize, String)> = None; 

            for (i, item) in app.content_items.iter().enumerate() {
                match item {
                    ContentItem::CodeFence(lang) => {
                        if let Some((start_idx, block_lang)) = block_start.take() {
                            let mut lines: Vec<(usize, String)> = Vec::new();
                            for j in (start_idx + 1)..i {
                                if let ContentItem::CodeLine(line) = &app.content_items[j] {
                                    lines.push((j, expand_tabs(line)));
                                }
                            }

                            if !lines.is_empty() && !block_lang.is_empty() {
                                let block_content: String = lines.iter()
                                    .map(|(_, l)| l.as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n");

                                let highlighted = hl.highlight_block(&block_content, &block_lang);
                                for (line_idx, (item_idx, _)) in lines.iter().enumerate() {
                                    if let Some(spans) = highlighted.get(line_idx) {
                                        highlights.insert(*item_idx, spans.clone());
                                    }
                                }
                            }
                        } else {
                            block_start = Some((i, lang.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }

        highlights
    };

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }
        let is_cursor_line = item_idx == cursor && is_focused;
        let is_hovered = app.mouse_hover_item == Some(item_idx);

        // Clone the item data to avoid borrow conflicts
        let item_clone = app.content_items[item_idx].clone();

        match item_clone {
            ContentItem::TextLine(ref line) => {
                let has_regular_link = app.item_link_at(item_idx).is_some();
                let has_wiki_link = !app.item_wiki_links_at(item_idx).is_empty();
                let has_link = (is_cursor_line || is_hovered) && (has_regular_link || has_wiki_link);
                let selected_link = if is_cursor_line { app.selected_link_index } else { 0 };
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                // Get fold state for H1-H3 headings
                let fold_state = if app.is_heading_at(item_idx) {
                    Some(app.is_heading_folded(item_idx))
                } else {
                    None
                };
                render_content_line(f, &app.theme, line, chunks[chunk_idx], is_cursor_line, has_link, selected_link, Some(wiki_validator), fold_state);
                if !skip_images {
                    let inline_images = extract_inline_images(line);
                    if !inline_images.is_empty() {
                        let text_height = calc_wrapped_height(line, 4);
                        render_inline_thumbnails(f, app, &inline_images, chunks[chunk_idx], text_height);
                    }
                }
            }
            ContentItem::Image(path) => {
                if !skip_images {
                    render_inline_image_with_cursor(f, app, &path, chunks[chunk_idx], is_cursor_line, is_hovered);
                }
            }
            ContentItem::CodeLine(line) => {
                let highlighted_spans = code_block_highlights.get(&item_idx).cloned();
                render_code_line(f, &app.theme, &line, highlighted_spans, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeFence(lang) => {
                render_code_fence(f, &app.theme, &lang, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TaskItem { ref text, checked, .. } => {
                let selected_link = if is_cursor_line { app.selected_link_index } else { 0 };
                let has_links = !app.item_wiki_links_at(item_idx).is_empty() || !app.item_links_at(item_idx).is_empty();
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                render_task_item(f, &app.theme, text, checked, chunks[chunk_idx], is_cursor_line, selected_link, has_links, Some(wiki_validator));
                if !skip_images {
                    let inline_images = extract_inline_images(text);
                    if !inline_images.is_empty() {
                        let text_height = calc_wrapped_height(text, 6);
                        render_inline_thumbnails(f, app, &inline_images, chunks[chunk_idx], text_height);
                    }
                }
            }
            ContentItem::TableRow { cells, is_separator, is_header, column_widths, alignments } => {
                let has_link = !is_separator
                    && (is_cursor_line || is_hovered)
                    && !app.item_links_at(item_idx).is_empty();
                render_table_row(f, &app.theme, &cells, is_separator, is_header, &column_widths, &alignments, chunks[chunk_idx], is_cursor_line, has_link);
            }
            ContentItem::Details { summary, content_lines, id } => {
                let is_open = app.details_open_states.get(&id).copied().unwrap_or(false);
                render_details(f, &app.theme, &summary, &content_lines, is_open, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::FrontmatterDelimiter { .. } => {
                render_frontmatter_delimiter(f, &app.theme, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::FrontmatterLine { ref key, ref value, .. } => {
                render_frontmatter_line(f, &app.theme, key, value, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TagBadges { ref tags, ref date } => {
                render_tag_badges_inline(f, &app.theme, tags, date.as_deref(), chunks[chunk_idx], is_cursor_line);
            }
        }
    }

    if app.buffer_search.active && !app.buffer_search.matches.is_empty() {
        apply_content_search_highlights(f, app, &visible_indices, &chunks);
    }
}

/// Visible width of a table cell after inline markdown shrinks
/// (e.g. `[label](url)` -> `label`). Measured in *display columns*, so wide
/// characters (CJK, emoji) contribute their full terminal width — not just 1
/// char each. Markdown markers stripped by `calc_formatting_shrinkage` are all
/// ASCII (1 col each), so subtracting their char-count from the display width
/// gives the visible-content's display width.
pub(crate) fn cell_visible_width(cell: &str) -> usize {
    let display_width = UnicodeWidthStr::width(cell);
    let total_chars = cell.chars().count();
    let marker_chars = calc_formatting_shrinkage(cell, total_chars);
    display_width.saturating_sub(marker_chars)
}

/// Per-column minimum width when shrinking a wide table to fit the terminal.
const TABLE_COLUMN_MIN_WIDTH: usize = 8;

/// Given the "natural" width of each column (max content width) and the available
/// budget for content (= terminal area minus borders/padding), return capped widths
/// that sum to at most `available`. Shrinks the widest column(s) first so narrow
/// columns keep their full width whenever possible. Each column stays at or above
/// `TABLE_COLUMN_MIN_WIDTH` unless its natural width is already below that.
pub(crate) fn cap_column_widths(natural: &[usize], available: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = natural.to_vec();
    if widths.is_empty() {
        return widths;
    }
    loop {
        let total: usize = widths.iter().sum();
        if total <= available {
            return widths;
        }
        // Pick the widest column that can still shrink.
        let mut target: Option<usize> = None;
        let mut max_w: usize = 0;
        for (i, &w) in widths.iter().enumerate() {
            let floor = TABLE_COLUMN_MIN_WIDTH.min(natural[i]);
            if w > floor && w > max_w {
                max_w = w;
                target = Some(i);
            }
        }
        match target {
            Some(i) => widths[i] -= 1,
            None => return widths, // every column already at its floor; can't shrink further
        }
    }
}

/// Distribute a pre-parsed list of inline spans across visual lines of at most
/// `width` display columns each.
///
/// Original span structure is preserved — each span carries its own whitespace
/// (a plain-text span that reads `" then "` keeps its leading and trailing
/// space, so adjacent styled spans sit against punctuation without any injected
/// space). Plain-text spans can be broken at internal whitespace if needed;
/// styled spans (links, bold, italic, code, wiki) are atomic — they fit on one
/// line or start a new line, overflowing as a single span if wider than `width`.
///
/// Use this downstream of `parse_inline_formatting` so the parser stays the
/// single source of truth for what counts as a markdown construct:
/// ```ignore
/// let spans = parse_inline_formatting(cell, theme, None, None::<fn(&str) -> bool>);
/// let lines = distribute_spans_across_lines(spans, width, theme.content.text);
/// ```
///
/// The returned lines own their content (`Span<'static>`).
pub(crate) fn distribute_spans_across_lines(
    spans: Vec<Span<'_>>,
    width: usize,
    plain_text_color: ratatui::style::Color,
) -> Vec<Vec<Span<'static>>> {
    if width == 0 {
        let owned: Vec<Span<'static>> = spans
            .into_iter()
            .map(|s| Span::styled(s.content.into_owned(), s.style))
            .collect();
        return vec![owned];
    }

    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut current_visible: usize = 0;

    for span in spans {
        let style = span.style;
        let span_visible = UnicodeWidthStr::width(span.content.as_ref());
        let is_plain = is_plain_text_span(&style, plain_text_color);

        if !is_plain {
            // Atomic span: must stay together.
            if current_visible > 0 && current_visible + span_visible > width {
                lines.push(std::mem::take(&mut current));
                current_visible = 0;
            }
            current.push(Span::styled(span.content.into_owned(), style));
            current_visible += span_visible;
            continue;
        }

        // Plain-text span: may need breaking at internal whitespace.
        let mut rest: &str = span.content.as_ref();
        while !rest.is_empty() {
            // If we're at the start of a fresh line, discard leading whitespace
            // (lines shouldn't start with a space, unless the content IS just spaces).
            if current_visible == 0 {
                let trimmed = rest.trim_start();
                if trimmed.is_empty() {
                    break;
                }
                rest = trimmed;
            }

            let rest_visible = UnicodeWidthStr::width(rest);
            if current_visible + rest_visible <= width {
                // Whole remainder fits on current line.
                current.push(Span::styled(rest.to_string(), style));
                current_visible += rest_visible;
                break;
            }

            // Need to break within `rest`. Find the longest prefix that fits AND ends at
            // a whitespace boundary.
            let remaining_budget = width.saturating_sub(current_visible);
            let (head, tail) = split_plain_at_whitespace(rest, remaining_budget);

            if !head.is_empty() {
                current.push(Span::styled(head.to_string(), style));
                lines.push(std::mem::take(&mut current));
                current_visible = 0;
                rest = tail;
                continue;
            }

            // No whitespace break fits in the budget. If there's content on the current
            // line, flush it so the next iteration tries with a fresh full-width line.
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                current_visible = 0;
                continue;
            }

            // Empty line and no whitespace break — hard-break the first word at
            // display-width boundaries.
            let (forced_head, forced_tail) = take_width(rest, width);
            if forced_head.is_empty() {
                // Degenerate: push the first char and move on.
                let first_char = rest.chars().next().unwrap();
                let first_len = first_char.len_utf8();
                current.push(Span::styled(rest[..first_len].to_string(), style));
                current_visible += UnicodeWidthChar::width(first_char).unwrap_or(1);
                rest = &rest[first_len..];
            } else {
                lines.push(vec![Span::styled(forced_head.to_string(), style)]);
                rest = forced_tail;
            }
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Return `(head, tail)` where `head` is the longest prefix of `s` whose display
/// width does not exceed `max_width` AND which ends at a whitespace boundary.
/// `tail` has leading whitespace stripped. Returns `("", s)` if no such prefix
/// exists.
fn split_plain_at_whitespace(s: &str, max_width: usize) -> (&str, &str) {
    let mut best_end: Option<usize> = None;
    let mut width_before_pos: usize = 0;

    for (pos, ch) in s.char_indices() {
        if ch.is_whitespace() {
            if width_before_pos <= max_width {
                best_end = Some(pos);
            } else {
                break;
            }
        }
        width_before_pos += UnicodeWidthChar::width(ch).unwrap_or(0);
    }

    match best_end {
        Some(end) => (&s[..end], s[end..].trim_start()),
        None => ("", s),
    }
}

/// Spans emitted by `parse_inline_formatting` for ordinary text carry only the
/// default content colour (no modifiers, no background). Use that as the "is
/// this plain text?" fingerprint so we know which spans can be broken at
/// whitespace during wrapping.
fn is_plain_text_span(style: &Style, plain_color: ratatui::style::Color) -> bool {
    style.bg.is_none()
        && style.add_modifier.is_empty()
        && style.sub_modifier.is_empty()
        && (style.fg.is_none() || style.fg == Some(plain_color.into()))
}

/// Split a string into a `(head, tail)` pair where `head` has display width `<= width`.
/// Used by `wrap_cell` for hard-breaking over-width words.
fn take_width(s: &str, width: usize) -> (&str, &str) {
    let mut w = 0usize;
    for (i, ch) in s.char_indices() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(1);
        if w + cw > width {
            return (&s[..i], &s[i..]);
        }
        w += cw;
    }
    (s, "")
}

/// If `text[start..]` begins with a bare `http://` or `https://` URL, return the
/// byte length of the URL (trailing sentence punctuation stripped). Used for
/// GFM-style autolinking both in rendering and in the Enter-to-open path.
pub(crate) fn detect_bare_url_len(text: &str, start: usize) -> Option<usize> {
    let rest = match text.get(start..) {
        Some(s) => s,
        None => return None,
    };
    let scheme_len = if rest.starts_with("https://") {
        8
    } else if rest.starts_with("http://") {
        7
    } else {
        return None;
    };

    // Walk from the scheme end until we hit a terminator or the string end.
    let mut end = rest.len();
    for (idx, ch) in rest[scheme_len..].char_indices() {
        if ch.is_whitespace() || matches!(ch, ')' | ']' | '>' | '<' | '"' | '\'' | '|') {
            end = scheme_len + idx;
            break;
        }
    }

    // Strip trailing sentence punctuation so `https://x.test.` -> `https://x.test`.
    while end > scheme_len {
        let last = rest[..end].chars().last().unwrap();
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }

    if end > scheme_len {
        Some(end)
    } else {
        None
    }
}

/// Calculate how many characters are removed by inline formatting before a given position
/// This accounts for **bold**, *italic*, ~~strikethrough~~, `code`, [[wiki links]], and [markdown](links)
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
        if pos + 1 < chars.len() && chars[pos] == '~' && chars[pos + 1] == '~' {
            if let Some(end) = find_double_marker(&chars, pos + 2, '~') {
                if end < up_to_pos {
                    shrinkage += 4;
                } else if pos + 2 < up_to_pos {
                    shrinkage += 2;
                }
                pos = end + 2;
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
                    // Full `[label](url)` seen before up_to_pos: strips `[` + `](` + url + `)` = 4 + url_len.
                    shrinkage += url_len + 4;
                } else if bracket_end < up_to_pos {
                    shrinkage += 1;
                }
                pos = paren_end + 1;
                continue;
            }
        }
        // Bare URL: rendered 1:1 (no shrinkage), but skip so inner chars aren't reprocessed.
        if chars[pos] == 'h' {
            let byte_pos: usize = chars[..pos].iter().map(|c| c.len_utf8()).sum();
            if let Some(url_len) = detect_bare_url_len(text, byte_pos) {
                // `pos` is a char index, `url_len` is bytes — convert by counting chars in the slice.
                let url_char_count = text[byte_pos..byte_pos + url_len].chars().count();
                pos += url_char_count;
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
fn calc_table_adjusted_col(raw_col: usize, cells: &[String], column_widths: &[usize], alignments: &[crate::app::Alignment]) -> usize {
    use crate::app::Alignment;
    let mut rendered_pos = 3;
    let mut raw_pos = 0;

    for (cell_idx, cell) in cells.iter().enumerate() {
        let col_width = column_widths.get(cell_idx).copied().unwrap_or(3);
        if raw_pos == 0 {
            raw_pos = 1;
        }

        let raw_cell_start = raw_pos;

        let cell_char_len = cell.chars().count();
        let cell_display_width = cell.width();
        let raw_cell_end = raw_cell_start + cell_char_len + 3; // " content |"

        if raw_col >= raw_cell_start && raw_col < raw_cell_end {
            let char_offset_in_raw_cell = raw_col.saturating_sub(raw_cell_start + 1); // +1 for leading space
            // Convert character offset to display width
            let display_offset: usize = cell.chars()
                .take(char_offset_in_raw_cell.min(cell_char_len))
                .map(|c| c.width().unwrap_or(1))
                .sum();
            let pad = col_width.saturating_sub(cell_display_width);
            let alignment = alignments.get(cell_idx).copied().unwrap_or(Alignment::Left);
            let content_padding = match alignment {
                Alignment::Left => 0,
                Alignment::Right => pad,
                Alignment::Center => pad / 2,
            };
            let rendered_content_start = rendered_pos + 1 + content_padding; // +1 for leading space

            return rendered_content_start + display_offset;
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
    let lines = app.editor.lines();

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }

        let source_line = app.content_item_source_lines.get(item_idx).copied().unwrap_or(usize::MAX);
        if source_line == usize::MAX {
            continue;
        }

        let raw_line = lines.get(source_line).copied().unwrap_or("");

        for (match_idx, m) in app.buffer_search.matches.iter().enumerate() {
            if m.row == source_line {
                let area = chunks[chunk_idx];
                let is_current = match_idx == current_match_idx;
                let highlight_color = if is_current {
                    theme.search.match_current
                } else {
                    theme.search.match_highlight
                };

                // Calculate the rendered column position based on content type
                // Use display width for CJK character support
                let adjusted_col = match &app.content_items.get(item_idx) {
                    Some(ContentItem::TableRow { cells, column_widths, alignments, is_separator, .. }) => {
                        if *is_separator {
                            continue;
                        }
                        calc_table_adjusted_col(m.start_col, cells, column_widths, alignments)
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
                        // Calculate display width of content before the match
                        let display_col = content_text.chars()
                            .take(content_start_col.saturating_sub(formatting_shrinkage))
                            .map(|c| c.width().unwrap_or(1))
                            .sum::<usize>();
                        rendered_prefix_len + display_col
                    }
                    Some(ContentItem::CodeLine(code)) => {
                        // Calculate display width of code before the match
                        let display_col: usize = code.chars()
                            .take(m.start_col)
                            .map(|c| c.width().unwrap_or(1))
                            .sum();
                        4 + display_col
                    }
                    Some(ContentItem::TaskItem { text, .. }) => {
                        if m.start_col < 6 {
                            continue;
                        }
                        let content_start_col = m.start_col - 6;
                        let formatting_shrinkage = calc_formatting_shrinkage(text, content_start_col);
                        let display_col: usize = text.chars()
                            .take(content_start_col.saturating_sub(formatting_shrinkage))
                            .map(|c| c.width().unwrap_or(1))
                            .sum();
                        6 + display_col
                    }
                    _ => {
                        // Calculate display width of raw line before the match
                        let display_col: usize = raw_line.chars()
                            .take(m.start_col)
                            .map(|c| c.width().unwrap_or(1))
                            .sum();
                        2 + display_col
                    }
                };

                let start_x = area.x + adjusted_col as u16;
                // Calculate display width of matched text
                let match_display_width: usize = raw_line.chars()
                    .skip(m.start_col)
                    .take(m.end_col - m.start_col)
                    .map(|c| c.width().unwrap_or(1))
                    .sum();

                for offset in 0..match_display_width {
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
        // Bare URL autolink (http:// or https://). Must run before the char-dispatch branches
        // so `h` starting a URL is recognised and consumed as a single link span.
        if c == 'h' {
            if let Some(url_len) = detect_bare_url_len(text, i) {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
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
                spans.push(Span::styled(&text[i..i + url_len], style));
                link_index += 1;
                // Advance the char iterator past the URL. Count chars (not bytes) in case
                // the URL contains non-ASCII (e.g. IDN host).
                let url_chars = text[i..i + url_len].chars().count();
                for _ in 1..url_chars {
                    chars.next();
                }
                current_start = i + url_len;
                continue;
            }
        }

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

        // Check for ~~strikethrough~~
        if c == '~' {
            if let Some(&(_, '~')) = chars.peek() {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
                chars.next(); 
                let strike_start = i + 2;
                let mut strike_end = None;

                while let Some((j, ch)) = chars.next() {
                    if ch == '~' {
                        if let Some(&(_, '~')) = chars.peek() {
                            strike_end = Some(j);
                            chars.next(); 
                            break;
                        }
                    }
                }

                if let Some(end) = strike_end {
                    spans.push(Span::styled(
                        &text[strike_start..end],
                        Style::default().fg(content_theme.text).add_modifier(Modifier::CROSSED_OUT),
                    ));
                    current_start = end + 2;
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

        // Check for !![image](url) - double-bang (text-only, no preview)
        // Must check before single-bang to avoid partial match
        if c == '!' {
            let remaining = &text[i..];

            if remaining.starts_with("!![") {
                if let Some(bracket_end) = remaining[2..].find("](") {
                    let after_bracket = &remaining[2 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        if i > current_start {
                            spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                        }

                        let alt_text = &remaining[3..2 + bracket_end];
                        let image_url = &after_bracket[..paren_end];

                        // Display as text link without [img:] prefix for cleaner look
                        let display_text = if alt_text.is_empty() {
                            image_url.to_string()
                        } else {
                            alt_text.to_string()
                        };

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

                        spans.push(Span::styled(display_text, style));
                        link_index += 1;

                        let total_link_len = 2 + bracket_end + 2 + paren_end + 1; // !![alt](url)
                        for _ in 0..total_link_len - 1 {
                            chars.next();
                        }
                        current_start = i + total_link_len;
                        continue;
                    }
                }
            }

            if remaining.starts_with("![") {
                if let Some(bracket_end) = remaining[1..].find("](") {
                    let after_bracket = &remaining[1 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        if i > current_start {
                            spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                        }

                        link_index += 1;

                        let total_link_len = 1 + bracket_end + 2 + paren_end + 1;
                        for _ in 0..total_link_len - 1 {
                            chars.next();
                        }
                        current_start = i + total_link_len;
                        continue;
                    }
                }
            }
        }

        // Check for [[wiki link]]
        if c == '[' {
            let remaining = &text[i..];
            if remaining.starts_with("[[") {
                if let Some(close_pos) = remaining[2..].find("]]") {
                    let raw_content = &remaining[2..2 + close_pos];
                    if !raw_content.is_empty() && !raw_content.contains('[') && !raw_content.contains(']') {
                        if i > current_start {
                            spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                        }

                        let (content, display_text) = if let Some(pipe_pos) = raw_content.find('|') {
                            (&raw_content[..pipe_pos], Some(&raw_content[pipe_pos + 1..]))
                        } else {
                            (raw_content, None)
                        };
                        let target = if let Some(hash_pos) = content.find('#') {
                            &content[..hash_pos]
                        } else {
                            content
                        };
                        let shown_text = display_text.unwrap_or(raw_content);

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

                        spans.push(Span::styled(shown_text.to_string(), style));
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
                    let link_url = &after_bracket[..paren_end];

                    let display_text = if link_text.is_empty() {
                        link_url
                    } else {
                        link_text
                    };

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

                    spans.push(Span::styled(display_text.to_string(), style));
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

/// Represents a word segment with its style for word-based wrapping
struct StyledWord {
    text: String,
    style: Style,
    width: usize,
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

    // Extract words from spans while preserving styles
    let mut styled_words: Vec<StyledWord> = Vec::new();
    for span in content_spans {
        let span_style = span.style;
        let span_text = span.content.to_string();

        // Split by whitespace while tracking positions
        let mut last_end = 0;
        let mut chars_iter = span_text.char_indices().peekable();

        while let Some((i, c)) = chars_iter.next() {
            if c.is_whitespace() {
                // Add word before this whitespace if any
                if i > last_end {
                    let word = &span_text[last_end..i];
                    styled_words.push(StyledWord {
                        text: word.to_string(),
                        style: span_style,
                        width: display_width(word),
                    });
                }
                // Add whitespace as its own "word" to preserve spacing
                let ws_start = i;
                let mut ws_end = i + c.len_utf8();
                // Consume consecutive whitespace
                while let Some(&(next_i, next_c)) = chars_iter.peek() {
                    if next_c.is_whitespace() {
                        ws_end = next_i + next_c.len_utf8();
                        chars_iter.next();
                    } else {
                        break;
                    }
                }
                styled_words.push(StyledWord {
                    text: span_text[ws_start..ws_end].to_string(),
                    style: span_style,
                    width: display_width(&span_text[ws_start..ws_end]),
                });
                last_end = ws_end;
            }
        }
        // Add remaining word after last whitespace
        if last_end < span_text.len() {
            let word = &span_text[last_end..];
            styled_words.push(StyledWord {
                text: word.to_string(),
                style: span_style,
                width: display_width(word),
            });
        }
    }

    let continuation_indent = " ".repeat(prefix_width);
    let continuation_available = available_width.saturating_sub(prefix_width);
    let mut lines: Vec<Line<'a>> = Vec::new();
    let mut current_line_spans: Vec<Span<'a>> = Vec::new();
    let mut current_line_width = 0usize;
    let mut is_first_line = true;

    for styled_word in styled_words {
        let max_width = if is_first_line { first_line_available } else { continuation_available };
        let is_whitespace = styled_word.text.chars().all(|c| c.is_whitespace());

        // Skip leading whitespace on continuation lines
        if current_line_width == 0 && is_whitespace && !is_first_line {
            continue;
        }

        // Check if word fits on current line
        if current_line_width + styled_word.width <= max_width {
            current_line_spans.push(Span::styled(styled_word.text, styled_word.style));
            current_line_width += styled_word.width;
        } else if styled_word.width > max_width && !is_whitespace {
            // Word is too long for any line - need to break it character by character
            let mut remaining = styled_word.text.as_str();
            let style = styled_word.style;

            while !remaining.is_empty() {
                let line_max = if is_first_line { first_line_available } else { continuation_available };
                let available_in_line = line_max.saturating_sub(current_line_width);

                if available_in_line == 0 {
                    // Flush current line
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

                // Find how many characters fit
                let mut fit_chars = 0;
                let mut fit_width = 0;
                for ch in remaining.chars() {
                    let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                    if fit_width + ch_width > available_in_line {
                        break;
                    }
                    fit_chars += ch.len_utf8();
                    fit_width += ch_width;
                }

                if fit_chars == 0 {
                    let ch = remaining.chars().next().unwrap();
                    fit_chars = ch.len_utf8();
                    fit_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
                }

                let (fitting, rest) = remaining.split_at(fit_chars);
                current_line_spans.push(Span::styled(fitting.to_string(), style));
                current_line_width += fit_width;
                remaining = rest;

                // If there's more, flush line
                if !remaining.is_empty() {
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
        } else if !is_whitespace {
            // Word doesn't fit on current line - start a new line
            // First, flush current line (but skip trailing whitespace)
            // Remove trailing whitespace spans from current line
            while let Some(last_span) = current_line_spans.last() {
                if last_span.content.chars().all(|c| c.is_whitespace()) {
                    current_line_spans.pop();
                } else {
                    break;
                }
            }

            if !current_line_spans.is_empty() || is_first_line {
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
            }

            current_line_spans.clear();
            current_line_spans.push(Span::styled(styled_word.text, styled_word.style));
            current_line_width = styled_word.width;
        }
    }

    while let Some(last_span) = current_line_spans.last() {
        if last_span.content.chars().all(|c| c.is_whitespace()) {
            current_line_spans.pop();
        } else {
            break;
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
    fold_state: Option<bool>,  // None = not foldable, Some(true) = folded, Some(false) = expanded
) where
    F: Fn(&str) -> bool,
{
    let line = &normalize_whitespace(line);
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding

    // Fold indicator for H1-H3 headings
    let fold_indicator = |is_folded: Option<bool>, color: ratatui::style::Color| -> Span {
        match is_folded {
            Some(true) => Span::styled("▶ ", Style::default().fg(color)),   // Folded
            Some(false) => Span::styled("▼ ", Style::default().fg(color)),  // Expanded
            None => Span::styled("  ", Style::default()),                    // Not foldable
        }
    };

    // Check headings from most specific (######) to least specific (#)
    let content_theme = &theme.content;
    let styled_line = if line.starts_with("###### ") {
        // H6: Smallest, italic, subtle (not foldable)
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
        // H5: Small, muted color (not foldable)
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
        // H4: Small prefix (not foldable)
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
        // H3: Medium prefix (foldable)
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            fold_indicator(fold_state, content_theme.heading3),
            Span::styled(
                line.trim_start_matches("### "),
                Style::default()
                    .fg(content_theme.heading3)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("## ") {
        // H2: Larger prefix (foldable)
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            fold_indicator(fold_state, content_theme.heading2),
            Span::styled(
                line.trim_start_matches("## "),
                Style::default()
                    .fg(content_theme.heading2)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("# ") {
        // H1: Largest, most prominent (foldable)
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            fold_indicator(fold_state, content_theme.heading1),
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
        // Blockquote - with inline formatting support
        let selected = if is_cursor { Some(selected_link) } else { None };
        let content = line.trim_start_matches("> ");
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("┃ ", Style::default().fg(content_theme.blockquote)),
        ];
        let formatted = parse_inline_formatting(content, theme, selected, wiki_link_validator);
        for span in formatted {
            let mut style = span.style;
            if style.fg.is_none() || style.fg == Some(content_theme.text.into()) {
                style = style.fg(content_theme.blockquote).add_modifier(Modifier::ITALIC);
            }
            spans.push(Span::styled(span.content, style));
        }
        Line::from(spans)
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
    line: &str,
    highlighted_spans: Option<Vec<Span<'static>>>,
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

    if let Some(hl_spans) = highlighted_spans {
        spans.extend(hl_spans);
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

fn render_task_item<F>(
    f: &mut Frame,
    theme: &Theme,
    text: &str,
    checked: bool,
    area: Rect,
    is_cursor: bool,
    selected_link: usize,
    has_links: bool,
    wiki_link_validator: Option<F>,
) where
    F: Fn(&str) -> bool,
{
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let checkbox_selected = is_cursor && has_links && selected_link == 0;
    let checkbox_color = if checkbox_selected {
        theme.warning 
    } else if checked {
        theme.success
    } else {
        theme.secondary
    };

    let expanded_text = expand_tabs(text);
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding

    let link_selected = if is_cursor && has_links && selected_link > 0 {
        Some(selected_link - 1)
    } else if is_cursor && !has_links {
        Some(selected_link)
    } else {
        None
    };
    let mut text_spans = parse_inline_formatting(&expanded_text, theme, link_selected, wiki_link_validator);
    if checked {
        text_spans = text_spans
            .into_iter()
            .map(|span| {
                let mut style = span.style;
                style = style.fg(theme.muted).add_modifier(Modifier::CROSSED_OUT);
                Span::styled(span.content, style)
            })
            .collect();
    }
    let checkbox_style = if checkbox_selected {
        Style::default()
            .fg(theme.background)
            .bg(theme.warning)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(checkbox_color).add_modifier(Modifier::BOLD)
    };

    let bracket_style = if checkbox_selected {
        Style::default().fg(theme.background).bg(theme.warning)
    } else {
        Style::default().fg(checkbox_color)
    };

    let mut spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        Span::styled("[", bracket_style),
        Span::styled(if checked { "x" } else { " " }, checkbox_style),
        Span::styled("]", bracket_style),
        Span::styled(" ", Style::default()),
    ];
    spans.extend(text_spans);

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
    natural_widths: &[usize],
    alignments: &[crate::app::Alignment],
    area: Rect,
    is_cursor: bool,
    has_link: bool,
) {
    let border_color = theme.border;
    let row_bg = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    // Cap widths against the row's available render width.
    // Row overhead: "  " (2) + leading │ (1) + per cell " content " (+2) + per-cell │ (N-1 between + 1 trailing) = 3 + 3N.
    let n = natural_widths.len();
    let overhead = 3 + 3 * n;
    let budget = (area.width as usize).saturating_sub(overhead);
    let widths = cap_column_widths(natural_widths, budget);

    if is_separator {
        // Separator is always a single line.
        let mut spans = vec![
            Span::styled(if is_cursor { "▶ " } else { "  " }, Style::default().fg(theme.warning)),
            Span::styled("│", Style::default().fg(border_color)),
        ];
        for (i, &width) in widths.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("┼", Style::default().fg(border_color)));
            }
            let dashes = "─".repeat(width + 2);
            spans.push(Span::styled(dashes, Style::default().fg(border_color)));
        }
        spans.push(Span::styled("│", Style::default().fg(border_color)));
        let line_area = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
        let paragraph = Paragraph::new(Line::from(spans)).style(row_bg);
        f.render_widget(paragraph, line_area);
        return;
    }

    let text_color = theme.content.text;

    // Parse each cell as inline markdown ONCE, then distribute the resulting spans
    // across visual lines. Parsing the whole cell keeps `parse_inline_formatting` as
    // the single source of truth for what counts as a construct — multi-word atoms
    // like `**warp decode**` or `[Top 5 Things](url)` are recognised correctly, no
    // matter how the wrap boundary falls.
    let per_cell_lines: Vec<Vec<Vec<Span<'static>>>> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let w = widths.get(i).copied().unwrap_or(0);
            let expanded = expand_tabs(c);
            let spans = parse_inline_formatting::<fn(&str) -> bool>(&expanded, theme, None, None);
            distribute_spans_across_lines(spans, w, text_color)
        })
        .collect();
    let row_height = per_cell_lines
        .iter()
        .map(|lines| lines.len())
        .max()
        .unwrap_or(1)
        .max(1);

    let default_style = if is_header {
        Style::default().fg(theme.info).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.foreground)
    };

    for line_idx in 0..row_height {
        // Cursor indicator shows only on the first visual line of the row.
        let cursor_indicator = if is_cursor && line_idx == 0 { "▶ " } else { "  " };
        let mut spans: Vec<Span<'static>> = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled("│", Style::default().fg(border_color)),
        ];

        for (i, cell_lines) in per_cell_lines.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("│", Style::default().fg(border_color)));
            }
            let line_spans_slice: &[Span<'static>] = cell_lines
                .get(line_idx)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let width = widths.get(i).copied().unwrap_or(0);
            let visible: usize = line_spans_slice
                .iter()
                .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                .sum();
            let pad = width.saturating_sub(visible);
            let alignment = alignments.get(i).copied().unwrap_or(crate::app::Alignment::Left);
            let (left_pad, right_pad) = match alignment {
                crate::app::Alignment::Left => (0, pad),
                crate::app::Alignment::Right => (pad, 0),
                crate::app::Alignment::Center => (pad / 2, pad - pad / 2),
            };

            spans.push(Span::styled(format!(" {}", " ".repeat(left_pad)), default_style));

            for sp in line_spans_slice.iter().cloned() {
                let style = if is_plain_text_span(&sp.style, text_color) {
                    default_style
                } else {
                    sp.style
                };
                spans.push(Span::styled(sp.content, style));
            }

            spans.push(Span::styled(format!("{} ", " ".repeat(right_pad)), default_style));
        }

        spans.push(Span::styled("│", Style::default().fg(border_color)));
        // "Open ↗" hint only on the first line, same as the cursor indicator.
        if has_link && line_idx == 0 {
            spans.push(Span::styled(" Open ↗", Style::default().fg(theme.content.link)));
        }

        if (area.y + line_idx as u16) >= area.y + area.height {
            break;
        }
        let line_area = Rect {
            x: area.x,
            y: area.y + line_idx as u16,
            width: area.width,
            height: 1,
        };
        let paragraph = Paragraph::new(Line::from(spans)).style(row_bg);
        f.render_widget(paragraph, line_area);
    }
}

fn render_inline_image_with_cursor(f: &mut Frame, app: &mut App, path: &str, area: Rect, is_cursor: bool, is_hovered: bool) {
    let is_remote = path.starts_with("http://") || path.starts_with("https://");
    let is_pending = is_remote && app.is_image_pending(path);

    let resolved_path = app.resolve_image_path(path);
    let resolved_path_str = resolved_path.as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let is_cached = app.is_image_cached(&resolved_path_str);

    // Check if we need to load a new image
    let need_load = match &app.current_image {
        Some(state) => state.path != resolved_path_str,
        None => true,
    };

    if need_load {
        // Load image from cache, disk, or trigger async fetch for remote
        let img = if let Some(img) = app.get_cached_image(&resolved_path_str) {
            Some(img)
        } else if is_remote {
            if !is_pending {
                app.start_remote_image_fetch(path);
            }
            None
        } else if let Some(ref resolved) = resolved_path {
            if let Ok(img) = image::open(resolved) {
                app.cache_image(&resolved_path_str, img);
                app.get_cached_image(&resolved_path_str)
            } else {
                None
            }
        } else {
            None
        };

        if let (Some(img), Some(picker)) = (img, &mut app.picker) {
            let protocol = picker.new_resize_protocol(img);
            app.current_image = Some(ImageState {
                image: protocol,
                path: resolved_path_str.clone(),
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
        " Loading... ".to_string()
    } else if show_hint {
        " Open ↗ ".to_string()
    } else {
        "".to_string()
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

    if is_pending || (is_remote && !is_cached && app.current_image.as_ref().map(|s| s.path != resolved_path_str).unwrap_or(true)) {
        let loading = Paragraph::new("  Loading remote image...")
            .style(Style::default().fg(theme.secondary).add_modifier(Modifier::ITALIC));
        f.render_widget(loading, inner_area);
        return;
    }

    if let Some(state) = &mut app.current_image {
        if state.path == resolved_path_str {
            let image_widget = StatefulImage::new();
            f.render_stateful_widget(image_widget, inner_area, &mut state.image);
        }
    } else if !is_remote {
        let placeholder = Paragraph::new("  [Image not found]")
            .style(Style::default().fg(theme.error).add_modifier(Modifier::ITALIC));
        f.render_widget(placeholder, inner_area);
    }
}

/// Render inline image thumbnails below text content
/// Returns the number of thumbnail rows rendered
fn render_inline_thumbnails(
    f: &mut Frame,
    app: &mut App,
    images: &[String],
    area: Rect,
    text_height: u16,
) -> u16 {
    if images.is_empty() || app.picker.is_none() {
        return 0;
    }
    let secondary_color = app.theme.secondary;
    let error_color = app.theme.error;
    let mut y_offset = text_height;

    for path in images {
        if y_offset + INLINE_THUMBNAIL_HEIGHT > area.height {
            break;
        }

        let thumb_area = Rect {
            x: area.x + 2,
            y: area.y + y_offset,
            width: area.width.saturating_sub(4).min(40), 
            height: INLINE_THUMBNAIL_HEIGHT,
        };
        let is_remote = path.starts_with("http://") || path.starts_with("https://");
        let resolved_path = app.resolve_image_path(path);
        let resolved_path_str = resolved_path.as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());

        let is_pending = is_remote && app.is_image_pending(path);
        let img = if let Some(img) = app.get_cached_image(&resolved_path_str) {
            Some(img)
        } else if is_remote {
            if !is_pending {
                app.start_remote_image_fetch(path);
            }
            None
        } else if let Some(ref resolved) = resolved_path {
            if let Ok(img) = image::open(resolved) {
                app.cache_image(&resolved_path_str, img);
                app.get_cached_image(&resolved_path_str)
            } else {
                None
            }
        } else {
            None
        };
        if let (Some(img), Some(picker)) = (img, &mut app.picker) {
            let protocol = picker.new_resize_protocol(img);
            let mut thumb_state = ImageState {
                image: protocol,
                path: resolved_path_str.clone(),
            };
            let image_widget = StatefulImage::new();
            f.render_stateful_widget(image_widget, thumb_area, &mut thumb_state.image);
        } else if is_pending {
            let loading = Paragraph::new("  ⏳ Loading...")
                .style(Style::default().fg(secondary_color).add_modifier(Modifier::ITALIC));
            f.render_widget(loading, thumb_area);
        } else if !is_remote && resolved_path.is_none() {
            let not_found = Paragraph::new("  ❌ Not found")
                .style(Style::default().fg(error_color).add_modifier(Modifier::ITALIC));
            f.render_widget(not_found, thumb_area);
        }

        y_offset += INLINE_THUMBNAIL_HEIGHT;
    }

    y_offset - text_height
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

fn render_frontmatter_delimiter(
    f: &mut Frame,
    theme: &Theme,
    area: Rect,
    is_cursor: bool,
) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let spans = vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
        Span::styled("---", Style::default().fg(theme.content.frontmatter)),
    ];

    let style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(Line::from(spans)).style(style);
    f.render_widget(paragraph, area);
}

/// Render tag badges as part of scrollable content (not fixed at top)
fn render_tag_badges_inline(
    f: &mut Frame,
    theme: &Theme,
    tags: &[String],
    date: Option<&str>,
    area: Rect,
    is_cursor: bool,
) {
    if area.height == 0 {
        return;
    }

    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let mut spans: Vec<Span> = vec![Span::styled(
        cursor_indicator,
        Style::default().fg(theme.warning),
    )];

    for (i, tag) in tags.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", Style::default()));
        }
        spans.push(Span::styled(
            format!(" {} ", tag),
            Style::default()
                .fg(theme.content.tag)
                .bg(theme.content.tag_background),
        ));
    }
    if let Some(d) = date {
        if !tags.is_empty() {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(d, Style::default().fg(theme.content.frontmatter)));
    }

    let y_offset = if area.height >= 2 { 1 } else { 0 };
    let tag_area = Rect {
        x: area.x,
        y: area.y + y_offset,
        width: area.width,
        height: 1,
    };

    let style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(Line::from(spans)).style(style);
    f.render_widget(paragraph, tag_area);
}

fn render_zen_content_status_line(f: &mut Frame, theme: &Theme, area: Rect) {
    let status_line = Line::from(vec![
        Span::styled(
            " FLOAT ",
            Style::default()
                .fg(theme.background)
                .bg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let paragraph = Paragraph::new(status_line);
    f.render_widget(paragraph, area);
}

fn render_frontmatter_line(
    f: &mut Frame,
    theme: &Theme,
    key: &str,
    value: &str,
    area: Rect,
    is_cursor: bool,
) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let spans = if key.is_empty() {
        // Continuation line (no key)
        vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled(value, Style::default().fg(theme.content.frontmatter)),
        ]
    } else {
        vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.warning)),
            Span::styled(format!("{}: ", key), Style::default().fg(theme.info)),
            Span::styled(value, Style::default().fg(theme.content.frontmatter)),
        ]
    };

    let style = if is_cursor {
        Style::default().bg(theme.selection)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(Line::from(spans)).style(style);
    f.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_visible_width_plain_text() {
        assert_eq!(cell_visible_width("Plain URL"), 9);
    }

    #[test]
    fn cell_visible_width_strips_markdown_link() {
        // `[label](url)` -> `label`. A prior off-by-one counted the closing `)` toward visible.
        assert_eq!(cell_visible_width("[Top 5](https://x.test)"), 5);
    }

    #[test]
    fn cell_visible_width_strips_bold_italic_code() {
        assert_eq!(cell_visible_width("**bold text**"), 9);
        assert_eq!(cell_visible_width("*em*"), 2);
        assert_eq!(cell_visible_width("`code`"), 4);
    }

    #[test]
    fn cell_visible_width_mixed_text_and_link() {
        // "one [a](u) two" renders as "one a two" = 9 visible chars.
        assert_eq!(cell_visible_width("one [a](https://u.test) two"), 9);
    }

    #[test]
    fn cell_visible_width_multiple_links_same_cell() {
        // Pins the off-by-one fix: before the fix, each link inflated visible by 1,
        // so a 2-link cell miscounted by 2 and tables with uneven link counts
        // misaligned their borders.
        // "[a](u1) [b](u2)" -> "a b" = 3 visible chars.
        assert_eq!(cell_visible_width("[a](https://u1.test) [b](https://u2.test)"), 3);
    }

    #[test]
    fn detect_bare_url_basic() {
        assert_eq!(detect_bare_url_len("see https://example.com now", 4), Some(19));
        assert_eq!(detect_bare_url_len("http://a.test", 0), Some(13));
    }

    #[test]
    fn detect_bare_url_strips_trailing_punctuation() {
        // GFM: the trailing `.` should not be part of the URL.
        assert_eq!(detect_bare_url_len("visit https://example.com.", 6), Some(19));
    }

    #[test]
    fn detect_bare_url_stops_at_delimiters() {
        // "https://x.test" = 14 chars; the `)` / `>` terminator is not included.
        assert_eq!(detect_bare_url_len("(https://x.test)", 1), Some(14));
        assert_eq!(detect_bare_url_len("<https://x.test>", 1), Some(14));
    }

    #[test]
    fn detect_bare_url_no_match_returns_none() {
        assert_eq!(detect_bare_url_len("nothing here", 0), None);
        assert_eq!(detect_bare_url_len("http:/broken", 0), None);  // missing second slash
    }

    #[test]
    fn cell_visible_width_counts_bare_url_one_to_one() {
        // Bare URL is not shrunk — visible width equals its character count.
        assert_eq!(cell_visible_width("visit https://x.test"), 20);
    }

    #[test]
    fn cell_visible_width_counts_emoji_as_two_columns() {
        // 🟡 is one char but displays as 2 columns in a terminal. The char-count
        // version under-counted: 1 (emoji) + 11 ("In-Progress") = 12, so column
        // widths were reserved at 12 cols while the cell actually renders in 13.
        // That off-by-one forced an unnecessary wrap.
        assert_eq!(cell_visible_width("🟡In-Progress"), 13);
        assert_eq!(cell_visible_width("🟡"), 2);
        // ASCII control: still matches char count.
        assert_eq!(cell_visible_width("In-Progress"), 11);
    }

    #[test]
    fn cap_column_widths_leaves_narrow_columns_alone() {
        // Natural sum = 7 + 12 + 500 = 519; budget = 107 (like a ~120-col terminal).
        // Expect narrow columns untouched, Description shrunk to fill what's left.
        let natural = vec![7, 12, 500];
        let capped = cap_column_widths(&natural, 107);
        assert_eq!(capped[0], 7);
        assert_eq!(capped[1], 12);
        assert_eq!(capped[0] + capped[1] + capped[2], 107);
    }

    #[test]
    fn cap_column_widths_no_shrink_when_it_fits() {
        let natural = vec![4, 6, 10];
        assert_eq!(cap_column_widths(&natural, 50), vec![4, 6, 10]);
    }

    #[test]
    fn cap_column_widths_respects_min_floor() {
        // If available is absurdly small, columns bottom out at TABLE_COLUMN_MIN_WIDTH
        // (unless their natural width is already below that — those stay at natural).
        let natural = vec![3, 50, 50]; // 3 is below the floor; leave it alone
        let capped = cap_column_widths(&natural, 5);
        assert_eq!(capped[0], 3);
        assert_eq!(capped[1], TABLE_COLUMN_MIN_WIDTH);
        assert_eq!(capped[2], TABLE_COLUMN_MIN_WIDTH);
    }

    // --- distribute_spans_across_lines ---
    // Tests use Style::default() for "plain" spans and a non-default modifier
    // (BOLD) as a proxy for any styled span (links/bold/code/etc.), matching
    // how `parse_inline_formatting` emits them.

    fn plain_color() -> ratatui::style::Color {
        ratatui::style::Color::Reset
    }

    fn plain(content: &'static str) -> Span<'static> {
        Span::styled(content.to_string(), Style::default())
    }

    fn atomic(content: &'static str) -> Span<'static> {
        // Any non-default style qualifies the span as "atomic" to our logic.
        Span::styled(content.to_string(), Style::default().add_modifier(Modifier::BOLD))
    }

    fn line_text(line: &[Span<'static>]) -> String {
        line.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn distribute_plain_text_wraps_at_word_boundary() {
        // "alpha beta gamma delta" at width 10: "alpha beta" = 10 fits, "gamma delta" = 11
        // does not, so "gamma" and "delta" each get their own line.
        let lines = distribute_spans_across_lines(
            vec![plain("alpha beta gamma delta")],
            10,
            plain_color(),
        );
        let texts: Vec<String> = lines.iter().map(|l| line_text(l)).collect();
        assert_eq!(
            texts,
            vec!["alpha beta".to_string(), "gamma".to_string(), "delta".to_string()]
        );
    }

    #[test]
    fn distribute_hard_breaks_over_wide_plain_word() {
        let lines = distribute_spans_across_lines(
            vec![plain("supercalifragilisticexpialidocious")],
            10,
            plain_color(),
        );
        assert!(lines.len() >= 4);
        for l in &lines {
            let width: usize = l.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
            assert!(width <= 10, "line {:?} exceeds width", line_text(l));
        }
    }

    #[test]
    fn distribute_keeps_atomic_span_on_one_line_even_when_wider_than_column() {
        // A styled span wider than the column is accepted as overflow — splitting its
        // content would corrupt the rendered markdown construct.
        let lines = distribute_spans_across_lines(vec![atomic("VeryLongStyledContent")], 10, plain_color());
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "VeryLongStyledContent");
    }

    #[test]
    fn distribute_packs_plain_then_atomic_on_same_line_when_it_fits() {
        // "see" (plain, 3) + "blog" (atomic, 4) -> "see blog" on one line, width 20.
        let lines = distribute_spans_across_lines(
            vec![plain("see "), atomic("blog")],
            20,
            plain_color(),
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "see blog");
    }

    #[test]
    fn distribute_breaks_to_new_line_when_atomic_would_overflow() {
        // "a short prefix " (plain, 15 incl. trailing space) + "XXXXXXX" atomic (7):
        // 15+7=22 > 18 budget, so atomic starts on a new line. The plain span's
        // trailing space is preserved on line 1 (invisible when rendered).
        let lines = distribute_spans_across_lines(
            vec![plain("a short prefix "), atomic("XXXXXXX")],
            18,
            plain_color(),
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]).trim_end(), "a short prefix");
        assert_eq!(line_text(&lines[1]), "XXXXXXX");
    }

    #[test]
    fn distribute_empty_input_returns_one_empty_line() {
        let lines = distribute_spans_across_lines(Vec::new(), 10, plain_color());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].is_empty());
    }

    #[test]
    fn distribute_width_zero_returns_one_line_owned() {
        // When width is 0 we don't wrap — caller decides how to handle.
        let lines = distribute_spans_across_lines(vec![plain("hello world")], 0, plain_color());
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn distribute_does_not_inject_space_between_atomic_and_adjacent_punctuation() {
        // Reproduces the `two kernels: \`gate+up\`, then \`down\`` case. Previously
        // the flatten-to-words step lost the fact that "," had no leading space,
        // and we injected one, bumping the visible width and forcing an extra wrap.
        // With span-preserving distribution, no space is injected.
        let spans = vec![
            plain("two kernels: "),
            atomic("gate+up"),
            plain(", then "),
            atomic("down"),
        ];
        let lines = distribute_spans_across_lines(spans, 31, plain_color());
        assert_eq!(lines.len(), 1, "got {:?}", lines.iter().map(|l| line_text(l)).collect::<Vec<_>>());
        assert_eq!(line_text(&lines[0]), "two kernels: gate+up, then down");
    }
}
