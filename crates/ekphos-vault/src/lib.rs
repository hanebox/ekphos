//! Filesystem-facing vault services for Ekphos.

mod frontmatter;
pub mod journal;

pub use frontmatter::Frontmatter;

use ekphos_core::{FrontmatterSummary, NoteId, NoteMetadata, VaultPath};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LoadedNote {
    pub metadata: NoteMetadata,
    pub content: String,
    pub absolute_path: PathBuf,
    pub modified_time: Option<SystemTime>,
    pub created_time: Option<SystemTime>,
    pub frontmatter: Option<Frontmatter>,
    pub content_start_line: usize,
}

#[derive(Debug, Clone)]
pub struct CatalogFolder {
    pub name: String,
    pub absolute_path: PathBuf,
    pub children: Vec<CatalogEntry>,
}

#[derive(Debug, Clone)]
pub enum CatalogEntry {
    Folder(CatalogFolder),
    Note(Box<LoadedNote>),
}

pub fn contains_markdown(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            !entry_path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) && contains_markdown(&entry_path)
        } else {
            entry_path.extension().is_some_and(|extension| extension == "md")
        }
    })
}

pub fn scan(root: &Path) -> io::Result<Vec<CatalogEntry>> {
    let mut entries = scan_entries(root, root)?;
    assign_unique_note_ids(&mut entries);
    Ok(entries)
}

fn assign_unique_note_ids(entries: &mut [CatalogEntry]) {
    let mut paths = Vec::new();
    collect_note_paths(entries, &mut paths);
    paths.sort_unstable();
    paths.dedup();

    let mut occupied = HashSet::with_capacity(paths.len());
    let mut assignments = HashMap::with_capacity(paths.len());
    for path in paths {
        let mut candidate = NoteId::for_path(&path).get();
        while !occupied.insert(candidate) {
            candidate = candidate.wrapping_add(1);
        }
        assignments.insert(path, NoteId::new(candidate));
    }
    apply_note_ids(entries, &assignments);
}

fn collect_note_paths(entries: &[CatalogEntry], paths: &mut Vec<VaultPath>) {
    for entry in entries {
        match entry {
            CatalogEntry::Folder(folder) => collect_note_paths(&folder.children, paths),
            CatalogEntry::Note(note) => paths.push(note.metadata.path.clone()),
        }
    }
}

fn apply_note_ids(entries: &mut [CatalogEntry], assignments: &HashMap<VaultPath, NoteId>) {
    for entry in entries {
        match entry {
            CatalogEntry::Folder(folder) => apply_note_ids(&mut folder.children, assignments),
            CatalogEntry::Note(note) => {
                if let Some(id) = assignments.get(&note.metadata.path) {
                    note.metadata.id = *id;
                }
            }
        }
    }
}

fn scan_entries(root: &Path, directory: &Path) -> io::Result<Vec<CatalogEntry>> {
    let mut items = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
                continue;
            }
            let children = scan_entries(root, &path).unwrap_or_default();
            items.push(CatalogEntry::Folder(CatalogFolder {
                name: path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
                absolute_path: path,
                children,
            }));
        } else if path.extension().is_some_and(|extension| extension == "md") {
            if let Ok(note) = load_note(root, &path) {
                items.push(CatalogEntry::Note(Box::new(note)));
            }
        }
    }
    Ok(items)
}

#[cfg(test)]
fn contains_note(entries: &[CatalogEntry]) -> bool {
    entries.iter().any(|entry| match entry {
        CatalogEntry::Note(_) => true,
        CatalogEntry::Folder(folder) => contains_note(&folder.children),
    })
}

pub fn load_note(root: &Path, path: &Path) -> io::Result<LoadedNote> {
    let content = fs::read_to_string(path)?;
    let filesystem = fs::metadata(path)?;
    let relative = path.strip_prefix(root).map_err(io::Error::other)?;
    let vault_path = VaultPath::from_relative_path(relative).map_err(io::Error::other)?;
    let (frontmatter, content_start_line) = Frontmatter::parse(&content);
    let modified_time = filesystem.modified().ok();
    let created_time = filesystem.created().ok();
    let summary = frontmatter
        .as_ref()
        .map(|value| FrontmatterSummary {
            title: value.title.clone(),
            tags: value.tags.clone(),
            date: value.date.clone(),
        })
        .unwrap_or_default();
    let title = path.file_stem().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    Ok(LoadedNote {
        metadata: NoteMetadata {
            id: NoteId::for_path(&vault_path),
            path: vault_path,
            title,
            file_size: filesystem.len(),
            modified_unix_seconds: unix_seconds(modified_time),
            created_unix_seconds: unix_seconds(created_time),
            frontmatter: summary,
        },
        content,
        absolute_path: path.to_path_buf(),
        modified_time,
        created_time,
        frontmatter,
        content_start_line,
    })
}

pub fn save_note(path: &Path, content: &str) -> io::Result<()> {
    fs::write(path, content)
}

fn unix_seconds(time: Option<SystemTime>) -> Option<u64> {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> PathBuf {
        let id = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("ekphos-vault-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("folder")).unwrap();
        fs::write(root.join("folder/note.md"), "---\ntags: [one]\n---\n# Note").unwrap();
        root
    }

    #[test]
    fn catalog_loads_metadata_and_body_without_platform_dependencies() {
        let root = fixture();
        let entries = scan(&root).unwrap();
        assert!(contains_note(&entries));
        let CatalogEntry::Folder(folder) = &entries[0] else {
            panic!("expected folder");
        };
        let CatalogEntry::Note(note) = &folder.children[0] else {
            panic!("expected note");
        };
        assert_eq!(note.metadata.path.as_str(), "folder/note.md");
        assert_eq!(note.metadata.frontmatter.tags, ["one"]);
        assert!(note.content.contains("# Note"));
        let first_id = note.metadata.id;
        let rescanned = scan(&root).unwrap();
        let CatalogEntry::Folder(folder) = &rescanned[0] else {
            panic!("expected folder");
        };
        let CatalogEntry::Note(note) = &folder.children[0] else {
            panic!("expected note");
        };
        assert_eq!(note.metadata.id, first_id);
        let _ = fs::remove_dir_all(root);
    }
}
