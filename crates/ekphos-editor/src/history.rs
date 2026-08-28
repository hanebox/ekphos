use std::collections::VecDeque;
use std::time::Instant;

use super::cursor::Position;

#[derive(Debug, Clone)]
pub enum EditOperation {
    Insert {
        pos: Position,
        text: String,
    },
    Delete {
        start: Position,
        end: Position,
        deleted_text: String,
    },
    SplitLine {
        pos: Position,
    },
    JoinLine {
        row: usize,
        col: usize,
    },
    BlockDelete {
        start_row: usize,
        end_row: usize,
        start_col: usize,
        end_col: usize,
        deleted_lines: Vec<String>,
    },
    #[cfg(test)]
    BlockInsert {
        start_row: usize,
        col: usize,
        lines: Vec<String>,
    },
    LineInsert {
        row: usize,
        lines: Vec<String>,
    },
    LineDelete {
        row: usize,
        lines: Vec<String>,
    },
}

impl EditOperation {
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Insert { text, .. } => text.capacity(),
            Self::Delete { deleted_text, .. } => deleted_text.capacity(),
            Self::BlockDelete { deleted_lines, .. } | Self::LineInsert { lines: deleted_lines, .. } | Self::LineDelete { lines: deleted_lines, .. } => deleted_lines.capacity() * std::mem::size_of::<String>() + deleted_lines.iter().map(String::capacity).sum::<usize>(),
            #[cfg(test)]
            Self::BlockInsert { lines, .. } => lines.capacity() * std::mem::size_of::<String>() + lines.iter().map(String::capacity).sum::<usize>(),
            Self::SplitLine { .. } | Self::JoinLine { .. } => 0,
        }
    }

    #[cfg(test)]
    pub fn inverse(&self) -> EditOperation {
        match self {
            EditOperation::Insert { pos, text } => {
                let end = calculate_end_position(*pos, text);
                EditOperation::Delete { start: *pos, end, deleted_text: text.clone() }
            }
            EditOperation::Delete { start, deleted_text, .. } => EditOperation::Insert { pos: *start, text: deleted_text.clone() },
            EditOperation::SplitLine { pos } => EditOperation::JoinLine { row: pos.row + 1, col: pos.col },
            EditOperation::JoinLine { row, col } => EditOperation::SplitLine { pos: Position::new(row - 1, *col) },
            EditOperation::BlockDelete { start_row, start_col, deleted_lines, .. } => EditOperation::BlockInsert { start_row: *start_row, col: *start_col, lines: deleted_lines.clone() },
            EditOperation::BlockInsert { start_row, col, lines } => {
                let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                EditOperation::BlockDelete { start_row: *start_row, end_row: start_row + lines.len().saturating_sub(1), start_col: *col, end_col: col + max_len.saturating_sub(1), deleted_lines: lines.clone() }
            }
            EditOperation::LineInsert { row, lines } => EditOperation::LineDelete { row: *row, lines: lines.clone() },
            EditOperation::LineDelete { row, lines } => EditOperation::LineInsert { row: *row, lines: lines.clone() },
        }
    }
}

pub(super) fn calculate_end_position(start: Position, text: &str) -> Position {
    if text.is_empty() {
        return start;
    }
    let newline_count = text.bytes().filter(|byte| *byte == b'\n').count();
    if newline_count == 0 {
        Position::new(start.row, start.col + text.chars().count())
    } else {
        Position::new(start.row + newline_count, text.rsplit_once('\n').map_or(0, |(_, line)| line.chars().count()))
    }
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub operations: Vec<EditOperation>,
    pub cursor_before: Position,
    pub cursor_after: Position,
    pub timestamp: Instant,
}

impl HistoryEntry {
    pub fn new(op: EditOperation, cursor_before: Position, cursor_after: Position) -> Self {
        Self { operations: vec![op], cursor_before, cursor_after, timestamp: Instant::now() }
    }

    /// Check if this entry can merge with another single-char insertion
    pub fn can_merge(&self, op: &EditOperation, merge_timeout_ms: u64) -> bool {
        if self.timestamp.elapsed().as_millis() > merge_timeout_ms as u128 {
            return false;
        }
        if let (Some(EditOperation::Insert { pos: last_pos, text: last_text }), EditOperation::Insert { pos, text }) = (self.operations.last(), op) {
            if text.chars().count() == 1 && last_text.chars().all(|c| !c.is_whitespace()) && text.chars().all(|c| !c.is_whitespace()) {
                let expected_col = last_pos.col + last_text.chars().count();
                return pos.row == last_pos.row && pos.col == expected_col;
            }
        }
        false
    }

    pub fn merge(&mut self, op: EditOperation, cursor_after: Position) {
        match (self.operations.last_mut(), op) {
            (Some(EditOperation::Insert { text: previous, .. }), EditOperation::Insert { text, .. }) => previous.push_str(&text),
            (_, op) => self.operations.push(op),
        }
        self.cursor_after = cursor_after;
        self.timestamp = Instant::now();
    }
    fn payload_bytes(&self) -> usize {
        self.operations.capacity() * std::mem::size_of::<EditOperation>() + self.operations.iter().map(EditOperation::retained_bytes).sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryStats {
    pub undo_entries: usize,
    pub redo_entries: usize,
    pub undo_payload_bytes: usize,
    pub redo_payload_bytes: usize,
    pub payload_limit_bytes: usize,
}

impl HistoryStats {
    pub fn total_entries(self) -> usize {
        self.undo_entries + self.redo_entries
    }

    pub fn total_payload_bytes(self) -> usize {
        self.undo_payload_bytes + self.redo_payload_bytes
    }
}

pub struct History {
    undo_stack: VecDeque<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    max_entries: usize,
    max_payload_bytes: usize,
    merge_timeout_ms: u64,
    undo_payload_bytes: usize,
    redo_payload_bytes: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    const DEFAULT_MAX_ENTRIES: usize = 1000;
    pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
    const DEFAULT_MERGE_TIMEOUT_MS: u64 = 500;

    pub fn new() -> Self {
        Self::with_limits(Self::DEFAULT_MAX_ENTRIES, Self::DEFAULT_MAX_PAYLOAD_BYTES)
    }

    pub fn with_limits(max_entries: usize, max_payload_bytes: usize) -> Self {
        Self { undo_stack: VecDeque::new(), redo_stack: Vec::new(), max_entries: max_entries.max(1), max_payload_bytes, merge_timeout_ms: Self::DEFAULT_MERGE_TIMEOUT_MS, undo_payload_bytes: 0, redo_payload_bytes: 0 }
    }

    pub fn record(&mut self, op: EditOperation, cursor_before: Position, cursor_after: Position) {
        self.redo_stack.clear();
        self.redo_payload_bytes = 0;
        if let Some(last) = self.undo_stack.back_mut() {
            let merge_estimate = last.payload_bytes().saturating_add(op.retained_bytes()).saturating_add(std::mem::size_of::<EditOperation>());
            if last.can_merge(&op, self.merge_timeout_ms) && merge_estimate <= self.max_payload_bytes {
                let old_bytes = last.payload_bytes();
                last.merge(op, cursor_after);
                self.undo_payload_bytes = self.undo_payload_bytes.saturating_sub(old_bytes) + last.payload_bytes();
                self.enforce_limits();
                return;
            }
        }
        let entry = HistoryEntry::new(op, cursor_before, cursor_after);
        self.undo_payload_bytes += entry.payload_bytes();
        self.undo_stack.push_back(entry);
        self.enforce_limits();
    }

    pub(super) fn record_group(&mut self, operations: Vec<EditOperation>, cursor_before: Position, cursor_after: Position) {
        if operations.is_empty() {
            return;
        }
        if operations.len() == 1 {
            self.record(operations.into_iter().next().expect("checked non-empty"), cursor_before, cursor_after);
            return;
        }
        self.redo_stack.clear();
        self.redo_payload_bytes = 0;
        let entry = HistoryEntry { operations, cursor_before, cursor_after, timestamp: Instant::now() };
        self.undo_payload_bytes += entry.payload_bytes();
        self.undo_stack.push_back(entry);
        self.enforce_limits();
    }
    fn enforce_limits(&mut self) {
        while self.undo_stack.len() > 1 && (self.undo_stack.len() + self.redo_stack.len() > self.max_entries || self.undo_payload_bytes + self.redo_payload_bytes > self.max_payload_bytes) {
            if let Some(entry) = self.undo_stack.pop_front() {
                self.undo_payload_bytes = self.undo_payload_bytes.saturating_sub(entry.payload_bytes());
            }
        }
    }

    /// Move one entry from undo to redo without cloning its payload.
    pub fn pop_undo(&mut self) -> Option<&HistoryEntry> {
        let entry = self.undo_stack.pop_back()?;
        let payload_bytes = entry.payload_bytes();
        self.undo_payload_bytes = self.undo_payload_bytes.saturating_sub(payload_bytes);
        self.redo_payload_bytes += payload_bytes;
        self.redo_stack.push(entry);
        self.redo_stack.last()
    }

    /// Move one entry from redo to undo without cloning its payload.
    pub fn pop_redo(&mut self) -> Option<&HistoryEntry> {
        let entry = self.redo_stack.pop()?;
        let payload_bytes = entry.payload_bytes();
        self.redo_payload_bytes = self.redo_payload_bytes.saturating_sub(payload_bytes);
        self.undo_payload_bytes += payload_bytes;
        self.undo_stack.push_back(entry);
        self.undo_stack.back()
    }

    pub fn stats(&self) -> HistoryStats {
        HistoryStats { undo_entries: self.undo_stack.len(), redo_entries: self.redo_stack.len(), undo_payload_bytes: self.undo_payload_bytes, redo_payload_bytes: self.redo_payload_bytes, payload_limit_bytes: self.max_payload_bytes }
    }

    pub fn set_limits(&mut self, max_entries: usize, max_payload_bytes: usize) {
        self.max_entries = max_entries.max(1);
        self.max_payload_bytes = max_payload_bytes;
        self.enforce_limits();
    }

    pub fn retained_bytes(&self) -> usize {
        let entry_bytes = |entry: &HistoryEntry| entry.operations.capacity() * std::mem::size_of::<EditOperation>() + entry.operations.iter().map(EditOperation::retained_bytes).sum::<usize>();
        self.undo_stack.capacity() * std::mem::size_of::<HistoryEntry>() + self.redo_stack.capacity() * std::mem::size_of::<HistoryEntry>() + self.undo_stack.iter().map(entry_bytes).sum::<usize>() + self.redo_stack.iter().map(entry_bytes).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_undo() {
        let mut history = History::new();
        let pos = Position::new(0, 0);
        history.record(EditOperation::Insert { pos, text: "a".into() }, pos, Position::new(0, 1));
        let entry = history.pop_undo();
        assert!(entry.is_some());
    }

    #[test]
    fn test_redo() {
        let mut history = History::new();
        let pos = Position::new(0, 0);
        history.record(EditOperation::Insert { pos, text: "a".into() }, pos, Position::new(0, 1));
        history.pop_undo();
        let entry = history.pop_redo();
        assert!(entry.is_some());
    }

    #[test]
    fn test_new_edit_clears_redo() {
        let mut history = History::new();
        let pos = Position::new(0, 0);
        history.record(EditOperation::Insert { pos, text: "a".into() }, pos, Position::new(0, 1));
        history.pop_undo();
        history.record(EditOperation::Insert { pos, text: "b".into() }, pos, Position::new(0, 1));
        assert!(history.pop_redo().is_none());
    }

    #[test]
    fn test_inverse_operations() {
        let insert_op = EditOperation::Insert { pos: Position::new(0, 0), text: "hello".into() };
        let inverse = insert_op.inverse();
        if let EditOperation::Delete { start, end, deleted_text } = inverse {
            assert_eq!(start.col, 0);
            assert_eq!(end.col, 5);
            assert_eq!(deleted_text, "hello");
        } else {
            panic!("Expected Delete operation");
        }
    }

    #[test]
    fn test_inverse_delete() {
        let delete_op = EditOperation::Delete { start: Position::new(1, 5), end: Position::new(1, 10), deleted_text: "world".into() };
        let inverse = delete_op.inverse();
        if let EditOperation::Insert { pos, text } = inverse {
            assert_eq!(pos.row, 1);
            assert_eq!(pos.col, 5);
            assert_eq!(text, "world");
        } else {
            panic!("Expected Insert operation");
        }
    }

    #[test]
    fn test_inverse_line_insert() {
        let op = EditOperation::LineInsert { row: 2, lines: vec!["line one".into(), "line two".into()] };
        let inverse = op.inverse();
        if let EditOperation::LineDelete { row, lines } = inverse {
            assert_eq!(row, 2);
            assert_eq!(lines, vec!["line one", "line two"]);
        } else {
            panic!("Expected LineDelete operation");
        }
    }

    #[test]
    fn test_inverse_line_delete() {
        let op = EditOperation::LineDelete { row: 3, lines: vec!["deleted line".into()] };
        let inverse = op.inverse();
        if let EditOperation::LineInsert { row, lines } = inverse {
            assert_eq!(row, 3);
            assert_eq!(lines, vec!["deleted line"]);
        } else {
            panic!("Expected LineInsert operation");
        }
    }

    #[test]
    fn test_inverse_block_insert() {
        let op = EditOperation::BlockInsert { start_row: 1, col: 5, lines: vec!["abc".into(), "def".into(), "ghi".into()] };
        let inverse = op.inverse();
        if let EditOperation::BlockDelete { start_row, end_row, start_col, end_col, deleted_lines } = inverse {
            assert_eq!(start_row, 1);
            assert_eq!(end_row, 3);
            assert_eq!(start_col, 5);
            assert_eq!(end_col, 7); // col + max_len - 1 = 5 + 3 - 1 = 7
            assert_eq!(deleted_lines, vec!["abc", "def", "ghi"]);
        } else {
            panic!("Expected BlockDelete operation");
        }
    }

    #[test]
    fn test_inverse_block_delete() {
        let op = EditOperation::BlockDelete { start_row: 0, end_row: 2, start_col: 10, end_col: 15, deleted_lines: vec!["foo".into(), "bar".into(), "baz".into()] };
        let inverse = op.inverse();
        if let EditOperation::BlockInsert { start_row, col, lines } = inverse {
            assert_eq!(start_row, 0);
            assert_eq!(col, 10);
            assert_eq!(lines, vec!["foo", "bar", "baz"]);
        } else {
            panic!("Expected BlockInsert operation");
        }
    }

    #[test]
    fn test_inverse_split_line() {
        let op = EditOperation::SplitLine { pos: Position::new(5, 10) };
        let inverse = op.inverse();
        if let EditOperation::JoinLine { row, col } = inverse {
            assert_eq!(row, 6);
            assert_eq!(col, 10);
        } else {
            panic!("Expected JoinLine operation");
        }
    }

    #[test]
    fn test_inverse_join_line() {
        let op = EditOperation::JoinLine { row: 3, col: 15 };
        let inverse = op.inverse();
        if let EditOperation::SplitLine { pos } = inverse {
            assert_eq!(pos.row, 2);
            assert_eq!(pos.col, 15);
        } else {
            panic!("Expected SplitLine operation");
        }
    }

    #[test]
    fn test_cursor_position_preserved_on_undo() {
        let mut history = History::new();
        let cursor_before = Position::new(5, 10);
        let cursor_after = Position::new(5, 15);
        history.record(EditOperation::Insert { pos: Position::new(5, 10), text: "hello".into() }, cursor_before, cursor_after);
        let entry = history.pop_undo().unwrap();
        assert_eq!(entry.cursor_before.row, 5);
        assert_eq!(entry.cursor_before.col, 10);
        assert_eq!(entry.cursor_after.row, 5);
        assert_eq!(entry.cursor_after.col, 15);
    }

    #[test]
    fn test_cursor_position_preserved_on_redo() {
        let mut history = History::new();
        let cursor_before = Position::new(3, 0);
        let cursor_after = Position::new(4, 0);
        history.record(EditOperation::SplitLine { pos: Position::new(3, 0) }, cursor_before, cursor_after);
        history.pop_undo();
        let entry = history.pop_redo().unwrap();
        assert_eq!(entry.cursor_before.row, 3);
        assert_eq!(entry.cursor_before.col, 0);
        assert_eq!(entry.cursor_after.row, 4);
        assert_eq!(entry.cursor_after.col, 0);
    }

    #[test]
    fn test_line_delete_cursor_restoration() {
        let mut history = History::new();
        // Simulating: cursor at line 5, delete line, cursor should restore to line 5 on undo
        let cursor_before = Position::new(5, 3);
        let cursor_after = Position::new(5, 0);
        history.record(EditOperation::LineDelete { row: 5, lines: vec!["   deleted line content".into()] }, cursor_before, cursor_after);
        let entry = history.pop_undo().unwrap();
        // After undo, cursor should go back to (5, 3)
        assert_eq!(entry.cursor_before.row, 5);
        assert_eq!(entry.cursor_before.col, 3);
    }

    #[test]
    fn test_multiple_undo_redo_cycle() {
        let mut history = History::new();
        // Use different operation types to prevent merging
        // First edit: insert at start
        history.record(EditOperation::Insert { pos: Position::new(0, 0), text: "hello".into() }, Position::new(0, 0), Position::new(0, 5));
        // Second edit: split line (won't merge with insert)
        history.record(EditOperation::SplitLine { pos: Position::new(0, 5) }, Position::new(0, 5), Position::new(1, 0));
        // Third edit: insert on new line (won't merge - different row)
        history.record(EditOperation::Insert { pos: Position::new(1, 0), text: "world".into() }, Position::new(1, 0), Position::new(1, 5));
        // Undo all three (in reverse order)
        let entry1 = history.pop_undo().unwrap();
        assert_eq!(entry1.cursor_before.row, 1);
        assert_eq!(entry1.cursor_before.col, 0);
        let entry2 = history.pop_undo().unwrap();
        assert_eq!(entry2.cursor_before.row, 0);
        assert_eq!(entry2.cursor_before.col, 5);
        let entry3 = history.pop_undo().unwrap();
        assert_eq!(entry3.cursor_before.row, 0);
        assert_eq!(entry3.cursor_before.col, 0);
        // Redo all three
        let redo1 = history.pop_redo().unwrap();
        assert_eq!(redo1.cursor_after.row, 0);
        assert_eq!(redo1.cursor_after.col, 5);
        let redo2 = history.pop_redo().unwrap();
        assert_eq!(redo2.cursor_after.row, 1);
        assert_eq!(redo2.cursor_after.col, 0);
        let redo3 = history.pop_redo().unwrap();
        assert_eq!(redo3.cursor_after.row, 1);
        assert_eq!(redo3.cursor_after.col, 5);
    }

    #[test]
    fn test_line_insert_multiple_lines_cursor() {
        let mut history = History::new();
        let cursor_before = Position::new(2, 5);
        let cursor_after = Position::new(5, 0); // After inserting 3 lines
        history.record(EditOperation::LineInsert { row: 3, lines: vec!["first inserted line".into(), "second inserted line".into(), "third inserted line".into()] }, cursor_before, cursor_after);
        let entry = history.pop_undo().unwrap();
        assert_eq!(entry.cursor_before.row, 2);
        assert_eq!(entry.cursor_before.col, 5);
    }

    #[test]
    fn test_block_delete_cursor_restoration() {
        let mut history = History::new();
        // Visual block select from (1,5) to (3,10), delete, cursor should restore
        let cursor_before = Position::new(1, 5);
        let cursor_after = Position::new(1, 5);
        history.record(EditOperation::BlockDelete { start_row: 1, end_row: 3, start_col: 5, end_col: 10, deleted_lines: vec!["12345".into(), "12345".into(), "12345".into()] }, cursor_before, cursor_after);
        let entry = history.pop_undo().unwrap();
        assert_eq!(entry.cursor_before.row, 1);
        assert_eq!(entry.cursor_before.col, 5);
    }

    #[test]
    fn test_double_inverse_is_original() {
        let original = EditOperation::LineInsert { row: 5, lines: vec!["test line".into()] };
        let inverse = original.inverse();
        let double_inverse = inverse.inverse();
        if let EditOperation::LineInsert { row, lines } = double_inverse {
            assert_eq!(row, 5);
            assert_eq!(lines, vec!["test line"]);
        } else {
            panic!("Expected LineInsert after double inverse");
        }
    }

    #[test]
    fn test_block_double_inverse_is_original() {
        let original = EditOperation::BlockDelete { start_row: 2, end_row: 4, start_col: 3, end_col: 8, deleted_lines: vec!["abc".into(), "def".into(), "ghi".into()] };
        let inverse = original.inverse();
        let double_inverse = inverse.inverse();
        if let EditOperation::BlockDelete { start_row, end_row, start_col, deleted_lines, .. } = double_inverse {
            assert_eq!(start_row, 2);
            assert_eq!(end_row, 4);
            assert_eq!(start_col, 3);
            assert_eq!(deleted_lines, vec!["abc", "def", "ghi"]);
        } else {
            panic!("Expected BlockDelete after double inverse");
        }
    }

    #[test]
    fn test_multiline_insert_inverse() {
        let op = EditOperation::Insert { pos: Position::new(2, 5), text: "hello\nworld\n!".into() };
        let inverse = op.inverse();
        if let EditOperation::Delete { start, end, deleted_text } = inverse {
            assert_eq!(start.row, 2);
            assert_eq!(start.col, 5);
            assert_eq!(end.row, 4);
            assert_eq!(end.col, 1);
            assert_eq!(deleted_text, "hello\nworld\n!");
        } else {
            panic!("Expected Delete operation");
        }
    }

    #[test]
    fn test_empty_line_insert() {
        let op = EditOperation::LineInsert { row: 0, lines: vec!["".into()] };
        let inverse = op.inverse();
        if let EditOperation::LineDelete { row, lines } = inverse {
            assert_eq!(row, 0);
            assert_eq!(lines, vec![""]);
        } else {
            panic!("Expected LineDelete operation");
        }
    }

    #[test]
    fn test_max_entries_limit() {
        let mut history = History::new();
        // Record more than max entries
        for i in 0..1100usize {
            history.record(EditOperation::Insert { pos: Position::new(0, i), text: "x".into() }, Position::new(0, i), Position::new(0, i + 1));
            // Sleep briefly to prevent merging
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // Count undo entries
        let mut count = 0;
        while history.pop_undo().is_some() {
            count += 1;
        }
        // Should be capped at max_entries (1000)
        assert!(count <= History::DEFAULT_MAX_ENTRIES);
    }

    #[test]
    fn payload_budget_evicts_old_entries_and_keeps_oversized_newest() {
        let mut history = History::with_limits(10, 8);
        for row in 0..3 {
            history.record(EditOperation::Insert { pos: Position::new(row, 0), text: "123456".into() }, Position::new(row, 0), Position::new(row, 6));
        }
        assert_eq!(history.stats().undo_entries, 1);
        assert!(history.stats().total_payload_bytes() >= 6);
        history.record(EditOperation::Delete { start: Position::new(0, 0), end: Position::new(0, 32), deleted_text: "x".repeat(32) }, Position::new(0, 32), Position::new(0, 0));
        let stats = history.stats();
        assert_eq!(stats.undo_entries, 1);
        assert!(stats.total_payload_bytes() >= 32);
        assert_eq!(stats.payload_limit_bytes, 8);
    }

    #[test]
    fn undo_redo_moves_the_same_payload_allocation() {
        let mut history = History::new();
        history.record(EditOperation::Insert { pos: Position::new(0, 0), text: "move me without cloning".repeat(1024) }, Position::new(0, 0), Position::new(0, 21 * 1024));
        let original_ptr = match &history.undo_stack.back().unwrap().operations[0] {
            EditOperation::Insert { text, .. } => text.as_ptr(),
            _ => unreachable!(),
        };
        let payload_bytes = history.stats().total_payload_bytes();
        let undone = history.pop_undo().unwrap();
        let undo_ptr = match &undone.operations[0] {
            EditOperation::Insert { text, .. } => text.as_ptr(),
            _ => unreachable!(),
        };
        assert_eq!(undo_ptr, original_ptr);
        assert_eq!(history.stats().total_payload_bytes(), payload_bytes);
        let redone = history.pop_redo().unwrap();
        let redo_ptr = match &redone.operations[0] {
            EditOperation::Insert { text, .. } => text.as_ptr(),
            _ => unreachable!(),
        };
        assert_eq!(redo_ptr, original_ptr);
        assert_eq!(history.stats().total_payload_bytes(), payload_bytes);
    }

    #[test]
    fn consecutive_typing_merges_into_one_string_payload() {
        let mut history = History::new();
        for col in 0..10_000 {
            history.record(EditOperation::Insert { pos: Position::new(0, col), text: "x".into() }, Position::new(0, col), Position::new(0, col + 1));
        }
        let entry = history.undo_stack.back().unwrap();
        assert_eq!(entry.operations.len(), 1);
        assert!(history.stats().total_payload_bytes() < 32 * 1024);
    }
}
