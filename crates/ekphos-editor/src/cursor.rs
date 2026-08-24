#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    pub row: usize,
    pub col: usize,
}

impl Position {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMove {
    Forward,
    Back,
    Up,
    Down,
    WordForward,
    WordBack,
    Head,
    End,
    Top,
    Bottom,
    FirstNonBlank,
    WordEndForward,
    BigWordForward,
    BigWordBack,
    BigWordEndForward,
    WordEndBackward,
    BigWordEndBackward,
    ParagraphForward,
    ParagraphBack,
    ScreenTop,
    ScreenMiddle,
    ScreenBottom,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
    MatchingBracket,
    GoToLine(usize),
    GoToColumn(usize),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub anchor: Position,
    pub active: bool,
}

impl Selection {
    pub fn start(&mut self, pos: Position) {
        self.anchor = pos;
        self.active = true;
    }

    pub fn cancel(&mut self) {
        self.active = false;
    }

    pub fn range(&self, cursor_pos: Position) -> Option<(Position, Position)> {
        if !self.active {
            return None;
        }

        if self.anchor.row < cursor_pos.row || (self.anchor.row == cursor_pos.row && self.anchor.col <= cursor_pos.col) {
            Some((self.anchor, cursor_pos))
        } else {
            Some((cursor_pos, self.anchor))
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Cursor {
    pub position: Position,
    pub selection: Selection,
    pub preferred_col: Option<usize>,
}

impl Cursor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pos(&self) -> Position {
        self.position
    }

    pub fn set_pos(&mut self, pos: Position, update_preferred: bool) {
        self.position = pos;
        if update_preferred {
            self.preferred_col = Some(pos.col);
        }
    }

    pub fn move_to(&mut self, row: usize, col: usize) {
        self.position = Position::new(row, col);
        self.preferred_col = Some(col);
    }

    pub fn start_selection(&mut self) {
        self.selection.start(self.position);
    }

    pub fn cancel_selection(&mut self) {
        self.selection.cancel();
    }

    pub fn has_selection(&self) -> bool {
        self.selection.active
    }

    pub fn selection_range(&self) -> Option<(Position, Position)> {
        self.selection.range(self.position)
    }
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn find_word_forward(line: &str, col: usize) -> usize {
    let len = line.chars().count();

    if col >= len {
        return len;
    }

    let mut pos = col;
    let mut leaving_word = true;
    for (index, ch) in line.chars().enumerate().skip(col) {
        if leaving_word && is_word_char(ch) {
            pos = index + 1;
            continue;
        }
        leaving_word = false;
        if !is_word_char(ch) {
            pos = index + 1;
        } else {
            return index;
        }
    }
    pos.min(len)
}

pub fn find_word_back(line: &str, col: usize) -> usize {
    if col == 0 {
        return 0;
    }

    let mut word_start = 0;
    let mut in_word = false;
    for (index, ch) in line.chars().enumerate().take(col) {
        if is_word_char(ch) {
            if !in_word {
                word_start = index;
            }
            in_word = true;
        } else {
            in_word = false;
        }
    }
    word_start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_forward() {
        assert_eq!(find_word_forward("hello world", 0), 6);
        assert_eq!(find_word_forward("hello world", 5), 6);
        assert_eq!(find_word_forward("hello world", 6), 11);
    }

    #[test]
    fn test_word_back() {
        assert_eq!(find_word_back("hello world", 11), 6);
        assert_eq!(find_word_back("hello world", 6), 0);
        assert_eq!(find_word_back("hello world", 5), 0);
    }

    #[test]
    fn test_selection_range() {
        let mut cursor = Cursor::new();
        cursor.move_to(0, 5);
        cursor.start_selection();
        cursor.move_to(0, 10);

        let range = cursor.selection_range();
        assert!(range.is_some());
        let (start, end) = range.unwrap();
        assert_eq!(start.col, 5);
        assert_eq!(end.col, 10);
    }
}
