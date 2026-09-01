//! Filesystem-facing vault services for Ekphos.

mod frontmatter;
pub mod journal;

pub use frontmatter::Frontmatter;

use ekphos_core::{FrontmatterSummary, NoteId, NoteMetadata, VaultPath};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_BODY_CACHE_BUDGET: usize = 8 * 1024 * 1024;

/// Text document kinds that Ekphos can open directly from the vault.
///
/// Markdown remains the only kind consumed by note-only services such as full
/// text search and the wikilink graph. Bases and canvases are catalogued here
/// so the application shell can navigate them without pretending their source
/// is Markdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VaultFileKind {
    Markdown,
    Base,
    Canvas,
}

impl VaultFileKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("md") => Some(Self::Markdown),
            Some("base") => Some(Self::Base),
            Some("canvas") => Some(Self::Canvas),
            _ => None,
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Base => "base",
            Self::Canvas => "canvas",
        }
    }

    pub const fn is_markdown(self) -> bool {
        matches!(self, Self::Markdown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFingerprint {
    pub size: u64,
    pub modified_nanos: Option<NonZeroU64>,
}

impl FileFingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self { size: metadata.len(), modified_nanos: metadata.modified().ok().and_then(system_time_nanos) }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogNote {
    pub metadata: NoteMetadata,
    pub kind: VaultFileKind,
    pub modified_time: Option<SystemTime>,
    pub created_time: Option<SystemTime>,
    pub has_frontmatter: bool,
    pub content_start_line: usize,
    pub fingerprint: FileFingerprint,
}

#[derive(Debug, Clone)]
pub struct CatalogFolder {
    pub name: String,
    pub absolute_path: PathBuf,
    pub children: Vec<CatalogEntry>,
}

#[derive(Debug, Clone)]
pub enum CatalogEntry {
    Folder(Box<CatalogFolder>),
    Note(Box<CatalogNote>),
}

#[derive(Debug, Clone, Default)]
pub struct Vault {
    root: PathBuf,
    notes: HashMap<NoteId, VaultRecord>,
}

#[derive(Debug, Clone)]
struct VaultRecord {
    path: VaultPath,
    fingerprint: FileFingerprint,
}

#[derive(Debug)]
pub enum VaultError {
    NotFound(NoteId),
    Changed(NoteId),
    InvalidEncoding(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for VaultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "note {id} is no longer in the vault"),
            Self::Changed(id) => write!(formatter, "note {id} changed while it was being loaded"),
            Self::InvalidEncoding(path) => write!(formatter, "note is not valid UTF-8: {}", path.display()),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for VaultError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl Vault {
    pub fn scan(root: impl AsRef<Path>) -> io::Result<(Self, Vec<CatalogEntry>)> {
        let root = root.as_ref().to_path_buf();
        let mut entries = scan_entries(&root, &root)?;
        assign_unique_note_ids(&mut entries);
        let mut notes = HashMap::new();
        collect_notes(&entries, &mut notes);
        Ok((Self { root, notes }, entries))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn fingerprint(&self, id: NoteId) -> Option<FileFingerprint> {
        self.notes.get(&id).map(|note| note.fingerprint)
    }

    pub fn validate(&self, id: NoteId) -> Result<FileFingerprint, VaultError> {
        let note = self.notes.get(&id).ok_or(VaultError::NotFound(id))?;
        let path = self.root.join(note.path.as_str());
        let current = fs::metadata(&path).map_err(|source| VaultError::Io { path: path.clone(), source }).map(|metadata| FileFingerprint::from_metadata(&metadata))?;
        if current != note.fingerprint {
            return Err(VaultError::Changed(id));
        }
        Ok(current)
    }

    /// Load one note body without retaining it in the vault catalog.
    pub fn load_body(&self, id: NoteId) -> Result<Arc<str>, VaultError> {
        let note = self.notes.get(&id).ok_or(VaultError::NotFound(id))?;
        let path = self.root.join(note.path.as_str());
        self.validate(id)?;
        let bytes = fs::read(&path).map_err(|source| VaultError::Io { path: path.clone(), source })?;
        self.validate(id)?;
        let body = String::from_utf8(bytes).map_err(|_| VaultError::InvalidEncoding(path))?;
        Ok(Arc::<str>::from(body))
    }

    /// Refresh one catalog record after an Ekphos save without rescanning the vault.
    pub fn refresh_note(&mut self, id: NoteId) -> Result<CatalogNote, VaultError> {
        let path = self.root.join(self.notes.get(&id).ok_or(VaultError::NotFound(id))?.path.as_str());
        let mut note = catalog_note(&self.root, &path).map_err(|source| VaultError::Io { path: path.clone(), source })?;
        note.metadata.id = id;
        self.notes.insert(id, VaultRecord { path: note.metadata.path.clone(), fingerprint: note.fingerprint });
        Ok(note)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BodyCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub bytes: usize,
    pub entries: usize,
}

#[derive(Debug)]
struct CachedBody {
    body: Arc<str>,
    fingerprint: FileFingerprint,
}

#[derive(Debug)]
pub struct BodyCache {
    budget: usize,
    bytes: usize,
    entries: HashMap<NoteId, CachedBody>,
    lru: VecDeque<NoteId>,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Default for BodyCache {
    fn default() -> Self {
        Self::new(DEFAULT_BODY_CACHE_BUDGET)
    }
}

impl BodyCache {
    pub fn new(budget: usize) -> Self {
        Self { budget, bytes: 0, entries: HashMap::new(), lru: VecDeque::new(), hits: 0, misses: 0, evictions: 0 }
    }

    /// Return and unpin a cached body, or load exactly one body from the vault.
    /// The returned active body is removed from the inactive LRU.
    pub fn take_or_load(&mut self, vault: &Vault, id: NoteId) -> Result<Arc<str>, VaultError> {
        let fingerprint = vault.validate(id)?;
        if self.entries.get(&id).is_some_and(|entry| entry.fingerprint == fingerprint) {
            self.hits = self.hits.saturating_add(1);
            let entry = self.entries.remove(&id).expect("cache entry checked above");
            self.bytes = self.bytes.saturating_sub(entry.body.len());
            self.lru.retain(|cached_id| *cached_id != id);
            return Ok(entry.body);
        }
        self.invalidate(id);
        self.misses = self.misses.saturating_add(1);
        vault.load_body(id)
    }

    /// Place an inactive body into the byte-weighted LRU.
    pub fn insert(&mut self, id: NoteId, fingerprint: FileFingerprint, body: Arc<str>) {
        self.invalidate(id);
        self.bytes = self.bytes.saturating_add(body.len());
        self.entries.insert(id, CachedBody { body, fingerprint });
        self.lru.push_back(id);
        while self.bytes > self.budget {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(entry.body.len());
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }

    pub fn invalidate(&mut self, id: NoteId) {
        if let Some(entry) = self.entries.remove(&id) {
            self.bytes = self.bytes.saturating_sub(entry.body.len());
        }
        self.lru.retain(|cached_id| *cached_id != id);
    }

    pub fn retain_valid(&mut self, vault: &Vault) {
        let stale: Vec<NoteId> = self.entries.iter().filter_map(|(&id, entry)| (vault.fingerprint(id) != Some(entry.fingerprint)).then_some(id)).collect();
        for id in stale {
            self.invalidate(id);
        }
    }

    pub fn stats(&self) -> BodyCacheStats {
        BodyCacheStats { hits: self.hits, misses: self.misses, evictions: self.evictions, bytes: self.bytes, entries: self.entries.len() }
    }
}

pub fn contains_supported_document(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let entry_path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            return false;
        };
        if file_type.is_symlink() {
            false
        } else if file_type.is_dir() {
            !entry_path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) && contains_supported_document(&entry_path)
        } else {
            file_type.is_file() && VaultFileKind::from_path(&entry_path).is_some()
        }
    })
}

pub fn contains_markdown(path: &Path) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let Ok(file_type) = entry.file_type() else {
            return false;
        };
        let entry_path = entry.path();
        if file_type.is_symlink() {
            false
        } else if file_type.is_dir() {
            !entry_path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) && contains_markdown(&entry_path)
        } else {
            file_type.is_file() && entry_path.extension().is_some_and(|extension| extension == "md")
        }
    })
}

/// Compatibility entry point for callers that only need the metadata tree.
pub fn scan(root: &Path) -> io::Result<Vec<CatalogEntry>> {
    Vault::scan(root).map(|(_, entries)| entries)
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
fn collect_notes(entries: &[CatalogEntry], notes: &mut HashMap<NoteId, VaultRecord>) {
    for entry in entries {
        match entry {
            CatalogEntry::Folder(folder) => collect_notes(&folder.children, notes),
            CatalogEntry::Note(note) => {
                notes.insert(note.metadata.id, VaultRecord { path: note.metadata.path.clone(), fingerprint: note.fingerprint });
            }
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
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')) {
                continue;
            }
            let children = scan_entries(root, &path).unwrap_or_default();
            items.push(CatalogEntry::Folder(Box::new(CatalogFolder { name: path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(), absolute_path: path, children })));
        } else if file_type.is_file() && VaultFileKind::from_path(&path).is_some() {
            if let Ok(note) = catalog_note(root, &path) {
                items.push(CatalogEntry::Note(Box::new(note)));
            }
        }
    }
    Ok(items)
}
fn catalog_note(root: &Path, path: &Path) -> io::Result<CatalogNote> {
    let kind = VaultFileKind::from_path(path).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsupported vault document"))?;
    let filesystem = fs::metadata(path)?;
    let relative = path.strip_prefix(root).map_err(io::Error::other)?;
    let vault_path = VaultPath::from_relative_path(relative).map_err(io::Error::other)?;
    let (frontmatter, content_start_line) = if kind.is_markdown() {
        match read_frontmatter(path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => (None, 0),
            Err(error) => return Err(error),
        }
    } else {
        (None, 0)
    };
    let modified_time = filesystem.modified().ok();
    let created_time = filesystem.created().ok();
    let has_frontmatter = frontmatter.is_some();
    let summary = frontmatter.map(|value| FrontmatterSummary { title: value.title, tags: value.tags, date: value.date }).unwrap_or_default();
    let title = path.file_stem().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
    Ok(CatalogNote {
        kind,
        metadata: NoteMetadata { id: NoteId::for_path(&vault_path), path: vault_path, title, file_size: filesystem.len(), modified_unix_seconds: unix_seconds(modified_time), created_unix_seconds: unix_seconds(created_time), frontmatter: summary },
        modified_time,
        created_time,
        has_frontmatter,
        content_start_line,
        fingerprint: FileFingerprint::from_metadata(&filesystem),
    })
}

/// Read no body bytes for ordinary notes and only the streamed frontmatter
/// prefix for frontmatter-bearing notes. An unclosed prefix is consumed to EOF
/// to preserve the legacy `(None, 0)` result without retaining a second body.
fn read_frontmatter(path: &Path) -> io::Result<(Option<Frontmatter>, usize)> {
    let mut probe = File::open(path)?;
    let mut prefix = [0u8; 5];
    let mut prefix_len = 0;
    while prefix_len < prefix.len() {
        let read = probe.read(&mut prefix[prefix_len..])?;
        if read == 0 {
            break;
        }
        prefix_len += read;
    }
    let prefix = &prefix[..prefix_len];
    if prefix != b"---" && !prefix.starts_with(b"---\n") && !prefix.starts_with(b"---\r\n") {
        return Ok((None, 0));
    }
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let Some(first) = lines.next() else {
        return Ok((None, 0));
    };
    if first? != "---" {
        return Ok((None, 0));
    }
    let mut yaml = String::new();
    let mut end_index = 1usize;
    for line in lines {
        let line = line?;
        if line == "---" {
            let frontmatter = serde_yaml::from_str::<Frontmatter>(&yaml).ok();
            return Ok((frontmatter, end_index + 1));
        }
        if !yaml.is_empty() {
            yaml.push('\n');
        }
        yaml.push_str(&line);
        end_index += 1;
    }
    Ok((None, 0))
}

static SAVE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Atomically replace a note with a flushed sibling temporary file.
pub fn save_note(path: &Path, content: &str) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "note has no parent directory"))?;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("note.md");
    let permissions = fs::metadata(path).ok().map(|metadata| metadata.permissions());
    let mut last_error = None;
    for _ in 0..16 {
        let counter = SAVE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{name}.ekphos-{}-{counter}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Ok(mut file) => {
                let result = (|| {
                    if let Some(permissions) = permissions.clone() {
                        file.set_permissions(permissions)?;
                    }
                    file.write_all(content.as_bytes())?;
                    file.flush()?;
                    file.sync_all()?;
                    fs::rename(&temporary, path)?;
                    if let Ok(directory) = File::open(parent) {
                        let _ = directory.sync_all();
                    }
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temporary);
                }
                return result;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "could not allocate temporary note path")))
}
fn unix_seconds(time: Option<SystemTime>) -> Option<u64> {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_secs()))
}
fn system_time_nanos(time: SystemTime) -> Option<NonZeroU64> {
    time.duration_since(UNIX_EPOCH).ok().and_then(|duration| u64::try_from(duration.as_nanos()).ok()).and_then(NonZeroU64::new)
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
    fn first_note(entries: &[CatalogEntry]) -> &CatalogNote {
        let CatalogEntry::Folder(folder) = &entries[0] else {
            panic!("expected folder");
        };
        let CatalogEntry::Note(note) = &folder.children[0] else {
            panic!("expected note");
        };
        note
    }

    #[test]
    fn phase3_metadata_layout_is_compact() {
        assert!(std::mem::size_of::<FileFingerprint>() <= 16);
        assert!(std::mem::size_of::<VaultRecord>() <= 40);
        assert!(std::mem::size_of::<CatalogEntry>() <= 16);
        assert!(std::mem::size_of::<CatalogNote>() <= 256);
    }

    #[test]
    fn catalog_is_metadata_only_and_body_loading_is_explicit() {
        let root = fixture();
        let (vault, entries) = Vault::scan(&root).unwrap();
        let note = first_note(&entries);
        assert_eq!(note.metadata.path.as_str(), "folder/note.md");
        assert_eq!(note.metadata.frontmatter.tags, ["one"]);
        assert_eq!(&*vault.load_body(note.metadata.id).unwrap(), "---\ntags: [one]\n---\n# Note");
        let first_id = note.metadata.id;
        let (_, rescanned) = Vault::scan(&root).unwrap();
        assert_eq!(first_note(&rescanned).metadata.id, first_id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn catalog_recognizes_markdown_bases_and_json_canvas_documents() {
        let root = fixture();
        fs::write(root.join("library.base"), "views:\n  - type: table\n").unwrap();
        fs::write(root.join("board.canvas"), r#"{"nodes":[],"edges":[]}"#).unwrap();
        fs::write(root.join("ignored.txt"), "not a vault document").unwrap();
        let (_, entries) = Vault::scan(&root).unwrap();
        fn collect(entries: &[CatalogEntry], kinds: &mut Vec<VaultFileKind>) {
            for entry in entries {
                match entry {
                    CatalogEntry::Folder(folder) => collect(&folder.children, kinds),
                    CatalogEntry::Note(note) => kinds.push(note.kind),
                }
            }
        }
        let mut kinds = Vec::new();
        collect(&entries, &mut kinds);
        kinds.sort_by_key(|kind| kind.extension());
        assert_eq!(kinds, vec![VaultFileKind::Base, VaultFileKind::Canvas, VaultFileKind::Markdown]);
        assert!(contains_supported_document(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn frontmatter_prefix_matches_full_document_parser() {
        let root = fixture();
        let cases = [("plain.md", "# Plain\nbody"), ("valid.md", "---\ntitle: Hello\ntags: [a, b]\n---\n# Body\nrest"), ("invalid.md", "---\n: invalid yaml [\n---\nbody"), ("unclosed.md", "---\ntitle: Never closes\nbody")];
        for (name, content) in cases {
            let path = root.join(name);
            fs::write(&path, content).unwrap();
            let expected = Frontmatter::parse(content);
            let actual = read_frontmatter(&path).unwrap();
            assert_eq!(actual.1, expected.1, "{name}");
            assert_eq!(actual.0.as_ref().and_then(|value| value.title.as_deref()), expected.0.as_ref().and_then(|value| value.title.as_deref()), "{name}");
            assert_eq!(actual.0.as_ref().map(|value| &value.tags), expected.0.as_ref().map(|value| &value.tags), "{name}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn body_cache_is_byte_weighted_and_reports_activity() {
        let root = fixture();
        fs::write(root.join("a.md"), "aaaa").unwrap();
        fs::write(root.join("b.md"), "bbbb").unwrap();
        fs::write(root.join("c.md"), "cccc").unwrap();
        let (vault, entries) = Vault::scan(&root).unwrap();
        let ids: Vec<NoteId> = entries
            .iter()
            .filter_map(|entry| match entry {
                CatalogEntry::Note(note) => Some(note.metadata.id),
                CatalogEntry::Folder(_) => None,
            })
            .collect();
        let mut cache = BodyCache::new(8);
        let a = cache.take_or_load(&vault, ids[0]).unwrap();
        cache.insert(ids[0], vault.fingerprint(ids[0]).unwrap(), a);
        let cached = cache.take_or_load(&vault, ids[0]).unwrap();
        cache.insert(ids[0], vault.fingerprint(ids[0]).unwrap(), cached);
        let b = cache.take_or_load(&vault, ids[1]).unwrap();
        cache.insert(ids[1], vault.fingerprint(ids[1]).unwrap(), b);
        let c = cache.take_or_load(&vault, ids[2]).unwrap();
        cache.insert(ids[2], vault.fingerprint(ids[2]).unwrap(), c);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 3);
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.bytes, 8);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_body_can_be_active_but_is_not_retained_in_the_inactive_cache() {
        let root = fixture();
        let oversized = "x".repeat(64);
        fs::write(root.join("large.md"), &oversized).unwrap();
        let (vault, entries) = Vault::scan(&root).unwrap();
        let id = entries
            .iter()
            .find_map(|entry| match entry {
                CatalogEntry::Note(note) if note.metadata.path.as_str() == "large.md" => Some(note.metadata.id),
                _ => None,
            })
            .unwrap();
        let mut cache = BodyCache::new(8);
        let active = cache.take_or_load(&vault, id).unwrap();
        assert_eq!(&*active, oversized);
        assert_eq!(cache.stats().bytes, 0);
        cache.insert(id, vault.fingerprint(id).unwrap(), active);
        let stats = cache.stats();
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.bytes, 0);
        assert_eq!(stats.evictions, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn changed_files_are_rejected_and_atomic_save_preserves_permissions() {
        let root = fixture();
        let (vault, entries) = Vault::scan(&root).unwrap();
        let note = first_note(&entries);
        fs::write(root.join(note.metadata.path.as_str()), "changed size").unwrap();
        assert!(matches!(vault.load_body(note.metadata.id), Err(VaultError::Changed(_))));
        let path = root.join("save.md");
        fs::write(&path, "old").unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        save_note(&path, "new body").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new body");
        assert_eq!(fs::metadata(&path).unwrap().permissions().readonly(), permissions.readonly());
        assert!(!fs::read_dir(&root).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().contains(".ekphos-")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn killed_note_writer_cannot_replace_the_live_note_with_a_partial_sibling() {
        let root = fixture();
        let path = root.join("transactional.md");
        fs::write(&path, "original complete note").unwrap();
        let interrupted = root.join(format!(".transactional.md.ekphos-{}-killed.tmp", std::process::id()));
        fs::write(&interrupted, "partial replacement").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "original complete note");

        save_note(&path, "next complete note").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "next complete note");
        assert_eq!(fs::read_to_string(&interrupted).unwrap(), "partial replacement");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_utf8_has_an_explicit_error() {
        let root = fixture();
        let path = root.join("invalid.md");
        fs::write(&path, [0xff, 0xfe]).unwrap();
        let (vault, entries) = Vault::scan(&root).unwrap();
        let id = entries
            .iter()
            .find_map(|entry| match entry {
                CatalogEntry::Note(note) if note.metadata.path.as_str() == "invalid.md" => Some(note.metadata.id),
                _ => None,
            })
            .unwrap();
        assert!(matches!(vault.load_body(id), Err(VaultError::InvalidEncoding(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn scans_ignore_file_directory_and_cyclic_symlinks() {
        use std::os::unix::fs::symlink;

        let root = fixture();
        let external = root.with_file_name(format!("{}-external", root.file_name().unwrap().to_string_lossy()));
        fs::create_dir_all(&external).unwrap();
        fs::write(external.join("outside.md"), "# Outside").unwrap();
        symlink(&external, root.join("external-link")).unwrap();
        symlink(root.join("folder/note.md"), root.join("linked-note.md")).unwrap();
        symlink(&root, root.join("cycle")).unwrap();

        let (_, entries) = Vault::scan(&root).unwrap();
        let mut paths = Vec::new();
        collect_note_paths(&entries, &mut paths);
        assert_eq!(paths.iter().map(VaultPath::as_str).collect::<Vec<_>>(), ["folder/note.md"]);

        let symlink_only = root.join("symlink-only");
        fs::create_dir_all(&symlink_only).unwrap();
        symlink(&external, symlink_only.join("external")).unwrap();
        assert!(!contains_markdown(&symlink_only));

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(external).unwrap();
    }
}
