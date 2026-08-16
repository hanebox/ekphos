/// If `text[start..]` begins with a bare `http://` or `https://` URL, return the
/// byte length of the URL (trailing sentence punctuation stripped). Used for
/// GFM-style autolinking both in rendering and in the Enter-to-open path.
pub(crate) fn detect_bare_url_len(text: &str, start: usize) -> Option<usize> {
    ekphos_core::markdown::bare_url_len(text, start)
}

pub(super) fn find_wiki_link_end(chars: &[char], start: usize) -> Option<usize> {
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

pub(super) fn find_markdown_link(chars: &[char], start: usize) -> Option<(usize, usize)> {
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
