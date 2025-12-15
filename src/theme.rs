#![allow(dead_code)]

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default = "default_notes_dir")]
    pub notes_dir: String,
    #[serde(default)]
    pub onboarding_complete: bool,
}

fn default_notes_dir() -> String {
    dirs::document_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("ekphos")
        .to_string_lossy()
        .to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            notes_dir: default_notes_dir(),
            onboarding_complete: false,
        }
    }
}

impl Config {
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

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ekphos")
            .join("config.toml")
    }

    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ekphos")
    }

    pub fn themes_dir() -> PathBuf {
        Self::config_dir().join("themes")
    }

    pub fn create_default_config() -> std::io::Result<()> {
        let config_dir = Self::config_dir();
        fs::create_dir_all(&config_dir)?;

        let config_path = Self::config_path();
        if !config_path.exists() {
            let default_config = Self::default();
            let toml_string = toml::to_string_pretty(&default_config)
                .unwrap_or_else(|_| String::new());
            fs::write(&config_path, toml_string)?;
        }
        Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme_name")]
    pub name: String,
    #[serde(default)]
    pub colors: ColorConfig,
}

fn default_theme_name() -> String {
    "catppuccin-mocha".to_string()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
            colors: ColorConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub colors: ColorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorConfig {
    // Base colors
    pub base: String,
    pub mantle: String,
    pub crust: String,
    pub surface0: String,
    pub surface1: String,
    pub surface2: String,
    pub overlay0: String,
    pub overlay1: String,
    pub overlay2: String,
    pub text: String,
    pub subtext0: String,
    pub subtext1: String,

    // Accent colors
    pub rosewater: String,
    pub flamingo: String,
    pub pink: String,
    pub mauve: String,
    pub red: String,
    pub maroon: String,
    pub peach: String,
    pub yellow: String,
    pub green: String,
    pub teal: String,
    pub sky: String,
    pub sapphire: String,
    pub blue: String,
    pub lavender: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        // Default: Catppuccin Mocha
        Self {
            base: "#1e1e2e".to_string(),
            mantle: "#181825".to_string(),
            crust: "#11111b".to_string(),
            surface0: "#313244".to_string(),
            surface1: "#45475a".to_string(),
            surface2: "#585b70".to_string(),
            overlay0: "#6c7086".to_string(),
            overlay1: "#7f849c".to_string(),
            overlay2: "#9399b2".to_string(),
            text: "#cdd6f4".to_string(),
            subtext0: "#a6adc8".to_string(),
            subtext1: "#bac2de".to_string(),
            rosewater: "#f5e0dc".to_string(),
            flamingo: "#f2cdcd".to_string(),
            pink: "#f5c2e7".to_string(),
            mauve: "#cba6f7".to_string(),
            red: "#f38ba8".to_string(),
            maroon: "#eba0ac".to_string(),
            peach: "#fab387".to_string(),
            yellow: "#f9e2af".to_string(),
            green: "#a6e3a1".to_string(),
            teal: "#94e2d5".to_string(),
            sky: "#89dceb".to_string(),
            sapphire: "#74c7ec".to_string(),
            blue: "#89b4fa".to_string(),
            lavender: "#b4befe".to_string(),
        }
    }
}

impl ColorConfig {
    /// Load theme from file
    pub fn load_from_file(path: &PathBuf) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let theme_file: ThemeFile = toml::from_str(&content).ok()?;
        Some(theme_file.colors)
    }

    /// Find and load theme by name from themes directories
    pub fn load_by_name(name: &str) -> Option<Self> {
        // Check user themes directory first
        let user_themes_dir = Config::themes_dir();
        if user_themes_dir.exists() {
            let theme_path = user_themes_dir.join(format!("{}.toml", name));
            if theme_path.exists() {
                if let Some(colors) = Self::load_from_file(&theme_path) {
                    return Some(colors);
                }
            }
        }

        // Check bundled themes in current directory
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

/// Runtime theme with parsed colors
#[derive(Debug, Clone)]
pub struct Theme {
    // Base colors
    pub base: Color,
    pub mantle: Color,
    pub crust: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub overlay0: Color,
    pub overlay1: Color,
    pub overlay2: Color,
    pub text: Color,
    pub subtext0: Color,
    pub subtext1: Color,

    // Accent colors
    pub rosewater: Color,
    pub flamingo: Color,
    pub pink: Color,
    pub mauve: Color,
    pub red: Color,
    pub maroon: Color,
    pub peach: Color,
    pub yellow: Color,
    pub green: Color,
    pub teal: Color,
    pub sky: Color,
    pub sapphire: Color,
    pub blue: Color,
    pub lavender: Color,
}

impl Theme {
    pub fn from_config(config: &ColorConfig) -> Self {
        Self {
            base: parse_hex_color(&config.base),
            mantle: parse_hex_color(&config.mantle),
            crust: parse_hex_color(&config.crust),
            surface0: parse_hex_color(&config.surface0),
            surface1: parse_hex_color(&config.surface1),
            surface2: parse_hex_color(&config.surface2),
            overlay0: parse_hex_color(&config.overlay0),
            overlay1: parse_hex_color(&config.overlay1),
            overlay2: parse_hex_color(&config.overlay2),
            text: parse_hex_color(&config.text),
            subtext0: parse_hex_color(&config.subtext0),
            subtext1: parse_hex_color(&config.subtext1),
            rosewater: parse_hex_color(&config.rosewater),
            flamingo: parse_hex_color(&config.flamingo),
            pink: parse_hex_color(&config.pink),
            mauve: parse_hex_color(&config.mauve),
            red: parse_hex_color(&config.red),
            maroon: parse_hex_color(&config.maroon),
            peach: parse_hex_color(&config.peach),
            yellow: parse_hex_color(&config.yellow),
            green: parse_hex_color(&config.green),
            teal: parse_hex_color(&config.teal),
            sky: parse_hex_color(&config.sky),
            sapphire: parse_hex_color(&config.sapphire),
            blue: parse_hex_color(&config.blue),
            lavender: parse_hex_color(&config.lavender),
        }
    }

    pub fn from_name(name: &str) -> Self {
        // Try to load from file first
        if let Some(colors) = ColorConfig::load_by_name(name) {
            return Self::from_config(&colors);
        }
        // Fallback to default
        Self::from_config(&ColorConfig::default())
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_config(&ColorConfig::default())
    }
}

fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
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
