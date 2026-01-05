use std::collections::VecDeque;
use std::time::Instant;

use super::cursor::Position;

#[derive(Debug, Clone)]
pub enum EditOperation {
    Insert { pos: Position, text: String },
    Delete { start: Position, end: Position, deleted_text: String },
    SplitLine { pos: Position },
    JoinLine { row: usize, col: usize },
    BlockDelete {
        start_row: usize,
        end_row: usize,
        start_col: usize,
        end_col: usize,
        deleted_lines: Vec<String>,
    },
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
    pub fn inverse(&self) -> EditOperation {
        match self {
            EditOperation::Insert { pos, text } => {
                let end = calculate_end_position(*pos, text);
                EditOperation::Delete { start: *pos, end, deleted_text: text.clone() }
            }
            EditOperation::Delete { start, deleted_text, .. } => {
                EditOperation::Insert { pos: *start, text: deleted_text.clone() }
            }
            EditOperation::SplitLine { pos } => {
                EditOperation::JoinLine { row: pos.row + 1, col: pos.col }
            }
            EditOperation::JoinLine { row, col } => {
                EditOperation::SplitLine { pos: Position::new(row - 1, *col) }
            }
            EditOperation::BlockDelete { start_row, start_col, deleted_lines, .. } => {
                EditOperation::BlockInsert {
                    start_row: *start_row,
                    col: *start_col,
                    lines: deleted_lines.clone(),
                }
            }
            EditOperation::BlockInsert { start_row, col, lines } => {
                let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
                EditOperation::BlockDelete {
                    start_row: *start_row,
                    end_row: start_row + lines.len().saturating_sub(1),
                    start_col: *col,
                    end_col: col + max_len.saturating_sub(1),
                    deleted_lines: lines.clone(),
                }
            }
            EditOperation::LineInsert { row, lines } => {
                EditOperation::LineDelete {
                    row: *row,
                    lines: lines.clone(),
                }
            }
            EditOperation::LineDelete { row, lines } => {
                EditOperation::LineInsert {
                    row: *row,
                    lines: lines.clone(),
                }
            }
        }
    }
}

fn calculate_end_position(start: Position, text: &str) -> Position {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return start;
    }

    if lines.len() == 1 {
        Position::new(start.row, start.col + text.chars().count())
    } else {
        Position::new(start.row + lines.len() - 1, lines.last().map_or(0, |l| l.chars().count()))
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
        Self {
            operations: vec![op],
            cursor_before,
            cursor_after,
            timestamp: Instant::now(),
        }
    }

    /// Check if this entry can merge with another single-char insertion
    pub fn can_merge(&self, op: &EditOperation, merge_timeout_ms: u64) -> bool {
        if self.timestamp.elapsed().as_millis() > merge_timeout_ms as u128 {
            return false;
        }

        if let (Some(EditOperation::Insert { pos: last_pos, text: last_text }), EditOperation::Insert { pos, text }) =
            (self.operations.last(), op)
        {
            if text.chars().count() == 1
                && last_text.chars().all(|c| !c.is_whitespace())
                && text.chars().all(|c| !c.is_whitespace())
            {
                let expected_col = last_pos.col + last_text.chars().count();
                return pos.row == last_pos.row && pos.col == expected_col;
            }
        }

        false
    }

    pub fn merge(&mut self, op: EditOperation, cursor_after: Position) {
        self.operations.push(op);
        self.cursor_after = cursor_after;
        self.timestamp = Instant::now();
    }
}

pub struct History {
    undo_stack: VecDeque<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    max_entries: usize,
    merge_timeout_ms: u64,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    const DEFAULT_MAX_ENTRIES: usize = 1000;
    const DEFAULT_MERGE_TIMEOUT_MS: u64 = 500;

    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(Self::DEFAULT_MAX_ENTRIES),
            redo_stack: Vec::new(),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            merge_timeout_ms: Self::DEFAULT_MERGE_TIMEOUT_MS,
        }
    }

    pub fn record(&mut self, op: EditOperation, cursor_before: Position, cursor_after: Position) {
        self.redo_stack.clear();

        if let Some(last) = self.undo_stack.back_mut() {
            if last.can_merge(&op, self.merge_timeout_ms) {
                last.merge(op, cursor_after);
                return;
            }
        }

        self.undo_stack.push_back(HistoryEntry::new(op, cursor_before, cursor_after));

        while self.undo_stack.len() > self.max_entries {
            self.undo_stack.pop_front();
        }
    }

    pub fn pop_undo(&mut self) -> Option<HistoryEntry> {
        if let Some(entry) = self.undo_stack.pop_back() {
            self.redo_stack.push(entry.clone());
            Some(entry)
        } else {
            None
        }
    }

    pub fn pop_redo(&mut self) -> Option<HistoryEntry> {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push_back(entry.clone());
            Some(entry)
        } else {
            None
        }
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
        let delete_op = EditOperation::Delete {
            start: Position::new(1, 5),
            end: Position::new(1, 10),
            deleted_text: "world".into(),
        };
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
        let op = EditOperation::LineInsert {
            row: 2,
            lines: vec!["line one".into(), "line two".into()],
        };
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
        let op = EditOperation::LineDelete {
            row: 3,
            lines: vec!["deleted line".into()],
        };
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
        let op = EditOperation::BlockInsert {
            start_row: 1,
            col: 5,
            lines: vec!["abc".into(), "def".into(), "ghi".into()],
        };
        let inverse = op.inverse();

        if let EditOperation::BlockDelete {
            start_row,
            end_row,
            start_col,
            end_col,
            deleted_lines,
        } = inverse
        {
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
        let op = EditOperation::BlockDelete {
            start_row: 0,
            end_row: 2,
            start_col: 10,
            end_col: 15,
            deleted_lines: vec!["foo".into(), "bar".into(), "baz".into()],
        };
        let inverse = op.inverse();

        if let EditOperation::BlockInsert {
            start_row,
            col,
            lines,
        } = inverse
        {
            assert_eq!(start_row, 0);
            assert_eq!(col, 10);
            assert_eq!(lines, vec!["foo", "bar", "baz"]);
        } else {
            panic!("Expected BlockInsert operation");
        }
    }

    #[test]
    fn test_inverse_split_line() {
        let op = EditOperation::SplitLine {
            pos: Position::new(5, 10),
        };
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

        history.record(
            EditOperation::Insert {
                pos: Position::new(5, 10),
                text: "hello".into(),
            },
            cursor_before,
            cursor_after,
        );

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

        history.record(
            EditOperation::SplitLine {
                pos: Position::new(3, 0),
            },
            cursor_before,
            cursor_after,
        );

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

        history.record(
            EditOperation::LineDelete {
                row: 5,
                lines: vec!["   deleted line content".into()],
            },
            cursor_before,
            cursor_after,
        );

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
        history.record(
            EditOperation::Insert {
                pos: Position::new(0, 0),
                text: "hello".into(),
            },
            Position::new(0, 0),
            Position::new(0, 5),
        );

        // Second edit: split line (won't merge with insert)
        history.record(
            EditOperation::SplitLine {
                pos: Position::new(0, 5),
            },
            Position::new(0, 5),
            Position::new(1, 0),
        );

        // Third edit: insert on new line (won't merge - different row)
        history.record(
            EditOperation::Insert {
                pos: Position::new(1, 0),
                text: "world".into(),
            },
            Position::new(1, 0),
            Position::new(1, 5),
        );

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

        history.record(
            EditOperation::LineInsert {
                row: 3,
                lines: vec![
                    "first inserted line".into(),
                    "second inserted line".into(),
                    "third inserted line".into(),
                ],
            },
            cursor_before,
            cursor_after,
        );

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

        history.record(
            EditOperation::BlockDelete {
                start_row: 1,
                end_row: 3,
                start_col: 5,
                end_col: 10,
                deleted_lines: vec!["12345".into(), "12345".into(), "12345".into()],
            },
            cursor_before,
            cursor_after,
        );

        let entry = history.pop_undo().unwrap();
        assert_eq!(entry.cursor_before.row, 1);
        assert_eq!(entry.cursor_before.col, 5);
    }

    #[test]
    fn test_double_inverse_is_original() {
        let original = EditOperation::LineInsert {
            row: 5,
            lines: vec!["test line".into()],
        };
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
        let original = EditOperation::BlockDelete {
            start_row: 2,
            end_row: 4,
            start_col: 3,
            end_col: 8,
            deleted_lines: vec!["abc".into(), "def".into(), "ghi".into()],
        };
        let inverse = original.inverse();
        let double_inverse = inverse.inverse();

        if let EditOperation::BlockDelete {
            start_row,
            end_row,
            start_col,
            deleted_lines,
            ..
        } = double_inverse
        {
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
        let op = EditOperation::Insert {
            pos: Position::new(2, 5),
            text: "hello\nworld\n!".into(),
        };
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
        let op = EditOperation::LineInsert {
            row: 0,
            lines: vec!["".into()],
        };
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
            history.record(
                EditOperation::Insert {
                    pos: Position::new(0, i),
                    text: "x".into(),
                },
                Position::new(0, i),
                Position::new(0, i + 1),
            );
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
}
