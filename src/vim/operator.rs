//! Vim operators (d, c, y, >, <)

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    SwapCase,
    Lowercase,
    Uppercase,
}

impl Operator {
    pub fn char(&self) -> char {
        match self {
            Operator::Delete => 'd',
            Operator::Change => 'c',
            Operator::Yank => 'y',
            Operator::Indent => '>',
            Operator::Outdent => '<',
            Operator::SwapCase => '~',
            Operator::Lowercase => 'u',
            Operator::Uppercase => 'U',
        }
    }

    #[allow(dead_code)]
    pub fn enters_insert_mode(&self) -> bool {
        matches!(self, Operator::Change)
    }

    #[allow(dead_code)]
    pub fn modifies_buffer(&self) -> bool {
        !matches!(self, Operator::Yank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_char() {
        assert_eq!(Operator::Delete.char(), 'd');
        assert_eq!(Operator::Change.char(), 'c');
        assert_eq!(Operator::Yank.char(), 'y');
        assert_eq!(Operator::Indent.char(), '>');
        assert_eq!(Operator::Outdent.char(), '<');
    }

    #[test]
    fn test_enters_insert_mode() {
        assert!(Operator::Change.enters_insert_mode());
        assert!(!Operator::Delete.enters_insert_mode());
        assert!(!Operator::Yank.enters_insert_mode());
    }

    #[test]
    fn test_modifies_buffer() {
        assert!(Operator::Delete.modifies_buffer());
        assert!(Operator::Change.modifies_buffer());
        assert!(!Operator::Yank.modifies_buffer());
        assert!(Operator::Indent.modifies_buffer());
    }
}
