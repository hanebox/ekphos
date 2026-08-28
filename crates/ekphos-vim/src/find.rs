//! Character find operations (f, F, t, T)

use crate::char_to_byte_index;

#[derive(Debug, Clone, Copy)]
pub struct FindState {
    pub char: char,
    pub forward: bool,
    pub till: bool,
}

impl FindState {
    pub fn new(char: char, forward: bool, till: bool) -> Self {
        Self { char, forward, till }
    }

    pub fn find_in_line(&self, line: &str, col: usize) -> Option<usize> {
        if self.forward {
            for (i, c) in line.chars().enumerate().skip(col + 1) {
                if c == self.char {
                    return Some(if self.till { i - 1 } else { i });
                }
            }
        } else {
            if col == 0 {
                return None;
            }
            let end_col = col.min(line.chars().count());
            let byte_end = char_to_byte_index(line, end_col);
            let mut i = end_col;
            for c in line[..byte_end].chars().rev() {
                i -= 1;
                if c == self.char {
                    return Some(if self.till { i + 1 } else { i });
                }
            }
        }
        None
    }

    pub fn reversed(&self) -> Self {
        Self { char: self.char, forward: !self.forward, till: self.till }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PendingFind {
    pub forward: bool,
    pub till: bool,
}

impl PendingFind {
    pub fn new(forward: bool, till: bool) -> Self {
        Self { forward, till }
    }

    pub fn into_find_state(self, char: char) -> FindState {
        FindState::new(char, self.forward, self.till)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_forward_basic() {
        let find = FindState::new('o', true, false);
        assert_eq!(find.find_in_line("hello world", 0), Some(4));
    }

    #[test]
    fn test_find_forward_multiple_occurrences() {
        let find = FindState::new('o', true, false);
        assert_eq!(find.find_in_line("hello world", 0), Some(4));
        assert_eq!(find.find_in_line("hello world", 4), Some(7));
    }

    #[test]
    fn test_find_forward_first_char() {
        let find = FindState::new('e', true, false);
        assert_eq!(find.find_in_line("hello", 0), Some(1));
    }

    #[test]
    fn test_find_forward_last_char() {
        let find = FindState::new('d', true, false);
        assert_eq!(find.find_in_line("hello world", 0), Some(10));
    }

    #[test]
    fn test_find_forward_from_middle() {
        let find = FindState::new('l', true, false);
        assert_eq!(find.find_in_line("hello world", 5), Some(9));
    }

    #[test]
    fn test_find_forward_at_last_position() {
        let find = FindState::new('x', true, false);
        assert_eq!(find.find_in_line("hello", 4), None);
    }

    #[test]
    fn test_find_forward_empty_string() {
        let find = FindState::new('x', true, false);
        assert_eq!(find.find_in_line("", 0), None);
    }

    #[test]
    fn test_find_forward_space() {
        let find = FindState::new(' ', true, false);
        assert_eq!(find.find_in_line("hello world", 0), Some(5));
    }

    #[test]
    fn test_find_forward_special_char() {
        let find = FindState::new('!', true, false);
        assert_eq!(find.find_in_line("hello!", 0), Some(5));
    }

    #[test]
    fn test_find_backward_basic() {
        let find = FindState::new('o', false, false);
        assert_eq!(find.find_in_line("hello world", 10), Some(7));
    }

    #[test]
    fn test_find_backward_multiple_occurrences() {
        let find = FindState::new('o', false, false);
        assert_eq!(find.find_in_line("hello world", 10), Some(7));
        assert_eq!(find.find_in_line("hello world", 7), Some(4));
    }

    #[test]
    fn test_find_backward_first_char() {
        let find = FindState::new('h', false, false);
        assert_eq!(find.find_in_line("hello", 4), Some(0));
    }

    #[test]
    fn test_find_backward_at_start() {
        let find = FindState::new('x', false, false);
        assert_eq!(find.find_in_line("hello", 0), None);
    }

    #[test]
    fn test_find_backward_at_position_1() {
        let find = FindState::new('h', false, false);
        assert_eq!(find.find_in_line("hello", 1), Some(0));
    }

    #[test]
    fn test_find_backward_not_found() {
        let find = FindState::new('z', false, false);
        assert_eq!(find.find_in_line("hello", 4), None);
    }

    #[test]
    fn test_till_forward_basic() {
        let find = FindState::new('o', true, true);
        assert_eq!(find.find_in_line("hello world", 0), Some(3));
    }

    #[test]
    fn test_till_forward_stops_before_char() {
        let find = FindState::new('l', true, true);
        assert_eq!(find.find_in_line("hello", 0), Some(1));
    }

    #[test]
    fn test_till_forward_adjacent_char() {
        let find = FindState::new('e', true, true);
        let result = find.find_in_line("hello", 0);
        assert!(result.is_some());
    }
    #[test]
    fn test_till_forward_adjacent_stops_before_target() {
        let find = FindState::new('e', true, true);
        assert_eq!(find.find_in_line("hello", 0), Some(0));
    }

    #[test]
    fn test_till_backward_adjacent_stops_after_target() {
        let find = FindState::new('b', false, true);
        assert_eq!(find.find_in_line("abc", 2), Some(2));
    }

    #[test]
    fn test_till_backward_basic() {
        let find = FindState::new('o', false, true);
        assert_eq!(find.find_in_line("hello world", 10), Some(8));
    }

    #[test]
    fn test_till_backward_stops_after_char() {
        let find = FindState::new('h', false, true);
        let result = find.find_in_line("hello", 4);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_not_found_forward() {
        let find = FindState::new('z', true, false);
        assert_eq!(find.find_in_line("hello world", 0), None);
    }

    #[test]
    fn test_find_not_found_backward() {
        let find = FindState::new('z', false, false);
        assert_eq!(find.find_in_line("hello world", 10), None);
    }

    #[test]
    fn test_till_not_found_forward() {
        let find = FindState::new('z', true, true);
        assert_eq!(find.find_in_line("hello world", 0), None);
    }

    #[test]
    fn test_till_not_found_backward() {
        let find = FindState::new('z', false, true);
        assert_eq!(find.find_in_line("hello world", 10), None);
    }

    #[test]
    fn test_reversed_forward_to_backward() {
        let find = FindState::new('x', true, false);
        let reversed = find.reversed();
        assert_eq!(reversed.char, 'x');
        assert!(!reversed.forward);
        assert!(!reversed.till);
    }

    #[test]
    fn test_reversed_backward_to_forward() {
        let find = FindState::new('x', false, false);
        let reversed = find.reversed();
        assert!(reversed.forward);
    }

    #[test]
    fn test_reversed_preserves_till() {
        let find = FindState::new('x', true, true);
        let reversed = find.reversed();
        assert!(reversed.till);
    }

    #[test]
    fn test_reversed_preserves_char() {
        let find = FindState::new('a', true, false);
        let reversed = find.reversed();
        assert_eq!(reversed.char, 'a');
    }

    #[test]
    fn test_double_reversed_is_original() {
        let find = FindState::new('x', true, false);
        let double_reversed = find.reversed().reversed();
        assert_eq!(double_reversed.char, find.char);
        assert_eq!(double_reversed.forward, find.forward);
        assert_eq!(double_reversed.till, find.till);
    }

    #[test]
    fn test_pending_find_forward() {
        let pending = PendingFind::new(true, false);
        let find = pending.into_find_state('x');
        assert_eq!(find.char, 'x');
        assert!(find.forward);
        assert!(!find.till);
    }

    #[test]
    fn test_pending_find_backward() {
        let pending = PendingFind::new(false, false);
        let find = pending.into_find_state('y');
        assert_eq!(find.char, 'y');
        assert!(!find.forward);
        assert!(!find.till);
    }

    #[test]
    fn test_pending_find_till_forward() {
        let pending = PendingFind::new(true, true);
        let find = pending.into_find_state('z');
        assert_eq!(find.char, 'z');
        assert!(find.forward);
        assert!(find.till);
    }

    #[test]
    fn test_pending_find_till_backward() {
        let pending = PendingFind::new(false, true);
        let find = pending.into_find_state('w');
        assert_eq!(find.char, 'w');
        assert!(!find.forward);
        assert!(find.till);
    }

    #[test]
    fn test_find_same_char_at_position() {
        let find = FindState::new('h', true, false);
        assert_eq!(find.find_in_line("hahaha", 0), Some(2));
    }

    #[test]
    fn test_find_single_char_string() {
        let find = FindState::new('a', true, false);
        assert_eq!(find.find_in_line("a", 0), None);
    }

    #[test]
    fn test_find_backward_single_char_string() {
        let find = FindState::new('a', false, false);
        assert_eq!(find.find_in_line("a", 0), None);
    }

    #[test]
    fn test_find_unicode_char() {
        let find = FindState::new('世', true, false);
        assert_eq!(find.find_in_line("hello世界", 0), Some(5));
    }

    #[test]
    fn test_find_newline_not_in_string() {
        let find = FindState::new('\n', true, false);
        assert_eq!(find.find_in_line("hello world", 0), None);
    }

    #[test]
    fn test_find_tab() {
        let find = FindState::new('\t', true, false);
        assert_eq!(find.find_in_line("hello\tworld", 0), Some(5));
    }
}
