use crate::keybindings::KeybindingsConfig;
pub use ekphos_editor::LineNumberMode;
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
pub fn expand_home(path: &str) -> PathBuf {
    expand_home_with(path, dirs::home_dir().as_deref())
}
fn expand_home_with(path: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else { return PathBuf::from(path) };
    if path == "~" {
        return home.to_path_buf();
    }
    if let Some(relative) = path.strip_prefix("~/") {
        return home.join(relative);
    }
    #[cfg(windows)]
    if let Some(relative) = path.strip_prefix("~\\") {
        return home.join(relative);
    }
    PathBuf::from(path)
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub editor: EditorConfig,
    pub keybindings: KeybindingsConfig,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_notes_dir")]
    pub notes_dir: String,
    #[serde(default = "default_journal_dir")]
    pub journal_dir: String,
    #[serde(default = "default_welcome_shown")]
    pub welcome_shown: bool,
    #[serde(default = "default_theme_name")]
    pub theme: String,
    #[serde(default = "default_show_empty_dir")]
    pub show_empty_dir: bool,
    #[serde(default = "default_syntax_theme")]
    pub syntax_theme: String,
    #[serde(default = "default_image_height")]
    pub image_height: u16,
    #[serde(default = "default_inline_image_height")]
    pub inline_image_height: u16,
    #[serde(default = "default_sidebar_width_percent")]
    pub sidebar_width_percent: i64,
    #[serde(default = "default_outline_width_percent")]
    pub outline_width_percent: i64,
    #[serde(default = "default_sidebar_collapsed")]
    pub sidebar_collapsed: bool,
    #[serde(default = "default_outline_collapsed")]
    pub outline_collapsed: bool,
    #[serde(default = "default_folders_first")]
    pub folders_first: bool,
    #[serde(default = "default_frontmatter_hidden")]
    pub frontmatter_hidden: bool,
    #[serde(default = "default_show_tags")]
    pub show_tags: bool,
    #[serde(default = "default_check_updates")]
    pub check_updates: bool,
    #[serde(default = "default_transparent_bg")]
    pub transparent_bg: bool,
    #[serde(default = "default_floating_cursor")]
    pub floating_cursor: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default = "legacy_editing_mode")]
    pub mode: EditingMode,
    #[serde(default = "default_line_wrap")]
    pub line_wrap: bool,
    #[serde(default = "default_tab_width")]
    pub tab_width: u16,
    #[serde(default = "default_left_padding")]
    pub left_padding: u16,
    #[serde(default = "default_right_padding")]
    pub right_padding: u16,
    #[serde(default)]
    pub line_numbers: LineNumberMode,
    #[serde(default = "default_scrolloff")]
    pub scrolloff: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EditingMode {
    #[default]
    Standard,
    Vim,
}

impl EditingMode {
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Vim => "Vim",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Standard => Self::Vim,
            Self::Vim => Self::Standard,
        }
    }
}

fn legacy_editing_mode() -> EditingMode {
    EditingMode::Vim
}
fn default_line_wrap() -> bool {
    true
}
fn default_tab_width() -> u16 {
    4
}
fn default_left_padding() -> u16 {
    0
}
fn default_right_padding() -> u16 {
    1
}
fn default_scrolloff() -> u8 {
    0
}
impl Default for EditorConfig {
    fn default() -> Self {
        Self { mode: EditingMode::Standard, line_wrap: default_line_wrap(), tab_width: default_tab_width(), left_padding: default_left_padding(), right_padding: default_right_padding(), line_numbers: LineNumberMode::default(), scrolloff: default_scrolloff() }
    }
}

fn legacy_editor_config() -> EditorConfig {
    EditorConfig { mode: EditingMode::Vim, ..EditorConfig::default() }
}
fn default_notes_dir() -> String {
    "~/Documents/ekphos".to_string()
}
fn default_journal_dir() -> String {
    "Journal".to_string()
}
fn default_welcome_shown() -> bool {
    true
}
fn default_show_empty_dir() -> bool {
    true
}
fn default_theme_name() -> String {
    "ekphos-dawn".to_string()
}
fn default_syntax_theme() -> String {
    "base16-ocean.dark".to_string()
}
fn default_image_height() -> u16 {
    8
}
fn default_inline_image_height() -> u16 {
    4
}
fn default_sidebar_width_percent() -> i64 {
    20
}
fn default_outline_width_percent() -> i64 {
    20
}
fn default_sidebar_collapsed() -> bool {
    false
}
fn default_outline_collapsed() -> bool {
    false
}
fn default_folders_first() -> bool {
    true
}
fn default_frontmatter_hidden() -> bool {
    true
}
fn default_show_tags() -> bool {
    true
}
fn default_check_updates() -> bool {
    true
}
fn default_transparent_bg() -> bool {
    false
}
fn default_floating_cursor() -> bool {
    false
}
impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            notes_dir: default_notes_dir(),
            journal_dir: default_journal_dir(),
            welcome_shown: default_welcome_shown(),
            theme: default_theme_name(),
            show_empty_dir: default_show_empty_dir(),
            syntax_theme: default_syntax_theme(),
            image_height: default_image_height(),
            inline_image_height: default_inline_image_height(),
            sidebar_width_percent: default_sidebar_width_percent(),
            outline_width_percent: default_outline_width_percent(),
            sidebar_collapsed: default_sidebar_collapsed(),
            outline_collapsed: default_outline_collapsed(),
            folders_first: default_folders_first(),
            frontmatter_hidden: default_frontmatter_hidden(),
            show_tags: default_show_tags(),
            check_updates: default_check_updates(),
            transparent_bg: default_transparent_bg(),
            floating_cursor: default_floating_cursor(),
        }
    }
}
impl Deref for Config {
    type Target = GeneralConfig;
    fn deref(&self) -> &Self::Target {
        &self.general
    }
}
impl DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.general
    }
}
#[derive(Deserialize)]
struct ConfigFile {
    #[serde(default)]
    general: Option<GeneralConfig>,
    #[serde(default = "legacy_editor_config")]
    editor: EditorConfig,
    #[serde(default)]
    keybindings: KeybindingsConfig,
    #[serde(flatten)]
    legacy_general: GeneralConfig,
}
impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let file = ConfigFile::deserialize(deserializer)?;
        Ok(Self { general: file.general.unwrap_or(file.legacy_general), editor: file.editor, keybindings: file.keybindings })
    }
}
impl Config {
    pub const MIN_PANEL_WIDTH_PERCENT: i64 = 5;
    pub const MAX_PANEL_WIDTH_PERCENT: i64 = 95;
    pub const MINIMIZED_PANEL_WIDTH_PERCENT: u16 = 10;
    pub const PANEL_RESIZE_STEP_PERCENT: i64 = 5;
    pub fn effective_panel_width_percent(width: i64) -> u16 {
        width.clamp(Self::MIN_PANEL_WIDTH_PERCENT, Self::MAX_PANEL_WIDTH_PERCENT) as u16
    }
    pub fn effective_sidebar_width_percent(&self) -> u16 {
        Self::effective_panel_width_percent(self.sidebar_width_percent)
    }
    pub fn effective_outline_width_percent(&self) -> u16 {
        Self::effective_panel_width_percent(self.outline_width_percent)
    }
    pub fn effective_image_height(&self) -> u16 {
        self.image_height.max(3)
    }
    pub fn effective_inline_image_height(&self) -> u16 {
        self.inline_image_height.max(3)
    }
    pub fn panel_width_is_minimized(width: i64) -> bool {
        Self::effective_panel_width_percent(width) < Self::MINIMIZED_PANEL_WIDTH_PERCENT
    }
    pub fn resized_panel_width_percent(width: i64, delta: i64) -> i64 {
        let effective = i64::from(Self::effective_panel_width_percent(width));
        effective.saturating_add(delta).clamp(Self::MIN_PANEL_WIDTH_PERCENT, Self::MAX_PANEL_WIDTH_PERCENT)
    }
    pub fn exists() -> bool {
        Self::config_path().exists()
    }
    pub fn load() -> Self {
        Self::load_from_dir(&Self::config_dir())
    }
    pub fn load_from_dir(config_dir: &std::path::Path) -> Self {
        let config_path = Self::config_path_in(config_dir);
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("Failed to parse config: {}", e),
                },
                Err(e) => eprintln!("Failed to read config: {}", e),
            }
        }
        Self::default()
    }
    pub fn load_or_create() -> Self {
        let config_dir = Self::config_dir();
        let config_path = Self::config_path();
        let themes_dir = Self::themes_dir();
        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }
        if !themes_dir.exists() {
            let _ = fs::create_dir_all(&themes_dir);
        }
        let default_theme_path = themes_dir.join("ekphos-dawn.toml");
        if !default_theme_path.exists() {
            let default_theme_content = include_str!("../themes/ekphos-dawn.toml");
            let _ = fs::write(&default_theme_path, default_theme_content);
        }
        if !config_path.exists() {
            let default_config = Self::default();
            if let Ok(toml_string) = toml::to_string_pretty(&default_config) {
                let _ = fs::write(&config_path, toml_string);
            }
        }
        Self::load()
    }
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }
    pub fn config_path_in(config_dir: &std::path::Path) -> PathBuf {
        config_dir.join("config.toml")
    }
    pub fn config_dir() -> PathBuf {
        if let Some(path) = std::env::var_os("EKPHOS_CONFIG_DIR").filter(|path| !path.is_empty()) {
            return PathBuf::from(path);
        }
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config").join("ekphos")
    }
    pub fn themes_dir() -> PathBuf {
        Self::config_dir().join("themes")
    }
    pub fn themes_dir_in(config_dir: &std::path::Path) -> PathBuf {
        config_dir.join("themes")
    }
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to_dir(&Self::config_dir())
    }
    pub fn save_to_dir(&self, config_dir: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir_all(config_dir)?;
        let config_path = Self::config_path_in(config_dir);
        let toml_string = toml::to_string_pretty(self).unwrap_or_else(|_| String::new());
        fs::write(&config_path, toml_string)?;
        Ok(())
    }
    pub fn notes_path(&self) -> PathBuf {
        expand_home(&self.notes_dir)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeFile {
    #[serde(default)]
    pub base: BaseColors,
    #[serde(default)]
    pub accent: AccentColors,
    #[serde(default)]
    pub semantic: SemanticColors,
    #[serde(default)]
    pub ui: UiColorsFile,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseColors {
    #[serde(default = "defaults::background")]
    pub background: String,
    #[serde(default = "defaults::background_secondary")]
    pub background_secondary: String,
    #[serde(default = "defaults::foreground")]
    pub foreground: String,
    #[serde(default = "defaults::muted")]
    pub muted: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccentColors {
    #[serde(default = "defaults::primary")]
    pub primary: String,
    #[serde(default = "defaults::secondary")]
    pub secondary: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticColors {
    #[serde(default = "defaults::error")]
    pub error: String,
    #[serde(default = "defaults::warning")]
    pub warning: String,
    #[serde(default = "defaults::success")]
    pub success: String,
    #[serde(default = "defaults::info")]
    pub info: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiColorsFile {
    #[serde(default = "defaults::border")]
    pub border: String,
    #[serde(default = "defaults::border_focused")]
    pub border_focused: String,
    #[serde(default = "defaults::selection")]
    pub selection: String,
    #[serde(default = "defaults::cursor")]
    pub cursor: String,
    #[serde(default)]
    pub statusbar: StatusbarColors,
    #[serde(default)]
    pub dialog: DialogColors,
    #[serde(default)]
    pub sidebar: SidebarColors,
    #[serde(default)]
    pub content: ContentColors,
    #[serde(default)]
    pub outline: OutlineColors,
    #[serde(default)]
    pub search: SearchColors,
    #[serde(default)]
    pub editor: EditorColors,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusbarColors {
    #[serde(default = "defaults::background")]
    pub background: String,
    #[serde(default = "defaults::foreground")]
    pub foreground: String,
    #[serde(default = "defaults::primary")]
    pub brand: String,
    #[serde(default = "defaults::muted")]
    pub mode: String,
    #[serde(default = "defaults::border")]
    pub separator: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogColors {
    #[serde(default = "defaults::background")]
    pub background: String,
    #[serde(default = "defaults::primary")]
    pub border: String,
    #[serde(default = "defaults::primary")]
    pub title: String,
    #[serde(default = "defaults::foreground")]
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarColors {
    #[serde(default = "defaults::background")]
    pub background: String,
    #[serde(default = "defaults::foreground")]
    pub item: String,
    #[serde(default = "defaults::warning")]
    pub item_selected: String,
    #[serde(default = "defaults::info")]
    pub folder: String,
    #[serde(default = "defaults::info")]
    pub folder_expanded: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentColors {
    #[serde(default = "defaults::background")]
    pub background: String,
    #[serde(default = "defaults::foreground")]
    pub text: String,
    #[serde(default = "defaults::primary")]
    pub heading1: String,
    #[serde(default = "defaults::success")]
    pub heading2: String,
    #[serde(default = "defaults::warning")]
    pub heading3: String,
    #[serde(default = "defaults::secondary")]
    pub heading4: String,
    #[serde(default = "defaults::info")]
    pub link: String,
    #[serde(default = "defaults::error")]
    pub link_invalid: String,
    #[serde(default = "defaults::success")]
    pub code: String,
    #[serde(default = "defaults::background_secondary")]
    pub code_background: String,
    #[serde(default = "defaults::muted")]
    pub blockquote: String,
    #[serde(default = "defaults::secondary")]
    pub list_marker: String,
    #[serde(default = "defaults::secondary")]
    pub tag: String,
    #[serde(default = "defaults::background_secondary")]
    pub tag_background: String,
    #[serde(default = "defaults::muted")]
    pub frontmatter: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineColors {
    #[serde(default = "defaults::background")]
    pub background: String,
    #[serde(default = "defaults::primary")]
    pub heading1: String,
    #[serde(default = "defaults::success")]
    pub heading2: String,
    #[serde(default = "defaults::warning")]
    pub heading3: String,
    #[serde(default = "defaults::secondary")]
    pub heading4: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchColors {
    #[serde(default = "defaults::background_secondary")]
    pub background: String,
    #[serde(default = "defaults::primary")]
    pub border: String,
    #[serde(default = "defaults::foreground")]
    pub input: String,
    #[serde(default = "defaults::warning")]
    pub match_highlight: String,
    #[serde(default = "defaults::search_match_current")]
    pub match_current: String,
    #[serde(default = "defaults::muted")]
    pub match_count: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorColors {
    #[serde(default = "defaults::primary")]
    pub heading1: String,
    #[serde(default = "defaults::success")]
    pub heading2: String,
    #[serde(default = "defaults::warning")]
    pub heading3: String,
    #[serde(default = "defaults::secondary")]
    pub heading4: String,
    #[serde(default = "defaults::info")]
    pub heading5: String,
    #[serde(default = "defaults::muted")]
    pub heading6: String,
    #[serde(default = "defaults::success")]
    pub code: String,
    #[serde(default = "defaults::info")]
    pub link: String,
    #[serde(default = "defaults::muted")]
    pub blockquote: String,
    #[serde(default = "defaults::secondary")]
    pub list_marker: String,
    #[serde(default = "defaults::warning")]
    pub bold: String,
    #[serde(default = "defaults::info")]
    pub italic: String,
}
mod defaults {
    pub fn background() -> String {
        "#1a1a24".to_string()
    }
    pub fn background_secondary() -> String {
        "#24243a".to_string()
    }
    pub fn foreground() -> String {
        "#c0caf5".to_string()
    }
    pub fn muted() -> String {
        "#565f89".to_string()
    }
    pub fn primary() -> String {
        "#7aa2f7".to_string()
    }
    pub fn secondary() -> String {
        "#bb9af7".to_string()
    }
    pub fn error() -> String {
        "#f7768e".to_string()
    }
    pub fn warning() -> String {
        "#e0af68".to_string()
    }
    pub fn success() -> String {
        "#9ece6a".to_string()
    }
    pub fn info() -> String {
        "#7dcfff".to_string()
    }
    pub fn border() -> String {
        "#3b4261".to_string()
    }
    pub fn border_focused() -> String {
        "#7aa2f7".to_string()
    }
    pub fn selection() -> String {
        "#283457".to_string()
    }
    pub fn cursor() -> String {
        "#c0caf5".to_string()
    }
    pub fn search_match_current() -> String {
        "#ff9e64".to_string()
    }
}
impl Default for BaseColors {
    fn default() -> Self {
        Self { background: defaults::background(), background_secondary: defaults::background_secondary(), foreground: defaults::foreground(), muted: defaults::muted() }
    }
}
impl Default for AccentColors {
    fn default() -> Self {
        Self { primary: defaults::primary(), secondary: defaults::secondary() }
    }
}
impl Default for SemanticColors {
    fn default() -> Self {
        Self { error: defaults::error(), warning: defaults::warning(), success: defaults::success(), info: defaults::info() }
    }
}
impl Default for StatusbarColors {
    fn default() -> Self {
        Self { background: defaults::background(), foreground: defaults::foreground(), brand: defaults::primary(), mode: defaults::muted(), separator: defaults::border() }
    }
}
impl Default for DialogColors {
    fn default() -> Self {
        Self { background: defaults::background(), border: defaults::primary(), title: defaults::primary(), text: defaults::foreground() }
    }
}
impl Default for SidebarColors {
    fn default() -> Self {
        Self { background: defaults::background(), item: defaults::foreground(), item_selected: defaults::warning(), folder: defaults::info(), folder_expanded: defaults::info() }
    }
}
impl Default for ContentColors {
    fn default() -> Self {
        Self {
            background: defaults::background(),
            text: defaults::foreground(),
            heading1: defaults::primary(),
            heading2: defaults::success(),
            heading3: defaults::warning(),
            heading4: defaults::secondary(),
            link: defaults::info(),
            link_invalid: defaults::error(),
            code: defaults::success(),
            code_background: defaults::background_secondary(),
            blockquote: defaults::muted(),
            list_marker: defaults::secondary(),
            tag: defaults::secondary(),
            tag_background: defaults::background_secondary(),
            frontmatter: defaults::muted(),
        }
    }
}
impl Default for OutlineColors {
    fn default() -> Self {
        Self { background: defaults::background(), heading1: defaults::primary(), heading2: defaults::success(), heading3: defaults::warning(), heading4: defaults::secondary() }
    }
}
impl Default for SearchColors {
    fn default() -> Self {
        Self { background: defaults::background_secondary(), border: defaults::primary(), input: defaults::foreground(), match_highlight: defaults::warning(), match_current: defaults::search_match_current(), match_count: defaults::muted() }
    }
}
impl Default for EditorColors {
    fn default() -> Self {
        Self {
            heading1: defaults::primary(),
            heading2: defaults::success(),
            heading3: defaults::warning(),
            heading4: defaults::secondary(),
            heading5: defaults::info(),
            heading6: defaults::muted(),
            code: defaults::success(),
            link: defaults::info(),
            blockquote: defaults::muted(),
            list_marker: defaults::secondary(),
            bold: defaults::warning(),
            italic: defaults::info(),
        }
    }
}
/// Names of the official themes bundled into the binary via `include_str!`.
/// Keep this in sync with the match arms in `ThemeFile::get_bundled_theme`.
pub const BUNDLED_THEMES: &[&str] = &["ekphos-dawn", "dracula", "kanagawa"];
/// A theme available for selection, with its origin.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeEntry {
    pub name: String,
    /// True for official themes shipped in the binary, false for user themes
    /// found in `~/.config/ekphos/themes/`.
    pub bundled: bool,
}
impl ThemeFile {
    pub fn load_from_file(path: &PathBuf) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        toml::from_str(&content).ok()
    }
    pub fn load_from_str(content: &str) -> Option<Self> {
        toml::from_str(content).ok()
    }
    fn get_bundled_theme(name: &str) -> Option<Self> {
        let content = match name {
            "ekphos-dawn" => include_str!("../themes/ekphos-dawn.toml"),
            "dracula" => include_str!("../themes/dracula.toml"),
            "kanagawa" => include_str!("../themes/kanagawa.toml"),
            _ => return None,
        };
        Self::load_from_str(content)
    }
    pub fn load_by_name(name: &str) -> Option<Self> {
        Self::load_by_name_in(name, &Config::themes_dir())
    }
    pub fn load_by_name_in(name: &str, user_themes_dir: &std::path::Path) -> Option<Self> {
        if user_themes_dir.exists() {
            let theme_path = user_themes_dir.join(format!("{}.toml", name));
            if theme_path.exists() {
                if let Some(theme) = Self::load_from_file(&theme_path) {
                    return Some(theme);
                }
            }
        }
        if let Some(theme) = Self::get_bundled_theme(name) {
            return Some(theme);
        }
        let bundled_themes = PathBuf::from("themes");
        if bundled_themes.exists() {
            let theme_path = bundled_themes.join(format!("{}.toml", name));
            if theme_path.exists() {
                if let Some(theme) = Self::load_from_file(&theme_path) {
                    return Some(theme);
                }
            }
        }
        None
    }
    /// List every selectable theme: the official bundled themes first (in their
    /// canonical order), then any user themes from `~/.config/ekphos/themes/`
    /// sorted alphabetically. Names are de-duplicated so a user copy of a
    /// bundled theme (e.g. the `ekphos-dawn.toml` written on first launch) is
    /// only listed once, as an official theme.
    pub fn list_available() -> Vec<ThemeEntry> {
        Self::list_available_in(&Config::themes_dir())
    }
    pub fn list_available_in(user_themes_dir: &std::path::Path) -> Vec<ThemeEntry> {
        let mut entries: Vec<ThemeEntry> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for name in BUNDLED_THEMES {
            if seen.insert((*name).to_string()) {
                entries.push(ThemeEntry { name: (*name).to_string(), bundled: true });
            }
        }
        if let Ok(read_dir) = fs::read_dir(user_themes_dir) {
            let mut user_names: Vec<String> = Vec::new();
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if !seen.contains(stem) {
                        user_names.push(stem.to_string());
                    }
                }
            }
            user_names.sort();
            for name in user_names {
                if seen.insert(name.clone()) {
                    entries.push(ThemeEntry { name, bundled: false });
                }
            }
        }
        entries
    }
}
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub background_secondary: Color,
    pub foreground: Color,
    pub muted: Color,
    pub primary: Color,
    pub secondary: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
    pub border: Color,
    pub border_focused: Color,
    pub selection: Color,
    pub cursor: Color,
    pub statusbar: StatusbarTheme,
    pub dialog: DialogTheme,
    pub sidebar: SidebarTheme,
    pub content: ContentTheme,
    pub outline: OutlineTheme,
    pub search: SearchTheme,
    pub editor: EditorTheme,
}
#[derive(Debug, Clone)]
pub struct StatusbarTheme {
    pub background: Color,
    pub foreground: Color,
    pub brand: Color,
    pub mode: Color,
    pub separator: Color,
}
#[derive(Debug, Clone)]
pub struct DialogTheme {
    pub background: Color,
    pub border: Color,
    pub title: Color,
    pub text: Color,
}
#[derive(Debug, Clone)]
pub struct SidebarTheme {
    pub background: Color,
    pub item: Color,
    pub item_selected: Color,
    pub folder: Color,
    pub folder_expanded: Color,
}
#[derive(Debug, Clone)]
pub struct ContentTheme {
    pub background: Color,
    pub text: Color,
    pub heading1: Color,
    pub heading2: Color,
    pub heading3: Color,
    pub heading4: Color,
    pub link: Color,
    pub link_invalid: Color,
    pub code: Color,
    pub code_background: Color,
    pub blockquote: Color,
    pub list_marker: Color,
    pub tag: Color,
    pub tag_background: Color,
    pub frontmatter: Color,
}
#[derive(Debug, Clone)]
pub struct OutlineTheme {
    pub background: Color,
    pub heading1: Color,
    pub heading2: Color,
    pub heading3: Color,
    pub heading4: Color,
}
#[derive(Debug, Clone)]
pub struct SearchTheme {
    pub background: Color,
    pub border: Color,
    pub input: Color,
    pub match_highlight: Color,
    pub match_current: Color,
    pub match_count: Color,
}
#[derive(Debug, Clone)]
pub struct EditorTheme {
    pub heading1: Color,
    pub heading2: Color,
    pub heading3: Color,
    pub heading4: Color,
    pub heading5: Color,
    pub heading6: Color,
    pub code: Color,
    pub link: Color,
    pub blockquote: Color,
    pub list_marker: Color,
    pub bold: Color,
    pub italic: Color,
}
impl Theme {
    pub fn from_file(tf: &ThemeFile) -> Self {
        Self {
            background: parse_hex_color(&tf.base.background),
            background_secondary: parse_hex_color(&tf.base.background_secondary),
            foreground: parse_hex_color(&tf.base.foreground),
            muted: parse_hex_color(&tf.base.muted),
            primary: parse_hex_color(&tf.accent.primary),
            secondary: parse_hex_color(&tf.accent.secondary),
            error: parse_hex_color(&tf.semantic.error),
            warning: parse_hex_color(&tf.semantic.warning),
            success: parse_hex_color(&tf.semantic.success),
            info: parse_hex_color(&tf.semantic.info),
            border: parse_hex_color(&tf.ui.border),
            border_focused: parse_hex_color(&tf.ui.border_focused),
            selection: parse_hex_color(&tf.ui.selection),
            cursor: parse_hex_color(&tf.ui.cursor),
            statusbar: StatusbarTheme {
                background: parse_hex_color(&tf.ui.statusbar.background),
                foreground: parse_hex_color(&tf.ui.statusbar.foreground),
                brand: parse_hex_color(&tf.ui.statusbar.brand),
                mode: parse_hex_color(&tf.ui.statusbar.mode),
                separator: parse_hex_color(&tf.ui.statusbar.separator),
            },
            dialog: DialogTheme { background: parse_hex_color(&tf.ui.dialog.background), border: parse_hex_color(&tf.ui.dialog.border), title: parse_hex_color(&tf.ui.dialog.title), text: parse_hex_color(&tf.ui.dialog.text) },
            sidebar: SidebarTheme {
                background: parse_hex_color(&tf.ui.sidebar.background),
                item: parse_hex_color(&tf.ui.sidebar.item),
                item_selected: parse_hex_color(&tf.ui.sidebar.item_selected),
                folder: parse_hex_color(&tf.ui.sidebar.folder),
                folder_expanded: parse_hex_color(&tf.ui.sidebar.folder_expanded),
            },
            content: ContentTheme {
                background: parse_hex_color(&tf.ui.content.background),
                text: parse_hex_color(&tf.ui.content.text),
                heading1: parse_hex_color(&tf.ui.content.heading1),
                heading2: parse_hex_color(&tf.ui.content.heading2),
                heading3: parse_hex_color(&tf.ui.content.heading3),
                heading4: parse_hex_color(&tf.ui.content.heading4),
                link: parse_hex_color(&tf.ui.content.link),
                link_invalid: parse_hex_color(&tf.ui.content.link_invalid),
                code: parse_hex_color(&tf.ui.content.code),
                code_background: parse_hex_color(&tf.ui.content.code_background),
                blockquote: parse_hex_color(&tf.ui.content.blockquote),
                list_marker: parse_hex_color(&tf.ui.content.list_marker),
                tag: parse_hex_color(&tf.ui.content.tag),
                tag_background: parse_hex_color(&tf.ui.content.tag_background),
                frontmatter: parse_hex_color(&tf.ui.content.frontmatter),
            },
            outline: OutlineTheme {
                background: parse_hex_color(&tf.ui.outline.background),
                heading1: parse_hex_color(&tf.ui.outline.heading1),
                heading2: parse_hex_color(&tf.ui.outline.heading2),
                heading3: parse_hex_color(&tf.ui.outline.heading3),
                heading4: parse_hex_color(&tf.ui.outline.heading4),
            },
            search: SearchTheme {
                background: parse_hex_color(&tf.ui.search.background),
                border: parse_hex_color(&tf.ui.search.border),
                input: parse_hex_color(&tf.ui.search.input),
                match_highlight: parse_hex_color(&tf.ui.search.match_highlight),
                match_current: parse_hex_color(&tf.ui.search.match_current),
                match_count: parse_hex_color(&tf.ui.search.match_count),
            },
            editor: EditorTheme {
                heading1: parse_hex_color(&tf.ui.editor.heading1),
                heading2: parse_hex_color(&tf.ui.editor.heading2),
                heading3: parse_hex_color(&tf.ui.editor.heading3),
                heading4: parse_hex_color(&tf.ui.editor.heading4),
                heading5: parse_hex_color(&tf.ui.editor.heading5),
                heading6: parse_hex_color(&tf.ui.editor.heading6),
                code: parse_hex_color(&tf.ui.editor.code),
                link: parse_hex_color(&tf.ui.editor.link),
                blockquote: parse_hex_color(&tf.ui.editor.blockquote),
                list_marker: parse_hex_color(&tf.ui.editor.list_marker),
                bold: parse_hex_color(&tf.ui.editor.bold),
                italic: parse_hex_color(&tf.ui.editor.italic),
            },
        }
    }
    pub fn from_name(name: &str) -> Self {
        if let Some(theme_file) = ThemeFile::load_by_name(name) {
            return Self::from_file(&theme_file);
        }
        Self::from_file(&ThemeFile::default())
    }
    pub fn from_name_in(name: &str, themes_dir: &std::path::Path) -> Self {
        if let Some(theme_file) = ThemeFile::load_by_name_in(name, themes_dir) {
            return Self::from_file(&theme_file);
        }
        Self::from_file(&ThemeFile::default())
    }
}
impl Default for Theme {
    fn default() -> Self {
        Self::from_file(&ThemeFile::default())
    }
}
fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#').trim_start_matches('\'').trim_end_matches('\'');
    if hex.len() == 6 && hex.is_ascii() {
        if let (Ok(r), Ok(g), Ok(b)) = (u8::from_str_radix(&hex[0..2], 16), u8::from_str_radix(&hex[2..4], 16), u8::from_str_radix(&hex[4..6], 16)) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::White
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::keybindings::{AppCommand, Keymap};
    #[test]
    fn keybindings_default_when_missing_from_toml() {
        let config: Config = toml::from_str("notes_dir = '/tmp/notes'").unwrap();
        let keymap = Keymap::from_config(&config.keybindings).unwrap();
        assert_eq!(keymap.binding_label(AppCommand::OpenGraph), "Ctrl+g");
    }
    #[test]
    fn fresh_configs_default_to_standard_editing() {
        let config = Config::default();
        assert_eq!(config.editor.mode, EditingMode::Standard);
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("mode = \"standard\""));
    }
    #[test]
    fn legacy_configs_without_an_editor_mode_keep_vim() {
        let without_editor: Config = toml::from_str("notes_dir = '/tmp/notes'").unwrap();
        assert_eq!(without_editor.editor.mode, EditingMode::Vim);
        let partial_editor: Config = toml::from_str("[editor]\nline_wrap = false\n").unwrap();
        assert_eq!(partial_editor.editor.mode, EditingMode::Vim);
    }
    #[test]
    fn explicit_editing_modes_round_trip() {
        for mode in [EditingMode::Standard, EditingMode::Vim] {
            let mut config = Config::default();
            config.editor.mode = mode;
            let serialized = toml::to_string_pretty(&config).unwrap();
            let parsed: Config = toml::from_str(&serialized).unwrap();
            assert_eq!(parsed.editor.mode, mode);
        }
    }
    #[test]
    fn general_table_deserializes_config_values() {
        let config: Config = toml::from_str("[general]\nnotes_dir = '/tmp/notes'\ncheck_updates = false\n").unwrap();
        assert_eq!(config.notes_dir, "/tmp/notes");
        assert!(!config.check_updates);
        assert_eq!(config.journal_dir, "Journal");
    }
    #[test]
    fn partial_keybinding_table_keeps_other_command_defaults() {
        let config: Config = toml::from_str("notes_dir = '/tmp/notes'\n[keybindings]\nopen_graph = ['alt+g']\n").unwrap();
        let keymap = Keymap::from_config(&config.keybindings).unwrap();
        assert_eq!(keymap.binding_label(AppCommand::OpenGraph), "Alt+g");
        assert_eq!(keymap.binding_label(AppCommand::Quit), "q");
    }
    #[test]
    fn journal_directory_defaults_when_missing_from_toml() {
        let config: Config = toml::from_str("notes_dir = '/tmp/notes'").unwrap();
        assert_eq!(config.journal_dir, "Journal");
    }
    #[test]
    fn journal_directory_deserializes_and_serializes_custom_value() {
        let config: Config = toml::from_str("journal_dir = 'Personal/Daily Notes'").unwrap();
        assert_eq!(config.journal_dir, "Personal/Daily Notes");
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("journal_dir = \"Personal/Daily Notes\""));
    }
    #[test]
    fn panel_widths_default_when_missing_from_toml() {
        let config: Config = toml::from_str("notes_dir = '/tmp/notes'").unwrap();
        assert_eq!(config.sidebar_width_percent, 20);
        assert_eq!(config.outline_width_percent, 20);
    }
    #[test]
    fn panel_widths_deserialize_custom_toml_values() {
        let config: Config = toml::from_str("sidebar_width_percent = 35\noutline_width_percent = 45\n").unwrap();
        assert_eq!(config.sidebar_width_percent, 35);
        assert_eq!(config.outline_width_percent, 45);
    }
    #[test]
    fn panel_widths_are_serialized() {
        let config = Config { general: GeneralConfig { sidebar_width_percent: 30, outline_width_percent: 40, ..GeneralConfig::default() }, ..Config::default() };
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.starts_with("[general]\n"));
        let document: toml::Table = toml::from_str(&serialized).unwrap();
        let general = document.get("general").and_then(toml::Value::as_table).unwrap();
        assert_eq!(general.get("sidebar_width_percent").and_then(toml::Value::as_integer), Some(30));
        assert_eq!(general.get("outline_width_percent").and_then(toml::Value::as_integer), Some(40));
        assert!(!document.contains_key("sidebar_width_percent"));
        assert!(!document.contains_key("outline_width_percent"));
    }
    #[test]
    fn image_height_defaults_when_missing_from_toml() {
        let config: Config = toml::from_str("notes_dir = '/tmp/notes'").unwrap();
        assert_eq!(config.image_height, 8);
        assert_eq!(config.effective_image_height(), 8);
        assert_eq!(config.inline_image_height, 4);
        assert_eq!(config.effective_inline_image_height(), 4);
    }
    #[test]
    fn image_height_deserializes_and_serializes_custom_value() {
        let config: Config = toml::from_str("image_height = 12\ninline_image_height = 6").unwrap();
        assert_eq!(config.image_height, 12);
        assert_eq!(config.effective_image_height(), 12);
        assert_eq!(config.inline_image_height, 6);
        assert_eq!(config.effective_inline_image_height(), 6);
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(serialized.contains("image_height = 12"));
        assert!(serialized.contains("inline_image_height = 6"));
    }
    #[test]
    fn image_height_has_border_safe_effective_minimum() {
        for configured_height in 0..=2 {
            let config: Config = toml::from_str(&format!("image_height = {configured_height}")).unwrap();
            assert_eq!(config.image_height, configured_height);
            assert_eq!(config.effective_image_height(), 3);
        }
        let config: Config = toml::from_str("image_height = 3").unwrap();
        assert_eq!(config.effective_image_height(), 3);
        for configured_height in 0..=2 {
            let config: Config = toml::from_str(&format!("inline_image_height = {configured_height}")).unwrap();
            assert_eq!(config.inline_image_height, configured_height);
            assert_eq!(config.effective_inline_image_height(), 3);
        }
    }
    #[test]
    fn effective_panel_width_is_clamped() {
        assert_eq!(Config::effective_panel_width_percent(i64::MIN), 5);
        assert_eq!(Config::effective_panel_width_percent(-10), 5);
        assert_eq!(Config::effective_panel_width_percent(5), 5);
        assert_eq!(Config::effective_panel_width_percent(55), 55);
        assert_eq!(Config::effective_panel_width_percent(95), 95);
        assert_eq!(Config::effective_panel_width_percent(i64::MAX), 95);
    }
    #[test]
    fn panel_widths_below_ten_percent_are_minimized() {
        assert!(Config::panel_width_is_minimized(-10));
        assert!(Config::panel_width_is_minimized(5));
        assert!(Config::panel_width_is_minimized(9));
        assert!(!Config::panel_width_is_minimized(10));
        assert!(!Config::panel_width_is_minimized(20));
    }
    #[test]
    fn panel_width_resize_uses_five_point_steps() {
        assert_eq!(Config::resized_panel_width_percent(20, -Config::PANEL_RESIZE_STEP_PERCENT), 15);
        assert_eq!(Config::resized_panel_width_percent(20, Config::PANEL_RESIZE_STEP_PERCENT), 25);
    }
    #[test]
    fn panel_width_resize_stops_at_bounds() {
        assert_eq!(Config::resized_panel_width_percent(5, -Config::PANEL_RESIZE_STEP_PERCENT), 5);
        assert_eq!(Config::resized_panel_width_percent(5, Config::PANEL_RESIZE_STEP_PERCENT), 10);
        assert_eq!(Config::resized_panel_width_percent(95, Config::PANEL_RESIZE_STEP_PERCENT), 95);
        assert_eq!(Config::resized_panel_width_percent(-100, Config::PANEL_RESIZE_STEP_PERCENT), 10);
        assert_eq!(Config::resized_panel_width_percent(100, -Config::PANEL_RESIZE_STEP_PERCENT), 90);
    }
    #[test]
    fn parse_hex_color_valid() {
        assert_eq!(parse_hex_color("#ff8800"), Color::Rgb(255, 136, 0));
        assert_eq!(parse_hex_color("'00ff00'"), Color::Rgb(0, 255, 0));
    }
    #[test]
    fn parse_hex_color_multibyte_no_panic() {
        assert_eq!(parse_hex_color("aé234"), Color::White); // 6 bytes, 5 chars
        assert_eq!(parse_hex_color("世界AB"), Color::White);
    }
    #[test]
    fn home_expansion_matches_the_previous_tilde_contract() {
        let home = Path::new("/fixture/home");
        assert_eq!(expand_home_with("~", Some(home)), home);
        assert_eq!(expand_home_with("~/notes", Some(home)), home.join("notes"));
        assert_eq!(expand_home_with("~other/notes", Some(home)), PathBuf::from("~other/notes"));
        assert_eq!(expand_home_with("relative/notes", Some(home)), PathBuf::from("relative/notes"));
        assert_eq!(expand_home_with("~/notes", None), PathBuf::from("~/notes"));
    }
}
