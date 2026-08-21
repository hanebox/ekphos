use super::*;

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
pub(crate) fn distribute_spans_across_lines(spans: Vec<Span<'_>>, width: usize, plain_text_color: ratatui::style::Color) -> Vec<Vec<Span<'static>>> {
    if width == 0 {
        let owned: Vec<Span<'static>> = spans.into_iter().map(|s| Span::styled(s.content.into_owned(), s.style)).collect();
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
pub(super) fn split_plain_at_whitespace(s: &str, max_width: usize) -> (&str, &str) {
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
pub(super) fn is_plain_text_span(style: &Style, plain_color: ratatui::style::Color) -> bool {
    style.bg.is_none() && style.add_modifier.is_empty() && style.sub_modifier.is_empty() && (style.fg.is_none() || style.fg == Some(plain_color.into()))
}

/// Split a string into a `(head, tail)` pair where `head` has display width `<= width`.
/// Used by `wrap_cell` for hard-breaking over-width words.
pub(super) fn take_width(s: &str, width: usize) -> (&str, &str) {
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

pub(super) fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

pub(super) fn display_width(s: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    s.width()
}

/// Map a terminal cell in a wrapped line back to its column in the original
/// rendered line. The link ranges stored by `App` use that unwrapped rendered
/// coordinate space.
///
/// This deliberately consumes the output of `wrap_line_for_cursor` instead of
/// approximating its word wrapping. In particular, the wrapper drops whitespace
/// at line boundaries and repeats the visual prefix on continuation rows.
pub(super) fn rendered_col_for_wrapped_click(
    spans: Vec<Span<'_>>,
    available_width: usize,
    visual_row: usize,
    visual_col: usize,
    theme: &Theme,
) -> Option<usize> {
    let mut prefix_width = 0usize;
    let mut original_content = String::new();

    for (i, span) in spans.iter().enumerate() {
        let span_text = span.content.as_ref();
        let span_width = display_width(span_text);
        if i == 0 || (i == 1 && span_width <= 3 && !span_text.chars().any(|c| c.is_alphanumeric())) {
            prefix_width += span_width;
        } else {
            original_content.push_str(span_text);
        }
    }

    let wrapped_lines = wrap_line_for_cursor(spans, available_width, theme);
    let mut search_start = 0usize;

    for (row, line) in wrapped_lines.iter().enumerate() {
        let rendered_line: String = line.spans.iter().map(|span| span.content.as_ref()).collect();

        let mut skipped_width = 0usize;
        let mut content_start = rendered_line.len();
        for (byte_idx, ch) in rendered_line.char_indices() {
            if skipped_width >= prefix_width {
                content_start = byte_idx;
                break;
            }
            skipped_width += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        if skipped_width == prefix_width && prefix_width == display_width(&rendered_line) {
            content_start = rendered_line.len();
        }

        let row_content = &rendered_line[content_start..];
        let relative_start = original_content.get(search_start..)?.find(row_content)?;
        let row_start = search_start + relative_start;

        if row == visual_row {
            let row_col = visual_col.checked_sub(prefix_width)?;
            if row_col >= display_width(row_content) {
                return None;
            }
            let logical_before_row = display_width(&original_content[..row_start]);
            return Some(prefix_width + logical_before_row + row_col);
        }

        search_start = row_start + row_content.len();
    }

    None
}

/// Convert mouse coordinates for prose/task items into the unwrapped rendered
/// column used by link metadata. Other item types retain their existing
/// single-row coordinate behavior.
pub(crate) fn content_item_click_col(app: &App, index: usize, item_area: Rect, mouse_x: u16, mouse_y: u16) -> Option<usize> {
    let visual_row = mouse_y.saturating_sub(item_area.y) as usize;
    let visual_col = mouse_x.saturating_sub(item_area.x) as usize;
    let available_width = (item_area.width as usize).saturating_sub(1);
    let cursor_indicator = if app.content_cursor == index { "▶ " } else { "  " };

    match app.content_items.get(index)? {
        ContentItem::TextLine { range, .. } => {
            let raw_line = app.document_slice(*range);
            let line = normalize_whitespace(raw_line);
            let mut spans = if line.starts_with("- ") || line.starts_with("* ") {
                let mut spans = vec![Span::styled(cursor_indicator, Style::default()), Span::styled("• ", Style::default())];
                spans.extend(parse_inline_formatting::<fn(&str) -> bool>(&line[2..], &app.theme, None, None));
                spans
            } else if line.starts_with("> ") {
                let mut spans = vec![Span::styled(cursor_indicator, Style::default()), Span::styled("┃ ", Style::default())];
                spans.extend(parse_inline_formatting::<fn(&str) -> bool>(&line[2..], &app.theme, None, None));
                spans
            } else {
                let mut spans = vec![Span::styled(cursor_indicator, Style::default())];
                spans.extend(parse_inline_formatting::<fn(&str) -> bool>(&line, &app.theme, None, None));
                spans
            };

            // `render_content_line` appends this hint while a linked item is
            // selected or hovered. It is after all links, but including it keeps
            // the fast-path/wrapped-path choice identical to the renderer.
            if app.item_has_link_at(index) && (app.content_cursor == index || app.mouse_hover_item == Some(index)) {
                spans.push(Span::styled(" Open ↗", Style::default()));
            }

            rendered_col_for_wrapped_click(spans, available_width, visual_row, visual_col, &app.theme)
        }
        ContentItem::TaskItem { text, checked, indent, .. } => {
            let expanded_text = expand_tabs(app.document_slice(*text));
            let mut spans = vec![Span::styled(cursor_indicator, Style::default())];
            if *indent > 0 {
                spans.push(Span::styled(" ".repeat(*indent as usize), Style::default()));
            }
            spans.extend([
                Span::styled("[", Style::default()),
                Span::styled(if *checked { "x" } else { " " }, Style::default()),
                Span::styled("]", Style::default()),
                Span::styled(" ", Style::default()),
            ]);
            spans.extend(parse_inline_formatting::<fn(&str) -> bool>(&expanded_text, &app.theme, None, None));

            rendered_col_for_wrapped_click(spans, available_width, visual_row, visual_col, &app.theme)
        }
        _ => Some(visual_col),
    }
}

/// Represents a word segment with its style for word-based wrapping
pub(super) struct StyledWord {
    text: String,
    style: Style,
    width: usize,
}

pub(super) fn wrap_line_for_cursor<'a>(first_line_spans: Vec<Span<'a>>, available_width: usize, _theme: &Theme) -> Vec<Line<'a>> {
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

    let content_width: usize = content_spans.iter().map(|s| display_width(&s.content)).sum();
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
pub(super) fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => result.push_str("    "),             // Tab to 4 spaces
            '\u{00A0}' => result.push(' '),              // Non-breaking space
            '\u{2000}'..='\u{200B}' => result.push(' '), // Various Unicode spaces
            '\u{202F}' => result.push(' '),              // Narrow no-break space
            '\u{205F}' => result.push(' '),              // Medium mathematical space
            '\u{3000}' => result.push(' '),              // Ideographic space
            _ => result.push(c),
        }
    }
    result
}
