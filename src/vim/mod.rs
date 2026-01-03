//! Vim mode implementation for Ekphos
//!
//! Comprehensive vim-style editing support:
//! - Multiple modes (Normal, Insert, Replace, Visual, Visual Line, Visual Block, Command)
//! - Motions with count prefixes (5j, 3w, 2dd)
//! - Operators (d, c, y, >, <)
//! - Text objects (iw, aw, i", a(, ip, etc.)
//! - Registers (named a-z, numbered 0-9, clipboard +/*)
//! - Character find (f, F, t, T) with repeat (;, ,)
//! - Macros (q to record, @ to play)
//! - Marks (m to set, ` or ' to jump)
//! - Command mode (:w, :q, :wq, :%s/pat/rep/g)

pub mod command;
pub mod find;
pub mod macro_record;
pub mod marks;
pub mod mode;
pub mod motion;
pub mod operator;
pub mod register;
pub mod text_object;

pub use find::{FindState, PendingFind};
pub use macro_record::MacroState;
pub use marks::MarkMap;
pub use mode::VimMode;
pub use motion::Motion;
pub use operator::Operator;
pub use register::RegisterMap;
pub use text_object::{TextObject, TextObjectScope};

use crate::editor::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingMark {
    Set,
    GotoExact,
    GotoLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingMacro {
    Record,
    Play,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchDirection {
    #[default]
    Forward,
    Backward,
}

/// Represents a repeatable change for the . (dot) command
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum LastChange {
    /// Delete line(s): dd with optional count
    DeleteLine(usize),
    /// Change line(s): cc/S with inserted text
    ChangeLine(usize, String),
    /// Yank line(s): yy with count
    YankLine(usize),
    /// Delete to end: D
    DeleteToEnd,
    /// Change to end: C with inserted text
    ChangeToEnd(String),
    /// Delete char forward: x with count
    DeleteCharForward(usize),
    /// Delete char backward: X with count
    DeleteCharBackward(usize),
    /// Substitute char: s with inserted text
    SubstituteChar(String),
    /// Replace char: r with replacement char
    ReplaceChar(char),
    /// Insert with starting command (i, a, I, A, o, O) and inserted text
    Insert(char, String),
    /// Delete word forward: dw with count
    DeleteWordForward(usize),
    /// Delete word backward: db with count
    DeleteWordBackward(usize),
    /// Change word: cw with count and inserted text
    ChangeWord(usize, String),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RecordedCommand {
    pub operator: Option<Operator>,
    pub motion: Option<Motion>,
    pub text_object: Option<(TextObjectScope, TextObject)>,
    pub count: Option<usize>,
    pub inserted_text: Option<String>,
    pub register: Option<char>,
}

impl RecordedCommand {
    pub fn new() -> Self {
        Self {
            operator: None,
            motion: None,
            text_object: None,
            count: None,
            inserted_text: None,
            register: None,
        }
    }
}

impl Default for RecordedCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct VimState {
    pub mode: VimMode,
    pub count: Option<usize>,
    pub registers: RegisterMap,
    pub macros: MacroState,
    pub marks: MarkMap,
    pub search_pattern: Option<String>,
    pub search_direction: SearchDirection,
    pub last_find: Option<FindState>,
    pub pending_find: Option<PendingFind>,
    pub command_buffer: String,
    pub search_buffer: String,
    pub status_message: Option<String>,
    pub last_command: Option<RecordedCommand>,
    pub recording_command: Option<RecordedCommand>,
    pub pending_g: bool,
    pub pending_z: bool,
    pub awaiting_replace: bool,
    pub pending_text_object_scope: Option<TextObjectScope>,
    pub insert_start_pos: Option<Position>,
    pub pending_mark: Option<PendingMark>,
    pub pending_macro: Option<PendingMacro>,
    pub pending_register: bool,
    pub last_change: Option<LastChange>,
    pub insert_buffer: String,
}

impl Default for VimState {
    fn default() -> Self {
        Self::new()
    }
}

impl VimState {
    pub fn new() -> Self {
        Self {
            mode: VimMode::Normal,
            count: None,
            registers: RegisterMap::new(),
            macros: MacroState::new(),
            marks: MarkMap::new(),
            search_pattern: None,
            search_direction: SearchDirection::Forward,
            last_find: None,
            pending_find: None,
            command_buffer: String::new(),
            search_buffer: String::new(),
            status_message: None,
            last_command: None,
            recording_command: None,
            pending_g: false,
            pending_z: false,
            awaiting_replace: false,
            pending_text_object_scope: None,
            insert_start_pos: None,
            pending_mark: None,
            pending_macro: None,
            pending_register: false,
            last_change: None,
            insert_buffer: String::new(),
        }
    }

    pub fn reset_pending(&mut self) {
        self.count = None;
        self.pending_g = false;
        self.pending_z = false;
        self.pending_find = None;
        self.awaiting_replace = false;
        self.pending_text_object_scope = None;
        self.pending_mark = None;
        self.pending_macro = None;
        self.pending_register = false;
        self.registers.clear_selection();

        if matches!(self.mode, VimMode::OperatorPending { .. }) {
            self.mode = VimMode::Normal;
        }
    }

    #[allow(dead_code)]
    pub fn enter_replace_mode(&mut self) {
        self.mode = VimMode::Replace;
        self.reset_pending();
    }

    #[allow(dead_code)]
    pub fn enter_visual_block_mode(&mut self) {
        self.mode = VimMode::VisualBlock;
        self.reset_pending();
    }

    pub fn get_count(&self) -> usize {
        self.count.unwrap_or(1)
    }

    pub fn accumulate_count(&mut self, digit: usize) {
        self.count = Some(self.count.unwrap_or(0) * 10 + digit);
    }

    #[allow(dead_code)]
    pub fn enter_operator_pending(&mut self, op: Operator) {
        let count = self.count.take();
        self.mode = VimMode::OperatorPending { operator: op, count };
        self.recording_command = Some(RecordedCommand {
            operator: Some(op),
            count,
            ..RecordedCommand::new()
        });
    }

    #[allow(dead_code)]
    pub fn enter_insert_mode(&mut self, start_pos: Position) {
        self.mode = VimMode::Insert;
        self.insert_start_pos = Some(start_pos);
        self.reset_pending();
    }

    #[allow(dead_code)]
    pub fn exit_insert_mode(&mut self) {
        self.mode = VimMode::Normal;
        self.insert_start_pos = None;
    }

    #[allow(dead_code)]
    pub fn enter_visual_mode(&mut self, line_wise: bool) {
        self.mode = if line_wise { VimMode::VisualLine } else { VimMode::Visual };
        self.reset_pending();
    }

    pub fn enter_command_mode(&mut self) {
        self.mode = VimMode::Command;
        self.command_buffer.clear();
        self.reset_pending();
    }

    #[allow(dead_code)]
    pub fn exit_command_mode(&mut self) {
        self.mode = VimMode::Normal;
        self.command_buffer.clear();
    }

    #[allow(dead_code)]
    pub fn record_command(&mut self, cmd: RecordedCommand) {
        self.last_command = Some(cmd);
        self.recording_command = None;
    }

    #[allow(dead_code)]
    pub fn status_display(&self) -> String {
        let mut parts = Vec::new();

        if let Some(reg) = self.macros.recording_register() {
            parts.push(format!("recording @{}", reg));
        }

        parts.push(self.mode.display_name().to_string());

        if let Some(count) = self.count {
            parts.push(format!("{}", count));
        }

        if let VimMode::OperatorPending { operator, count } = &self.mode {
            if let Some(c) = count {
                parts.push(format!("{}", c));
            }
            parts.push(format!("{}-", operator.char()));
        }

        if self.pending_g { parts.push("g-".to_string()); }
        if self.pending_z { parts.push("z-".to_string()); }
        if self.pending_find.is_some() { parts.push("f-".to_string()); }
        if self.awaiting_replace { parts.push("r-".to_string()); }

        if let Some(pending) = &self.pending_mark {
            parts.push(match pending {
                PendingMark::Set => "m-".to_string(),
                PendingMark::GotoExact => "`-".to_string(),
                PendingMark::GotoLine => "'-".to_string(),
            });
        }

        if let Some(pending) = &self.pending_macro {
            parts.push(match pending {
                PendingMacro::Record => "q-".to_string(),
                PendingMacro::Play => "@-".to_string(),
            });
        }

        if let Some(scope) = &self.pending_text_object_scope {
            parts.push(match scope {
                TextObjectScope::Inner => "i-".to_string(),
                TextObjectScope::Around => "a-".to_string(),
            });
        }

        if let Some(reg) = self.registers.get_selected() {
            parts.push(format!("\"{}", reg));
        }

        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vim_state_new() {
        let state = VimState::new();
        assert_eq!(state.mode, VimMode::Normal);
        assert_eq!(state.count, None);
        assert!(!state.pending_g);
        assert!(!state.pending_z);
    }

    #[test]
    fn test_accumulate_count() {
        let mut state = VimState::new();
        state.accumulate_count(5);
        assert_eq!(state.count, Some(5));
        state.accumulate_count(3);
        assert_eq!(state.count, Some(53));
    }

    #[test]
    fn test_get_count_default() {
        let state = VimState::new();
        assert_eq!(state.get_count(), 1);
    }

    #[test]
    fn test_get_count_with_value() {
        let mut state = VimState::new();
        state.count = Some(5);
        assert_eq!(state.get_count(), 5);
    }

    #[test]
    fn test_reset_pending() {
        let mut state = VimState::new();
        state.count = Some(5);
        state.pending_g = true;
        state.pending_z = true;
        state.awaiting_replace = true;
        state.reset_pending();
        assert_eq!(state.count, None);
        assert!(!state.pending_g);
        assert!(!state.pending_z);
        assert!(!state.awaiting_replace);
    }

    #[test]
    fn test_enter_command_mode() {
        let mut state = VimState::new();
        state.command_buffer = "test".to_string();
        state.enter_command_mode();
        assert_eq!(state.mode, VimMode::Command);
        assert!(state.command_buffer.is_empty());
    }

    #[test]
    fn test_exit_command_mode() {
        let mut state = VimState::new();
        state.mode = VimMode::Command;
        state.command_buffer = "test".to_string();
        state.exit_command_mode();
        assert_eq!(state.mode, VimMode::Normal);
        assert!(state.command_buffer.is_empty());
    }

    #[test]
    fn test_enter_visual_mode() {
        let mut state = VimState::new();
        state.enter_visual_mode(false);
        assert_eq!(state.mode, VimMode::Visual);

        state.enter_visual_mode(true);
        assert_eq!(state.mode, VimMode::VisualLine);
    }

    #[test]
    fn test_status_display_normal() {
        let state = VimState::new();
        assert_eq!(state.status_display(), "NORMAL");
    }

    #[test]
    fn test_status_display_with_count() {
        let mut state = VimState::new();
        state.count = Some(5);
        assert_eq!(state.status_display(), "NORMAL 5");
    }

    #[test]
    fn test_status_display_pending_g() {
        let mut state = VimState::new();
        state.pending_g = true;
        assert_eq!(state.status_display(), "NORMAL g-");
    }

    #[test]
    fn test_recorded_command_default() {
        let cmd = RecordedCommand::default();
        assert!(cmd.operator.is_none());
        assert!(cmd.motion.is_none());
        assert!(cmd.count.is_none());
    }

    #[test]
    fn test_enter_replace_mode() {
        let mut state = VimState::new();
        state.enter_replace_mode();
        assert_eq!(state.mode, VimMode::Replace);
    }

    #[test]
    fn test_enter_visual_block_mode() {
        let mut state = VimState::new();
        state.enter_visual_block_mode();
        assert_eq!(state.mode, VimMode::VisualBlock);
    }

    #[test]
    fn test_reset_pending_clears_mark_and_macro() {
        let mut state = VimState::new();
        state.pending_mark = Some(PendingMark::Set);
        state.pending_macro = Some(PendingMacro::Record);
        state.reset_pending();
        assert!(state.pending_mark.is_none());
        assert!(state.pending_macro.is_none());
    }

    #[test]
    fn test_status_display_pending_mark() {
        let mut state = VimState::new();
        state.pending_mark = Some(PendingMark::Set);
        assert!(state.status_display().contains("m-"));

        state.pending_mark = Some(PendingMark::GotoExact);
        assert!(state.status_display().contains("`-"));

        state.pending_mark = Some(PendingMark::GotoLine);
        assert!(state.status_display().contains("'-"));
    }

    #[test]
    fn test_status_display_pending_macro() {
        let mut state = VimState::new();
        state.pending_macro = Some(PendingMacro::Record);
        assert!(state.status_display().contains("q-"));

        state.pending_macro = Some(PendingMacro::Play);
        assert!(state.status_display().contains("@-"));
    }

    #[test]
    fn test_status_display_recording() {
        let mut state = VimState::new();
        state.macros.start_recording('a');
        assert!(state.status_display().contains("recording @a"));
    }

    #[test]
    fn test_macros_initialized() {
        let state = VimState::new();
        assert!(!state.macros.is_recording());
    }

    #[test]
    fn test_marks_initialized() {
        let state = VimState::new();
        assert!(state.marks.get('a').is_none());
    }
}
