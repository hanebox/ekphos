use super::*;

/// Number of visual rows a code line occupies once wrapped. Built from the exact
/// same spans and wrap routine as `render_code_line`, so the layout's row budget
/// always matches what gets drawn.
///
/// `calc_wrapped_height` (used for prose) can't stand in here: it collapses
/// internal whitespace runs and assumes a narrower content width, so for code
/// with aligned trailing comments followed by wide (CJK) text it under-counted
/// rows and the renderer clipped the wrapped tail (issue #59).
pub(super) fn code_line_height(line: &str, highlight: Option<&Vec<Span<'static>>>, inner_width: u16, theme: &Theme) -> u16 {
    let mut spans = vec![Span::styled("  ", Style::default()), Span::styled("│ ", Style::default())];
    if let Some(hl) = highlight {
        spans.extend(hl.iter().cloned());
    } else {
        spans.push(Span::styled(expand_tabs(line), Style::default()));
    }
    let available_width = (inner_width as usize).saturating_sub(1);
    (wrap_line_for_cursor(spans, available_width, theme).len() as u16).max(1)
}

pub(super) fn render_content_line<F>(
    f: &mut Frame,
    line: &str,
    context: RenderContext<'_>,
    wiki_link_validator: Option<F>,
    fold_state: Option<bool>, // None = not foldable, Some(true) = folded, Some(false) = expanded
) where
    F: Fn(&str) -> bool,
{
    let RenderContext { theme, area, is_cursor, selected_link, has_link } = context;
    let line = &normalize_whitespace(line);
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding
    let fold_indicator = |is_folded: Option<bool>, color: ratatui::style::Color| -> Span {
        match is_folded {
            Some(true) => Span::styled("▶ ", Style::default().fg(color)),  // Folded
            Some(false) => Span::styled("▼ ", Style::default().fg(color)), // Expanded
            None => Span::styled("  ", Style::default()),                  // Not foldable
        }
    };
    let content_theme = &theme.content;
    let styled_line = if line.starts_with("###### ") {
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled(line.trim_start_matches("###### "), Style::default().fg(content_theme.text).add_modifier(Modifier::ITALIC))])
    } else if line.starts_with("##### ") {
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled(line.trim_start_matches("##### "), Style::default().fg(content_theme.heading4).add_modifier(Modifier::BOLD))])
    } else if line.starts_with("#### ") {
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("› ", Style::default().fg(content_theme.heading4)), Span::styled(line.trim_start_matches("#### "), Style::default().fg(content_theme.heading4).add_modifier(Modifier::BOLD))])
    } else if line.starts_with("### ") {
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), fold_indicator(fold_state, content_theme.heading3), Span::styled(line.trim_start_matches("### "), Style::default().fg(content_theme.heading3).add_modifier(Modifier::BOLD))])
    } else if line.starts_with("## ") {
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), fold_indicator(fold_state, content_theme.heading2), Span::styled(line.trim_start_matches("## "), Style::default().fg(content_theme.heading2).add_modifier(Modifier::BOLD))])
    } else if line.starts_with("# ") {
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), fold_indicator(fold_state, content_theme.heading1), Span::styled(line.trim_start_matches("# ").to_uppercase(), Style::default().fg(content_theme.heading1).add_modifier(Modifier::BOLD))])
    } else if line.starts_with("- ") {
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("• ", Style::default().fg(content_theme.list_marker))];
        spans.extend(parse_inline_formatting(line.trim_start_matches("- "), theme, selected, wiki_link_validator));
        Line::from(spans)
    } else if line.starts_with("> ") {
        let selected = if is_cursor { Some(selected_link) } else { None };
        let content = line.trim_start_matches("> ");
        let mut spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("┃ ", Style::default().fg(content_theme.blockquote))];
        let formatted = parse_inline_formatting(content, theme, selected, wiki_link_validator);
        for span in formatted {
            let mut style = span.style;
            if style.fg.is_none() || style.fg == Some(content_theme.text) {
                style = style.fg(content_theme.blockquote).add_modifier(Modifier::ITALIC);
            }
            spans.push(Span::styled(span.content, style));
        }
        Line::from(spans)
    } else if line == "---" || line == "***" || line == "___" {
        let hr_width = available_width.saturating_sub(2);
        Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("─".repeat(hr_width), Style::default().fg(theme.border))])
    } else if line.starts_with("* ") {
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("• ", Style::default().fg(content_theme.list_marker))];
        spans.extend(parse_inline_formatting(line.trim_start_matches("* "), theme, selected, wiki_link_validator));
        Line::from(spans)
    } else {
        let selected = if is_cursor { Some(selected_link) } else { None };
        let mut spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning))];
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
    let wrapped_lines = wrap_line_for_cursor(final_line.spans, available_width, theme);
    let bg_style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
        let line_area = Rect { x: area.x, y: area.y.saturating_add(i as u16), width: area.width, height: 1 };
        if line_area.y < area.y + area.height {
            let paragraph = Paragraph::new(wrapped_line.clone()).style(bg_style);
            f.render_widget(paragraph, line_area);
        }
    }
}

pub(super) fn render_code_line(f: &mut Frame, theme: &Theme, line: &str, highlighted_spans: Option<Vec<Span<'static>>>, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let expanded_line = expand_tabs(line);
    let available_width = (area.width as usize).saturating_sub(1); // 1 char right padding
    let content_theme = &theme.content;
    let mut spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("│ ", Style::default().fg(theme.border))];
    if let Some(hl_spans) = highlighted_spans {
        spans.extend(hl_spans);
    } else {
        spans.push(Span::styled(expanded_line, Style::default().fg(content_theme.code)));
    }
    let wrapped_lines = wrap_line_for_cursor(spans, available_width, theme);
    let bg_style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default().bg(content_theme.code_background) };
    for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
        let line_area = Rect { x: area.x, y: area.y.saturating_add(i as u16), width: area.width, height: 1 };
        if line_area.y < area.y + area.height {
            let paragraph = Paragraph::new(wrapped_line.clone()).style(bg_style);
            f.render_widget(paragraph, line_area);
        }
    }
}

pub(super) fn render_code_fence(f: &mut Frame, theme: &Theme, _lang: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let content_theme = &theme.content;
    let styled_line = Line::from(vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("───", Style::default().fg(theme.border))]);
    let style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default().bg(content_theme.code_background) };
    let paragraph = Paragraph::new(styled_line).style(style).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

pub(super) fn render_task_item<F>(f: &mut Frame, text: &str, checked: bool, indent: usize, context: RenderContext<'_>, wiki_link_validator: Option<F>)
where
    F: Fn(&str) -> bool,
{
    let RenderContext { theme, area, is_cursor, selected_link, has_link: has_links } = context;
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
    let checkbox_style = if checkbox_selected { Style::default().fg(theme.background).bg(theme.warning).add_modifier(Modifier::BOLD) } else { Style::default().fg(checkbox_color).add_modifier(Modifier::BOLD) };
    let bracket_style = if checkbox_selected { Style::default().fg(theme.background).bg(theme.warning) } else { Style::default().fg(checkbox_color) };
    let mut spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning))];
    if indent > 0 {
        spans.push(Span::styled(" ".repeat(indent), Style::default()));
    }
    spans.extend([Span::styled("[", bracket_style), Span::styled(if checked { "x" } else { " " }, checkbox_style), Span::styled("]", bracket_style), Span::styled(" ", Style::default())]);
    spans.extend(text_spans);
    let wrapped_lines = wrap_line_for_cursor(spans, available_width, theme);
    let bg_style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    for (i, wrapped_line) in wrapped_lines.iter().enumerate() {
        let line_area = Rect { x: area.x, y: area.y.saturating_add(i as u16), width: area.width, height: 1 };
        if line_area.y < area.y + area.height {
            let paragraph = Paragraph::new(wrapped_line.clone()).style(bg_style);
            f.render_widget(paragraph, line_area);
        }
    }
}

pub(super) fn render_table_row(f: &mut Frame, document: &DocumentSnapshot, cells: &[DocumentRange], row_flags: (bool, bool), natural_widths: &[u16], alignments: &[crate::app::Alignment], context: RenderContext<'_>) {
    let (is_separator, is_header) = row_flags;
    let RenderContext { theme, area, is_cursor, has_link, .. } = context;
    let border_color = theme.border;
    let row_bg = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    let n = natural_widths.len();
    let overhead = 3 + 3 * n;
    let budget = (area.width as usize).saturating_sub(overhead);
    let natural_widths: Vec<usize> = natural_widths.iter().map(|width| *width as usize).collect();
    let widths = cap_column_widths(&natural_widths, budget);
    if is_separator {
        let mut spans = vec![Span::styled(if is_cursor { "▶ " } else { "  " }, Style::default().fg(theme.warning)), Span::styled("│", Style::default().fg(border_color))];
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
    let per_cell_lines: Vec<Vec<Vec<Span<'static>>>> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let w = widths.get(i).copied().unwrap_or(0);
            let expanded = expand_tabs(document.slice(*c));
            let mut all_visual_lines: Vec<Vec<Span<'static>>> = Vec::new();
            for logical in split_cell_by_br(&expanded) {
                let spans = parse_inline_formatting::<fn(&str) -> bool>(logical, theme, None, None);
                all_visual_lines.extend(distribute_spans_across_lines(spans, w, text_color));
            }
            if all_visual_lines.is_empty() {
                all_visual_lines.push(Vec::new());
            }
            all_visual_lines
        })
        .collect();
    let row_height = per_cell_lines.iter().map(|lines| lines.len()).max().unwrap_or(1).max(1);
    let default_style = if is_header { Style::default().fg(theme.info).add_modifier(Modifier::BOLD) } else { Style::default().fg(theme.foreground) };
    for line_idx in 0..row_height {
        let cursor_indicator = if is_cursor && line_idx == 0 { "▶ " } else { "  " };
        let mut spans: Vec<Span<'static>> = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("│", Style::default().fg(border_color))];
        for (i, cell_lines) in per_cell_lines.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("│", Style::default().fg(border_color)));
            }
            let line_spans_slice: &[Span<'static>] = cell_lines.get(line_idx).map(|v| v.as_slice()).unwrap_or(&[]);
            let width = widths.get(i).copied().unwrap_or(0);
            let visible: usize = line_spans_slice.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
            let pad = width.saturating_sub(visible);
            let alignment = alignments.get(i).copied().unwrap_or(crate::app::Alignment::Left);
            let (left_pad, right_pad) = match alignment {
                crate::app::Alignment::Left => (0, pad),
                crate::app::Alignment::Right => (pad, 0),
                crate::app::Alignment::Center => (pad / 2, pad - pad / 2),
            };
            spans.push(Span::styled(format!(" {}", " ".repeat(left_pad)), default_style));
            for sp in line_spans_slice.iter().cloned() {
                let style = if is_plain_text_span(&sp.style, text_color) { default_style } else { sp.style };
                spans.push(Span::styled(sp.content, style));
            }
            spans.push(Span::styled(format!("{} ", " ".repeat(right_pad)), default_style));
        }
        spans.push(Span::styled("│", Style::default().fg(border_color)));
        if has_link && line_idx == 0 {
            spans.push(Span::styled(" Open ↗", Style::default().fg(theme.content.link)));
        }
        if (area.y + line_idx as u16) >= area.y + area.height {
            break;
        }
        let line_area = Rect { x: area.x, y: area.y + line_idx as u16, width: area.width, height: 1 };
        let paragraph = Paragraph::new(Line::from(spans)).style(row_bg);
        f.render_widget(paragraph, line_area);
    }
}

pub(super) fn render_inline_image_with_cursor(f: &mut Frame, app: &mut App, item_index: usize, path: DocumentRange, area: Rect, viewport: Rect, selection: (bool, bool)) {
    let (is_cursor, is_hovered) = selection;
    let path = app.document_slice(path).to_owned();
    let configured_height = app.state.config.effective_image_height();
    let configured_area = Rect { height: configured_height, ..area };
    let visible_area = configured_area.intersection(viewport);
    let normalized_path = normalize_image_destination(&path);
    let is_remote = normalized_path.starts_with("http://") || normalized_path.starts_with("https://");
    let is_pending = is_remote && app.is_image_pending(&normalized_path);
    let resolved_path = app.resolve_image_path(&path);
    let resolved_path_str = resolved_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
    let state_key = standalone_image_state_key(item_index, &resolved_path_str);
    let is_cached = app.is_image_cached(&resolved_path_str);
    let warning_color = app.state.theme.warning;
    let info_color = app.state.theme.info;
    let secondary_color = app.state.theme.secondary;
    let selection_color = app.state.theme.selection;
    let error_color = app.state.theme.error;
    let show_hint = is_cursor || is_hovered;
    let border_color = if is_cursor {
        warning_color
    } else if is_hovered {
        info_color
    } else if is_pending {
        secondary_color
    } else {
        info_color
    };
    let title = if is_pending {
        " Loading... ".to_string()
    } else if show_hint {
        " Open ↗ ".to_string()
    } else {
        "".to_string()
    };
    let block = Block::default().title(title).borders(image_frame_borders(visible_area.height, configured_height)).border_style(Style::default().fg(border_color));
    let inner_area = block.inner(visible_area);
    let image_size = Size::new(visible_area.width.saturating_sub(2), configured_height.saturating_sub(2));
    if is_cursor {
        let bg = Paragraph::new("").style(Style::default().bg(selection_color));
        f.render_widget(bg, visible_area);
    }
    f.render_widget(block, visible_area);
    ensure_image_state(app, &state_key, resolved_path.as_deref(), &resolved_path_str, &normalized_path, is_remote, image_size);
    let is_pending = app.is_image_pending(&resolved_path_str);
    let load_failed = app.image_load_failed(&resolved_path_str);
    if inner_area.width == 0 || inner_area.height == 0 || image_size.height == 0 {
        return;
    }
    if is_pending || (is_remote && !is_cached && !load_failed && !app.images.image_states.contains_key(&state_key)) {
        let loading = Paragraph::new("  Loading remote image...").style(Style::default().fg(secondary_color).add_modifier(Modifier::ITALIC));
        f.render_widget(loading, inner_area);
        return;
    }
    if let Some(state) = app.images.image_states.get(&state_key) {
        let image_widget = SlicedImage::new(&state.image, SignedPosition::from((0, 0)));
        f.render_widget(image_widget, inner_area);
    } else if !is_remote || load_failed {
        let placeholder = Paragraph::new("  [Image not found]").style(Style::default().fg(error_color).add_modifier(Modifier::ITALIC));
        f.render_widget(placeholder, inner_area);
    }
}

/// Render inline image thumbnails below text content
/// Returns the number of thumbnail rows rendered
pub(super) fn render_inline_thumbnails(f: &mut Frame, app: &mut App, item_index: usize, area: Rect, viewport: Rect, heights: (u16, u16), is_cursor_line: bool) -> u16 {
    let (text_height, image_height) = heights;
    let link_range = app.document.document_link_ranges.get(item_index).copied().unwrap_or_default();
    let task_offset = usize::from(matches!(app.document.content_items.get(item_index), Some(ContentItem::TaskItem { .. })) && link_range.len > 0);
    let image_count = app.inline_image_count_at(item_index);
    if image_count == 0 {
        return 0;
    }
    let secondary_color = app.state.theme.secondary;
    let error_color = app.state.theme.error;
    let info_color = app.state.theme.info;
    let warning_color = app.state.theme.warning;
    let selection_color = app.state.theme.selection;
    let thumbnail_width = inline_thumbnail_width(area.width, image_height);
    let per_row = inline_thumbnails_per_row(area.width, image_height);
    let mut image_index = 0usize;
    for link_offset in 0..link_range.len as usize {
        let link_index = link_range.start as usize + link_offset;
        let Some(LinkInfo::Image { path, .. }) = app.document.document_links.get(link_index) else {
            continue;
        };
        let path = path.clone();
        let selection_index = link_offset + task_offset;
        let row = image_index / per_row;
        let column = image_index % per_row;
        let y_offset = text_height.saturating_add(u16::try_from(row).unwrap_or(u16::MAX).saturating_mul(image_height));
        let x_offset = INLINE_THUMBNAIL_HORIZONTAL_PADDING.saturating_add(u16::try_from(column).unwrap_or(u16::MAX).saturating_mul(thumbnail_width.saturating_add(INLINE_THUMBNAIL_GAP)));
        let configured_thumb_area = Rect { x: area.x.saturating_add(x_offset), y: area.y.saturating_add(y_offset), width: thumbnail_width, height: image_height };
        let thumb_area = configured_thumb_area.intersection(viewport);
        if thumb_area.height == 0 || thumb_area.width == 0 {
            break;
        }
        app.state.inline_image_rects.push(InlineImageRect { item_index, selection_index, rect: thumb_area });
        let normalized_path = normalize_image_destination(&path);
        let is_remote = normalized_path.starts_with("http://") || normalized_path.starts_with("https://");
        let resolved_path = app.resolve_image_path(&path);
        let resolved_path_str = resolved_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
        let state_key = inline_image_state_key(item_index, selection_index, &resolved_path_str);
        let is_pending = is_remote && app.is_image_pending(&normalized_path);
        let is_selected = is_cursor_line && app.document.selected_link_index == selection_index;
        let is_hovered = app.state.mouse_hover_inline_image == Some((item_index, selection_index));
        let title = if is_pending {
            " Loading... "
        } else if is_selected || is_hovered {
            " Open ↗ "
        } else {
            ""
        };
        let border_color = if is_selected {
            warning_color
        } else if is_hovered {
            info_color
        } else {
            secondary_color
        };
        let block = Block::default().title(title).borders(image_frame_borders(thumb_area.height, image_height)).border_style(Style::default().fg(border_color));
        let image_area = block.inner(thumb_area);
        let image_size = Size::new(thumbnail_width.saturating_sub(2), image_height.saturating_sub(2));
        if is_selected {
            let highlight = Paragraph::new("").style(Style::default().bg(selection_color));
            f.render_widget(highlight, thumb_area);
        }
        f.render_widget(block, thumb_area);
        ensure_image_state(app, &state_key, resolved_path.as_deref(), &resolved_path_str, &normalized_path, is_remote, image_size);
        let is_pending = app.is_image_pending(&resolved_path_str);
        let load_failed = app.image_load_failed(&resolved_path_str);
        if image_area.width == 0 || image_area.height == 0 || image_size.height == 0 {
            continue;
        }
        if let Some(state) = app.images.image_states.get(&state_key) {
            let image_widget = SlicedImage::new(&state.image, SignedPosition::from((0, 0)));
            f.render_widget(image_widget, image_area);
        } else if is_pending {
            let loading = Paragraph::new("  ⏳ Loading...").style(Style::default().fg(secondary_color).add_modifier(Modifier::ITALIC));
            f.render_widget(loading, image_area);
        } else if load_failed || (!is_remote && resolved_path.is_none()) {
            let not_found = Paragraph::new("  ❌ Not found").style(Style::default().fg(error_color).add_modifier(Modifier::ITALIC));
            f.render_widget(not_found, image_area);
        }
        image_index += 1;
    }
    inline_thumbnails_height(image_count, area.width, image_height)
}

pub(super) fn render_details(f: &mut Frame, document: &DocumentSnapshot, summary: Option<DocumentRange>, content_lines: &[u32], is_open: bool, context: RenderContext<'_>) {
    let RenderContext { theme, area, is_cursor, .. } = context;
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let toggle_indicator = if is_open { "▼ " } else { "▶ " };
    let mut lines: Vec<Line> = Vec::new();
    let expanded_summary = expand_tabs(summary.map_or("Details", |range| document.slice(range)));
    let summary_spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled(toggle_indicator, Style::default().fg(theme.info)), Span::styled(expanded_summary, Style::default().fg(theme.info).add_modifier(Modifier::BOLD))];
    lines.push(Line::from(summary_spans));
    if is_open {
        for content in content_lines {
            let expanded_content = expand_tabs(document.line(*content as usize).unwrap_or(""));
            let content_spans = vec![Span::styled("  ", Style::default()), Span::styled("│ ", Style::default().fg(theme.border)), Span::styled(expanded_content, Style::default().fg(theme.foreground))];
            lines.push(Line::from(content_spans));
        }
    }
    let style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    let paragraph = Paragraph::new(lines).style(style).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

pub(super) fn render_frontmatter_delimiter(f: &mut Frame, theme: &Theme, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let spans = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled("---", Style::default().fg(theme.content.frontmatter))];
    let style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    let paragraph = Paragraph::new(Line::from(spans)).style(style);
    f.render_widget(paragraph, area);
}

/// Render tag badges as part of scrollable content (not fixed at top)
pub(super) fn render_tag_badges_inline(f: &mut Frame, theme: &Theme, tags: &[Box<str>], date: Option<&str>, area: Rect, is_cursor: bool) {
    if area.height == 0 {
        return;
    }
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let mut spans: Vec<Span> = vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning))];
    for (i, tag) in tags.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", Style::default()));
        }
        spans.push(Span::styled(format!(" {} ", tag), Style::default().fg(theme.content.tag).bg(theme.content.tag_background)));
    }
    if let Some(d) = date {
        if !tags.is_empty() {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(d, Style::default().fg(theme.content.frontmatter)));
    }
    let y_offset = if area.height >= 2 { 1 } else { 0 };
    let tag_area = Rect { x: area.x, y: area.y + y_offset, width: area.width, height: 1 };
    let style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    let paragraph = Paragraph::new(Line::from(spans)).style(style);
    f.render_widget(paragraph, tag_area);
}

pub(super) fn render_zen_content_status_line(f: &mut Frame, theme: &Theme, area: Rect) {
    let status_line = Line::from(vec![Span::styled(" FLOAT ", Style::default().fg(theme.background).bg(theme.warning).add_modifier(Modifier::BOLD))]);
    let paragraph = Paragraph::new(status_line);
    f.render_widget(paragraph, area);
}

pub(super) fn render_frontmatter_line(f: &mut Frame, theme: &Theme, key: &str, value: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };
    let spans = if key.is_empty() {
        vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled(value, Style::default().fg(theme.content.frontmatter))]
    } else {
        vec![Span::styled(cursor_indicator, Style::default().fg(theme.warning)), Span::styled(format!("{}: ", key), Style::default().fg(theme.info)), Span::styled(value, Style::default().fg(theme.content.frontmatter))]
    };
    let style = if is_cursor { Style::default().bg(theme.selection) } else { Style::default() };
    let paragraph = Paragraph::new(Line::from(spans)).style(style);
    f.render_widget(paragraph, area);
}
