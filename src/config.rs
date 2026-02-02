#![allow(dead_code)]

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_notes_dir")]
    pub notes_dir: String,
    #[serde(default = "default_welcome_shown")]
    pub welcome_shown: bool,
    #[serde(default = "default_theme_name")]
    pub theme: String,
    #[serde(default = "default_show_empty_dir")]
    pub show_empty_dir: bool,
    #[serde(default = "default_syntax_theme")]
    pub syntax_theme: String,
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
    #[serde(default)]
    pub editor: EditorConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LineNumberMode {
    None,
    #[default]
    Absolute,
    Relative,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
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

fn default_line_wrap() -> bool { true }
fn default_tab_width() -> u16 { 4 }
fn default_left_padding() -> u16 { 0 }
fn default_right_padding() -> u16 { 1 }
fn default_scrolloff() -> u8 { 0 }

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            line_wrap: default_line_wrap(),
            tab_width: default_tab_width(),
            left_padding: default_left_padding(),
            right_padding: default_right_padding(),
            line_numbers: LineNumberMode::default(),
            scrolloff: default_scrolloff(),
        }
    }
}

fn default_notes_dir() -> String { "~/Documents/ekphos".to_string() }
fn default_welcome_shown() -> bool { true }
fn default_show_empty_dir() -> bool { true }
fn default_theme_name() -> String { "ekphos-dawn".to_string() }
fn default_syntax_theme() -> String { "base16-ocean.dark".to_string() }
fn default_sidebar_collapsed() -> bool { false }
fn default_outline_collapsed() -> bool { false }
fn default_folders_first() -> bool { true }
fn default_frontmatter_hidden() -> bool { true }
fn default_show_tags() -> bool { true }
fn default_check_updates() -> bool { true }
fn default_transparent_bg() -> bool { false }

impl Default for Config {
    fn default() -> Self {
        Self {
            notes_dir: default_notes_dir(),
            welcome_shown: default_welcome_shown(),
            theme: default_theme_name(),
            show_empty_dir: default_show_empty_dir(),
            syntax_theme: default_syntax_theme(),
            sidebar_collapsed: default_sidebar_collapsed(),
            outline_collapsed: default_outline_collapsed(),
            folders_first: default_folders_first(),
            frontmatter_hidden: default_frontmatter_hidden(),
            show_tags: default_show_tags(),
            check_updates: default_check_updates(),
            transparent_bg: default_transparent_bg(),
            editor: EditorConfig::default(),
        }
    }
}

impl Config {
    pub fn exists() -> bool { Self::config_path().exists() }

    pub fn load() -> Self {
        let config_path = Self::config_path();
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

        if !config_dir.exists() { let _ = fs::create_dir_all(&config_dir); }
        if !themes_dir.exists() { let _ = fs::create_dir_all(&themes_dir); }

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

    pub fn config_path() -> PathBuf { Self::config_dir().join("config.toml") }
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("ekphos")
    }
    pub fn themes_dir() -> PathBuf { Self::config_dir().join("themes") }

    pub fn save(&self) -> std::io::Result<()> {
        let config_dir = Self::config_dir();
        fs::create_dir_all(&config_dir)?;
        let config_path = Self::config_path();
        let toml_string = toml::to_string_pretty(self).unwrap_or_else(|_| String::new());
        fs::write(&config_path, toml_string)?;
        Ok(())
    }

    pub fn notes_path(&self) -> PathBuf {
        let path = shellexpand::tilde(&self.notes_dir).to_string();
        PathBuf::from(path)
    }
}

// ============================================================================
// Theme File Format (TOML parsing structures)
// ============================================================================

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

// Default color values module
mod defaults {
    pub fn background() -> String { "#1a1a24".to_string() }
    pub fn background_secondary() -> String { "#24243a".to_string() }
    pub fn foreground() -> String { "#c0caf5".to_string() }
    pub fn muted() -> String { "#565f89".to_string() }
    pub fn primary() -> String { "#7aa2f7".to_string() }
    pub fn secondary() -> String { "#bb9af7".to_string() }
    pub fn error() -> String { "#f7768e".to_string() }
    pub fn warning() -> String { "#e0af68".to_string() }
    pub fn success() -> String { "#9ece6a".to_string() }
    pub fn info() -> String { "#7dcfff".to_string() }
    pub fn border() -> String { "#3b4261".to_string() }
    pub fn border_focused() -> String { "#7aa2f7".to_string() }
    pub fn selection() -> String { "#283457".to_string() }
    pub fn cursor() -> String { "#c0caf5".to_string() }
    pub fn search_match_current() -> String { "#ff9e64".to_string() }
}

impl Default for BaseColors {
    fn default() -> Self {
        Self {
            background: defaults::background(),
            background_secondary: defaults::background_secondary(),
            foreground: defaults::foreground(),
            muted: defaults::muted(),
        }
    }
}

impl Default for AccentColors {
    fn default() -> Self {
        Self { primary: defaults::primary(), secondary: defaults::secondary() }
    }
}

impl Default for SemanticColors {
    fn default() -> Self {
        Self {
            error: defaults::error(),
            warning: defaults::warning(),
            success: defaults::success(),
            info: defaults::info(),
        }
    }
}

impl Default for StatusbarColors {
    fn default() -> Self {
        Self {
            background: defaults::background(),
            foreground: defaults::foreground(),
            brand: defaults::primary(),
            mode: defaults::muted(),
            separator: defaults::border(),
        }
    }
}

impl Default for DialogColors {
    fn default() -> Self {
        Self {
            background: defaults::background(),
            border: defaults::primary(),
            title: defaults::primary(),
            text: defaults::foreground(),
        }
    }
}

impl Default for SidebarColors {
    fn default() -> Self {
        Self {
            background: defaults::background(),
            item: defaults::foreground(),
            item_selected: defaults::warning(),
            folder: defaults::info(),
            folder_expanded: defaults::info(),
        }
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
        Self {
            background: defaults::background(),
            heading1: defaults::primary(),
            heading2: defaults::success(),
            heading3: defaults::warning(),
            heading4: defaults::secondary(),
        }
    }
}

impl Default for SearchColors {
    fn default() -> Self {
        Self {
            background: defaults::background_secondary(),
            border: defaults::primary(),
            input: defaults::foreground(),
            match_highlight: defaults::warning(),
            match_current: defaults::search_match_current(),
            match_count: defaults::muted(),
        }
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
            _ => return None,
        };
        Self::load_from_str(content)
    }

    pub fn load_by_name(name: &str) -> Option<Self> {
        let user_themes_dir = Config::themes_dir();
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
}

// ============================================================================
// Runtime Theme (parsed colors for UI rendering)
// ============================================================================

#[derive(Debug, Clone)]
pub struct Theme {
    // Base colors
    pub background: Color,
    pub background_secondary: Color,
    pub foreground: Color,
    pub muted: Color,

    // Accent colors
    pub primary: Color,
    pub secondary: Color,

    // Semantic colors
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,

    // UI colors
    pub border: Color,
    pub border_focused: Color,
    pub selection: Color,
    pub cursor: Color,

    // Component-specific colors
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
            dialog: DialogTheme {
                background: parse_hex_color(&tf.ui.dialog.background),
                border: parse_hex_color(&tf.ui.dialog.border),
                title: parse_hex_color(&tf.ui.dialog.title),
                text: parse_hex_color(&tf.ui.dialog.text),
            },
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
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_file(&ThemeFile::default())
    }
}

fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#').trim_start_matches('\'').trim_end_matches('\'');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::White
}
