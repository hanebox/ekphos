use unicode_width::UnicodeWidthChar;

pub fn detect_bare_url_len(text: &str, start: usize) -> Option<usize> {
    ekphos_core::markdown::bare_url_len(text, start)
}

pub fn calc_formatting_shrinkage(text: &str, up_to_pos: usize) -> usize {
    if !text.as_bytes().iter().any(|byte| matches!(byte, b'*' | b'_' | b'~' | b'`' | b'[' | b'!')) {
        return 0;
    }
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
        if chars[pos] == '!' && pos + 2 < chars.len() && chars[pos + 1] == '!' && chars[pos + 2] == '[' {
            if let Some((bracket_end, paren_end)) = find_markdown_link(&chars, pos + 2) {
                let image_end = paren_end + 1;
                if image_end <= up_to_pos {
                    let full_width = image_end - pos;
                    let alt_width = bracket_end.saturating_sub(pos + 3);
                    shrinkage += full_width.saturating_sub(alt_width);
                }
                pos = image_end;
                continue;
            }
        }
        if chars[pos] == '!' && pos + 1 < chars.len() && chars[pos + 1] == '[' && (pos == 0 || chars[pos - 1] != '!') {
            if let Some((_, paren_end)) = find_markdown_link(&chars, pos + 1) {
                let image_end = paren_end + 1;
                shrinkage += image_end.min(up_to_pos).saturating_sub(pos);
                pos = image_end;
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
                    shrinkage += url_len + 4;
                } else if bracket_end < up_to_pos {
                    shrinkage += 1;
                }
                pos = paren_end + 1;
                continue;
            }
        }
        if chars[pos] == 'h' {
            let byte_pos: usize = chars[..pos].iter().map(|c| c.len_utf8()).sum();
            if let Some(url_len) = detect_bare_url_len(text, byte_pos) {
                let url_char_count = text[byte_pos..byte_pos + url_len].chars().count();
                pos += url_char_count;
                continue;
            }
        }
        pos += 1;
    }
    shrinkage
}
pub fn find_double_marker(chars: &[char], start: usize, marker: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == marker && chars[i + 1] == marker {
            return Some(i);
        }
        i += 1;
    }
    None
}
pub fn find_single_marker(chars: &[char], start: usize, marker: char) -> Option<usize> {
    for i in start..chars.len() {
        if chars[i] == marker {
            if (marker == '*' || marker == '_') && i + 1 < chars.len() && chars[i + 1] == marker {
                continue;
            }
            return Some(i);
        }
    }
    None
}

pub fn find_wiki_link_end(chars: &[char], start: usize) -> Option<usize> {
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

pub fn find_markdown_link(chars: &[char], start: usize) -> Option<(usize, usize)> {
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

pub fn cell_visible_width(cell: &str) -> usize {
    let display_width: usize = cell.chars().map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) }).sum();
    display_width.saturating_sub(calc_formatting_shrinkage(cell, cell.chars().count()))
}
