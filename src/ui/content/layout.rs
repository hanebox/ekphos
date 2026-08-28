use super::*;

pub fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    app.begin_image_frame();
    let mut code_block_highlights = std::collections::HashMap::new();
    let is_focused = app.state.focus == Focus::Content && app.editor.mode == Mode::Normal;
    let skip_images = app.state.dialog != DialogState::None || app.state.show_welcome;
    let theme = &app.state.theme;
    let border_style = if app.editor.floating_cursor_mode {
        Style::default().fg(theme.warning)
    } else if is_focused {
        Style::default().fg(theme.primary)
    } else {
        Style::default().fg(theme.border)
    };
    let floating_indicator = if app.editor.floating_cursor_mode { " [FLOAT] " } else { "" };
    let title = app.current_note().map(|n| format!(" {}{} ", n.title, floating_indicator)).unwrap_or_else(|| format!(" Content{} ", floating_indicator));

    const ZEN_MAX_WIDTH: u16 = 95;
    let inner_area = if app.state.zen_mode {
        let content_width = area.width.min(ZEN_MAX_WIDTH);
        let x_offset = (area.width.saturating_sub(content_width)) / 2;
        if app.editor.floating_cursor_mode {
            let status_area = Rect { x: area.x + x_offset, y: area.y, width: content_width, height: 1 };
            render_zen_content_status_line(f, theme, status_area);
        }
        let y_offset = if app.editor.floating_cursor_mode { 2 } else { 1 };
        Rect { x: area.x + x_offset, y: area.y + y_offset, width: content_width, height: area.height.saturating_sub(y_offset) }
    } else {
        let block = Block::default().title(title).borders(Borders::ALL).border_style(border_style);
        let inner = block.inner(area);
        f.render_widget(block, area);
        inner
    };
    app.editor.editor_area = if app.state.zen_mode { inner_area } else { area };
    app.state.inline_image_rects.clear();
    if app.document.content_items.is_empty() {
        app.finish_image_frame();
        return;
    }
    let cursor = app.document.content_cursor;
    let available_width = inner_area.width.saturating_sub(4) as usize;
    let max_item_height = inner_area.height.max(1);
    let standalone_image_height = app.state.config.effective_image_height();
    let inline_image_height = app.state.config.effective_inline_image_height();
    let document = app.document.active_document.as_ref().expect("normal-mode content requires a document snapshot");
    let document_tables = &app.document.document_tables;
    let document_link_ranges = &app.document.document_link_ranges;
    let mut scratch = std::mem::take(&mut app.document.content_render_scratch);
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
            let word_width = cell_visible_width(word);
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
    if scratch.height_generation != app.document.document_generation || scratch.height_width != available_width || scratch.item_text_heights.len() != app.document.content_items.len() {
        scratch.item_text_heights.clear();
        scratch.item_text_heights.extend(app.document.content_items.iter().enumerate().map(|(idx, item)| match item {
            ContentItem::TextLine { range, .. } => {
                let line = document.slice(*range);
                if app.inline_image_count_at(idx) == 0 {
                    calc_wrapped_height(line, 4)
                } else {
                    let prose_source = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("> ")).unwrap_or(line);
                    let prose = inline_prose_text(prose_source, theme);
                    if prose.is_empty() {
                        0
                    } else {
                        calc_wrapped_height(&prose, 4)
                    }
                }
            }
            ContentItem::TaskItem { text, indent, .. } => {
                let text = document.slice(*text);
                if app.inline_image_count_at(idx) == 0 {
                    calc_wrapped_height(text, 6 + *indent as usize)
                } else {
                    let prose = inline_prose_text(text, theme);
                    calc_wrapped_height(&prose, 6 + *indent as usize)
                }
            }
            _ => 0,
        }));
        scratch.height_generation = app.document.document_generation;
        scratch.height_width = available_width;
    }
    let item_text_heights = &scratch.item_text_heights;
    let details_states = &app.document.details_open_states;
    let get_item_height = |idx: usize, item: &ContentItem| -> u16 {
        match item {
            ContentItem::TextLine { .. } => {
                let inline_image_count = document_link_ranges.get(idx).map_or(0, |range| range.image_count as usize);
                if inline_image_count == 0 {
                    item_text_heights[idx]
                } else {
                    item_text_heights[idx].saturating_add(inline_thumbnails_height(inline_image_count, inner_area.width, inline_image_height))
                }
            }
            ContentItem::Image { .. } => standalone_image_height,
            ContentItem::CodeLine { range, .. } => code_line_height(document.slice(*range), code_block_highlights.get(&idx), inner_area.width, theme).min(max_item_height),
            ContentItem::CodeFence { .. } => 1u16,
            ContentItem::TaskItem { .. } => {
                let inline_image_count = document_link_ranges.get(idx).map_or(0, |range| range.image_count as usize);
                if inline_image_count == 0 {
                    item_text_heights[idx]
                } else {
                    item_text_heights[idx].saturating_add(inline_thumbnails_height(inline_image_count, inner_area.width, inline_image_height))
                }
            }
            ContentItem::TableRow { cells, is_separator, table, .. } => {
                if *is_separator {
                    1u16
                } else {
                    let metadata = document_tables.get(*table as usize);
                    let column_widths = metadata.map_or(&[][..], |metadata| metadata.column_widths.as_ref());
                    let n = column_widths.len();
                    let overhead = 3 + 3 * n;
                    let budget = (inner_area.width as usize).saturating_sub(overhead);
                    let natural: Vec<usize> = column_widths.iter().map(|width| *width as usize).collect();
                    let capped = cap_column_widths(&natural, budget);
                    let text_color = theme.content.text;
                    let row_lines = cells
                        .iter()
                        .enumerate()
                        .map(|(i, cell)| {
                            let w = capped.get(i).copied().unwrap_or(0);
                            let expanded = expand_tabs(document.slice(*cell));
                            let mut total: usize = 0;
                            for logical in split_cell_by_br(&expanded) {
                                let spans = parse_inline_formatting::<fn(&str) -> bool>(logical, theme, None, None);
                                total += distribute_spans_across_lines(spans, w, text_color).len();
                            }
                            total.max(1)
                        })
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    (row_lines as u16).min(max_item_height)
                }
            }
            ContentItem::Details { content_lines, source_line, .. } => {
                let is_open = details_states.get(&(*source_line as usize)).copied().unwrap_or(false);
                if is_open {
                    1 + content_lines.len() as u16
                } else {
                    1u16
                }
            }
            ContentItem::FrontmatterLine { .. } => 1u16,
            ContentItem::FrontmatterDelimiter { .. } => 1u16,
            ContentItem::TagBadges => 2u16, // 1 line padding + 1 line for tags
        }
    };
    let scroll_offset = if app.editor.floating_cursor_mode {
        let base_offset = if app.document.content_scroll_offset > 0 { app.document.content_scroll_offset.saturating_sub(1) } else { 0 };
        let mut height_from_offset = 0u16;
        let mut last_visible_idx = base_offset;
        for (i, item) in app.document.content_items.iter().enumerate().skip(base_offset) {
            if !app.is_content_item_visible(i) {
                continue;
            }
            let item_height = get_item_height(i, item);
            if height_from_offset + item_height > inner_area.height {
                break;
            }
            height_from_offset += item_height;
            last_visible_idx = i;
        }
        if cursor < base_offset {
            app.document.content_scroll_offset = cursor + 1;
            cursor
        } else if cursor > last_visible_idx {
            let mut cumulative_height = 0u16;
            for (i, item) in app.document.content_items.iter().enumerate() {
                if !app.is_content_item_visible(i) {
                    continue;
                }
                if i <= cursor {
                    cumulative_height += get_item_height(i, item);
                }
                if i == cursor {
                    break;
                }
            }
            let mut new_offset = 0;
            let mut height_so_far = 0u16;
            for (i, item) in app.document.content_items.iter().enumerate() {
                if !app.is_content_item_visible(i) {
                    continue;
                }
                if i > cursor {
                    break;
                }
                height_so_far += get_item_height(i, item);
                if cumulative_height - height_so_far <= inner_area.height {
                    new_offset = i + 1;
                    break;
                }
            }
            app.document.content_scroll_offset = new_offset + 1;
            new_offset
        } else {
            base_offset
        }
    } else {
        let mut first_page_height = 0u16;
        let mut first_page_last_idx = 0;
        for (i, item) in app.document.content_items.iter().enumerate() {
            if !app.is_content_item_visible(i) {
                continue;
            }
            let item_height = get_item_height(i, item);
            if first_page_height + item_height > inner_area.height {
                break;
            }
            first_page_height += item_height;
            first_page_last_idx = i;
        }
        if cursor <= first_page_last_idx {
            app.document.content_scroll_offset = 1;
            0
        } else {
            let mut height_from_cursor = 0u16;
            let mut first_visible_idx = cursor;
            for i in (0..=cursor).rev() {
                if !app.is_content_item_visible(i) {
                    continue;
                }
                let item_height = get_item_height(i, &app.document.content_items[i]);
                if height_from_cursor + item_height > inner_area.height {
                    break;
                }
                height_from_cursor += item_height;
                first_visible_idx = i;
            }
            app.document.content_scroll_offset = first_visible_idx + 1;
            first_visible_idx
        }
    };
    scratch.constraints.clear();
    scratch.visible_indices.clear();
    let mut total_height = 0u16;
    for (i, item) in app.document.content_items.iter().enumerate().skip(scroll_offset) {
        if !app.is_content_item_visible(i) {
            continue;
        }
        if total_height >= inner_area.height {
            break;
        }
        let item_height = get_item_height(i, item);
        let visible_height = visible_item_height(total_height, inner_area.height, item_height);
        scratch.constraints.push(Constraint::Length(visible_height));
        scratch.visible_indices.push(i);
        total_height = total_height.saturating_add(visible_height);
    }
    if scratch.constraints.is_empty() {
        app.state.content_area = inner_area;
        app.state.content_item_rects.clear();
        app.document.content_render_scratch = scratch;
        app.finish_image_frame();
        return;
    }
    let chunks = Layout::default().direction(Direction::Vertical).constraints(scratch.constraints.clone()).split(inner_area);
    let visible_indices = &scratch.visible_indices;
    code_block_highlights = visible_code_block_highlights(app, visible_indices);
    app.state.content_area = inner_area;
    app.state.content_item_rects.clear();
    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx < chunks.len() {
            app.state.content_item_rects.push((item_idx, chunks[chunk_idx]));
        }
    }
    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }
        let is_cursor_line = item_idx == cursor && is_focused;
        let is_hovered = app.state.mouse_hover_item == Some(item_idx);
        match &app.document.content_items[item_idx] {
            ContentItem::TextLine { range, .. } => {
                let line = app.document_slice(*range);
                let has_text_link = app.item_all_links_at(item_idx).iter().any(|link| !matches!(link, LinkInfo::Image { .. }));
                let selected_is_image = is_cursor_line && matches!(app.current_selected_link(), Some(LinkInfo::Image { .. }));
                let hovered_image = app.state.mouse_hover_inline_image.map(|(hovered_item, _)| hovered_item == item_idx).unwrap_or(false);
                let has_link = (is_cursor_line || is_hovered) && has_text_link && !selected_is_image && !hovered_image;
                let selected_link = if is_cursor_line { app.document.selected_link_index } else { 0 };
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                let fold_state = if app.is_heading_at(item_idx) { Some(app.is_heading_folded(item_idx)) } else { None };
                let context = RenderContext::new(&app.state.theme, chunks[chunk_idx], is_cursor_line, selected_link, has_link);
                render_content_line(f, line, context, Some(wiki_validator), fold_state);
                if !skip_images && app.inline_image_count_at(item_idx) > 0 {
                    let text_height = item_text_heights[item_idx];
                    render_inline_thumbnails(f, app, item_idx, chunks[chunk_idx], inner_area, (text_height, inline_image_height), is_cursor_line);
                }
            }
            ContentItem::Image { path, .. } => {
                if !skip_images {
                    render_inline_image_with_cursor(f, app, item_idx, *path, chunks[chunk_idx], inner_area, (is_cursor_line, is_hovered));
                }
            }
            ContentItem::CodeLine { range, .. } => {
                let highlighted_spans = code_block_highlights.get(&item_idx).cloned();
                render_code_line(f, &app.state.theme, app.document_slice(*range), highlighted_spans, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeFence { language, .. } => {
                render_code_fence(f, &app.state.theme, app.document_slice(*language), chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TaskItem { text, checked, indent, .. } => {
                let text = app.document_slice(*text);
                let selected_link = if is_cursor_line { app.document.selected_link_index } else { 0 };
                let has_links = !app.item_all_links_at(item_idx).is_empty();
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                let context = RenderContext::new(&app.state.theme, chunks[chunk_idx], is_cursor_line, selected_link, has_links);
                render_task_item(f, text, *checked, *indent as usize, context, Some(wiki_validator));
                if !skip_images && app.inline_image_count_at(item_idx) > 0 {
                    let text_height = item_text_heights[item_idx];
                    render_inline_thumbnails(f, app, item_idx, chunks[chunk_idx], inner_area, (text_height, inline_image_height), is_cursor_line);
                }
            }
            ContentItem::TableRow { cells, is_separator, is_header, table, .. } => {
                let has_link = !*is_separator && (is_cursor_line || is_hovered) && !app.item_all_links_at(item_idx).is_empty();
                let context = RenderContext::new(&app.state.theme, chunks[chunk_idx], is_cursor_line, 0, has_link);
                if let (Some(document), Some(metadata)) = (app.document(), app.table_metadata(*table)) {
                    render_table_row(f, document, cells, (*is_separator, *is_header), &metadata.column_widths, &metadata.alignments, context);
                }
            }
            ContentItem::Details { summary, content_lines, source_line } => {
                let is_open = app.document.details_open_states.get(&(*source_line as usize)).copied().unwrap_or(false);
                if let Some(document) = app.document() {
                    let context = RenderContext::new(&app.state.theme, chunks[chunk_idx], is_cursor_line, 0, false);
                    render_details(f, document, *summary, content_lines, is_open, context);
                }
            }
            ContentItem::FrontmatterDelimiter { .. } => {
                render_frontmatter_delimiter(f, &app.state.theme, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::FrontmatterLine { key, value, .. } => {
                render_frontmatter_line(f, &app.state.theme, app.document_slice(*key), app.document_slice(*value), chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TagBadges => {
                if let Some(frontmatter) = app.current_note().and_then(|note| note.frontmatter.as_ref()) {
                    render_tag_badges_inline(f, &app.state.theme, &frontmatter.tags, frontmatter.date.as_deref(), chunks[chunk_idx], is_cursor_line);
                }
            }
        }
    }
    if app.search.buffer_search.active && !app.search.buffer_search.matches.is_empty() {
        apply_content_search_highlights(f, app, visible_indices, &chunks);
    }
    app.document.content_render_scratch = scratch;
    app.finish_image_frame();
}
fn visible_code_block_highlights(app: &mut App, visible_indices: &[usize]) -> std::collections::HashMap<usize, Vec<Span<'static>>> {
    let mut blocks = Vec::new();
    let mut block_start: Option<(usize, DocumentRange)> = None;
    for (index, item) in app.document.content_items.iter().enumerate() {
        if let ContentItem::CodeFence { language, .. } = item {
            if let Some((start, language_range)) = block_start.take() {
                let intersects_viewport = visible_indices.iter().any(|visible| *visible >= start && *visible <= index);
                if intersects_viewport {
                    let language = app.document_slice(language_range);
                    if !language.is_empty() {
                        blocks.push((start, index, language.to_owned()));
                    }
                }
            } else {
                block_start = Some((index, *language));
            }
        }
    }
    if blocks.is_empty() {
        return std::collections::HashMap::new();
    }
    app.ensure_highlighter();
    let Some(highlighter) = app.get_highlighter() else {
        return std::collections::HashMap::new();
    };
    let mut highlights = std::collections::HashMap::new();
    for (start, end, language) in blocks {
        let lines: Vec<(usize, String)> = ((start + 1)..end)
            .filter_map(|index| match app.document.content_items.get(index) {
                Some(ContentItem::CodeLine { range, .. }) => Some((index, expand_tabs(app.document_slice(*range)))),
                _ => None,
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        let mut content = String::new();
        for (line_index, (_, line)) in lines.iter().enumerate() {
            if line_index > 0 {
                content.push('\n');
            }
            content.push_str(line);
        }
        let block = highlighter.highlight_block(&content, &language);
        for (line_index, (item_index, _)) in lines.iter().enumerate() {
            if let Some(spans) = block.get(line_index) {
                highlights.insert(*item_index, spans.clone());
            }
        }
    }
    highlights
}
