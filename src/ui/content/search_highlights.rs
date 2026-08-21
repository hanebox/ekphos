use super::*;

pub(super) fn apply_content_search_highlights(f: &mut Frame, app: &App, visible_indices: &[usize], chunks: &[Rect]) {
    let theme = &app.theme;
    let current_match_idx = app.buffer_search.current_match_index;
    let Some(document) = app.document() else {
        return;
    };

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }

        let source_line = app.content_items.get(item_idx).map(ContentItem::source_line).unwrap_or(usize::MAX);
        let raw_line = document.line(source_line).unwrap_or("");

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
                    Some(ContentItem::TableRow {
                        cells, table, is_separator, ..
                    }) => {
                        if *is_separator {
                            continue;
                        }
                        let Some(metadata) = app.table_metadata(*table) else {
                            continue;
                        };
                        calc_table_adjusted_col(m.start_col, document, cells, &metadata.column_widths, &metadata.alignments)
                    }
                    Some(ContentItem::TextLine { range, .. }) => {
                        let line = normalize_whitespace(document.slice(*range));
                        let (rendered_prefix_len, raw_prefix_len, content_text) = if line.starts_with("###### ") {
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
                        let display_col = content_text
                            .chars()
                            .take(content_start_col.saturating_sub(formatting_shrinkage))
                            .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
                            .sum::<usize>();
                        rendered_prefix_len + display_col
                    }
                    Some(ContentItem::CodeLine { range, .. }) => {
                        let code = document.slice(*range);
                        // Calculate display width of code before the match
                        let display_col: usize = code
                            .chars()
                            .take(m.start_col)
                            .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
                            .sum();
                        4 + display_col
                    }
                    Some(ContentItem::TaskItem { text, indent, .. }) => {
                        let text = document.slice(*text);
                        let prefix = 6 + *indent as usize;
                        if m.start_col < prefix {
                            continue;
                        }
                        let content_start_col = m.start_col - prefix;
                        let formatting_shrinkage = calc_formatting_shrinkage(text, content_start_col);
                        let display_col: usize = text
                            .chars()
                            .take(content_start_col.saturating_sub(formatting_shrinkage))
                            .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
                            .sum();
                        prefix + display_col
                    }
                    _ => {
                        // Calculate display width of raw line before the match
                        let display_col: usize = raw_line
                            .chars()
                            .take(m.start_col)
                            .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
                            .sum();
                        2 + display_col
                    }
                };

                let start_x = area.x + adjusted_col as u16;
                // Calculate display width of matched text
                let match_display_width: usize = raw_line
                    .chars()
                    .skip(m.start_col)
                    .take(m.end_col - m.start_col)
                    .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
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
