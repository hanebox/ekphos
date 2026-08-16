//! Stable, platform-independent contracts shared by Ekphos subsystems.

pub mod markdown;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Component, Path};

/// Stable note identity used across subsystem boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct NoteId(u32);

impl NoteId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// Temporary adapter for code that still addresses the catalog by index.
    pub fn from_index(index: usize) -> Result<Self, CoreError> {
        u32::try_from(index).map(Self).map_err(|_| CoreError::NoteIdOverflow(index))
    }

    /// Temporary adapter for UI collections that still store vector positions.
    pub const fn as_index(self) -> usize {
        self.0 as usize
    }

    /// Deterministic identity for a normalized vault path.
    pub fn for_path(path: &VaultPath) -> Self {
        let mut hash = 0x811c_9dc5u32;
        for byte in path.as_str().bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
        Self(hash)
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A normalized, UTF-8, vault-relative path using `/` separators.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VaultPath(String);

impl VaultPath {
    pub fn new(path: impl AsRef<str>) -> Result<Self, CoreError> {
        let normalized = path.as_ref().replace('\\', "/");
        let candidate = Path::new(&normalized);
        if normalized.is_empty() || candidate.is_absolute() || candidate.components().any(|component| !matches!(component, Component::Normal(_))) {
            return Err(CoreError::InvalidVaultPath(normalized));
        }
        Ok(Self(normalized))
    }

    pub fn from_relative_path(path: &Path) -> Result<Self, CoreError> {
        Self::new(path.to_string_lossy())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VaultPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontmatterSummary {
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub id: NoteId,
    pub path: VaultPath,
    pub title: String,
    pub file_size: u64,
    pub modified_unix_seconds: Option<u64>,
    pub created_unix_seconds: Option<u64>,
    pub frontmatter: FrontmatterSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    InvalidVaultPath(String),
    NoteIdOverflow(usize),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVaultPath(path) => write!(formatter, "invalid vault-relative path: {path}"),
            Self::NoteIdOverflow(index) => write!(formatter, "catalog index {index} does not fit in NoteId"),
        }
    }
}

impl std::error::Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_paths_are_normalized_and_confined() {
        assert_eq!(VaultPath::new("folder\\note.md").unwrap().as_str(), "folder/note.md");
        for invalid in ["", ".", "..", "../note.md", "/note.md", "folder/../note.md"] {
            assert!(VaultPath::new(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn note_identity_is_stable_for_a_path() {
        let path = VaultPath::new("folder/note.md").unwrap();
        assert_eq!(NoteId::for_path(&path), NoteId::for_path(&path));
        assert_ne!(NoteId::for_path(&path), NoteId::for_path(&VaultPath::new("other.md").unwrap()));
    }
}
