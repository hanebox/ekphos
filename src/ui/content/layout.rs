use super::*;

pub fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    // pre-compute code block highlights for proper syntax state tracking.
    // computed up front (before the layout/height pass) so code-line heights can
    // be measured from the exact same spans the renderer wraps, see the
    // CodeLine arm of get_item_height.
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
                                let block_content: String = lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");

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
        let block = Block::default().title(title).borders(Borders::ALL).border_style(border_style);

        let inner = block.inner(area);
        f.render_widget(block, area);
        inner
    };
    app.editor_area = if app.zen_mode { inner_area } else { area };
    app.inline_image_rects.clear();

    if app.content_items.is_empty() {
        return;
    }

    let cursor = app.content_cursor;
    let available_width = inner_area.width.saturating_sub(4) as usize;
    let max_item_height = inner_area.height.max(1);
    let standalone_image_height = app.config.effective_image_height();
    let inline_image_height = app.config.effective_inline_image_height();
    let inline_image_selections: Vec<Vec<(String, usize)>> = (0..app.content_items.len()).map(|index| app.item_inline_image_selections_at(index)).collect();

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
            // Use the *visible* width so a single-word markdown atom like
            // `[label](https://very-long-url)` counts as its rendered label width
            // (~ "label") instead of its raw source. Otherwise the height calc
            // over-reserves lines and the layout shows blank padding rows.
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

    let item_text_heights: Vec<u16> = app
        .content_items
        .iter()
        .enumerate()
        .map(|(idx, item)| match item {
            ContentItem::TextLine(line) => {
                if inline_image_selections[idx].is_empty() {
                    calc_wrapped_height(line, 4)
                } else {
                    let prose_source = line
                        .strip_prefix("- ")
                        .or_else(|| line.strip_prefix("* "))
                        .or_else(|| line.strip_prefix("> "))
                        .unwrap_or(line);
                    let prose = inline_prose_text(prose_source, theme);
                    if prose.is_empty() {
                        0
                    } else {
                        calc_wrapped_height(&prose, 4)
                    }
                }
            }
            ContentItem::TaskItem { text, indent, .. } => {
                if inline_image_selections[idx].is_empty() {
                    calc_wrapped_height(text, 6 + *indent)
                } else {
                    let prose = inline_prose_text(text, theme);
                    calc_wrapped_height(&prose, 6 + *indent)
                }
            }
            _ => 0,
        })
        .collect();

    let details_states = &app.details_open_states;
    let get_item_height = |idx: usize, item: &ContentItem| -> u16 {
        match item {
            ContentItem::TextLine(_) => {
                let inline_images = &inline_image_selections[idx];
                if inline_images.is_empty() {
                    item_text_heights[idx]
                } else {
                    item_text_heights[idx].saturating_add(inline_thumbnails_height(inline_images.len(), inner_area.width, inline_image_height))
                }
            }
            ContentItem::Image(_) => standalone_image_height,
            ContentItem::CodeLine(line) => code_line_height(line, code_block_highlights.get(&idx), inner_area.width, theme).min(max_item_height),
            ContentItem::CodeFence(_) => 1u16,
            ContentItem::TaskItem { .. } => {
                let inline_images = &inline_image_selections[idx];
                if inline_images.is_empty() {
                    item_text_heights[idx]
                } else {
                    item_text_heights[idx].saturating_add(inline_thumbnails_height(inline_images.len(), inner_area.width, inline_image_height))
                }
            }
            ContentItem::TableRow {
                cells,
                is_separator,
                column_widths,
                ..
            } => {
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
                    let row_lines = cells
                        .iter()
                        .enumerate()
                        .map(|(i, cell)| {
                            let w = capped.get(i).copied().unwrap_or(0);
                            let expanded = expand_tabs(cell);
                            // `<br>` inside a cell opens a new logical line; each logical line
                            // wraps independently and stacks vertically within the cell.
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
            let item_height = get_item_height(i, item);
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
                    cumulative_height += get_item_height(i, item);
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
                height_so_far += get_item_height(i, item);
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
            let item_height = get_item_height(i, item);
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
                let item_height = get_item_height(i, &app.content_items[i]);
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
        let item_height = get_item_height(i, item);
        let visible_height = visible_item_height(total_height, inner_area.height, item_height);
        constraints.push(Constraint::Length(visible_height));
        visible_indices.push(i);
        total_height = total_height.saturating_add(visible_height);
    }

    if constraints.is_empty() {
        app.content_area = inner_area;
        app.content_item_rects.clear();
        return;
    }

    let chunks = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner_area);

    app.content_area = inner_area;
    app.content_item_rects.clear();
    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx < chunks.len() {
            app.content_item_rects.push((item_idx, chunks[chunk_idx]));
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
            ContentItem::TextLine(ref line) => {
                let has_text_link = app.item_all_links_at(item_idx).iter().any(|link| !matches!(link, LinkInfo::Image { .. }));
                let selected_is_image = is_cursor_line && matches!(app.current_selected_link(), Some(LinkInfo::Image { .. }));
                let hovered_image = app.mouse_hover_inline_image.map(|(hovered_item, _)| hovered_item == item_idx).unwrap_or(false);
                let has_link = (is_cursor_line || is_hovered) && has_text_link && !selected_is_image && !hovered_image;
                let selected_link = if is_cursor_line { app.selected_link_index } else { 0 };
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                // Get fold state for H1-H3 headings
                let fold_state = if app.is_heading_at(item_idx) {
                    Some(app.is_heading_folded(item_idx))
                } else {
                    None
                };
                let context = RenderContext::new(&app.theme, chunks[chunk_idx], is_cursor_line, selected_link, has_link);
                render_content_line(f, line, context, Some(wiki_validator), fold_state);
                if !skip_images {
                    let inline_images = &inline_image_selections[item_idx];
                    if !inline_images.is_empty() {
                        let text_height = item_text_heights[item_idx];
                        render_inline_thumbnails(
                            f,
                            app,
                            item_idx,
                            inline_images,
                            chunks[chunk_idx],
                            inner_area,
                            text_height,
                            inline_image_height,
                            is_cursor_line,
                        );
                    }
                }
            }
            ContentItem::Image(path) => {
                if !skip_images {
                    render_inline_image_with_cursor(f, app, item_idx, &path, chunks[chunk_idx], inner_area, is_cursor_line, is_hovered);
                }
            }
            ContentItem::CodeLine(line) => {
                let highlighted_spans = code_block_highlights.get(&item_idx).cloned();
                render_code_line(f, &app.theme, &line, highlighted_spans, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeFence(lang) => {
                render_code_fence(f, &app.theme, &lang, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::TaskItem { ref text, checked, indent, .. } => {
                let selected_link = if is_cursor_line { app.selected_link_index } else { 0 };
                let has_links = !app.item_wiki_links_at(item_idx).is_empty() || !app.item_links_at(item_idx).is_empty();
                let wiki_validator = |target: &str| app.wiki_link_exists(target);
                let context = RenderContext::new(&app.theme, chunks[chunk_idx], is_cursor_line, selected_link, has_links);
                render_task_item(f, text, checked, indent, context, Some(wiki_validator));
                if !skip_images {
                    let inline_images = &inline_image_selections[item_idx];
                    if !inline_images.is_empty() {
                        let text_height = item_text_heights[item_idx];
                        render_inline_thumbnails(
                            f,
                            app,
                            item_idx,
                            inline_images,
                            chunks[chunk_idx],
                            inner_area,
                            text_height,
                            inline_image_height,
                            is_cursor_line,
                        );
                    }
                }
            }
            ContentItem::TableRow {
                cells,
                is_separator,
                is_header,
                column_widths,
                alignments,
            } => {
                let has_link = !is_separator && (is_cursor_line || is_hovered) && !app.item_links_at(item_idx).is_empty();
                let context = RenderContext::new(&app.theme, chunks[chunk_idx], is_cursor_line, 0, has_link);
                render_table_row(f, &cells, is_separator, is_header, &column_widths, &alignments, context);
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
