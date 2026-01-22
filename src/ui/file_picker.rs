use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, SearchPickerMode, SearchPickerState};

const POPUP_MAX_WIDTH: u16 = 80;
const POPUP_MAX_WIDTH_WITH_PREVIEW: u16 = 110;
const POPUP_MAX_VISIBLE_ITEMS: usize = 10;
const POPUP_MAX_VISIBLE_ITEMS_CONTENT: usize = 16;
const POPUP_MIN_CONTENT_HEIGHT: usize = 3;
const POPUP_MIN_CONTENT_HEIGHT_WITH_PREVIEW: usize = 16;
const PREVIEW_LINES_BEFORE: usize = 5;
const PREVIEW_LINES_AFTER: usize = 8;

pub fn render_search_picker(f: &mut Frame, app: &mut App) {
    if let SearchPickerState::Open {
        mode,
        query,
        file_results,
        content_results,
        selected_index,
        scroll_offset,
        search_in_progress,
        ..
    } = &app.search_picker
    {
        let theme = &app.theme;
        let area = f.area();

        // Content mode uses wider layout for preview panel
        let has_preview = *mode == SearchPickerMode::Content && !content_results.is_empty();
        let base_width = if has_preview { POPUP_MAX_WIDTH_WITH_PREVIEW } else { POPUP_MAX_WIDTH };
        let popup_width = base_width.min((area.width as f32 * 0.9) as u16).min(area.width.saturating_sub(4));

        // For content mode with preview, we split into left (list) and right (preview)
        let list_width = if has_preview {
            (popup_width / 2).min(60)
        } else {
            popup_width
        };

        // Height calculation depends on mode
        let (results_len, content_height) = match mode {
            SearchPickerMode::Files => {
                let visible_items = file_results.len().min(POPUP_MAX_VISIBLE_ITEMS);
                let height: usize = file_results
                    .iter()
                    .skip(*scroll_offset)
                    .take(visible_items)
                    .map(|r| if r.folder_hint.is_some() { 2 } else { 1 })
                    .sum();
                (file_results.len(), height.max(POPUP_MIN_CONTENT_HEIGHT))
            }
            SearchPickerMode::Content => {
                // Fixed height for content mode to accommodate preview
                let visible_items = content_results.len().min(POPUP_MAX_VISIBLE_ITEMS_CONTENT);
                let height = if has_preview {
                    // Use fixed height for preview layout
                    POPUP_MIN_CONTENT_HEIGHT_WITH_PREVIEW
                } else {
                    let h: usize = content_results
                        .iter()
                        .skip(*scroll_offset)
                        .take(visible_items)
                        .map(|r| if r.folder_hint.is_some() { 3 } else { 2 })
                        .sum();
                    h.max(POPUP_MIN_CONTENT_HEIGHT)
                };
                (content_results.len(), height)
            }
        };

        // Add 1 for top padding, 2 for mode tabs + input line, 1 for spacing, 1 for separator, plus 2 for borders, +1 for bottom padding
        let popup_height = (content_height as u16 + 8).min(area.height.saturating_sub(4));

        // Center the popup
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area behind the popup
        f.render_widget(Clear, popup_area);

        // Render the main popup border first
        let popup_block = Block::default()
            .title(" Search (Ctrl+K) ")
            .title_bottom(Line::from(if results_len == 0 {
                if *search_in_progress {
                    " ... ".to_string()
                } else {
                    " No matches ".to_string()
                }
            } else {
                format!(" {}/{} ", selected_index + 1, results_len)
            }).right_aligned())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.info))
            .style(Style::default().bg(theme.background_secondary));

        f.render_widget(popup_block, popup_area);

        // Inner area (inside borders)
        let inner_area = Rect::new(
            popup_area.x + 1,
            popup_area.y + 1,
            popup_area.width.saturating_sub(2),
            popup_area.height.saturating_sub(2),
        );

        // Build header lines (mode tabs + input)
        let mut header_lines: Vec<Line> = Vec::new();

        // Top padding
        header_lines.push(Line::from(""));

        // Mode tabs line
        let files_style = if *mode == SearchPickerMode::Files {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        let content_style = if *mode == SearchPickerMode::Content {
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };

        header_lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled("← ", Style::default().fg(theme.muted)),
            Span::styled("Files", files_style),
            Span::styled(" | ", Style::default().fg(theme.muted)),
            Span::styled("Content", content_style),
            Span::styled(" →", Style::default().fg(theme.muted)),
        ]));

        // Empty line for spacing
        header_lines.push(Line::from(""));

        // Input line
        let placeholder = if *mode == SearchPickerMode::Files {
            "Search notes..."
        } else {
            "Search content..."
        };

        let input_line = if query.is_empty() {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(placeholder, Style::default().fg(theme.muted)),
            ])
        } else {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(query.clone(), Style::default().fg(theme.foreground)),
                Span::styled("█", Style::default().fg(theme.primary)),
            ])
        };
        header_lines.push(input_line);

        // Separator line
        header_lines.push(Line::from(Span::styled(
            "─".repeat(inner_area.width as usize),
            Style::default().fg(theme.muted),
        )));

        let header_height = header_lines.len() as u16;
        let header_area = Rect::new(inner_area.x, inner_area.y, inner_area.width, header_height);
        let header = Paragraph::new(header_lines).style(Style::default().bg(theme.background_secondary));
        f.render_widget(header, header_area);

        // Results area (below header)
        let results_area = Rect::new(
            inner_area.x,
            inner_area.y + header_height,
            inner_area.width,
            inner_area.height.saturating_sub(header_height),
        );

        if has_preview {
            // Split into list (left) and preview (right)
            let list_area = Rect::new(
                results_area.x,
                results_area.y,
                list_width.saturating_sub(1),
                results_area.height,
            );
            let preview_area = Rect::new(
                results_area.x + list_width,
                results_area.y,
                results_area.width.saturating_sub(list_width),
                results_area.height,
            );

            // Render vertical separator
            let sep_area = Rect::new(
                results_area.x + list_width.saturating_sub(1),
                results_area.y,
                1,
                results_area.height,
            );
            let sep_lines: Vec<Line> = (0..sep_area.height)
                .map(|_| Line::from(Span::styled("│", Style::default().fg(theme.muted))))
                .collect();
            let sep = Paragraph::new(sep_lines).style(Style::default().bg(theme.background_secondary));
            f.render_widget(sep, sep_area);

            // Render compact content list
            let mut list_lines: Vec<Line> = Vec::new();
            let max_name_width = (list_area.width as usize).saturating_sub(4);
            render_content_results_compact(&mut list_lines, content_results, *selected_index, *scroll_offset, max_name_width, list_area.width, theme, query);
            let list = Paragraph::new(list_lines).style(Style::default().bg(theme.background_secondary));
            f.render_widget(list, list_area);

            // Render preview
            render_preview(f, app, content_results, *selected_index, preview_area, query, theme);

            // Store areas for mouse handling
            app.search_picker_area = popup_area;
            app.search_picker_results_area = list_area;
        } else {
            // Regular layout without preview
            let mut result_lines: Vec<Line> = Vec::new();
            let max_name_width = (results_area.width as usize).saturating_sub(6);

            match mode {
                SearchPickerMode::Files => {
                    if file_results.is_empty() {
                        result_lines.push(Line::from(vec![
                            Span::raw(" "),
                            Span::styled(
                                if query.is_empty() { "Type to search files..." } else { "No matching files" },
                                Style::default().fg(theme.muted),
                            ),
                        ]));
                    } else {
                        render_file_results(&mut result_lines, file_results, *selected_index, *scroll_offset, max_name_width, results_area.width, theme);
                    }
                }
                SearchPickerMode::Content => {
                    if *search_in_progress {
                        result_lines.push(Line::from(vec![
                            Span::raw(" "),
                            Span::styled("Searching...", Style::default().fg(theme.muted)),
                        ]));
                    } else if content_results.is_empty() {
                        result_lines.push(Line::from(vec![
                            Span::raw(" "),
                            Span::styled(
                                if query.is_empty() { "Type to search content..." } else { "No matching content" },
                                Style::default().fg(theme.muted),
                            ),
                        ]));
                    } else {
                        render_content_results(&mut result_lines, content_results, *selected_index, *scroll_offset, max_name_width, results_area.width, theme);
                    }
                }
            }

            let results = Paragraph::new(result_lines).style(Style::default().bg(theme.background_secondary));
            f.render_widget(results, results_area);

            // Store areas for mouse handling
            app.search_picker_area = popup_area;
            app.search_picker_results_area = results_area;
        }
    }
}

fn render_file_results(
    lines: &mut Vec<Line>,
    results: &[crate::app::FilePickerResult],
    selected_index: usize,
    scroll_offset: usize,
    max_name_width: usize,
    popup_width: u16,
    theme: &crate::config::Theme,
) {
    for (idx, result) in results.iter().enumerate().skip(scroll_offset).take(POPUP_MAX_VISIBLE_ITEMS) {
        let is_selected = idx == selected_index;

        // Truncate display name if too long
        let display_name = if result.display_name.chars().count() > max_name_width {
            let truncated: String = result.display_name.chars().take(max_name_width.saturating_sub(1)).collect();
            format!("{}…", truncated)
        } else {
            result.display_name.clone()
        };

        let style = if is_selected {
            Style::default()
                .fg(theme.background)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.foreground)
        };

        // Main line with title
        if is_selected {
            let content_width = (popup_width as usize).saturating_sub(2);
            let used_width = 1 + display_name.chars().count();
            let padding_right = " ".repeat(content_width.saturating_sub(used_width));
            lines.push(Line::from(vec![
                Span::styled(" ".to_string(), style),
                Span::styled(display_name, style),
                Span::styled(padding_right, style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(display_name, style),
            ]));
        }

        // Folder hint line
        if let Some(ref folder) = result.folder_hint {
            let hint_style = if is_selected {
                Style::default().fg(theme.muted).bg(theme.primary)
            } else {
                Style::default().fg(theme.muted)
            };

            let hint_text = if folder.chars().count() > max_name_width.saturating_sub(2) {
                let truncated: String = folder.chars().take(max_name_width.saturating_sub(3)).collect();
                format!("  {}…", truncated)
            } else {
                format!("  {}", folder)
            };

            if is_selected {
                let content_width = (popup_width as usize).saturating_sub(2);
                let padding_right = " ".repeat(content_width.saturating_sub(hint_text.chars().count()));
                lines.push(Line::from(vec![
                    Span::styled(hint_text, hint_style),
                    Span::styled(padding_right, Style::default().bg(theme.primary)),
                ]));
            } else {
                lines.push(Line::from(Span::styled(hint_text, hint_style)));
            }
        }
    }
}

fn render_content_results(
    lines: &mut Vec<Line>,
    results: &[crate::app::ContentSearchResult],
    selected_index: usize,
    scroll_offset: usize,
    max_name_width: usize,
    popup_width: u16,
    theme: &crate::config::Theme,
) {
    for (idx, result) in results.iter().enumerate().skip(scroll_offset).take(POPUP_MAX_VISIBLE_ITEMS_CONTENT) {
        let is_selected = idx == selected_index;

        let content_width = (popup_width as usize).saturating_sub(2);

        // First line: Note title + line number
        let line_hint = format!(":L{}", result.line_number);
        let available_for_title = max_name_width.saturating_sub(line_hint.len() + 1);

        let display_name = if result.display_name.chars().count() > available_for_title {
            let truncated: String = result.display_name.chars().take(available_for_title.saturating_sub(1)).collect();
            format!("{}…", truncated)
        } else {
            result.display_name.clone()
        };

        let title_style = if is_selected {
            Style::default()
                .fg(theme.background)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.foreground)
        };

        let line_hint_style = if is_selected {
            Style::default().fg(theme.muted).bg(theme.primary)
        } else {
            Style::default().fg(theme.muted)
        };

        if is_selected {
            let used_width = 1 + display_name.chars().count() + line_hint.len();
            let padding = " ".repeat(content_width.saturating_sub(used_width));
            lines.push(Line::from(vec![
                Span::styled(" ".to_string(), title_style),
                Span::styled(display_name, title_style),
                Span::styled(padding, title_style),
                Span::styled(line_hint, line_hint_style),
            ]));
        } else {
            let used_width = 1 + display_name.chars().count() + line_hint.len();
            let padding = " ".repeat(content_width.saturating_sub(used_width));
            lines.push(Line::from(vec![
                Span::raw(" "),
                Span::styled(display_name, title_style),
                Span::raw(padding),
                Span::styled(line_hint, line_hint_style),
            ]));
        }

        // Second line: Matched line with highlight
        let matched_line = &result.matched_line;
        let match_start = result.match_start;
        let match_end = result.match_end;

        // Truncate matched line if needed
        let max_line_width = max_name_width.saturating_sub(2);
        let line_chars: Vec<char> = matched_line.chars().collect();
        let (display_line, adj_start, adj_end) = if line_chars.len() > max_line_width {
            let truncated: String = line_chars.iter().take(max_line_width.saturating_sub(1)).collect();
            let adj_start = match_start.min(max_line_width.saturating_sub(1));
            let adj_end = match_end.min(max_line_width.saturating_sub(1));
            (format!("{}…", truncated), adj_start, adj_end)
        } else {
            (matched_line.clone(), match_start, match_end)
        };

        let match_style = if is_selected {
            Style::default()
                .fg(theme.warning)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD)
        };

        let normal_style = if is_selected {
            Style::default().fg(theme.muted).bg(theme.primary)
        } else {
            Style::default().fg(theme.muted)
        };

        // Split the line into before, match, after parts
        let display_chars: Vec<char> = display_line.chars().collect();
        let before: String = display_chars.iter().take(adj_start).collect();
        let matched: String = display_chars.iter().skip(adj_start).take(adj_end.saturating_sub(adj_start)).collect();
        let after: String = display_chars.iter().skip(adj_end).collect();

        if is_selected {
            let used_width = 2 + display_line.chars().count();
            let padding_right = " ".repeat(content_width.saturating_sub(used_width));
            lines.push(Line::from(vec![
                Span::styled("  ", normal_style),
                Span::styled(before, normal_style),
                Span::styled(matched, match_style),
                Span::styled(after, normal_style),
                Span::styled(padding_right, normal_style),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(before, normal_style),
                Span::styled(matched, match_style),
                Span::styled(after, normal_style),
            ]));
        }

        // Third line: Folder hint (if available)
        if let Some(ref folder) = result.folder_hint {
            let hint_style = if is_selected {
                Style::default().fg(theme.muted).bg(theme.primary)
            } else {
                Style::default().fg(theme.muted)
            };

            let hint_text = if folder.chars().count() > max_name_width.saturating_sub(2) {
                let truncated: String = folder.chars().take(max_name_width.saturating_sub(3)).collect();
                format!("  {}…", truncated)
            } else {
                format!("  {}", folder)
            };

            if is_selected {
                let padding_right = " ".repeat(content_width.saturating_sub(hint_text.chars().count()));
                lines.push(Line::from(vec![
                    Span::styled(hint_text, hint_style),
                    Span::styled(padding_right, Style::default().bg(theme.primary)),
                ]));
            } else {
                lines.push(Line::from(Span::styled(hint_text, hint_style)));
            }
        }
    }
}

/// Compact content results for the left panel (single line per result)
/// Format: "L{num} → {matched line with highlighted query}"
fn render_content_results_compact(
    lines: &mut Vec<Line>,
    results: &[crate::app::ContentSearchResult],
    selected_index: usize,
    scroll_offset: usize,
    _max_name_width: usize,
    area_width: u16,
    theme: &crate::config::Theme,
    query: &str,
) {
    let query_lower = query.to_lowercase();

    for (idx, result) in results.iter().enumerate().skip(scroll_offset).take(POPUP_MAX_VISIBLE_ITEMS_CONTENT) {
        let is_selected = idx == selected_index;
        let content_width = (area_width as usize).saturating_sub(2);

        // Format: "L42 → matched line content"
        let line_prefix = format!("L{} → ", result.line_number);
        let prefix_len = line_prefix.chars().count();
        let available_for_content = content_width.saturating_sub(prefix_len + 1); // +1 for leading space

        // Trim and truncate the matched line
        let matched_line = result.matched_line.trim();
        let display_line: String = if matched_line.chars().count() > available_for_content {
            let truncated: String = matched_line.chars().take(available_for_content.saturating_sub(1)).collect();
            format!("{}…", truncated)
        } else {
            matched_line.to_string()
        };

        let prefix_style = if is_selected {
            Style::default().fg(theme.muted).bg(theme.primary)
        } else {
            Style::default().fg(theme.muted)
        };

        let normal_style = if is_selected {
            Style::default()
                .fg(theme.background)
                .bg(theme.primary)
        } else {
            Style::default().fg(theme.foreground)
        };

        let highlight_style = if is_selected {
            Style::default()
                .fg(theme.warning)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD)
        };

        let mut spans = vec![
            Span::styled(" ", normal_style),
            Span::styled(line_prefix, prefix_style),
        ];

        // Highlight query matches in the line
        if !query_lower.is_empty() {
            let line_lower = display_line.to_lowercase();
            let line_chars: Vec<char> = display_line.chars().collect();
            let line_chars_len = line_chars.len();
            let mut last_end = 0;

            let mut search_start = 0;
            while let Some(byte_pos) = line_lower.get(search_start..).and_then(|s| s.find(&query_lower)) {
                let match_byte_start = search_start + byte_pos;
                let match_char_start = line_lower.get(..match_byte_start).map(|s| s.chars().count()).unwrap_or(0);
                let match_char_end = match_char_start + query_lower.chars().count();

                // Bounds check before slicing
                let safe_char_start = match_char_start.min(line_chars_len);
                let safe_char_end = match_char_end.min(line_chars_len);

                // Add text before match
                if safe_char_start > last_end && last_end < line_chars_len {
                    let before: String = line_chars[last_end..safe_char_start.min(line_chars_len)].iter().collect();
                    spans.push(Span::styled(before, normal_style));
                }

                // Add highlighted match
                if safe_char_start < line_chars_len {
                    let matched: String = line_chars[safe_char_start..safe_char_end].iter().collect();
                    spans.push(Span::styled(matched, highlight_style));
                }

                last_end = safe_char_end;
                search_start = match_byte_start.saturating_add(query_lower.len());
                if search_start >= line_lower.len() {
                    break;
                }
            }

            // Add remaining text
            if last_end < line_chars_len {
                let after: String = line_chars[last_end..].iter().collect();
                spans.push(Span::styled(after, normal_style));
            }
        } else {
            spans.push(Span::styled(display_line.clone(), normal_style));
        }

        // Pad to fill the width
        let used_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let padding_needed = content_width.saturating_sub(used_width);
        if padding_needed > 0 {
            spans.push(Span::styled(" ".repeat(padding_needed), normal_style));
        }

        lines.push(Line::from(spans));
    }
}

/// Render preview panel showing context around the selected match
fn render_preview(
    f: &mut Frame,
    app: &App,
    results: &[crate::app::ContentSearchResult],
    selected_index: usize,
    area: Rect,
    query: &str,
    theme: &crate::config::Theme,
) {
    // Split area: fixed header (2 lines) + scrollable content
    let header_height = 2u16;
    if area.height <= header_height {
        return;
    }

    let header_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: header_height,
    };
    let content_area = Rect {
        x: area.x,
        y: area.y + header_height,
        width: area.width,
        height: area.height - header_height,
    };

    let query_lower = query.to_lowercase();

    if let Some(result) = results.get(selected_index) {
        let note_idx = result.note_index;
        let match_line = result.line_number.saturating_sub(1); // Convert to 0-indexed

        // Render fixed header
        let file_header = if let Some(ref folder) = result.folder_hint {
            format!("{}/{}", folder, result.display_name)
        } else {
            result.display_name.clone()
        };
        let max_header_width = (area.width as usize).saturating_sub(2);
        let display_header: String = if file_header.chars().count() > max_header_width {
            let truncated: String = file_header.chars().take(max_header_width.saturating_sub(1)).collect();
            format!("{}…", truncated)
        } else {
            file_header
        };
        let header_lines = vec![
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(display_header, Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(Span::styled(
                " ".repeat(area.width as usize),
                Style::default().fg(theme.muted),
            )),
        ];
        let header = Paragraph::new(header_lines)
            .style(Style::default().bg(theme.background_secondary));
        f.render_widget(header, header_area);

        // Render scrollable content
        let mut content_lines: Vec<Line> = Vec::new();
        let mut match_display_line: usize = 0;

        if let Some(note_lines) = app.search_index.lines.get(note_idx) {
            let start_line = match_line.saturating_sub(PREVIEW_LINES_BEFORE);
            let end_line = (match_line + PREVIEW_LINES_AFTER + 1).min(note_lines.len());

            let prefix_width = 7; // "  42 │ " = 7 chars
            let content_width = (area.width as usize).saturating_sub(prefix_width);

            for line_num in start_line..end_line {
                if let Some(line_content) = note_lines.get(line_num) {
                    let display_line_num = line_num + 1; // 1-indexed for display
                    let is_match_line = line_num == match_line;

                    if is_match_line {
                        match_display_line = content_lines.len();
                    }

                    let line_num_style = if is_match_line {
                        Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.muted)
                    };
                    let normal_style = Style::default().fg(theme.foreground);
                    let highlight_style = Style::default().fg(theme.warning).add_modifier(Modifier::BOLD);

                    let wrapped_segments = wrap_line(line_content, content_width);

                    for (seg_idx, segment) in wrapped_segments.iter().enumerate() {
                        let prefix = if seg_idx == 0 {
                            format!("{:>4} │ ", display_line_num)
                        } else {
                            "     │ ".to_string()
                        };

                        let mut spans = vec![
                            Span::styled(prefix, line_num_style),
                        ];

                        if !query_lower.is_empty() {
                            let seg_lower = segment.to_lowercase();
                            let seg_chars: Vec<char> = segment.chars().collect();
                            let seg_chars_len = seg_chars.len();
                            let mut last_end = 0;

                            let mut search_start = 0;
                            while let Some(byte_pos) = seg_lower.get(search_start..).and_then(|s| s.find(&query_lower)) {
                                let match_byte_start = search_start + byte_pos;
                                let match_char_start = seg_lower.get(..match_byte_start).map(|s| s.chars().count()).unwrap_or(0);
                                let match_char_end = match_char_start + query_lower.chars().count();

                                // Bounds check before slicing
                                let safe_char_start = match_char_start.min(seg_chars_len);
                                let safe_char_end = match_char_end.min(seg_chars_len);

                                if safe_char_start > last_end && last_end < seg_chars_len {
                                    let before: String = seg_chars[last_end..safe_char_start.min(seg_chars_len)].iter().collect();
                                    spans.push(Span::styled(before, normal_style));
                                }

                                if safe_char_start < seg_chars_len {
                                    let matched: String = seg_chars[safe_char_start..safe_char_end].iter().collect();
                                    spans.push(Span::styled(matched, highlight_style));
                                }

                                last_end = safe_char_end;
                                search_start = match_byte_start.saturating_add(query_lower.len());
                                if search_start >= seg_lower.len() {
                                    break;
                                }
                            }

                            if last_end < seg_chars_len {
                                let after: String = seg_chars[last_end..].iter().collect();
                                spans.push(Span::styled(after, normal_style));
                            }
                        } else {
                            spans.push(Span::styled(segment.clone(), normal_style));
                        }

                        content_lines.push(Line::from(spans));
                    }
                }
            }
        }

        if content_lines.is_empty() {
            content_lines.push(Line::from(Span::styled(
                " No preview available",
                Style::default().fg(theme.muted),
            )));
        }

        // Calculate scroll to ensure match line is visible
        let visible_height = content_area.height as usize;
        let target_position = visible_height / 3;
        let scroll_offset = match_display_line.saturating_sub(target_position);

        let content = Paragraph::new(content_lines)
            .style(Style::default().bg(theme.background_secondary))
            .scroll((scroll_offset as u16, 0));
        f.render_widget(content, content_area);
    } else {
        // No result selected - show empty state
        let empty_lines = vec![
            Line::from(Span::styled(" No preview available", Style::default().fg(theme.muted))),
        ];
        let empty = Paragraph::new(empty_lines)
            .style(Style::default().bg(theme.background_secondary));
        f.render_widget(empty, area);
    }
}

/// Wrap a line of text into segments that fit within max_width (character count)
fn wrap_line(line: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![line.to_string()];
    }

    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= max_width {
        return vec![line.to_string()];
    }

    let mut segments = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + max_width).min(chars.len());
        let segment: String = chars[start..end].iter().collect();
        segments.push(segment);
        start = end;
    }

    if segments.is_empty() {
        segments.push(line.to_string());
    }

    segments
}
