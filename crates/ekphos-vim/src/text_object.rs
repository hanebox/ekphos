//! Vim text objects (iw, aw, i", a(, etc.)

use ekphos_editor::{EditorSnapshot, Position};

use crate::char_to_byte_index;

trait LineSource {
    fn line(&self, row: usize) -> Option<&str>;
    fn line_count(&self) -> usize;
}

struct BorrowedLines<'a>(&'a [&'a str]);

impl LineSource for BorrowedLines<'_> {
    fn line(&self, row: usize) -> Option<&str> {
        self.0.get(row).copied()
    }

    fn line_count(&self) -> usize {
        self.0.len()
    }
}

impl LineSource for EditorSnapshot {
    fn line(&self, row: usize) -> Option<&str> {
        self.line(row)
    }

    fn line_count(&self) -> usize {
        self.line_count()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObject {
    Word,
    BigWord,
    Paragraph,
    SingleQuote,
    DoubleQuote,
    BackQuote,
    Parentheses,
    Brackets,
    Braces,
    AngleBrackets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjectScope {
    Inner,
    Around,
}

impl TextObject {
    pub fn parse(first: char, second: char) -> Option<(TextObjectScope, TextObject)> {
        let scope = match first {
            'i' => TextObjectScope::Inner,
            'a' => TextObjectScope::Around,
            _ => return None,
        };

        let object = match second {
            'w' => TextObject::Word,
            'W' => TextObject::BigWord,
            'p' => TextObject::Paragraph,
            '\'' => TextObject::SingleQuote,
            '"' => TextObject::DoubleQuote,
            '`' => TextObject::BackQuote,
            '(' | ')' | 'b' => TextObject::Parentheses,
            '[' | ']' => TextObject::Brackets,
            '{' | '}' | 'B' => TextObject::Braces,
            '<' | '>' => TextObject::AngleBrackets,
            _ => return None,
        };

        Some((scope, object))
    }

    pub fn delimiters(&self) -> Option<(char, char)> {
        match self {
            TextObject::SingleQuote => Some(('\'', '\'')),
            TextObject::DoubleQuote => Some(('"', '"')),
            TextObject::BackQuote => Some(('`', '`')),
            TextObject::Parentheses => Some(('(', ')')),
            TextObject::Brackets => Some(('[', ']')),
            TextObject::Braces => Some(('{', '}')),
            TextObject::AngleBrackets => Some(('<', '>')),
            _ => None,
        }
    }

    pub fn find_bounds(&self, scope: TextObjectScope, lines: &[&str], pos: Position) -> Option<(Position, Position)> {
        self.find_bounds_from(scope, &BorrowedLines(lines), pos)
    }

    pub fn find_bounds_snapshot(&self, scope: TextObjectScope, snapshot: &EditorSnapshot, pos: Position) -> Option<(Position, Position)> {
        self.find_bounds_from(scope, snapshot, pos)
    }

    fn find_bounds_from(&self, scope: TextObjectScope, lines: &impl LineSource, pos: Position) -> Option<(Position, Position)> {
        match self {
            TextObject::Word => find_word_bounds(lines, pos, scope, false),
            TextObject::BigWord => find_word_bounds(lines, pos, scope, true),
            TextObject::Paragraph => find_paragraph_bounds(lines, pos, scope),
            TextObject::SingleQuote | TextObject::DoubleQuote | TextObject::BackQuote => {
                let (open, close) = self.delimiters()?;
                find_quote_bounds(lines, pos, scope, open, close)
            }
            TextObject::Parentheses | TextObject::Brackets | TextObject::Braces | TextObject::AngleBrackets => {
                let (open, close) = self.delimiters()?;
                find_bracket_bounds(lines, pos, scope, open, close)
            }
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn find_word_bounds(lines: &impl LineSource, pos: Position, scope: TextObjectScope, big_word: bool) -> Option<(Position, Position)> {
    let line = lines.line(pos.row)?;
    let line_len = line.chars().count();

    if line_len == 0 {
        return Some((pos, pos));
    }

    let col = pos.col.min(line_len.saturating_sub(1));
    let current_char = line.chars().nth(col)?;

    let is_word = if big_word {
        !current_char.is_whitespace()
    } else {
        is_word_char(current_char)
    };

    let mut start = col;
    let start_byte = char_to_byte_index(line, col);
    for c in line[..start_byte].chars().rev() {
        let c_is_word = if big_word { !c.is_whitespace() } else { is_word_char(c) };
        if c_is_word != is_word {
            break;
        }
        start -= 1;
    }

    let mut end = col;
    for c in line.chars().skip(col) {
        let c_is_word = if big_word { !c.is_whitespace() } else { is_word_char(c) };
        if c_is_word != is_word {
            break;
        }
        end += 1;
    }

    if scope == TextObjectScope::Around {
        let mut has_trailing = false;
        for c in line.chars().skip(end) {
            if !c.is_whitespace() {
                break;
            }
            end += 1;
            has_trailing = true;
        }
        if !has_trailing {
            let start_byte = char_to_byte_index(line, start);
            for c in line[..start_byte].chars().rev() {
                if !c.is_whitespace() {
                    break;
                }
                start -= 1;
            }
        }
    }

    Some((Position::new(pos.row, start), Position::new(pos.row, end)))
}

fn find_paragraph_bounds(lines: &impl LineSource, pos: Position, scope: TextObjectScope) -> Option<(Position, Position)> {
    let mut start_row = pos.row;
    let mut end_row = pos.row;

    while start_row > 0 {
        if lines.line(start_row.saturating_sub(1)).map_or(true, |line| line.trim().is_empty()) {
            break;
        }
        start_row -= 1;
    }

    while end_row < lines.line_count() {
        if lines.line(end_row).map_or(true, |line| line.trim().is_empty()) {
            break;
        }
        end_row += 1;
    }

    if scope == TextObjectScope::Around {
        while end_row < lines.line_count() && lines.line(end_row).is_some_and(|line| line.trim().is_empty()) {
            end_row += 1;
        }
    }

    let end_col = if end_row > 0 && end_row <= lines.line_count() {
        lines.line(end_row.saturating_sub(1)).map_or(0, |line| line.chars().count())
    } else {
        0
    };

    Some((Position::new(start_row, 0), Position::new(end_row.saturating_sub(1).max(start_row), end_col)))
}

fn find_quote_bounds(lines: &impl LineSource, pos: Position, scope: TextObjectScope, open: char, _close: char) -> Option<(Position, Position)> {
    let line = lines.line(pos.row)?;

    let mut in_quote = false;
    let mut quote_start = None;
    let mut found_pair = None;
    let mut seek_pair = None;

    for (i, c) in line.chars().enumerate() {
        if c == open {
            if in_quote {
                if quote_start.is_some_and(|start| start <= pos.col && i >= pos.col) {
                    found_pair = Some((quote_start.unwrap(), i));
                    break;
                }
                if seek_pair.is_none() && quote_start.is_some_and(|start| start >= pos.col) {
                    seek_pair = Some((quote_start.unwrap(), i));
                }
                in_quote = false;
                quote_start = None;
            } else {
                in_quote = true;
                quote_start = Some(i);
            }
        }
    }

    let (start, end) = found_pair.or(seek_pair)?;

    match scope {
        TextObjectScope::Inner => Some((Position::new(pos.row, start + 1), Position::new(pos.row, end))),
        TextObjectScope::Around => Some((Position::new(pos.row, start), Position::new(pos.row, end + 1))),
    }
}

fn find_bracket_bounds(lines: &impl LineSource, pos: Position, scope: TextObjectScope, open: char, close: char) -> Option<(Position, Position)> {
    let mut open_pos = None;
    let row = pos.row;
    let col = pos.col;

    let current_line = lines.line(row)?;
    let current_line_len = current_line.chars().count();
    if col > current_line_len {
        return None;
    }

    // Step 1: Check if cursor is directly on an opening bracket
    if current_line.chars().nth(col) == Some(open) {
        open_pos = Some(Position::new(row, col));
    }

    // Step 2: Search backward on CURRENT LINE ONLY first
    // This prevents unmatched brackets from previous lines from interfering
    if open_pos.is_none() {
        let mut depth = 0;
        let byte_end = char_to_byte_index(current_line, col);
        let mut c = col;
        for ch in current_line[..byte_end].chars().rev() {
            c -= 1;
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    open_pos = Some(Position::new(row, c));
                    break;
                }
                depth -= 1;
            }
        }
    }

    // Step 3: Seek forward on current line for an open bracket
    if open_pos.is_none() {
        for (c, ch) in current_line.chars().enumerate().skip(col) {
            if ch == open {
                open_pos = Some(Position::new(pos.row, c));
                break;
            }
        }
    }

    // Step 4: If nothing found on current line, search backward across previous lines
    // This handles multi-line bracket pairs
    if open_pos.is_none() {
        let mut depth = 0;
        let mut search_row = row;
        while search_row > 0 {
            search_row -= 1;
            let line = lines.line(search_row)?;
            let mut c = line.chars().count();

            for ch in line.chars().rev() {
                c -= 1;
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    if depth == 0 {
                        open_pos = Some(Position::new(search_row, c));
                        break;
                    }
                    depth -= 1;
                }
            }

            if open_pos.is_some() {
                break;
            }
        }
    }

    if open_pos.is_none() {
        return None;
    }

    let open_pos = open_pos?;
    let mut depth = 1;
    let mut search_row = open_pos.row;
    let search_col = open_pos.col + 1;

    loop {
        let line = lines.line(search_row)?;
        let start_col = if search_row == open_pos.row { search_col } else { 0 };

        for (c, ch) in line.chars().enumerate().skip(start_col) {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    let close_pos = Position::new(search_row, c);
                    return match scope {
                        TextObjectScope::Inner => Some((Position::new(open_pos.row, open_pos.col + 1), close_pos)),
                        TextObjectScope::Around => Some((open_pos, Position::new(close_pos.row, close_pos.col + 1))),
                    };
                }
            }
        }

        search_row += 1;
        if search_row >= lines.line_count() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Parse Tests ====================

    #[test]
    fn test_parse_word() {
        assert_eq!(TextObject::parse('i', 'w'), Some((TextObjectScope::Inner, TextObject::Word)));
        assert_eq!(TextObject::parse('a', 'w'), Some((TextObjectScope::Around, TextObject::Word)));
    }

    #[test]
    fn test_parse_big_word() {
        assert_eq!(TextObject::parse('i', 'W'), Some((TextObjectScope::Inner, TextObject::BigWord)));
        assert_eq!(TextObject::parse('a', 'W'), Some((TextObjectScope::Around, TextObject::BigWord)));
    }

    #[test]
    fn test_parse_quotes() {
        assert_eq!(TextObject::parse('i', '"'), Some((TextObjectScope::Inner, TextObject::DoubleQuote)));
        assert_eq!(TextObject::parse('a', '"'), Some((TextObjectScope::Around, TextObject::DoubleQuote)));
        assert_eq!(TextObject::parse('i', '\''), Some((TextObjectScope::Inner, TextObject::SingleQuote)));
        assert_eq!(TextObject::parse('a', '\''), Some((TextObjectScope::Around, TextObject::SingleQuote)));
        assert_eq!(TextObject::parse('i', '`'), Some((TextObjectScope::Inner, TextObject::BackQuote)));
        assert_eq!(TextObject::parse('a', '`'), Some((TextObjectScope::Around, TextObject::BackQuote)));
    }

    #[test]
    fn test_parse_parentheses() {
        assert_eq!(TextObject::parse('i', '('), Some((TextObjectScope::Inner, TextObject::Parentheses)));
        assert_eq!(TextObject::parse('a', '('), Some((TextObjectScope::Around, TextObject::Parentheses)));
        assert_eq!(TextObject::parse('i', ')'), Some((TextObjectScope::Inner, TextObject::Parentheses)));
        assert_eq!(TextObject::parse('a', ')'), Some((TextObjectScope::Around, TextObject::Parentheses)));
        assert_eq!(TextObject::parse('i', 'b'), Some((TextObjectScope::Inner, TextObject::Parentheses)));
        assert_eq!(TextObject::parse('a', 'b'), Some((TextObjectScope::Around, TextObject::Parentheses)));
    }

    #[test]
    fn test_parse_brackets() {
        assert_eq!(TextObject::parse('i', '['), Some((TextObjectScope::Inner, TextObject::Brackets)));
        assert_eq!(TextObject::parse('a', '['), Some((TextObjectScope::Around, TextObject::Brackets)));
        assert_eq!(TextObject::parse('i', ']'), Some((TextObjectScope::Inner, TextObject::Brackets)));
        assert_eq!(TextObject::parse('a', ']'), Some((TextObjectScope::Around, TextObject::Brackets)));
    }

    #[test]
    fn test_parse_braces() {
        assert_eq!(TextObject::parse('i', '{'), Some((TextObjectScope::Inner, TextObject::Braces)));
        assert_eq!(TextObject::parse('a', '{'), Some((TextObjectScope::Around, TextObject::Braces)));
        assert_eq!(TextObject::parse('i', '}'), Some((TextObjectScope::Inner, TextObject::Braces)));
        assert_eq!(TextObject::parse('a', '}'), Some((TextObjectScope::Around, TextObject::Braces)));
        assert_eq!(TextObject::parse('i', 'B'), Some((TextObjectScope::Inner, TextObject::Braces)));
        assert_eq!(TextObject::parse('a', 'B'), Some((TextObjectScope::Around, TextObject::Braces)));
    }

    #[test]
    fn test_parse_angle_brackets() {
        assert_eq!(TextObject::parse('i', '<'), Some((TextObjectScope::Inner, TextObject::AngleBrackets)));
        assert_eq!(TextObject::parse('a', '<'), Some((TextObjectScope::Around, TextObject::AngleBrackets)));
        assert_eq!(TextObject::parse('i', '>'), Some((TextObjectScope::Inner, TextObject::AngleBrackets)));
        assert_eq!(TextObject::parse('a', '>'), Some((TextObjectScope::Around, TextObject::AngleBrackets)));
    }

    #[test]
    fn test_parse_paragraph() {
        assert_eq!(TextObject::parse('i', 'p'), Some((TextObjectScope::Inner, TextObject::Paragraph)));
        assert_eq!(TextObject::parse('a', 'p'), Some((TextObjectScope::Around, TextObject::Paragraph)));
    }

    #[test]
    fn test_parse_invalid_scope() {
        assert_eq!(TextObject::parse('x', 'w'), None);
        assert_eq!(TextObject::parse('d', 'w'), None);
        assert_eq!(TextObject::parse('I', 'w'), None);
        assert_eq!(TextObject::parse('A', 'w'), None);
    }

    #[test]
    fn test_parse_invalid_object() {
        assert_eq!(TextObject::parse('i', 'x'), None);
        assert_eq!(TextObject::parse('i', 'z'), None);
        assert_eq!(TextObject::parse('i', '1'), None);
        assert_eq!(TextObject::parse('a', '!'), None);
    }

    // ==================== Delimiters Tests ====================

    #[test]
    fn test_delimiters_all() {
        assert_eq!(TextObject::SingleQuote.delimiters(), Some(('\'', '\'')));
        assert_eq!(TextObject::DoubleQuote.delimiters(), Some(('"', '"')));
        assert_eq!(TextObject::BackQuote.delimiters(), Some(('`', '`')));
        assert_eq!(TextObject::Parentheses.delimiters(), Some(('(', ')')));
        assert_eq!(TextObject::Brackets.delimiters(), Some(('[', ']')));
        assert_eq!(TextObject::Braces.delimiters(), Some(('{', '}')));
        assert_eq!(TextObject::AngleBrackets.delimiters(), Some(('<', '>')));
        assert_eq!(TextObject::Word.delimiters(), None);
        assert_eq!(TextObject::BigWord.delimiters(), None);
        assert_eq!(TextObject::Paragraph.delimiters(), None);
    }

    // ==================== Word Bounds Tests ====================

    #[test]
    fn test_find_word_bounds_inner() {
        let lines = vec!["hello world"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 2));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 5))));
    }

    #[test]
    fn test_find_word_bounds_around() {
        let lines = vec!["hello world"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 2));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 6))));
    }

    #[test]
    fn test_find_word_bounds_at_start() {
        let lines = vec!["hello world"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 5))));
    }

    #[test]
    fn test_find_word_bounds_at_end() {
        let lines = vec!["hello world"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 10));
        assert_eq!(bounds, Some((Position::new(0, 6), Position::new(0, 11))));
    }

    #[test]
    fn test_find_word_bounds_single_word() {
        let lines = vec!["hello"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 2));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 5))));
    }

    #[test]
    fn test_find_word_bounds_with_underscore() {
        let lines = vec!["hello_world test"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 5));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 11))));
    }

    #[test]
    fn test_find_word_bounds_punctuation() {
        let lines = vec!["foo.bar"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 3), Position::new(0, 4))));
    }

    #[test]
    fn test_find_word_bounds_empty_line() {
        let lines = vec![""];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 0))));
    }

    #[test]
    fn test_find_word_bounds_around_trailing_space() {
        let lines = vec!["hello world"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 8));
        assert!(bounds.is_some());
    }

    #[test]
    fn test_find_word_bounds_around_leading_space() {
        let lines = vec!["hello world"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 10));
        assert!(bounds.is_some());
    }

    // ==================== Big Word Bounds Tests ====================

    #[test]
    fn test_find_big_word_bounds_inner() {
        let lines = vec!["foo.bar baz"];
        let bounds = TextObject::BigWord.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 2));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 7))));
    }

    #[test]
    fn test_find_big_word_bounds_around() {
        let lines = vec!["foo.bar baz"];
        let bounds = TextObject::BigWord.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 2));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 8))));
    }

    #[test]
    fn test_find_big_word_bounds_complex() {
        let lines = vec!["http://example.com next"];
        let bounds = TextObject::BigWord.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 5));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 18))));
    }

    // ==================== Quote Bounds Tests ====================

    #[test]
    fn test_find_quote_bounds_inner() {
        let lines = vec!["say \"hello\" there"];
        let bounds = TextObject::DoubleQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 6));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 10))));
    }

    #[test]
    fn test_find_quote_bounds_around() {
        let lines = vec!["say \"hello\" there"];
        let bounds = TextObject::DoubleQuote.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 6));
        assert_eq!(bounds, Some((Position::new(0, 4), Position::new(0, 11))));
    }

    #[test]
    fn test_find_quote_bounds_single_quote() {
        let lines = vec!["say 'hello' there"];
        let bounds = TextObject::SingleQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 6));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 10))));
    }

    #[test]
    fn test_find_quote_bounds_backtick() {
        let lines = vec!["say `hello` there"];
        let bounds = TextObject::BackQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 6));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 10))));
    }

    #[test]
    fn test_find_quote_bounds_empty_quotes() {
        let lines = vec!["say \"\" there"];
        let bounds = TextObject::DoubleQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 5));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 5))));
    }

    #[test]
    fn test_find_quote_bounds_at_quote_char() {
        let lines = vec!["say \"hello\" there"];
        let bounds = TextObject::DoubleQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 4));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 10))));
    }

    #[test]
    fn test_find_quote_bounds_no_quotes() {
        let lines = vec!["say hello there"];
        let bounds = TextObject::DoubleQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 6));
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_quote_bounds_unmatched_quote() {
        let lines = vec!["say \"hello there"];
        let bounds = TextObject::DoubleQuote.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 6));
        assert_eq!(bounds, None);
    }

    // ==================== Bracket Bounds Tests ====================

    #[test]
    fn test_find_bracket_bounds_inner() {
        let lines = vec!["(hello)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(0, 6))));
    }

    #[test]
    fn test_find_bracket_bounds_around() {
        let lines = vec!["(hello)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(0, 7))));
    }

    #[test]
    fn test_find_bracket_bounds_square() {
        let lines = vec!["[hello]"];
        let bounds = TextObject::Brackets.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(0, 6))));
    }

    #[test]
    fn test_find_bracket_bounds_curly() {
        let lines = vec!["{hello}"];
        let bounds = TextObject::Braces.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(0, 6))));
    }

    #[test]
    fn test_find_bracket_bounds_angle() {
        let lines = vec!["<hello>"];
        let bounds = TextObject::AngleBrackets.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(0, 6))));
    }

    #[test]
    fn test_find_bracket_bounds_nested() {
        let lines = vec!["((inner))"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, Some((Position::new(0, 2), Position::new(0, 7))));
    }

    #[test]
    fn test_find_bracket_bounds_deeply_nested() {
        let lines = vec!["(((deep)))"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 4));
        assert_eq!(bounds, Some((Position::new(0, 3), Position::new(0, 7))));
    }

    #[test]
    fn test_find_bracket_bounds_multiline() {
        let lines = vec!["(", "  hello", ")"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(1, 3));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(2, 0))));
    }

    #[test]
    fn test_find_bracket_bounds_multiline_around() {
        let lines = vec!["(", "  hello", ")"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Around, &lines, Position::new(1, 3));
        assert_eq!(bounds, Some((Position::new(0, 0), Position::new(2, 1))));
    }

    #[test]
    fn test_find_bracket_bounds_empty() {
        let lines = vec!["()"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(0, 1))));
    }

    #[test]
    fn test_find_bracket_bounds_at_open_bracket() {
        let lines = vec!["(hello)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 1), Position::new(0, 6))));
    }

    #[test]
    fn test_find_bracket_bounds_unmatched() {
        let lines = vec!["(hello"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 3));
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_bracket_bounds_no_brackets() {
        let lines = vec!["hello"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 2));
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_bracket_bounds_seek_forward_parentheses() {
        let lines = vec!["foo (bar)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 8))));
    }

    #[test]
    fn test_find_bracket_bounds_seek_forward_braces() {
        let lines = vec!["foo {bar}"];
        let bounds = TextObject::Braces.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 8))));
    }

    #[test]
    fn test_find_bracket_bounds_seek_forward_brackets() {
        let lines = vec!["foo [bar]"];
        let bounds = TextObject::Brackets.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 8))));
    }

    #[test]
    fn test_find_bracket_bounds_seek_forward_angle() {
        let lines = vec!["foo <bar>"];
        let bounds = TextObject::AngleBrackets.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 5), Position::new(0, 8))));
    }

    #[test]
    fn test_find_bracket_bounds_seek_forward_around() {
        let lines = vec!["foo (bar)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 0));
        assert_eq!(bounds, Some((Position::new(0, 4), Position::new(0, 9))));
    }

    #[test]
    fn test_find_bracket_bounds_seek_mid_line() {
        let lines = vec!["foo bar (baz)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 2));
        assert_eq!(bounds, Some((Position::new(0, 9), Position::new(0, 12))));
    }

    #[test]
    fn test_find_bracket_bounds_cursor_after_closed_pair() {
        let lines = vec!["(foo) (bar)"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 5));
        assert_eq!(bounds, Some((Position::new(0, 7), Position::new(0, 10))));
    }

    #[test]
    fn test_find_bracket_bounds_cursor_after_only_pair() {
        let lines = vec!["(foo) bar"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 6));
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_bracket_bounds_complex_code() {
        let lines = vec!["fn test(a: (i32, i32)) {"];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 12));
        assert_eq!(bounds, Some((Position::new(0, 12), Position::new(0, 20))));
    }

    #[test]
    fn test_find_bracket_bounds_unmatched_on_previous_line() {
        // Regression test: unmatched brackets on previous lines should not
        // interfere with bracket matching on current line
        let lines = vec![
            "abc()",                       // line 0: matched
            "text with unmatched ( paren", // line 1: unmatched opening
            "abc(...)",                    // line 2: should still work!
        ];
        // Cursor at start of line 2, should seek forward to find abc(...)
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(2, 0));
        assert_eq!(bounds, Some((Position::new(2, 4), Position::new(2, 7))));
    }

    #[test]
    fn test_find_bracket_bounds_unmatched_does_not_match_later_close() {
        // An unmatched opening bracket should not match with a closing bracket
        // from a different pair on a later line
        let lines = vec![
            "unmatched (", // line 0: unmatched opening
            "abc(foo)",    // line 1: complete pair
        ];
        // Cursor inside abc(foo), should find that pair, not the unmatched one
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(1, 5));
        assert_eq!(bounds, Some((Position::new(1, 4), Position::new(1, 7))));
    }

    #[test]
    fn test_find_bracket_bounds_multiple_unmatched() {
        // Multiple unmatched brackets on different lines
        let lines = vec![
            "( unmatched",         // line 0
            "another ( unmatched", // line 1
            "",                    // line 2
            "valid(content)",      // line 3
        ];
        let bounds = TextObject::Parentheses.find_bounds(TextObjectScope::Inner, &lines, Position::new(3, 7));
        assert_eq!(bounds, Some((Position::new(3, 6), Position::new(3, 13))));
    }

    // ==================== Paragraph Bounds Tests ====================

    #[test]
    fn test_find_paragraph_bounds_inner() {
        let lines = vec!["line1", "line2", "", "line3"];
        let bounds = TextObject::Paragraph.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert!(bounds.is_some());
        let (start, end) = bounds.unwrap();
        assert_eq!(start.row, 0);
        assert!(end.row <= 1);
    }

    #[test]
    fn test_find_paragraph_bounds_around() {
        let lines = vec!["line1", "line2", "", "line3"];
        let bounds = TextObject::Paragraph.find_bounds(TextObjectScope::Around, &lines, Position::new(0, 0));
        assert!(bounds.is_some());
    }

    #[test]
    fn test_find_paragraph_bounds_single_line() {
        let lines = vec!["only line"];
        let bounds = TextObject::Paragraph.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert!(bounds.is_some());
    }

    #[test]
    fn test_find_paragraph_bounds_at_empty_line() {
        let lines = vec!["line1", "", "line2"];
        let bounds = TextObject::Paragraph.find_bounds(TextObjectScope::Inner, &lines, Position::new(1, 0));
        assert!(bounds.is_some());
    }

    #[test]
    fn test_find_paragraph_bounds_multiple_paragraphs() {
        let lines = vec!["para1", "", "para2", "para2cont", "", "para3"];
        let bounds = TextObject::Paragraph.find_bounds(TextObjectScope::Inner, &lines, Position::new(2, 0));
        assert!(bounds.is_some());
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_find_bounds_empty_lines() {
        let lines: Vec<&str> = vec![];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 0));
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_bounds_row_out_of_bounds() {
        let lines = vec!["hello"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(10, 0));
        assert_eq!(bounds, None);
    }

    #[test]
    fn test_find_bounds_col_beyond_line() {
        let lines = vec!["hello"];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 100));
        assert!(bounds.is_some());
    }

    #[test]
    fn test_find_bounds_whitespace_only() {
        let lines = vec!["   "];
        let bounds = TextObject::Word.find_bounds(TextObjectScope::Inner, &lines, Position::new(0, 1));
        assert!(bounds.is_some());
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
        assert!(!is_word_char('@'));
        assert!(!is_word_char('#'));
        assert!(!is_word_char('-'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char('\t'));
    }
}
