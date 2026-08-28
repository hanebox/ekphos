use std::cell::RefCell;
use std::cmp::Ordering;
use std::sync::Arc;

/// Immutable, cheaply cloned view of an editor revision.
///
/// Lines are shared with the mutable buffer. Creating a snapshot allocates only
/// the compact line-reference table; subsequent snapshots of the same revision
/// clone one `Arc` instead of copying the document.
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    lines: Arc<[Arc<String>]>,
    text_bytes: usize,
}

impl EditorSnapshot {
    pub fn line(&self, row: usize) -> Option<&str> {
        self.lines.get(row).map(|line| line.as_str())
    }

    pub fn iter_lines(&self) -> impl ExactSizeIterator<Item = &str> {
        self.lines.iter().map(|line| line.as_str())
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn text_bytes(&self) -> usize {
        self.text_bytes
    }

    pub fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.text_bytes);
        for (index, line) in self.iter_lines().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(line);
        }
        text
    }

    pub fn reference_bytes(&self) -> usize {
        self.lines.len() * std::mem::size_of::<Arc<String>>()
    }
}

/// Line-based gap buffer for efficient text editing.
/// Uses two vectors: `before` (lines before gap) and `after` (lines after gap, reversed).
/// Provides O(1) operations for localized edits.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    before: Vec<Arc<String>>,
    after: Vec<Arc<String>>,
    cached_snapshot: RefCell<Option<EditorSnapshot>>,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self { before: vec![Arc::new(String::new())], after: Vec::new(), cached_snapshot: RefCell::new(None) }
    }
}

impl TextBuffer {
    pub fn from_lines(lines: Vec<String>) -> Self {
        if lines.is_empty() {
            return Self::default();
        }
        Self { before: lines.into_iter().map(Arc::new).collect(), after: Vec::new(), cached_snapshot: RefCell::new(None) }
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.before.len() + self.after.len()
    }

    pub fn is_empty(&self) -> bool {
        self.line_count() == 1 && self.before.first().is_none_or(|line| line.is_empty())
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
        self.invalidate_snapshot();
        self.before.get_mut(row).map(Arc::make_mut)
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.line(row).map_or(0, |l| l.chars().count())
    }

    pub fn iter_lines(&self) -> impl Iterator<Item = &str> {
        self.before.iter().chain(self.after.iter().rev()).map(|line| line.as_str())
    }

    pub fn snapshot(&self) -> EditorSnapshot {
        if let Some(snapshot) = self.cached_snapshot.borrow().as_ref() {
            return snapshot.clone();
        }
        let lines: Arc<[Arc<String>]> = self.before.iter().chain(self.after.iter().rev()).cloned().collect();
        let text_bytes = lines.iter().map(|line| line.len()).sum::<usize>() + lines.len().saturating_sub(1);
        let snapshot = EditorSnapshot { lines, text_bytes };
        *self.cached_snapshot.borrow_mut() = Some(snapshot.clone());
        snapshot
    }

    pub fn retained_bytes(&self) -> usize {
        (self.before.capacity() + self.after.capacity()) * std::mem::size_of::<Arc<String>>() + self.before.iter().chain(self.after.iter()).map(|line| line.capacity()).sum::<usize>() + self.cached_snapshot.borrow().as_ref().map_or(0, EditorSnapshot::reference_bytes)
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
            let byte_idx = char_to_byte_index(line, col);
            return (byte_idx < line.len()).then(|| line.remove(byte_idx));
        }
        None
    }

    pub fn delete_range(&mut self, row: usize, start_col: usize, end_col: usize) -> String {
        if let Some(line) = self.line_mut(row) {
            let start_byte = char_to_byte_index(line, start_col);
            let end_byte = char_to_byte_index(line, end_col);
            if start_byte < end_byte {
                return line.drain(start_byte..end_byte).collect();
            }
        }
        String::new()
    }

    pub fn insert_line(&mut self, row: usize, content: String) {
        self.move_gap_to(row);
        self.invalidate_snapshot();
        self.before.push(Arc::new(content));
    }

    pub fn split_line(&mut self, row: usize, col: usize) -> bool {
        self.move_gap_to(row + 1);
        self.invalidate_snapshot();
        if let Some(line) = self.before.get_mut(row) {
            let line = Arc::make_mut(line);
            let byte_idx = char_to_byte_index(line, col);
            let remainder = line.split_off(byte_idx);
            self.before.push(Arc::new(remainder));
            return true;
        }
        false
    }

    pub fn join_with_previous(&mut self, row: usize) -> bool {
        if row == 0 || row >= self.line_count() {
            return false;
        }
        self.move_gap_to(row + 1);
        self.invalidate_snapshot();
        if row < self.before.len() {
            let current_line = self.before.remove(row);
            if let Some(prev_line) = self.before.get_mut(row - 1) {
                let prev_line = Arc::make_mut(prev_line);
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
        self.move_gap_to(row + 1);
        self.invalidate_snapshot();
        if self.line_count() == 1 {
            let content = std::mem::take(Arc::make_mut(&mut self.before[0]));
            return Some(content);
        }
        self.before.pop().map(|line| Arc::try_unwrap(line).unwrap_or_else(|line| (*line).clone()))
    }
    fn discard_line(&mut self, row: usize) {
        if row >= self.line_count() {
            return;
        }
        self.move_gap_to(row + 1);
        self.invalidate_snapshot();
        if self.line_count() == 1 {
            Arc::make_mut(&mut self.before[0]).clear();
        } else {
            self.before.pop();
        }
    }

    pub fn get_text_range(&self, start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> String {
        if start_row == end_row {
            if let Some(line) = self.line(start_row) {
                return char_slice(line, start_col, end_col).to_owned();
            }
            return String::new();
        }
        let first_bytes = self.line(start_row).map_or(0, |line| line.len().saturating_sub(char_to_byte_index(line, start_col)));
        let middle_bytes = ((start_row + 1)..end_row).filter_map(|row| self.line(row)).map(|line| line.len().saturating_add(1)).sum::<usize>();
        let last_bytes = self.line(end_row).map_or(0, |line| char_to_byte_index(line, end_col));
        let mut result = String::with_capacity(first_bytes.saturating_add(1).saturating_add(middle_bytes).saturating_add(last_bytes));
        if let Some(line) = self.line(start_row) {
            result.push_str(&line[char_to_byte_index(line, start_col)..]);
            result.push('\n');
        }
        for row in (start_row + 1)..end_row {
            if let Some(line) = self.line(row) {
                result.push_str(line);
                result.push('\n');
            }
        }
        if let Some(line) = self.line(end_row) {
            result.push_str(&line[..char_to_byte_index(line, end_col)]);
        }
        result
    }

    pub fn delete_text_range(&mut self, start_row: usize, start_col: usize, end_row: usize, end_col: usize) -> String {
        let deleted = self.get_text_range(start_row, start_col, end_row, end_col);
        self.discard_text_range(start_row, start_col, end_row, end_col);
        deleted
    }
    pub(super) fn discard_text_range(&mut self, start_row: usize, start_col: usize, end_row: usize, end_col: usize) {
        if start_row == end_row {
            self.delete_range(start_row, start_col, end_col);
        } else {
            self.move_gap_to(end_row + 1);
            let end_remainder: String = self.line(end_row).map(|line| line[char_to_byte_index(line, end_col)..].to_owned()).unwrap_or_default();
            for _ in (start_row + 1)..=end_row {
                self.discard_line(start_row + 1);
            }
            if let Some(line) = self.line_mut(start_row) {
                let byte_idx = char_to_byte_index(line, start_col);
                line.truncate(byte_idx);
                line.push_str(&end_remainder);
            }
        }
    }
    fn invalidate_snapshot(&mut self) {
        self.cached_snapshot.get_mut().take();
    }
}

pub(super) fn char_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

pub(super) fn char_slice(s: &str, start: usize, end: usize) -> &str {
    let start = char_to_byte_index(s, start);
    let end = char_to_byte_index(s, end);
    &s[start.min(end)..end]
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
        let buf = TextBuffer::from_lines(vec!["line one".into(), "line two".into(), "line three".into()]);
        let text = buf.get_text_range(0, 5, 2, 4);
        assert_eq!(text, "one\nline two\nline");
        assert_eq!(text.capacity(), text.len());
    }

    #[test]
    fn snapshots_share_unchanged_lines_and_preserve_old_revisions() {
        let mut buffer = TextBuffer::from_lines(vec!["first".into(), "second".into()]);
        let old = buffer.snapshot();
        let old_second = Arc::clone(&old.lines[1]);
        buffer.insert_char(0, 5, '!');
        let new = buffer.snapshot();
        assert_eq!(old.line(0), Some("first"));
        assert_eq!(new.line(0), Some("first!"));
        assert!(!Arc::ptr_eq(&old.lines[0], &new.lines[0]));
        assert!(Arc::ptr_eq(&old_second, &new.lines[1]));
        assert_eq!(new.to_text(), "first!\nsecond");
    }
}
