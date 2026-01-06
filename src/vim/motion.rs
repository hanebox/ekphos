//! Vim motions

use crate::editor::Position;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum Motion {
    Left,
    Right,
    WordForward,
    WordBackward,
    WordEndForward,
    BigWordForward,
    BigWordBackward,
    BigWordEndForward,
    WordEndBackward,
    BigWordEndBackward,
    LineStart,
    FirstNonBlank,
    LineEnd,
    Up,
    Down,
    DocumentStart,
    DocumentEnd,
    GoToLine(usize),
    ParagraphForward,
    ParagraphBackward,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
    MatchingBracket,
    FindChar,
    RepeatFind,
    RepeatFindReverse,
    SearchNext,
    SearchPrev,
}

impl Motion {
    #[allow(dead_code)]
    pub fn is_linewise(&self) -> bool {
        matches!(
            self,
            Motion::Up
                | Motion::Down
                | Motion::DocumentStart
                | Motion::DocumentEnd
                | Motion::GoToLine(_)
                | Motion::ParagraphForward
                | Motion::ParagraphBackward
                | Motion::ScreenTop
                | Motion::ScreenMiddle
                | Motion::ScreenBottom
        )
    }

    #[allow(dead_code)]
    pub fn is_exclusive(&self) -> bool {
        matches!(
            self,
            Motion::Left
                | Motion::Right
                | Motion::WordForward
                | Motion::BigWordForward
                | Motion::WordBackward
                | Motion::BigWordBackward
        )
    }
}

#[allow(dead_code)]
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[allow(dead_code)]
pub fn find_word_forward(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    if col >= len {
        return len;
    }

    let mut pos = col;
    let start_char = chars[pos];
    let is_word = is_word_char(start_char);
    let is_space = start_char.is_whitespace();

    if is_space {
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
    } else if is_word {
        while pos < len && is_word_char(chars[pos]) {
            pos += 1;
        }
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
    } else {
        while pos < len && !is_word_char(chars[pos]) && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
    }

    pos
}

#[allow(dead_code)]
pub fn find_word_back(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();

    if col == 0 || chars.is_empty() {
        return 0;
    }

    let mut pos = col.min(chars.len()).saturating_sub(1);

    while pos > 0 && chars[pos].is_whitespace() {
        pos -= 1;
    }

    if pos == 0 && chars[0].is_whitespace() {
        return 0;
    }

    let is_word = is_word_char(chars[pos]);

    while pos > 0 {
        let prev = chars[pos - 1];
        let prev_is_word = is_word_char(prev);
        if prev.is_whitespace() || prev_is_word != is_word {
            break;
        }
        pos -= 1;
    }

    pos
}

#[allow(dead_code)]
pub fn find_word_end_forward(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    if col >= len.saturating_sub(1) {
        return len.saturating_sub(1);
    }

    let mut pos = col + 1;

    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }

    if pos >= len {
        return len.saturating_sub(1);
    }

    let is_word = is_word_char(chars[pos]);

    while pos < len.saturating_sub(1) {
        let next = chars[pos + 1];
        let next_is_word = is_word_char(next);
        if next.is_whitespace() || next_is_word != is_word {
            break;
        }
        pos += 1;
    }

    pos
}

#[allow(dead_code)]
pub fn find_big_word_forward(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    if col >= len {
        return len;
    }

    let mut pos = col;

    while pos < len && !chars[pos].is_whitespace() {
        pos += 1;
    }

    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }

    pos
}

#[allow(dead_code)]
pub fn find_big_word_back(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();

    if col == 0 || chars.is_empty() {
        return 0;
    }

    let mut pos = col.min(chars.len()).saturating_sub(1);

    while pos > 0 && chars[pos].is_whitespace() {
        pos -= 1;
    }

    while pos > 0 && !chars[pos - 1].is_whitespace() {
        pos -= 1;
    }

    pos
}

#[allow(dead_code)]
pub fn find_big_word_end_forward(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    if col >= len.saturating_sub(1) {
        return len.saturating_sub(1);
    }

    let mut pos = col + 1;

    while pos < len && chars[pos].is_whitespace() {
        pos += 1;
    }

    while pos < len.saturating_sub(1) && !chars[pos + 1].is_whitespace() {
        pos += 1;
    }

    pos.min(len.saturating_sub(1))
}

#[allow(dead_code)]
pub fn find_first_non_blank(line: &str) -> usize {
    line.chars()
        .position(|c| !c.is_whitespace())
        .unwrap_or(0)
}

#[allow(dead_code)]
pub fn find_matching_bracket(lines: &[&str], pos: Position) -> Option<Position> {
    let line = lines.get(pos.row)?;
    let chars: Vec<char> = line.chars().collect();
    let current = *chars.get(pos.col)?;

    let (open, close, forward) = match current {
        '(' => ('(', ')', true),
        ')' => ('(', ')', false),
        '[' => ('[', ']', true),
        ']' => ('[', ']', false),
        '{' => ('{', '}', true),
        '}' => ('{', '}', false),
        '<' => ('<', '>', true),
        '>' => ('<', '>', false),
        _ => return None,
    };

    let mut depth = 1;
    let mut row = pos.row;
    let mut col = pos.col;

    if forward {
        col += 1;
        loop {
            let line_chars: Vec<char> = lines.get(row)?.chars().collect();
            while col < line_chars.len() {
                let c = line_chars[col];
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position::new(row, col));
                    }
                }
                col += 1;
            }
            row += 1;
            col = 0;
            if row >= lines.len() {
                return None;
            }
        }
    } else {
        if col == 0 {
            if row == 0 {
                return None;
            }
            row -= 1;
            col = lines.get(row)?.chars().count();
        } else {
            col -= 1;
        }
        loop {
            let line_chars: Vec<char> = lines.get(row)?.chars().collect();
            loop {
                let c = *line_chars.get(col)?;
                if c == close {
                    depth += 1;
                } else if c == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position::new(row, col));
                    }
                }
                if col == 0 {
                    break;
                }
                col -= 1;
            }
            if row == 0 {
                return None;
            }
            row -= 1;
            col = lines.get(row).map(|l| l.chars().count().saturating_sub(1)).unwrap_or(0);
        }
    }
}

#[allow(dead_code)]
pub fn find_paragraph_forward(lines: &[&str], row: usize) -> usize {
    let mut r = row;

    while r < lines.len() && !lines[r].trim().is_empty() {
        r += 1;
    }

    while r < lines.len() && lines[r].trim().is_empty() {
        r += 1;
    }

    r.min(lines.len().saturating_sub(1))
}

#[allow(dead_code)]
pub fn find_paragraph_backward(lines: &[&str], row: usize) -> usize {
    if row == 0 {
        return 0;
    }

    let mut r = row;

    while r > 0 && lines[r].trim().is_empty() {
        r -= 1;
    }

    while r > 0 && !lines[r - 1].trim().is_empty() {
        r -= 1;
    }

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Word Forward Tests ====================

    #[test]
    fn test_word_forward_basic() {
        assert_eq!(find_word_forward("hello world", 0), 6);
        assert_eq!(find_word_forward("hello world", 6), 11);
    }

    #[test]
    fn test_word_forward_punctuation() {
        assert_eq!(find_word_forward("foo.bar", 0), 3);
        assert_eq!(find_word_forward("foo.bar", 3), 4);
        assert_eq!(find_word_forward("foo..bar", 3), 5);
    }

    #[test]
    fn test_word_forward_empty_string() {
        assert_eq!(find_word_forward("", 0), 0);
    }

    #[test]
    fn test_word_forward_single_char() {
        assert_eq!(find_word_forward("a", 0), 1);
        assert_eq!(find_word_forward(".", 0), 1);
    }

    #[test]
    fn test_word_forward_multiple_spaces() {
        assert_eq!(find_word_forward("hello    world", 0), 9);
        assert_eq!(find_word_forward("   hello", 0), 3);
    }

    #[test]
    fn test_word_forward_at_end() {
        assert_eq!(find_word_forward("hello", 5), 5);
        assert_eq!(find_word_forward("hello", 10), 5);
    }

    #[test]
    fn test_word_forward_underscore() {
        assert_eq!(find_word_forward("foo_bar baz", 0), 8);
        assert_eq!(find_word_forward("_private var", 0), 9);
    }

    #[test]
    fn test_word_forward_numbers() {
        assert_eq!(find_word_forward("var123 next", 0), 7);
        assert_eq!(find_word_forward("123abc def", 0), 7);
    }

    // ==================== Word Back Tests ====================

    #[test]
    fn test_word_back_basic() {
        assert_eq!(find_word_back("hello world", 11), 6);
        assert_eq!(find_word_back("hello world", 6), 0);
    }

    #[test]
    fn test_word_back_punctuation() {
        assert_eq!(find_word_back("foo.bar", 7), 4);
        assert_eq!(find_word_back("foo.bar", 4), 3);
        assert_eq!(find_word_back("foo.bar", 3), 0);
    }

    #[test]
    fn test_word_back_empty_string() {
        assert_eq!(find_word_back("", 0), 0);
    }

    #[test]
    fn test_word_back_at_start() {
        assert_eq!(find_word_back("hello", 0), 0);
    }

    #[test]
    fn test_word_back_multiple_spaces() {
        assert_eq!(find_word_back("hello    world", 14), 9);
        assert_eq!(find_word_back("hello    world", 9), 0);
    }

    #[test]
    fn test_word_back_col_beyond_line() {
        assert_eq!(find_word_back("hello", 100), 0);
    }

    // ==================== Word End Forward Tests ====================

    #[test]
    fn test_word_end_forward_basic() {
        assert_eq!(find_word_end_forward("hello world", 0), 4);
        assert_eq!(find_word_end_forward("hello world", 4), 10);
    }

    #[test]
    fn test_word_end_forward_short_words() {
        assert_eq!(find_word_end_forward("a b c", 0), 2);
        assert_eq!(find_word_end_forward("a b c", 2), 4);
    }

    #[test]
    fn test_word_end_forward_at_end() {
        assert_eq!(find_word_end_forward("hello", 4), 4);
    }

    #[test]
    fn test_word_end_forward_punctuation() {
        assert_eq!(find_word_end_forward("foo.bar", 0), 2);
        assert_eq!(find_word_end_forward("foo.bar", 3), 6);
    }

    #[test]
    fn test_word_end_forward_multiple_spaces() {
        assert_eq!(find_word_end_forward("hello    world", 4), 13);
    }

    // ==================== Big Word Forward Tests ====================

    #[test]
    fn test_big_word_forward_basic() {
        assert_eq!(find_big_word_forward("hello world", 0), 6);
    }

    #[test]
    fn test_big_word_forward_punctuation() {
        assert_eq!(find_big_word_forward("foo.bar baz", 0), 8);
        assert_eq!(find_big_word_forward("foo.bar.baz qux", 0), 12);
    }

    #[test]
    fn test_big_word_forward_empty() {
        assert_eq!(find_big_word_forward("", 0), 0);
    }

    #[test]
    fn test_big_word_forward_at_end() {
        assert_eq!(find_big_word_forward("hello", 5), 5);
    }

    // ==================== Big Word Back Tests ====================

    #[test]
    fn test_big_word_back_basic() {
        assert_eq!(find_big_word_back("hello world", 11), 6);
    }

    #[test]
    fn test_big_word_back_punctuation() {
        assert_eq!(find_big_word_back("foo.bar baz", 11), 8);
        assert_eq!(find_big_word_back("foo.bar baz", 8), 0);
    }

    #[test]
    fn test_big_word_back_empty() {
        assert_eq!(find_big_word_back("", 0), 0);
    }

    #[test]
    fn test_big_word_back_at_start() {
        assert_eq!(find_big_word_back("hello", 0), 0);
    }

    // ==================== Big Word End Forward Tests ====================

    #[test]
    fn test_big_word_end_forward_basic() {
        assert_eq!(find_big_word_end_forward("hello world", 0), 4);
    }

    #[test]
    fn test_big_word_end_forward_punctuation() {
        assert_eq!(find_big_word_end_forward("foo.bar baz", 0), 6);
    }

    #[test]
    fn test_big_word_end_forward_at_end() {
        assert_eq!(find_big_word_end_forward("hello", 4), 4);
    }

    // ==================== First Non Blank Tests ====================

    #[test]
    fn test_first_non_blank_no_indent() {
        assert_eq!(find_first_non_blank("hello"), 0);
    }

    #[test]
    fn test_first_non_blank_spaces() {
        assert_eq!(find_first_non_blank("  hello"), 2);
        assert_eq!(find_first_non_blank("    code"), 4);
    }

    #[test]
    fn test_first_non_blank_tabs() {
        assert_eq!(find_first_non_blank("\t\thello"), 2);
        assert_eq!(find_first_non_blank("\thello"), 1);
    }

    #[test]
    fn test_first_non_blank_mixed() {
        assert_eq!(find_first_non_blank(" \t hello"), 3);
    }

    #[test]
    fn test_first_non_blank_all_whitespace() {
        assert_eq!(find_first_non_blank("   "), 0);
        assert_eq!(find_first_non_blank("\t\t"), 0);
    }

    #[test]
    fn test_first_non_blank_empty() {
        assert_eq!(find_first_non_blank(""), 0);
    }

    // ==================== Matching Bracket Tests ====================

    #[test]
    fn test_matching_bracket_parens() {
        let lines = vec!["(hello)"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 6)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 6)), Some(Position::new(0, 0)));
    }

    #[test]
    fn test_matching_bracket_square() {
        let lines = vec!["[hello]"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 6)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 6)), Some(Position::new(0, 0)));
    }

    #[test]
    fn test_matching_bracket_curly() {
        let lines = vec!["{hello}"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 6)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 6)), Some(Position::new(0, 0)));
    }

    #[test]
    fn test_matching_bracket_angle() {
        let lines = vec!["<hello>"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 6)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 6)), Some(Position::new(0, 0)));
    }

    #[test]
    fn test_matching_bracket_nested() {
        let lines = vec!["((a))"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 4)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 1)), Some(Position::new(0, 3)));
    }

    #[test]
    fn test_matching_bracket_deep_nested() {
        let lines = vec!["(((a)))"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 6)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 2)), Some(Position::new(0, 4)));
    }

    #[test]
    fn test_matching_bracket_multiline() {
        let lines = vec!["(", "hello", ")"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(2, 0)));
    }

    #[test]
    fn test_matching_bracket_unmatched() {
        let lines = vec!["(hello"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), None);
    }

    #[test]
    fn test_matching_bracket_not_on_bracket() {
        let lines = vec!["hello"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), None);
    }

    #[test]
    fn test_matching_bracket_empty_lines() {
        let lines: Vec<&str> = vec![];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), None);
    }

    #[test]
    fn test_matching_bracket_mixed_types() {
        let lines = vec!["({[]})"];
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 0)), Some(Position::new(0, 5)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 1)), Some(Position::new(0, 4)));
        assert_eq!(find_matching_bracket(&lines, Position::new(0, 2)), Some(Position::new(0, 3)));
    }

    // ==================== Paragraph Tests ====================

    #[test]
    fn test_paragraph_forward_basic() {
        let lines = vec!["line1", "line2", "", "line3"];
        assert_eq!(find_paragraph_forward(&lines, 0), 3);
    }

    #[test]
    fn test_paragraph_forward_multiple_empty() {
        let lines = vec!["line1", "", "", "line2"];
        assert_eq!(find_paragraph_forward(&lines, 0), 3);
    }

    #[test]
    fn test_paragraph_forward_at_end() {
        let lines = vec!["line1", "line2"];
        assert_eq!(find_paragraph_forward(&lines, 1), 1);
    }

    #[test]
    fn test_paragraph_forward_empty_doc() {
        let lines = vec![""];
        assert_eq!(find_paragraph_forward(&lines, 0), 0);
    }

    #[test]
    fn test_paragraph_forward_whitespace_only_line() {
        let lines = vec!["line1", "   ", "line2"];
        assert_eq!(find_paragraph_forward(&lines, 0), 2);
    }

    #[test]
    fn test_paragraph_backward_basic() {
        let lines = vec!["line1", "", "line2", "line3"];
        assert_eq!(find_paragraph_backward(&lines, 3), 2);
    }

    #[test]
    fn test_paragraph_backward_at_start() {
        let lines = vec!["line1", "line2"];
        assert_eq!(find_paragraph_backward(&lines, 0), 0);
    }

    #[test]
    fn test_paragraph_backward_multiple_empty() {
        let lines = vec!["line1", "", "", "line2"];
        assert_eq!(find_paragraph_backward(&lines, 3), 3);
    }

    // ==================== Motion Classification Tests ====================

    #[test]
    fn test_motion_is_linewise() {
        assert!(Motion::Up.is_linewise());
        assert!(Motion::Down.is_linewise());
        assert!(Motion::DocumentStart.is_linewise());
        assert!(Motion::DocumentEnd.is_linewise());
        assert!(Motion::GoToLine(5).is_linewise());
        assert!(Motion::ParagraphForward.is_linewise());
        assert!(Motion::ParagraphBackward.is_linewise());
        assert!(Motion::ScreenTop.is_linewise());
        assert!(Motion::ScreenMiddle.is_linewise());
        assert!(Motion::ScreenBottom.is_linewise());
        assert!(!Motion::Left.is_linewise());
        assert!(!Motion::Right.is_linewise());
        assert!(!Motion::WordForward.is_linewise());
        assert!(!Motion::LineEnd.is_linewise());
    }

    #[test]
    fn test_motion_is_exclusive() {
        assert!(Motion::Left.is_exclusive());
        assert!(Motion::Right.is_exclusive());
        assert!(Motion::WordForward.is_exclusive());
        assert!(Motion::BigWordForward.is_exclusive());
        assert!(Motion::WordBackward.is_exclusive());
        assert!(Motion::BigWordBackward.is_exclusive());
        assert!(!Motion::LineEnd.is_exclusive());
        assert!(!Motion::WordEndForward.is_exclusive());
        assert!(!Motion::Up.is_exclusive());
        assert!(!Motion::Down.is_exclusive());
    }

    // ==================== is_word_char Tests ====================

    #[test]
    fn test_is_word_char_letters() {
        assert!(is_word_char('a'));
        assert!(is_word_char('z'));
        assert!(is_word_char('A'));
        assert!(is_word_char('Z'));
    }

    #[test]
    fn test_is_word_char_numbers() {
        assert!(is_word_char('0'));
        assert!(is_word_char('9'));
    }

    #[test]
    fn test_is_word_char_underscore() {
        assert!(is_word_char('_'));
    }

    #[test]
    fn test_is_word_char_punctuation() {
        assert!(!is_word_char('.'));
        assert!(!is_word_char(','));
        assert!(!is_word_char('!'));
        assert!(!is_word_char('-'));
        assert!(!is_word_char(' '));
    }
}
