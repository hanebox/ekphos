//! Vim mode definitions

use super::operator::Operator;

#[derive(Debug, Clone, PartialEq)]
pub enum VimMode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
    Search { forward: bool },
    SearchLocked { forward: bool },
    OperatorPending { operator: Operator, count: Option<usize> },
}

impl Default for VimMode {
    fn default() -> Self {
        VimMode::Normal
    }
}

impl VimMode {
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
    fn test_display_name() {
        assert_eq!(VimMode::Normal.display_name(), "NORMAL");
        assert_eq!(VimMode::Insert.display_name(), "INSERT");
        assert_eq!(VimMode::Visual.display_name(), "VISUAL");
        assert_eq!(VimMode::VisualLine.display_name(), "V-LINE");
        assert_eq!(VimMode::Command.display_name(), "COMMAND");
    }

    #[test]
    fn test_operator_pending_display() {
        let mode = VimMode::OperatorPending {
            operator: Operator::Delete,
            count: Some(2),
        };
        assert_eq!(mode.display_name(), "NORMAL");
    }
}
