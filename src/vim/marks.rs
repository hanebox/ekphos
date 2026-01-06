//! Vim marks (m, `, ')

use crate::editor::Position;
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct MarkMap {
    marks: HashMap<char, Position>,
    last_jump: Option<Position>,
    last_change: Option<Position>,
    last_insert: Option<Position>,
}

impl MarkMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, mark: char, pos: Position) {
        if mark.is_ascii_lowercase() || mark.is_ascii_uppercase() {
            self.marks.insert(mark, pos);
        }
    }

    pub fn get(&self, mark: char) -> Option<Position> {
        match mark {
            'a'..='z' | 'A'..='Z' => self.marks.get(&mark).copied(),
            '\'' | '`' => self.last_jump,
            '.' => self.last_change,
            '^' => self.last_insert,
            _ => None,
        }
    }

    pub fn set_last_jump(&mut self, pos: Position) {
        self.last_jump = Some(pos);
    }

    #[allow(dead_code)]
    pub fn set_last_change(&mut self, pos: Position) {
        self.last_change = Some(pos);
    }

    #[allow(dead_code)]
    pub fn set_last_insert(&mut self, pos: Position) {
        self.last_insert = Some(pos);
    }

    #[allow(dead_code)]
    pub fn delete(&mut self, mark: char) {
        self.marks.remove(&mark);
    }

    #[allow(dead_code)]
    pub fn list(&self) -> Vec<(char, Position)> {
        let mut result: Vec<_> = self.marks.iter().map(|(&c, &p)| (c, p)).collect();
        result.sort_by_key(|(c, _)| *c);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Basic Set/Get Tests ====================

    #[test]
    fn test_new_mark_map() {
        let marks = MarkMap::new();
        assert!(marks.get('a').is_none());
    }

    #[test]
    fn test_set_and_get_mark() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(10, 5));
        assert_eq!(marks.get('a'), Some(Position::new(10, 5)));
    }

    #[test]
    fn test_set_overwrites_mark() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(10, 5));
        marks.set('a', Position::new(20, 10));
        assert_eq!(marks.get('a'), Some(Position::new(20, 10)));
    }

    // ==================== Lowercase Mark Tests ====================

    #[test]
    fn test_all_lowercase_marks() {
        let mut marks = MarkMap::new();
        for (i, c) in ('a'..='z').enumerate() {
            marks.set(c, Position::new(i, i));
        }
        for (i, c) in ('a'..='z').enumerate() {
            assert_eq!(marks.get(c), Some(Position::new(i, i)));
        }
    }

    #[test]
    fn test_lowercase_a() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(0, 0));
        assert_eq!(marks.get('a'), Some(Position::new(0, 0)));
    }

    #[test]
    fn test_lowercase_z() {
        let mut marks = MarkMap::new();
        marks.set('z', Position::new(99, 99));
        assert_eq!(marks.get('z'), Some(Position::new(99, 99)));
    }

    // ==================== Uppercase Mark Tests ====================

    #[test]
    fn test_uppercase_marks() {
        let mut marks = MarkMap::new();
        marks.set('A', Position::new(100, 0));
        assert_eq!(marks.get('A'), Some(Position::new(100, 0)));
    }

    #[test]
    fn test_all_uppercase_marks() {
        let mut marks = MarkMap::new();
        for (i, c) in ('A'..='Z').enumerate() {
            marks.set(c, Position::new(i + 100, i));
        }
        for (i, c) in ('A'..='Z').enumerate() {
            assert_eq!(marks.get(c), Some(Position::new(i + 100, i)));
        }
    }

    #[test]
    fn test_uppercase_and_lowercase_separate() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(1, 1));
        marks.set('A', Position::new(2, 2));
        assert_eq!(marks.get('a'), Some(Position::new(1, 1)));
        assert_eq!(marks.get('A'), Some(Position::new(2, 2)));
    }

    // ==================== Invalid Mark Tests ====================

    #[test]
    fn test_invalid_mark_number() {
        let mut marks = MarkMap::new();
        marks.set('1', Position::new(10, 5));
        assert_eq!(marks.get('1'), None);
    }

    #[test]
    fn test_invalid_mark_symbol() {
        let mut marks = MarkMap::new();
        marks.set('@', Position::new(10, 5));
        assert_eq!(marks.get('@'), None);
    }

    #[test]
    fn test_invalid_mark_space() {
        let mut marks = MarkMap::new();
        marks.set(' ', Position::new(10, 5));
        assert_eq!(marks.get(' '), None);
    }

    // ==================== Special Marks Tests ====================

    #[test]
    fn test_last_jump_single_quote() {
        let mut marks = MarkMap::new();
        marks.set_last_jump(Position::new(5, 3));
        assert_eq!(marks.get('\''), Some(Position::new(5, 3)));
    }

    #[test]
    fn test_last_jump_backtick() {
        let mut marks = MarkMap::new();
        marks.set_last_jump(Position::new(5, 3));
        assert_eq!(marks.get('`'), Some(Position::new(5, 3)));
    }

    #[test]
    fn test_last_jump_both_return_same() {
        let mut marks = MarkMap::new();
        marks.set_last_jump(Position::new(5, 3));
        assert_eq!(marks.get('\''), marks.get('`'));
    }

    #[test]
    fn test_last_change() {
        let mut marks = MarkMap::new();
        marks.set_last_change(Position::new(15, 8));
        assert_eq!(marks.get('.'), Some(Position::new(15, 8)));
    }

    #[test]
    fn test_last_insert() {
        let mut marks = MarkMap::new();
        marks.set_last_insert(Position::new(20, 0));
        assert_eq!(marks.get('^'), Some(Position::new(20, 0)));
    }

    #[test]
    fn test_special_marks_initially_none() {
        let marks = MarkMap::new();
        assert_eq!(marks.get('\''), None);
        assert_eq!(marks.get('`'), None);
        assert_eq!(marks.get('.'), None);
        assert_eq!(marks.get('^'), None);
    }

    #[test]
    fn test_special_marks_update() {
        let mut marks = MarkMap::new();
        marks.set_last_jump(Position::new(1, 1));
        marks.set_last_jump(Position::new(2, 2));
        assert_eq!(marks.get('\''), Some(Position::new(2, 2)));
    }

    // ==================== Delete Mark Tests ====================

    #[test]
    fn test_delete_mark() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(10, 5));
        marks.delete('a');
        assert_eq!(marks.get('a'), None);
    }

    #[test]
    fn test_delete_nonexistent_mark() {
        let mut marks = MarkMap::new();
        marks.delete('a');
        assert_eq!(marks.get('a'), None);
    }

    #[test]
    fn test_delete_one_of_multiple() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(1, 1));
        marks.set('b', Position::new(2, 2));
        marks.delete('a');
        assert_eq!(marks.get('a'), None);
        assert_eq!(marks.get('b'), Some(Position::new(2, 2)));
    }

    // ==================== List Marks Tests ====================

    #[test]
    fn test_list_marks_sorted() {
        let mut marks = MarkMap::new();
        marks.set('b', Position::new(20, 0));
        marks.set('a', Position::new(10, 5));
        let list = marks.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, 'a');
        assert_eq!(list[1].0, 'b');
    }

    #[test]
    fn test_list_marks_empty() {
        let marks = MarkMap::new();
        let list = marks.list();
        assert!(list.is_empty());
    }

    #[test]
    fn test_list_marks_all() {
        let mut marks = MarkMap::new();
        marks.set('z', Position::new(0, 0));
        marks.set('a', Position::new(0, 0));
        marks.set('m', Position::new(0, 0));
        marks.set('A', Position::new(0, 0));
        marks.set('Z', Position::new(0, 0));
        let list = marks.list();
        assert_eq!(list.len(), 5);
        assert_eq!(list[0].0, 'A');
        assert_eq!(list[1].0, 'Z');
        assert_eq!(list[2].0, 'a');
        assert_eq!(list[3].0, 'm');
        assert_eq!(list[4].0, 'z');
    }

    #[test]
    fn test_list_does_not_include_special_marks() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(0, 0));
        marks.set_last_jump(Position::new(1, 1));
        marks.set_last_change(Position::new(2, 2));
        marks.set_last_insert(Position::new(3, 3));
        let list = marks.list();
        assert_eq!(list.len(), 1);
    }

    // ==================== Position Tests ====================

    #[test]
    fn test_mark_at_origin() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(0, 0));
        assert_eq!(marks.get('a'), Some(Position::new(0, 0)));
    }

    #[test]
    fn test_mark_large_position() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(1000000, 1000000));
        assert_eq!(marks.get('a'), Some(Position::new(1000000, 1000000)));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_set_same_mark_multiple_times() {
        let mut marks = MarkMap::new();
        for i in 0..10 {
            marks.set('a', Position::new(i, i));
        }
        assert_eq!(marks.get('a'), Some(Position::new(9, 9)));
    }

    #[test]
    fn test_delete_and_rset() {
        let mut marks = MarkMap::new();
        marks.set('a', Position::new(1, 1));
        marks.delete('a');
        marks.set('a', Position::new(2, 2));
        assert_eq!(marks.get('a'), Some(Position::new(2, 2)));
    }
}
