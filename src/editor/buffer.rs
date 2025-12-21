use std::cmp::Ordering;

/// Line-based gap buffer for efficient text editing.
/// Uses two vectors: `before` (lines before gap) and `after` (lines after gap, reversed).
/// Provides O(1) operations for localized edits.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    before: Vec<String>,
    after: Vec<String>,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            before: vec![String::new()],
            after: Vec::new(),
        }
    }
}

impl TextBuffer {
    pub fn from_lines(lines: Vec<String>) -> Self {
        if lines.is_empty() {
            return Self::default();
        }
        Self { before: lines, after: Vec::new() }
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.before.len() + self.after.len()
    }

    pub fn is_empty(&self) -> bool {
        self.line_count() == 1 && self.before.first().map_or(true, |l| l.is_empty())
    }

    #[inline]
    fn gap_pos(&self) -> usize {
        self.before.len()
    }

    fn move_gap_to(&mut self, row: usize) {
        let current = self.gap_pos();
        match row.cmp(&current) {
            Ordering::Equal => {}
            Ordering::Less => {
                for _ in row..current {
                    if let Some(line) = self.before.pop() {
                        self.after.push(line);
                    }
                }
            }
            Ordering::Greater => {
                let target = row.min(self.line_count());
                for _ in current..target {
                    if let Some(line) = self.after.pop() {
                        self.before.push(line);
                    }
                }
            }
        }
    }

    pub fn line(&self, row: usize) -> Option<&str> {
        let gap_pos = self.gap_pos();
        if row < gap_pos {
            self.before.get(row).map(|s| s.as_str())
        } else {
            let after_idx = self.after.len().checked_sub(row - gap_pos + 1)?;
            self.after.get(after_idx).map(|s| s.as_str())
        }
    }

    pub fn line_mut(&mut self, row: usize) -> Option<&mut String> {
        self.move_gap_to(row + 1);
        self.before.get_mut(row)
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.line(row).map_or(0, |l| l.chars().count())
    }

    pub fn lines(&self) -> Vec<&str> {
        let mut result = Vec::with_capacity(self.line_count());
        for line in &self.before {
            result.push(line.as_str());
        }
        for line in self.after.iter().rev() {
            result.push(line.as_str());
        }
        result
    }

    pub fn insert_char(&mut self, row: usize, col: usize, c: char) {
        if let Some(line) = self.line_mut(row) {
            let byte_idx = char_to_byte_index(line, col);
            line.insert(byte_idx, c);
        }
    }

    pub fn insert_str(&mut self, row: usize, col: usize, s: &str) {
        if let Some(line) = self.line_mut(row) {
            let byte_idx = char_to_byte_index(line, col);
            line.insert_str(byte_idx, s);
        }
    }

    pub fn delete_char(&mut self, row: usize, col: usize) -> Option<char> {
        if let Some(line) = self.line_mut(row) {
            let chars: Vec<char> = line.chars().collect();
            if col < chars.len() {
                let byte_idx = char_to_byte_index(line, col);
                return Some(line.remove(byte_idx));
            }
        }
        None
    }

    pub fn delete_range(&mut self, row: usize, start_col: usize, end_col: usize) -> String {
        if let Some(line) = self.line_mut(row) {
            let chars: Vec<char> = line.chars().collect();
            let start = start_col.min(chars.len());
            let end = end_col.min(chars.len());
            if start < end {
                let start_byte = char_to_byte_index(line, start);
                let end_byte = char_to_byte_index(line, end);
                return line.drain(start_byte..end_byte).collect();
            }
        }
        String::new()
    }

    pub fn insert_line(&mut self, row: usize, content: String) {
        self.move_gap_to(row);
        self.before.push(content);
    }

    pub fn split_line(&mut self, row: usize, col: usize) -> bool {
        self.move_gap_to(row + 1);
        if let Some(line) = self.before.get_mut(row) {
            let byte_idx = char_to_byte_index(line, col);
            let remainder = line.split_off(byte_idx);
            self.before.push(remainder);
            return true;
        }
        false
    }

    pub fn join_with_previous(&mut self, row: usize) -> bool {
        if row == 0 || row >= self.line_count() {
            return false;
        }
        self.move_gap_to(row + 1);
        if row < self.before.len() {
            let current_line = self.before.remove(row);
            if let Some(prev_line) = self.before.get_mut(row - 1) {
                prev_line.push_str(&current_line);
                return true;
            }
        }
        false
    }

    pub fn delete_line(&mut self, row: usize) -> Option<String> {
        if row >= self.line_count() {
            return None;
        }
        if self.line_count() == 1 {
            let content = std::mem::take(&mut self.before[0]);
            return Some(content);
        }
        self.move_gap_to(row + 1);
        self.before.pop()
    }

    pub fn get_text_range(&self, start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> String {
        if start_row == end_row {
            if let Some(line) = self.line(start_row) {
                let chars: Vec<char> = line.chars().collect();
                let start = start_col.min(chars.len());
                let end = end_col.min(chars.len());
                return chars[start..end].iter().collect();
            }
            return String::new();
        }

        let mut result = String::new();

        if let Some(line) = self.line(start_row) {
            let chars: Vec<char> = line.chars().collect();
            let start = start_col.min(chars.len());
            result.push_str(&chars[start..].iter().collect::<String>());
            result.push('\n');
        }

        for row in (start_row + 1)..end_row {
            if let Some(line) = self.line(row) {
                result.push_str(line);
                result.push('\n');
            }
        }

        if let Some(line) = self.line(end_row) {
            let chars: Vec<char> = line.chars().collect();
            let end = end_col.min(chars.len());
            result.push_str(&chars[..end].iter().collect::<String>());
        }

        result
    }

    pub fn delete_text_range(&mut self, start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> String {
        let deleted = self.get_text_range(start_row, start_col, end_row, end_col);

        if start_row == end_row {
            self.delete_range(start_row, start_col, end_col);
        } else {
            self.move_gap_to(end_row + 1);

            let end_remainder: String = self
                .line(end_row)
                .map(|l| {
                    let chars: Vec<char> = l.chars().collect();
                    chars[end_col.min(chars.len())..].iter().collect()
                })
                .unwrap_or_default();

            for _ in (start_row + 1)..=end_row {
                self.delete_line(start_row + 1);
            }

            if let Some(line) = self.line_mut(start_row) {
                let byte_idx = char_to_byte_index(line, start_col);
                line.truncate(byte_idx);
                line.push_str(&end_remainder);
            }
        }

        deleted
    }
}

fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buf = TextBuffer::default();
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line(0), Some(""));
    }

    #[test]
    fn test_from_lines() {
        let buf = TextBuffer::from_lines(vec!["hello".into(), "world".into()]);
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line(0), Some("hello"));
        assert_eq!(buf.line(1), Some("world"));
    }

    #[test]
    fn test_insert_char() {
        let mut buf = TextBuffer::from_lines(vec!["hello".into()]);
        buf.insert_char(0, 5, '!');
        assert_eq!(buf.line(0), Some("hello!"));
    }

    #[test]
    fn test_delete_char() {
        let mut buf = TextBuffer::from_lines(vec!["hello".into()]);
        let deleted = buf.delete_char(0, 4);
        assert_eq!(deleted, Some('o'));
        assert_eq!(buf.line(0), Some("hell"));
    }

    #[test]
    fn test_split_line() {
        let mut buf = TextBuffer::from_lines(vec!["hello world".into()]);
        buf.split_line(0, 5);
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.line(0), Some("hello"));
        assert_eq!(buf.line(1), Some(" world"));
    }

    #[test]
    fn test_join_lines() {
        let mut buf = TextBuffer::from_lines(vec!["hello".into(), " world".into()]);
        buf.join_with_previous(1);
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line(0), Some("hello world"));
    }

    #[test]
    fn test_get_text_range() {
        let buf = TextBuffer::from_lines(vec![
            "line one".into(),
            "line two".into(),
            "line three".into(),
        ]);
        let text = buf.get_text_range(0, 5, 2, 4);
        assert_eq!(text, "one\nline two\nline");
    }
}
