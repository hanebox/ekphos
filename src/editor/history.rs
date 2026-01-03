use std::collections::VecDeque;
use std::time::Instant;

use super::cursor::Position;

#[derive(Debug, Clone)]
pub enum EditOperation {
    Insert { pos: Position, text: String },
    Delete { start: Position, end: Position, deleted_text: String },
    SplitLine { pos: Position },
    JoinLine { row: usize, col: usize },
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
}
