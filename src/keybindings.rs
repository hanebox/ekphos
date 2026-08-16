use std::collections::BTreeMap;
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppCommand {
    Quit,
    FocusNext,
    FocusPrevious,
    MoveDown,
    MoveUp,
    GoFirst,
    GoLast,
    Activate,
    ShowHelp,
    ReloadFiles,
    ReloadConfig,
    OpenQuickSearch,
    FindInBuffer,
    OpenGraph,
    OpenThemeSelector,
    OpenJournal,
    HistoryBack,
    HistoryForward,
    ToggleSidebar,
    ToggleOutline,
    ShrinkPanel,
    GrowPanel,
    ToggleZen,
    OpenSelected,
    ContentAction,
    NextTarget,
    PreviousTarget,
    ToggleFloatingCursor,
    HalfPageDown,
    HalfPageUp,
    ToggleFrontmatter,
    ToggleFold,
    FoldAll,
    UnfoldAll,
    EditNote,
    CreateNote,
    CreateFolder,
    DeleteItem,
    RenameItem,
    CutItem,
    PasteItem,
    CancelCut,
    SidebarSearch,
    CycleSort,
}

impl AppCommand {
    pub const ALL: [Self; 44] = [
        Self::Quit,
        Self::FocusNext,
        Self::FocusPrevious,
        Self::MoveDown,
        Self::MoveUp,
        Self::GoFirst,
        Self::GoLast,
        Self::Activate,
        Self::ShowHelp,
        Self::ReloadFiles,
        Self::ReloadConfig,
        Self::OpenQuickSearch,
        Self::FindInBuffer,
        Self::OpenGraph,
        Self::OpenThemeSelector,
        Self::OpenJournal,
        Self::HistoryBack,
        Self::HistoryForward,
        Self::ToggleSidebar,
        Self::ToggleOutline,
        Self::ShrinkPanel,
        Self::GrowPanel,
        Self::ToggleZen,
        Self::OpenSelected,
        Self::ContentAction,
        Self::NextTarget,
        Self::PreviousTarget,
        Self::ToggleFloatingCursor,
        Self::HalfPageDown,
        Self::HalfPageUp,
        Self::ToggleFrontmatter,
        Self::ToggleFold,
        Self::FoldAll,
        Self::UnfoldAll,
        Self::EditNote,
        Self::CreateNote,
        Self::CreateFolder,
        Self::DeleteItem,
        Self::RenameItem,
        Self::CutItem,
        Self::PasteItem,
        Self::CancelCut,
        Self::SidebarSearch,
        Self::CycleSort,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::FocusNext => "focus_next",
            Self::FocusPrevious => "focus_previous",
            Self::MoveDown => "move_down",
            Self::MoveUp => "move_up",
            Self::GoFirst => "go_first",
            Self::GoLast => "go_last",
            Self::Activate => "activate",
            Self::ShowHelp => "show_help",
            Self::ReloadFiles => "reload_files",
            Self::ReloadConfig => "reload_config",
            Self::OpenQuickSearch => "open_quick_search",
            Self::FindInBuffer => "find_in_buffer",
            Self::OpenGraph => "open_graph",
            Self::OpenThemeSelector => "open_theme_selector",
            Self::OpenJournal => "open_journal",
            Self::HistoryBack => "history_back",
            Self::HistoryForward => "history_forward",
            Self::ToggleSidebar => "toggle_sidebar",
            Self::ToggleOutline => "toggle_outline",
            Self::ShrinkPanel => "shrink_panel",
            Self::GrowPanel => "grow_panel",
            Self::ToggleZen => "toggle_zen",
            Self::OpenSelected => "open_selected",
            Self::ContentAction => "content_action",
            Self::NextTarget => "next_target",
            Self::PreviousTarget => "previous_target",
            Self::ToggleFloatingCursor => "toggle_floating_cursor",
            Self::HalfPageDown => "half_page_down",
            Self::HalfPageUp => "half_page_up",
            Self::ToggleFrontmatter => "toggle_frontmatter",
            Self::ToggleFold => "toggle_fold",
            Self::FoldAll => "fold_all",
            Self::UnfoldAll => "unfold_all",
            Self::EditNote => "edit_note",
            Self::CreateNote => "create_note",
            Self::CreateFolder => "create_folder",
            Self::DeleteItem => "delete_item",
            Self::RenameItem => "rename_item",
            Self::CutItem => "cut_item",
            Self::PasteItem => "paste_item",
            Self::CancelCut => "cancel_cut",
            Self::SidebarSearch => "sidebar_search",
            Self::CycleSort => "cycle_sort",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }

    pub const fn default_bindings(self) -> &'static [&'static str] {
        match self {
            Self::Quit => &["q"],
            Self::FocusNext => &["tab", "l", "right"],
            Self::FocusPrevious => &["shift+tab", "h", "left"],
            Self::MoveDown => &["down", "j"],
            Self::MoveUp => &["up", "k"],
            Self::GoFirst => &["g g"],
            Self::GoLast => &["shift+g"],
            Self::Activate => &["enter"],
            Self::ShowHelp => &["?"],
            Self::ReloadFiles => &["shift+r"],
            Self::ReloadConfig => &["ctrl+shift+r"],
            Self::OpenQuickSearch => &["ctrl+k"],
            Self::FindInBuffer => &["ctrl+f"],
            Self::OpenGraph => &["ctrl+g"],
            Self::OpenThemeSelector => &["ctrl+t"],
            Self::OpenJournal => &["t"],
            Self::HistoryBack => &["-"],
            Self::HistoryForward => &["="],
            Self::ToggleSidebar => &["ctrl+b"],
            Self::ToggleOutline => &["ctrl+o"],
            Self::ShrinkPanel => &["<"],
            Self::GrowPanel => &[">"],
            Self::ToggleZen => &["ctrl+z"],
            Self::OpenSelected => &["o"],
            Self::ContentAction => &["space"],
            Self::NextTarget => &["]"],
            Self::PreviousTarget => &["["],
            Self::ToggleFloatingCursor => &["shift+j", "shift+k"],
            Self::HalfPageDown => &["ctrl+d"],
            Self::HalfPageUp => &["ctrl+u"],
            Self::ToggleFrontmatter => &["ctrl+m"],
            Self::ToggleFold => &["z a"],
            Self::FoldAll => &["z shift+m"],
            Self::UnfoldAll => &["z shift+r"],
            Self::EditNote => &["e"],
            Self::CreateNote => &["n"],
            Self::CreateFolder => &["shift+n"],
            Self::DeleteItem => &["d"],
            Self::RenameItem => &["r"],
            Self::CutItem => &["x"],
            Self::PasteItem => &["p"],
            Self::CancelCut => &["esc"],
            Self::SidebarSearch => &["/"],
            Self::CycleSort => &["s"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct KeybindingsConfig(pub BTreeMap<String, Vec<String>>);

impl Default for KeybindingsConfig {
    fn default() -> Self {
        let bindings = AppCommand::ALL
            .into_iter()
            .map(|command| {
                (
                    command.id().to_string(),
                    command.default_bindings().iter().map(|binding| (*binding).to_string()).collect(),
                )
            })
            .collect();
        Self(bindings)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyChord {
    fn from_event(event: KeyEvent) -> Self {
        Self::normalized(event.code, event.modifiers)
    }

    fn normalized(mut code: KeyCode, mut modifiers: KeyModifiers) -> Self {
        modifiers &= KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT | KeyModifiers::SUPER;

        if code == KeyCode::BackTab {
            code = KeyCode::Tab;
            modifiers.insert(KeyModifiers::SHIFT);
        }

        if let KeyCode::Char(ch) = code {
            if ch.is_ascii_uppercase() {
                code = KeyCode::Char(ch.to_ascii_lowercase());
                modifiers.insert(KeyModifiers::SHIFT);
            } else if !ch.is_alphabetic() {
                // Terminals disagree on whether shifted punctuation carries the
                // SHIFT flag. The character itself is unambiguous.
                modifiers.remove(KeyModifiers::SHIFT);
            }
        }

        Self { code, modifiers }
    }

    fn parse(spec: &str) -> Result<Self, String> {
        let mut modifiers = KeyModifiers::NONE;
        let mut key = None;

        for token in spec.split('+').map(str::trim) {
            if token.is_empty() {
                return Err("use the name 'plus' for the + key".to_string());
            }
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
                "alt" | "option" => modifiers.insert(KeyModifiers::ALT),
                "shift" => modifiers.insert(KeyModifiers::SHIFT),
                "super" | "cmd" | "command" => modifiers.insert(KeyModifiers::SUPER),
                _ if key.is_none() => key = Some(parse_key_code(token)?),
                _ => return Err(format!("more than one key in chord '{spec}'")),
            }
        }

        let code = key.ok_or_else(|| format!("missing key in chord '{spec}'"))?;
        Ok(Self::normalized(code, modifiers))
    }
}

fn parse_key_code(token: &str) -> Result<KeyCode, String> {
    let lower = token.to_ascii_lowercase();
    let named = match lower.as_str() {
        "backspace" => Some(KeyCode::Backspace),
        "enter" | "return" => Some(KeyCode::Enter),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" | "page_up" => Some(KeyCode::PageUp),
        "pagedown" | "page_down" => Some(KeyCode::PageDown),
        "tab" => Some(KeyCode::Tab),
        "backtab" => Some(KeyCode::BackTab),
        "delete" | "del" => Some(KeyCode::Delete),
        "insert" | "ins" => Some(KeyCode::Insert),
        "esc" | "escape" => Some(KeyCode::Esc),
        "space" => Some(KeyCode::Char(' ')),
        "plus" => Some(KeyCode::Char('+')),
        "minus" => Some(KeyCode::Char('-')),
        _ => None,
    };
    if let Some(code) = named {
        return Ok(code);
    }

    if let Some(number) = lower.strip_prefix('f') {
        if let Ok(number) = number.parse::<u8>() {
            if (1..=24).contains(&number) {
                return Ok(KeyCode::F(number));
            }
        }
    }

    let mut chars = token.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Ok(KeyCode::Char(ch)),
        _ => Err(format!("unknown key '{token}'")),
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            write!(f, "Ctrl+")?;
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            write!(f, "Alt+")?;
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            write!(f, "Shift+")?;
        }
        if self.modifiers.contains(KeyModifiers::SUPER) {
            write!(f, "Super+")?;
        }

        let key = match self.code {
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "Shift+Tab".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::F(number) => format!("F{number}"),
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char('+') => "Plus".to_string(),
            KeyCode::Char(ch) => ch.to_string(),
            KeyCode::Esc => "Esc".to_string(),
            _ => format!("{:?}", self.code),
        };
        write!(f, "{key}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct KeySequence(Vec<KeyChord>);

impl KeySequence {
    fn parse(spec: &str) -> Result<Self, String> {
        let chords: Result<Vec<_>, _> = spec.split_whitespace().map(KeyChord::parse).collect();
        let chords = chords?;
        if chords.is_empty() {
            return Err("binding cannot be empty".to_string());
        }
        Ok(Self(chords))
    }

    fn starts_with(&self, prefix: &[KeyChord]) -> bool {
        self.0.starts_with(prefix)
    }
}

impl fmt::Display for KeySequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, chord) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, " ")?;
            }
            write!(f, "{chord}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingValidationError {
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindingFallback {
    Defaults,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingWarning {
    pub issues: Vec<String>,
    pub fallback: KeybindingFallback,
    pub scroll: usize,
}

impl KeybindingWarning {
    pub fn new(error: KeybindingValidationError, fallback: KeybindingFallback) -> Self {
        Self {
            issues: error.issues,
            fallback,
            scroll: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyResolution {
    NoMatch,
    Pending,
    Command(AppCommand),
}

#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: BTreeMap<AppCommand, Vec<KeySequence>>,
    pending: Vec<KeyChord>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::from_config(&KeybindingsConfig::default()).expect("built-in keybindings must be valid")
    }
}

impl Keymap {
    pub fn from_config(config: &KeybindingsConfig) -> Result<Self, KeybindingValidationError> {
        let mut raw: BTreeMap<AppCommand, Vec<String>> = AppCommand::ALL
            .into_iter()
            .map(|command| (command, command.default_bindings().iter().map(|binding| (*binding).to_string()).collect()))
            .collect();
        let mut issues = Vec::new();

        for (id, specs) in &config.0 {
            if let Some(command) = AppCommand::from_id(id) {
                raw.insert(command, specs.clone());
            } else {
                issues.push(format!("Unknown keybinding command '{id}'."));
            }
        }

        let mut bindings = BTreeMap::new();
        for command in AppCommand::ALL {
            let mut parsed = Vec::new();
            for spec in raw.remove(&command).unwrap_or_default() {
                match KeySequence::parse(&spec) {
                    Ok(sequence) if !parsed.contains(&sequence) => parsed.push(sequence),
                    Ok(_) => {}
                    Err(error) => issues.push(format!("Invalid binding '{spec}' for '{}': {error}.", command.id())),
                }
            }
            bindings.insert(command, parsed);
        }

        let entries: Vec<_> = AppCommand::ALL
            .into_iter()
            .flat_map(|command| bindings.get(&command).into_iter().flatten().map(move |sequence| (command, sequence)))
            .collect();
        for left_index in 0..entries.len() {
            let (left_command, left) = entries[left_index];
            for &(right_command, right) in &entries[left_index + 1..] {
                if left_command == right_command {
                    continue;
                }
                if left == right {
                    issues.push(format!(
                        "Binding '{left}' is assigned to both '{}' and '{}'.",
                        left_command.id(),
                        right_command.id()
                    ));
                } else if left.0.len() < right.0.len() && right.starts_with(&left.0) {
                    issues.push(format!(
                        "Binding '{left}' for '{}' conflicts with the prefix of '{right}' for '{}'.",
                        left_command.id(),
                        right_command.id()
                    ));
                } else if right.0.len() < left.0.len() && left.starts_with(&right.0) {
                    issues.push(format!(
                        "Binding '{right}' for '{}' conflicts with the prefix of '{left}' for '{}'.",
                        right_command.id(),
                        left_command.id()
                    ));
                }
            }
        }

        if issues.is_empty() {
            Ok(Self { bindings, pending: Vec::new() })
        } else {
            issues.sort();
            issues.dedup();
            Err(KeybindingValidationError { issues })
        }
    }

    pub fn resolve(&mut self, event: KeyEvent, mut available: impl FnMut(AppCommand) -> bool) -> KeyResolution {
        let chord = KeyChord::from_event(event);
        self.pending.push(chord);

        let resolution = self.resolve_pending(&mut available);
        if resolution != KeyResolution::NoMatch || self.pending.len() == 1 {
            if resolution == KeyResolution::NoMatch {
                self.pending.clear();
            }
            return resolution;
        }

        // A failed continuation may still be a valid first chord by itself.
        self.pending.clear();
        self.pending.push(chord);
        let retry = self.resolve_pending(&mut available);
        if retry == KeyResolution::NoMatch {
            self.pending.clear();
        }
        retry
    }

    fn resolve_pending(&mut self, available: &mut impl FnMut(AppCommand) -> bool) -> KeyResolution {
        let mut has_prefix = false;
        for command in AppCommand::ALL {
            if !available(command) {
                continue;
            }
            for sequence in self.bindings.get(&command).into_iter().flatten() {
                if sequence.0 == self.pending {
                    self.pending.clear();
                    return KeyResolution::Command(command);
                }
                if sequence.starts_with(&self.pending) {
                    has_prefix = true;
                }
            }
        }
        if has_prefix {
            KeyResolution::Pending
        } else {
            KeyResolution::NoMatch
        }
    }

    pub fn reset_pending(&mut self) {
        self.pending.clear();
    }

    pub fn binding_label(&self, command: AppCommand) -> String {
        let bindings = self.bindings.get(&command).map(Vec::as_slice).unwrap_or(&[]);
        if bindings.is_empty() {
            "Unbound".to_string()
        } else {
            bindings.iter().map(ToString::to_string).collect::<Vec<_>>().join(" / ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(entries: &[(&str, &[&str])]) -> KeybindingsConfig {
        KeybindingsConfig(
            entries
                .iter()
                .map(|(command, bindings)| ((*command).to_string(), bindings.iter().map(|binding| (*binding).to_string()).collect()))
                .collect(),
        )
    }

    #[test]
    fn partial_config_replaces_one_command_and_keeps_defaults() {
        let keymap = Keymap::from_config(&config(&[("open_graph", &["alt+g"])])).expect("valid keymap");

        assert_eq!(keymap.binding_label(AppCommand::OpenGraph), "Alt+g");
        assert_eq!(keymap.binding_label(AppCommand::Quit), "q");
    }

    #[test]
    fn secondary_bindings_and_unbinding_are_supported() {
        let keymap = Keymap::from_config(&config(&[("open_graph", &["alt+g", "ctrl+g"]), ("show_help", &[])])).expect("valid keymap");

        assert_eq!(keymap.binding_label(AppCommand::OpenGraph), "Alt+g / Ctrl+g");
        assert_eq!(keymap.binding_label(AppCommand::ShowHelp), "Unbound");
    }

    #[test]
    fn equivalent_case_and_modifier_spellings_clash() {
        let error = Keymap::from_config(&config(&[("open_graph", &["Ctrl+G"]), ("show_help", &["control+shift+g"])])).expect_err("bindings should clash");

        assert!(error
            .issues
            .iter()
            .any(|issue| { issue.contains("Ctrl+Shift+g") && issue.contains("open_graph") && issue.contains("show_help") }));
    }

    #[test]
    fn prefix_clashes_name_both_commands() {
        let error = Keymap::from_config(&config(&[("open_graph", &["g"])])).expect_err("g should clash with default g g");

        assert!(error
            .issues
            .iter()
            .any(|issue| { issue.contains("prefix") && issue.contains("open_graph") && issue.contains("go_first") }));
    }

    #[test]
    fn resolves_sequences_and_retries_failed_continuations() {
        let mut keymap = Keymap::default();
        let available = |_| true;

        assert_eq!(
            keymap.resolve(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), available),
            KeyResolution::Pending
        );
        assert_eq!(
            keymap.resolve(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE), available),
            KeyResolution::Command(AppCommand::GoFirst)
        );
        assert_eq!(
            keymap.resolve(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE), available),
            KeyResolution::Pending
        );
        assert_eq!(
            keymap.resolve(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL), available,),
            KeyResolution::Command(AppCommand::OpenGraph)
        );
    }

    #[test]
    fn unavailable_sequences_do_not_capture_their_prefix() {
        let mut keymap = Keymap::default();

        assert_eq!(
            keymap.resolve(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE), |command| !matches!(
                command,
                AppCommand::ToggleFold | AppCommand::FoldAll | AppCommand::UnfoldAll
            ),),
            KeyResolution::NoMatch
        );
    }

    #[test]
    fn config_round_trips_as_a_toml_table() {
        #[derive(Debug, Serialize, Deserialize)]
        struct Wrapper {
            keybindings: KeybindingsConfig,
        }

        let wrapper = Wrapper {
            keybindings: config(&[("open_graph", &["alt+g"])]),
        };
        let serialized = toml::to_string_pretty(&wrapper).unwrap();
        assert!(serialized.contains("[keybindings]"));
        assert!(serialized.contains("open_graph = [\"alt+g\"]"));
        let parsed: Wrapper = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.keybindings, wrapper.keybindings);
    }
}
