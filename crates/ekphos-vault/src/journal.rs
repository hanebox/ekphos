//! Journal mode.
//!
//! Pressing `t` opens today's daily note in
//! `<journal_dir>/<year>/journal.<date>.md`, creating the year directory and a
//! small dated template when needed. Dates use the user's local timezone, so
//! entries roll over at local midnight rather than UTC.

use chrono::{Local, NaiveDate};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalEntryAction {
    Created,
    Opened,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub path: PathBuf,
    pub action: JournalEntryAction,
}

/// Capture today's local date once so every part of an entry uses the same day.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Filename for a journal entry, e.g. `journal.2024-05-29.md`.
pub fn filename_for_date(date: NaiveDate) -> String {
    format!("journal.{}.md", date.format("%Y-%m-%d"))
}

/// Vault-relative path for a journal entry.
pub fn entry_relative_path(journal_dir: &str, date: NaiveDate) -> Result<PathBuf, String> {
    let journal_dir = journal_dir.trim();
    let configured_path = Path::new(journal_dir);
    if journal_dir.is_empty() || configured_path.is_absolute() || configured_path.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err("journal_dir must be a non-empty relative path inside the notes directory".to_string());
    }
    Ok(configured_path.join(date.format("%Y").to_string()).join(filename_for_date(date)))
}

/// Initial content for a fresh journal entry, with the date filled in.
pub fn new_entry_content(date: NaiveDate) -> String {
    let formatted_date = date.format("%Y-%m-%d");
    let weekday = date.format("%A");
    format!("---\ntitle: Journal — {formatted_date}\ntags: [journal]\ndate: {formatted_date}\n---\n\n# {formatted_date} · {weekday}\n\n")
}

/// Open today's canonical entry, fall back to a same-day legacy root entry, or
/// create a new canonical entry. Existing files are never moved or overwritten.
pub fn open_or_create_entry(notes_dir: &Path, journal_dir: &str, date: NaiveDate) -> Result<JournalEntry, String> {
    let relative_path = entry_relative_path(journal_dir, date)?;
    let canonical_path = notes_dir.join(&relative_path);
    if canonical_path.exists() {
        return existing_entry(canonical_path);
    }
    let legacy_path = notes_dir.join(filename_for_date(date));
    if legacy_path.exists() {
        return existing_entry(legacy_path);
    }
    let parent = canonical_path.parent().ok_or_else(|| "invalid journal path".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("failed to create directory {}: {error}", parent.display()))?;
    fs::write(&canonical_path, new_entry_content(date)).map_err(|error| format!("failed to create {}: {error}", canonical_path.display()))?;
    Ok(JournalEntry { path: canonical_path, action: JournalEntryAction::Created })
}
fn existing_entry(path: PathBuf) -> Result<JournalEntry, String> {
    if !path.is_file() {
        return Err(format!("path is not a file: {}", path.display()));
    }
    Ok(JournalEntry { path, action: JournalEntryAction::Opened })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);
    struct TempWorkspace {
        path: PathBuf,
    }
    impl TempWorkspace {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("ekphos-journal-{label}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }
    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
    fn test_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()
    }

    #[test]
    fn entry_path_uses_configured_directory_and_year() {
        assert_eq!(entry_relative_path("Journal", test_date()).unwrap(), PathBuf::from("Journal/2026/journal.2026-08-06.md"));
        assert_eq!(entry_relative_path("Personal/Daily Notes", test_date()).unwrap(), PathBuf::from("Personal/Daily Notes/2026/journal.2026-08-06.md"));
    }

    #[test]
    fn entry_path_rejects_locations_outside_the_vault() {
        let absolute = PathBuf::from(std::path::MAIN_SEPARATOR.to_string()).join("Journal").to_string_lossy().to_string();
        for configured in ["", "   ", ".", "..", "../Journal", "Journal/../Other"] {
            assert!(entry_relative_path(configured, test_date()).is_err());
        }
        assert!(entry_relative_path(&absolute, test_date()).is_err());
    }

    #[test]
    fn entry_content_has_fixed_date_frontmatter_and_heading() {
        let content = new_entry_content(test_date());
        assert!(content.starts_with("---\n"));
        assert!(content.contains("title: Journal — 2026-08-06"));
        assert!(content.contains("tags: [journal]"));
        assert!(content.contains("date: 2026-08-06"));
        assert!(content.contains("# 2026-08-06 · Thursday"));
    }

    #[test]
    fn creates_nested_entry_without_overwriting_it_on_reopen() {
        let workspace = TempWorkspace::new("create");
        let notes_dir = workspace.path.join("vault");
        let created = open_or_create_entry(&notes_dir, "Journal", test_date()).unwrap();
        assert_eq!(created.action, JournalEntryAction::Created);
        assert_eq!(created.path, notes_dir.join("Journal/2026/journal.2026-08-06.md"));
        assert!(created.path.is_file());
        fs::write(&created.path, "keep this content").unwrap();
        let opened = open_or_create_entry(&notes_dir, "Journal", test_date()).unwrap();
        assert_eq!(opened.action, JournalEntryAction::Opened);
        assert_eq!(opened.path, created.path);
        assert_eq!(fs::read_to_string(opened.path).unwrap(), "keep this content");
    }

    #[test]
    fn opens_legacy_root_entry_without_moving_it() {
        let workspace = TempWorkspace::new("legacy");
        let notes_dir = workspace.path.join("vault");
        fs::create_dir_all(&notes_dir).unwrap();
        let legacy_path = notes_dir.join("journal.2026-08-06.md");
        fs::write(&legacy_path, "legacy").unwrap();
        let opened = open_or_create_entry(&notes_dir, "Journal", test_date()).unwrap();
        assert_eq!(opened.action, JournalEntryAction::Opened);
        assert_eq!(opened.path, legacy_path);
        assert!(!notes_dir.join("Journal").exists());
        assert_eq!(fs::read_to_string(opened.path).unwrap(), "legacy");
    }

    #[test]
    fn canonical_entry_wins_when_legacy_entry_also_exists() {
        let workspace = TempWorkspace::new("canonical");
        let notes_dir = workspace.path.join("vault");
        let canonical_path = notes_dir.join("Journal/2026/journal.2026-08-06.md");
        fs::create_dir_all(canonical_path.parent().unwrap()).unwrap();
        fs::write(&canonical_path, "canonical").unwrap();
        fs::write(notes_dir.join("journal.2026-08-06.md"), "legacy").unwrap();
        let opened = open_or_create_entry(&notes_dir, "Journal", test_date()).unwrap();
        assert_eq!(opened.action, JournalEntryAction::Opened);
        assert_eq!(opened.path, canonical_path);
        assert_eq!(fs::read_to_string(opened.path).unwrap(), "canonical");
    }

    #[test]
    fn invalid_configuration_cannot_create_outside_the_vault() {
        let workspace = TempWorkspace::new("invalid");
        let notes_dir = workspace.path.join("vault");
        let error = open_or_create_entry(&notes_dir, "../escape", test_date()).unwrap_err();
        assert!(error.contains("journal_dir"));
        assert!(!workspace.path.join("escape").exists());
    }

    #[test]
    fn reports_directory_creation_failures() {
        let workspace = TempWorkspace::new("mkdir-error");
        let notes_dir = workspace.path.join("vault");
        fs::create_dir_all(&notes_dir).unwrap();
        fs::write(notes_dir.join("Journal"), "not a directory").unwrap();
        let error = open_or_create_entry(&notes_dir, "Journal", test_date()).unwrap_err();
        assert!(error.contains("failed to create directory"));
    }
}
