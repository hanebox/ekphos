//! Vim mode definitions

use super::operator::Operator;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    #[default]
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
    Search {
        forward: bool,
    },
    SearchLocked {
        forward: bool,
    },
    OperatorPending {
        operator: Operator,
        count: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimInputMode {
    Normal,
    Insert,
    Replace,
    Visual,
    Command,
    Search,
    SearchLocked,
}

impl VimMode {
    pub fn input_mode(self) -> VimInputMode {
        match self {
            VimMode::Normal | VimMode::OperatorPending { .. } => VimInputMode::Normal,
            VimMode::Insert => VimInputMode::Insert,
            VimMode::Replace => VimInputMode::Replace,
            VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock => VimInputMode::Visual,
            VimMode::Command => VimInputMode::Command,
            VimMode::Search { .. } => VimInputMode::Search,
            VimMode::SearchLocked { .. } => VimInputMode::SearchLocked,
        }
    }

    pub fn is_visual(&self) -> bool {
        matches!(self, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock)
    }

    pub fn is_insert(&self) -> bool {
        matches!(self, VimMode::Insert)
    }

    pub fn is_replace(&self) -> bool {
        matches!(self, VimMode::Replace)
    }

    pub fn is_normal(&self) -> bool {
        matches!(self, VimMode::Normal)
    }

    pub fn is_command(&self) -> bool {
        matches!(self, VimMode::Command)
    }

    pub fn is_search(&self) -> bool {
        matches!(self, VimMode::Search { .. })
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Replace => "REPLACE",
            VimMode::Visual => "VISUAL",
            VimMode::VisualLine => "V-LINE",
            VimMode::VisualBlock => "V-BLOCK",
            VimMode::Command => "COMMAND",
            VimMode::Search { .. } => "SEARCH",
            VimMode::SearchLocked { .. } => "SEARCH LOCKED",
            VimMode::OperatorPending { .. } => "NORMAL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode() {
        assert_eq!(VimMode::default(), VimMode::Normal);
    }

    #[test]
    fn test_is_visual() {
        assert!(VimMode::Visual.is_visual());
        assert!(VimMode::VisualLine.is_visual());
        assert!(!VimMode::Normal.is_visual());
        assert!(!VimMode::Insert.is_visual());
    }

    #[test]
    fn test_is_insert() {
        assert!(VimMode::Insert.is_insert());
        assert!(!VimMode::Normal.is_insert());
    }

    #[test]
    fn test_is_normal() {
        assert!(VimMode::Normal.is_normal());
        assert!(!VimMode::Insert.is_normal());
    }

    #[test]
    fn test_is_command() {
        assert!(VimMode::Command.is_command());
        assert!(!VimMode::Normal.is_command());
    }

    #[test]
    fn input_modes_define_the_application_boundary() {
        assert_eq!(VimMode::Normal.input_mode(), VimInputMode::Normal);
        assert_eq!(VimMode::OperatorPending { operator: Operator::Delete, count: Some(2) }.input_mode(), VimInputMode::Normal);
        assert_eq!(VimMode::Insert.input_mode(), VimInputMode::Insert);
        assert_eq!(VimMode::Replace.input_mode(), VimInputMode::Replace);
        assert_eq!(VimMode::VisualBlock.input_mode(), VimInputMode::Visual);
        assert_eq!(VimMode::Command.input_mode(), VimInputMode::Command);
        assert_eq!(VimMode::Search { forward: true }.input_mode(), VimInputMode::Search);
        assert_eq!(VimMode::SearchLocked { forward: false }.input_mode(), VimInputMode::SearchLocked);
    }

    #[test]
    fn test_display_name() {
        assert_eq!(VimMode::Normal.display_name(), "NORMAL");
        assert_eq!(VimMode::Insert.display_name(), "INSERT");
        assert_eq!(VimMode::Visual.display_name(), "VISUAL");
        assert_eq!(VimMode::VisualLine.display_name(), "V-LINE");
        assert_eq!(VimMode::Command.display_name(), "COMMAND");
    }

    #[test]
    fn test_operator_pending_display() {
        let mode = VimMode::OperatorPending { operator: Operator::Delete, count: Some(2) };
        assert_eq!(mode.display_name(), "NORMAL");
    }
}
