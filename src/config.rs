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
    #[serde(default)]
    pub editor: EditorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    #[serde(default = "default_line_wrap")]
    pub line_wrap: bool,
}

fn default_line_wrap() -> bool {
    true
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            line_wrap: default_line_wrap(),
        }
    }
}

fn default_notes_dir() -> String {
    "~/Documents/ekphos".to_string()
}

fn default_welcome_shown() -> bool {
    true 
}

fn default_show_empty_dir() -> bool {
    true 
}

fn default_theme_name() -> String {
    "catppuccin-mocha".to_string()
}

fn default_syntax_theme() -> String {
    "base16-ocean.dark".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            notes_dir: default_notes_dir(),
            welcome_shown: default_welcome_shown(),
            theme: default_theme_name(),
            show_empty_dir: default_show_empty_dir(),
            syntax_theme: default_syntax_theme(),
            editor: EditorConfig::default(),
        }
    }
}

impl Config {
    pub fn exists() -> bool {
        Self::config_path().exists()
    }

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

    /// Load config, creating default config directory and file if they don't exist
    /// Does NOT override existing config or theme files
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

        let default_theme_path = themes_dir.join("catppuccin-mocha.toml");
        if !default_theme_path.exists() {
            let default_theme_content = include_str!("../themes/catppuccin-mocha.toml");
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

    pub fn config_dir() -> PathBuf {
        // Always use ~/.config/ekphos/ on macOS and Linux
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("ekphos")
    }

    pub fn themes_dir() -> PathBuf {
        Self::config_dir().join("themes")
    }

    pub fn save(&self) -> std::io::Result<()> {
        let config_dir = Self::config_dir();
        fs::create_dir_all(&config_dir)?;

        let config_path = Self::config_path();
        let toml_string = toml::to_string_pretty(self)
            .unwrap_or_else(|_| String::new());
        fs::write(&config_path, toml_string)?;
        Ok(())
    }

    pub fn notes_path(&self) -> PathBuf {
        let path = shellexpand::tilde(&self.notes_dir).to_string();
        PathBuf::from(path)
    }
}


// Alacritty-compatible theme file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    pub colors: ThemeColors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub primary: PrimaryColors,
    #[serde(default)]
    pub cursor: CursorColors,
    #[serde(default)]
    pub selection: SelectionColors,
    pub normal: TerminalColors,
    pub bright: TerminalColors,
    // Alacritty-specific fields we ignore but need to accept
    #[serde(default, skip_serializing)]
    pub vi_mode_cursor: Option<IgnoredColors>,
    #[serde(default, skip_serializing)]
    pub search: Option<IgnoredSearch>,
    #[serde(default, skip_serializing)]
    pub hints: Option<IgnoredHints>,
    #[serde(default, skip_serializing)]
    pub footer_bar: Option<IgnoredColors>,
    #[serde(default, skip_serializing)]
    pub line_indicator: Option<IgnoredColors>,
    #[serde(default, skip_serializing)]
    pub indexed_colors: Option<Vec<IgnoredIndexedColor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryColors {
    pub background: String,
    pub foreground: String,
    // Optional Alacritty fields
    #[serde(default, skip_serializing)]
    pub dim_foreground: Option<String>,
    #[serde(default, skip_serializing)]
    pub bright_foreground: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CursorColors {
    #[serde(default = "default_cursor_text")]
    pub text: String,
    #[serde(default = "default_cursor")]
    pub cursor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectionColors {
    #[serde(default = "default_selection_text")]
    pub text: String,
    #[serde(default = "default_selection_bg")]
    pub background: String,
}

fn default_cursor_text() -> String {
    "#1e1e2e".to_string()
}

fn default_cursor() -> String {
    "#f5e0dc".to_string()
}

fn default_selection_text() -> String {
    "#cdd6f4".to_string() // Light foreground color
}

fn default_selection_bg() -> String {
    "#585b70".to_string() // Subtle gray background (bright_black)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalColors {
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
}

// Structs to accept and ignore Alacritty-specific fields
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IgnoredColors {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub foreground: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IgnoredSearch {
    #[serde(default)]
    pub matches: Option<IgnoredColors>,
    #[serde(default)]
    pub focused_match: Option<IgnoredColors>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IgnoredHints {
    #[serde(default)]
    pub start: Option<IgnoredColors>,
    #[serde(default)]
    pub end: Option<IgnoredColors>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IgnoredIndexedColor {
    #[serde(default)]
    pub index: Option<u8>,
    #[serde(default)]
    pub color: Option<String>,
}

impl Default for ThemeColors {
    fn default() -> Self {
        // Default: Catppuccin Mocha theme
        Self {
            primary: PrimaryColors {
                background: "#1e1e2e".to_string(),
                foreground: "#cdd6f4".to_string(),
                dim_foreground: None,
                bright_foreground: None,
            },
            cursor: CursorColors {
                text: "#1e1e2e".to_string(),
                cursor: "#f5e0dc".to_string(),
            },
            selection: SelectionColors {
                text: "#cdd6f4".to_string(),
                background: "#585b70".to_string(),
            },
            normal: TerminalColors {
                black: "#45475a".to_string(),
                red: "#f38ba8".to_string(),
                green: "#a6e3a1".to_string(),
                yellow: "#f9e2af".to_string(),
                blue: "#89b4fa".to_string(),
                magenta: "#f5c2e7".to_string(),
                cyan: "#94e2d5".to_string(),
                white: "#bac2de".to_string(),
            },
            bright: TerminalColors {
                black: "#585b70".to_string(),
                red: "#f38ba8".to_string(),
                green: "#a6e3a1".to_string(),
                yellow: "#f9e2af".to_string(),
                blue: "#89b4fa".to_string(),
                magenta: "#f5c2e7".to_string(),
                cyan: "#94e2d5".to_string(),
                white: "#a6adc8".to_string(),
            },
            // Ignored Alacritty fields
            vi_mode_cursor: None,
            search: None,
            hints: None,
            footer_bar: None,
            line_indicator: None,
            indexed_colors: None,
        }
    }
}

impl ThemeColors {
    /// Load theme from file
    pub fn load_from_file(path: &PathBuf) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let theme_file: ThemeFile = toml::from_str(&content).ok()?;
        Some(theme_file.colors)
    }

    pub fn load_from_str(content: &str) -> Option<Self> {
        let theme_file: ThemeFile = toml::from_str(content).ok()?;
        Some(theme_file.colors)
    }

    fn get_bundled_theme(name: &str) -> Option<Self> {
        let content = match name {
            "catppuccin-mocha" => include_str!("../themes/catppuccin-mocha.toml"),
            _ => return None,
        };
        Self::load_from_str(content)
    }

    /// Find and load theme by name from themes directories
    pub fn load_by_name(name: &str) -> Option<Self> {
        let user_themes_dir = Config::themes_dir();
        if user_themes_dir.exists() {
            let theme_path = user_themes_dir.join(format!("{}.toml", name));
            if theme_path.exists() {
                if let Some(colors) = Self::load_from_file(&theme_path) {
                    return Some(colors);
                }
            }
        }

        if let Some(colors) = Self::get_bundled_theme(name) {
            return Some(colors);
        }

        let bundled_themes = PathBuf::from("themes");
        if bundled_themes.exists() {
            let theme_path = bundled_themes.join(format!("{}.toml", name));
            if theme_path.exists() {
                if let Some(colors) = Self::load_from_file(&theme_path) {
                    return Some(colors);
                }
            }
        }

        None
    }
}

/// Runtime theme with parsed colors for UI rendering
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,

    pub cursor_text: Color,
    pub cursor: Color,

    pub selection_text: Color,
    pub selection_bg: Color,

    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,

    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_magenta: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,
}

impl Theme {
    pub fn from_colors(colors: &ThemeColors) -> Self {
        Self {
            background: parse_hex_color(&colors.primary.background),
            foreground: parse_hex_color(&colors.primary.foreground),
            cursor_text: parse_hex_color(&colors.cursor.text),
            cursor: parse_hex_color(&colors.cursor.cursor),
            selection_text: parse_hex_color(&colors.selection.text),
            selection_bg: parse_hex_color(&colors.selection.background),
            black: parse_hex_color(&colors.normal.black),
            red: parse_hex_color(&colors.normal.red),
            green: parse_hex_color(&colors.normal.green),
            yellow: parse_hex_color(&colors.normal.yellow),
            blue: parse_hex_color(&colors.normal.blue),
            magenta: parse_hex_color(&colors.normal.magenta),
            cyan: parse_hex_color(&colors.normal.cyan),
            white: parse_hex_color(&colors.normal.white),
            bright_black: parse_hex_color(&colors.bright.black),
            bright_red: parse_hex_color(&colors.bright.red),
            bright_green: parse_hex_color(&colors.bright.green),
            bright_yellow: parse_hex_color(&colors.bright.yellow),
            bright_blue: parse_hex_color(&colors.bright.blue),
            bright_magenta: parse_hex_color(&colors.bright.magenta),
            bright_cyan: parse_hex_color(&colors.bright.cyan),
            bright_white: parse_hex_color(&colors.bright.white),
        }
    }

    pub fn from_name(name: &str) -> Self {
        if let Some(colors) = ThemeColors::load_by_name(name) {
            return Self::from_colors(&colors);
        }
        Self::from_colors(&ThemeColors::default())
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_colors(&ThemeColors::default())
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
