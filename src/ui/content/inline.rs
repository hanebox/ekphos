use super::*;

pub(super) use ekphos_integrations::text::calc_formatting_shrinkage;

pub(super) fn parse_inline_formatting<'a, F>(text: &'a str, theme: &Theme, selected_link: Option<usize>, wiki_link_validator: Option<F>) -> Vec<Span<'a>>
where
    F: Fn(&str) -> bool,
{
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut current_start = 0;
    let mut link_index = 0;
    let mut removed_preview_image = false;
    let content_theme = &theme.content;
    while let Some((i, c)) = chars.next() {
        if c == 'h' {
            if let Some(url_len) = detect_bare_url_len(text, i) {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
                let is_selected = selected_link == Some(link_index);
                let style = if is_selected { Style::default().fg(theme.background).bg(theme.warning).add_modifier(Modifier::BOLD) } else { Style::default().fg(content_theme.link).add_modifier(Modifier::UNDERLINED) };
                spans.push(Span::styled(&text[i..i + url_len], style));
                link_index += 1;
                let url_chars = text[i..i + url_len].chars().count();
                for _ in 1..url_chars {
                    chars.next();
                }
                current_start = i + url_len;
                continue;
            }
        }
        if c == '*' {
            if let Some(&(_, '*')) = chars.peek() {
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
                    spans.push(Span::styled(&text[bold_start..end], Style::default().fg(content_theme.text).add_modifier(Modifier::BOLD)));
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
                    if ch == '*' && chars.peek().map(|&(_, c)| c != '*').unwrap_or(true) {
                        italic_end = Some(j);
                        break;
                    }
                }
                if let Some(end) = italic_end {
                    spans.push(Span::styled(&text[italic_start..end], Style::default().fg(content_theme.text).add_modifier(Modifier::ITALIC)));
                    current_start = end + 1;
                } else {
                    current_start = i;
                }
                continue;
            }
        }
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
                    spans.push(Span::styled(&text[bold_start..end], Style::default().fg(content_theme.text).add_modifier(Modifier::BOLD)));
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
                    if ch == '_' && chars.peek().map(|&(_, c)| c != '_').unwrap_or(true) {
                        italic_end = Some(j);
                        break;
                    }
                }
                if let Some(end) = italic_end {
                    spans.push(Span::styled(&text[italic_start..end], Style::default().fg(content_theme.text).add_modifier(Modifier::ITALIC)));
                    current_start = end + 1;
                } else {
                    current_start = i;
                }
                continue;
            }
        }
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
                    spans.push(Span::styled(&text[strike_start..end], Style::default().fg(content_theme.text).add_modifier(Modifier::CROSSED_OUT)));
                    current_start = end + 2;
                } else {
                    current_start = i;
                }
                continue;
            }
        }
        if c == '`' {
            if i > current_start {
                spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
            }
            let code_start = i + 1;
            let mut code_end = None;
            for (j, ch) in chars.by_ref() {
                if ch == '`' {
                    code_end = Some(j);
                    break;
                }
            }
            if let Some(end) = code_end {
                spans.push(Span::styled(&text[code_start..end], Style::default().fg(content_theme.code).bg(content_theme.code_background)));
                current_start = end + 1;
            } else {
                current_start = i;
            }
            continue;
        }
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
                        let display_text = if alt_text.is_empty() { image_url.to_string() } else { alt_text.to_string() };
                        let is_selected = selected_link == Some(link_index);
                        let style = if is_selected { Style::default().fg(theme.background).bg(theme.warning).add_modifier(Modifier::BOLD) } else { Style::default().fg(content_theme.link).add_modifier(Modifier::UNDERLINED) };
                        spans.push(Span::styled(display_text, style));
                        link_index += 1;
                        let total_link_len = 2 + bracket_end + 2 + paren_end + 1; // !![alt](url)
                        let link_end = i + total_link_len;
                        while chars.peek().is_some_and(|&(j, _)| j < link_end) {
                            chars.next();
                        }
                        current_start = link_end;
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
                        removed_preview_image = true;
                        let total_link_len = 1 + bracket_end + 2 + paren_end + 1;
                        let link_end = i + total_link_len;
                        while chars.peek().is_some_and(|&(j, _)| j < link_end) {
                            chars.next();
                        }
                        current_start = link_end;
                        continue;
                    }
                }
            }
        }
        if c == '[' {
            if let Some(link) = ekphos_core::markdown::wiki_link_at(text, i) {
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                }
                let is_selected = selected_link == Some(link_index);
                let is_valid = wiki_link_validator.as_ref().map(|validator| validator(link.target)).unwrap_or(false);
                let style = if is_selected {
                    Style::default().fg(theme.background).bg(theme.warning).add_modifier(Modifier::BOLD)
                } else if is_valid {
                    Style::default().fg(content_theme.link).add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default().fg(content_theme.link_invalid).add_modifier(Modifier::UNDERLINED)
                };
                spans.push(Span::styled(link.display_text().to_string(), style));
                link_index += 1;
                while chars.peek().is_some_and(|&(next, _)| next < link.range.end) {
                    chars.next();
                }
                current_start = link.range.end;
                continue;
            }
            if let Some(link) = ekphos_core::markdown::markdown_link_at(text, i) {
                if link.kind == ekphos_core::markdown::MarkdownLinkKind::Link {
                    if i > current_start {
                        spans.push(Span::styled(&text[current_start..i], Style::default().fg(content_theme.text)));
                    }
                    let display_text = if link.label.is_empty() { link.destination } else { link.label };
                    let is_selected = selected_link == Some(link_index);
                    let style = if is_selected { Style::default().fg(theme.background).bg(theme.warning).add_modifier(Modifier::BOLD) } else { Style::default().fg(content_theme.link).add_modifier(Modifier::UNDERLINED) };
                    spans.push(Span::styled(display_text.to_string(), style));
                    link_index += 1;
                    while chars.peek().is_some_and(|&(next, _)| next < link.range.end) {
                        chars.next();
                    }
                    current_start = link.range.end;
                    continue;
                }
            }
        }
    }
    if current_start < text.len() {
        spans.push(Span::styled(&text[current_start..], Style::default().fg(content_theme.text)));
    }
    if removed_preview_image {
        while spans.first().is_some_and(|span| span.content.trim().is_empty()) {
            spans.remove(0);
        }
        while spans.last().is_some_and(|span| span.content.trim().is_empty()) {
            spans.pop();
        }
        if let Some(first) = spans.first_mut() {
            let trimmed = first.content.trim_start().to_string();
            first.content = trimmed.into();
        }
        if let Some(last) = spans.last_mut() {
            let trimmed = last.content.trim_end().to_string();
            last.content = trimmed.into();
        }
    }
    if spans.is_empty() && !removed_preview_image {
        spans.push(Span::styled(text, Style::default().fg(content_theme.text)));
    }
    spans
}
