use ratatui::style::{Color, Modifier, Style as RatatuiStyle};
use ratatui::text::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;

#[derive(Clone, PartialEq, Eq)]
struct CacheKey {
    content_hash: u64,
    lang: String,
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.content_hash.hash(state);
        self.lang.hash(state);
    }
}

fn hash_content(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

const MAX_CACHE_ENTRIES: usize = 100;

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
    cache: RefCell<HashMap<CacheKey, Vec<Vec<Span<'static>>>>>,
}

impl Highlighter {
    pub fn new(theme_name: &str) -> Self {
        let theme_set = ThemeSet::load_defaults();
        let valid_theme = if theme_set.themes.contains_key(theme_name) {
            theme_name.to_string()
        } else {
            "base16-ocean.dark".to_string()
        };
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set,
            theme_name: valid_theme,
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn highlight_block(&self, content: &str, lang: &str) -> Vec<Vec<Span<'static>>> {
        let content_hash = hash_content(content);
        let key = CacheKey {
            content_hash,
            lang: lang.to_string(),
        };

        {
            let cache = self.cache.borrow();
            if let Some(cached) = cache.get(&key) {
                return cached.clone();
            }
        }

        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes[&self.theme_name];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let result: Vec<Vec<Span<'static>>> = content
            .split('\n')
            .map(|line| {
                let line_with_newline = format!("{}\n", line);
                match highlighter.highlight_line(&line_with_newline, &self.syntax_set) {
                    Ok(ranges) => ranges
                        .into_iter()
                        .map(|(style, text)| {
                            let cleaned = text.trim_end_matches('\n');
                            self.style_to_span(cleaned, style)
                        })
                        .filter(|span| !span.content.is_empty())
                        .collect(),
                    Err(_) => vec![Span::raw(line.to_string())],
                }
            })
            .collect();

        {
            let mut cache = self.cache.borrow_mut();
            if cache.len() >= MAX_CACHE_ENTRIES {
                // Simple eviction: clear half the cache
                let keys_to_remove: Vec<_> = cache.keys().take(MAX_CACHE_ENTRIES / 2).cloned().collect();
                for k in keys_to_remove {
                    cache.remove(&k);
                }
            }
            cache.insert(key, result.clone());
        }

        result
    }

    #[allow(dead_code)]
    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    #[allow(dead_code)]
    pub fn set_theme(&mut self, theme_name: &str) {
        let valid_theme = if self.theme_set.themes.contains_key(theme_name) {
            theme_name.to_string()
        } else {
            "base16-ocean.dark".to_string()
        };
        if self.theme_name != valid_theme {
            self.theme_name = valid_theme;
            self.clear_cache();
        }
    }

    fn style_to_span(&self, text: &str, style: Style) -> Span<'static> {
        let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);

        let mut ratatui_style = RatatuiStyle::default().fg(fg);

        if style.font_style.contains(FontStyle::BOLD) {
            ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
        }
        if style.font_style.contains(FontStyle::ITALIC) {
            ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
        }
        if style.font_style.contains(FontStyle::UNDERLINE) {
            ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
        }

        Span::styled(text.to_string(), ratatui_style)
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new("base16-ocean.dark")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_block_trailing_empty_line() {
        let h = Highlighter::default();
        let content = "line1\nline2\n";
        let result = h.highlight_block(content, "txt");
        assert_eq!(result.len(), 3, "Should produce 3 lines including trailing empty");
    }

    #[test]
    fn test_highlight_block_cjk_no_panic() {
        let h = Highlighter::default();
        let content = "print(\"你好世界\")\nx = \"测试\"";
        let result = h.highlight_block(content, "python");
        assert_eq!(result.len(), 2);
        assert!(!result[0].is_empty());
        assert!(!result[1].is_empty());
    }

    #[test]
    fn test_highlight_block_c_with_cjk_comments() {
        let h = Highlighter::default();
        let content = "#include \"user/user.h\"\nint main(int argc, char *argv[]) {\n    // 错误检查\n    if (argc != 2) {\n        printf(\"hello\");\n    }\n}";
        let result = h.highlight_block(content, "c");
        assert_eq!(result.len(), 7);
        let line_after_cjk = &result[3];
        eprintln!("Line 3 spans: {:?}", line_after_cjk.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>());
        assert!(line_after_cjk.len() > 1,
            "Line after CJK comment should have multiple highlighted spans, got {} span(s): {:?}",
            line_after_cjk.len(),
            line_after_cjk.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>());
    }
}
