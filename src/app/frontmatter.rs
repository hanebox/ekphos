use std::collections::HashMap;
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Frontmatter {
    pub title: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub date: Option<String>,
    pub author: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

impl Frontmatter {
    /// Parse YAML frontmatter from content.
    /// Returns the parsed Frontmatter (if valid) and the line index where content starts.
    pub fn parse(content: &str) -> (Option<Self>, usize) {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || lines[0].trim() != "---" {
            return (None, 0);
        }
        let mut end_index = None;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                end_index = Some(i);
                break;
            }
        }

        let end_index = match end_index {
            Some(i) => i,
            None => return (None, 0), // No closing delimiter
        };

        let yaml_content: String = lines[1..end_index].join("\n");

        let frontmatter = serde_yaml::from_str::<Frontmatter>(&yaml_content).ok();
        let content_start_line = end_index + 1;

        (frontmatter, content_start_line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_frontmatter() {
        let content = r#"---
title: Test Note
tags: [rust, cli]
date: 2024-01-15
---
# Heading
Content here"#;

        let (fm, start) = Frontmatter::parse(content);
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.title, Some("Test Note".to_string()));
        assert_eq!(fm.tags, vec!["rust", "cli"]);
        assert_eq!(fm.date, Some("2024-01-15".to_string()));
        assert_eq!(start, 5);
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Just a heading\nSome content";
        let (fm, start) = Frontmatter::parse(content);
        assert!(fm.is_none());
        assert_eq!(start, 0);
    }

    #[test]
    fn test_parse_unclosed_frontmatter() {
        let content = "---\ntitle: Test\nNo closing delimiter";
        let (fm, start) = Frontmatter::parse(content);
        assert!(fm.is_none());
        assert_eq!(start, 0);
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let content = "---\n: invalid yaml [\n---\nContent";
        let (fm, start) = Frontmatter::parse(content);
        assert!(fm.is_none());
        assert_eq!(start, 3);
    }

    #[test]
    fn test_parse_tags_multiline() {
        let content = r#"---
tags:
  - rust
  - cli
  - tui
---
Content"#;

        let (fm, start) = Frontmatter::parse(content);
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.tags, vec!["rust", "cli", "tui"]);
        assert_eq!(start, 6);
    }
}
