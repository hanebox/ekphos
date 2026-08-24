use ratatui::style::{Color, Modifier, Style as RatatuiStyle};
use ratatui::text::Span;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, ThemeSet};
use syntect::parsing::SyntaxSet;

pub const DEFAULT_SYNTAX_CACHE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    content_hash: u64,
    lang: String,
    theme: String,
}

struct CacheEntry {
    lines: Vec<Vec<Span<'static>>>,
    bytes: usize,
}

struct HighlightCache {
    entries: HashMap<CacheKey, CacheEntry>,
    lru: VecDeque<CacheKey>,
    bytes: usize,
    budget: usize,
}

impl HighlightCache {
    fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru: VecDeque::new(),
            bytes: 0,
            budget,
        }
    }

    fn get(&mut self, key: &CacheKey) -> Option<Vec<Vec<Span<'static>>>> {
        let lines = self.entries.get(key)?.lines.clone();
        self.touch(key);
        Some(lines)
    }

    fn insert(&mut self, key: CacheKey, lines: Vec<Vec<Span<'static>>>) {
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
            self.lru.retain(|candidate| candidate != &key);
        }

        let bytes = cache_entry_bytes(&key, &lines);
        self.bytes = self.bytes.saturating_add(bytes);
        self.lru.push_back(key.clone());
        self.entries.insert(key, CacheEntry { lines, bytes });

        // Keep the newest indivisible result even when one code block alone is
        // larger than the configured cache budget.
        while self.bytes > self.budget && self.entries.len() > 1 {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
    }

    fn touch(&mut self, key: &CacheKey) {
        self.lru.retain(|candidate| candidate != key);
        self.lru.push_back(key.clone());
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
        self.bytes = 0;
    }
}

fn hash_content(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn cache_entry_bytes(key: &CacheKey, lines: &[Vec<Span<'static>>]) -> usize {
    std::mem::size_of::<CacheKey>()
        + key.lang.capacity()
        + key.theme.capacity()
        + std::mem::size_of_val(lines)
        + lines
            .iter()
            .map(|spans| {
                spans.capacity() * std::mem::size_of::<Span<'static>>()
                    + spans
                        .iter()
                        .map(|span| match &span.content {
                            Cow::Borrowed(_) => 0,
                            Cow::Owned(text) => text.capacity(),
                        })
                        .sum::<usize>()
            })
            .sum::<usize>()
}

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
    definition_bytes: usize,
    cache: RefCell<HighlightCache>,
}

impl Highlighter {
    pub fn new(theme_name: &str) -> Self {
        Self::with_cache_budget(theme_name, DEFAULT_SYNTAX_CACHE_BYTES)
    }

    fn with_cache_budget(theme_name: &str, cache_budget: usize) -> Self {
        let theme_set = ThemeSet::load_defaults();
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let valid_theme = valid_theme_name(&theme_set, theme_name);

        // Syntect exposes no retained-heap accounting. Its serialized dump is a
        // stable, separately observable lower-bound proxy that keeps definition
        // ownership distinct from rendered result-cache ownership.
        let definition_bytes = syntect::dumps::dump_binary(&syntax_set).len() + syntect::dumps::dump_binary(&theme_set).len();

        Self {
            syntax_set,
            theme_set,
            theme_name: valid_theme,
            definition_bytes,
            cache: RefCell::new(HighlightCache::new(cache_budget)),
        }
    }

    pub fn highlight_block(&self, content: &str, lang: &str) -> Vec<Vec<Span<'static>>> {
        let key = CacheKey {
            content_hash: hash_content(content),
            lang: lang.to_string(),
            theme: self.theme_name.clone(),
        };

        if let Some(cached) = self.cache.borrow_mut().get(&key) {
            return cached;
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
                let line_with_newline = format!("{line}\n");
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

        self.cache.borrow_mut().insert(key, result.clone());
        result
    }

    pub fn definition_bytes(&self) -> usize {
        self.definition_bytes
    }

    pub fn retained_cache_bytes(&self) -> usize {
        self.cache.borrow().bytes
    }

    pub fn cache_entries(&self) -> usize {
        self.cache.borrow().entries.len()
    }

    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    pub fn set_theme(&mut self, theme_name: &str) {
        let valid_theme = valid_theme_name(&self.theme_set, theme_name);
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

fn valid_theme_name(theme_set: &ThemeSet, requested: &str) -> String {
    if theme_set.themes.contains_key(requested) {
        requested.to_string()
    } else {
        "base16-ocean.dark".to_string()
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
        let content = "print(\"\u{4f60}\u{597d}\u{4e16}\u{754c}\")\nx = \"\u{6d4b}\u{8bd5}\"";
        let result = h.highlight_block(content, "python");
        assert_eq!(result.len(), 2);
        assert!(!result[0].is_empty());
        assert!(!result[1].is_empty());
    }

    #[test]
    fn test_highlight_block_c_with_cjk_comments() {
        let h = Highlighter::default();
        let content =
            "#include \"user/user.h\"\nint main(int argc, char *argv[]) {\n    // \u{9519}\u{8bef}\u{68c0}\u{67e5}\n    if (argc != 2) {\n        printf(\"hello\");\n    }\n}";
        let result = h.highlight_block(content, "c");
        assert_eq!(result.len(), 7);
        let line_after_cjk = &result[3];
        assert!(
            line_after_cjk.len() > 1,
            "Line after CJK comment should have multiple highlighted spans, got {} span(s): {:?}",
            line_after_cjk.len(),
            line_after_cjk.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn syntax_cache_is_byte_weighted_and_keeps_only_one_oversized_result() {
        let h = Highlighter::with_cache_budget("base16-ocean.dark", 1);
        h.highlight_block("let first = 1;", "rust");
        h.highlight_block("let second = 2;", "rust");

        assert_eq!(h.cache_entries(), 1);
        assert!(h.retained_cache_bytes() > 1);
        assert!(h.definition_bytes() > 0);
    }

    #[test]
    fn theme_change_invalidates_results() {
        let mut h = Highlighter::default();
        h.highlight_block("let value = true;", "rust");
        assert_eq!(h.cache_entries(), 1);

        let alternative = h.theme_set.themes.keys().find(|name| *name != &h.theme_name).unwrap().clone();
        h.set_theme(&alternative);
        assert_eq!(h.cache_entries(), 0);
    }
}
