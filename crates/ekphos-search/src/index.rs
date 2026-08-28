use bincode::Options;
use ekphos_core::NoteId;
use memmap2::{Mmap, MmapOptions};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub const INDEX_VERSION: u32 = 5;
const INDEX_MAGIC: [u8; 8] = *b"EKPHSRCH";
const ENDIAN_MARKER: u32 = 0x0102_0304;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
static CACHE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchFileFingerprint {
    pub size: u64,
    /// Nanoseconds since the Unix epoch, or zero if the filesystem does not
    /// expose a modification timestamp.
    pub modified_nanos: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchSource {
    pub note_id: NoteId,
    pub relative_path: Box<str>,
    pub absolute_path: PathBuf,
    pub fingerprint: SearchFileFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedFile {
    pub note_id: u32,
    pub relative_path: Box<str>,
    pub fingerprint: SearchFileFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchCacheHeader {
    pub magic: [u8; 8],
    pub format_version: u32,
    pub vault_identity: u64,
    pub endian_marker: u32,
    pub pointer_width: u8,
    pub note_id_width: u8,
    pub line_number_width: u8,
    pub reserved: u8,
    pub files: Vec<CachedFile>,
}

impl Default for SearchCacheHeader {
    fn default() -> Self {
        Self { magic: INDEX_MAGIC, format_version: INDEX_VERSION, vault_identity: 0, endian_marker: ENDIAN_MARKER, pointer_width: usize::BITS as u8, note_id_width: u32::BITS as u8, line_number_width: u32::BITS as u8, reserved: 0, files: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(C)]
pub struct PackedPosting {
    note_id: u32,
    line_number: u32,
}

impl PackedPosting {
    pub const fn note_id(self) -> NoteId {
        NoteId::new(self.note_id)
    }

    pub const fn line_number(self) -> u32 {
        self.line_number
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TermEntry {
    term: Box<str>,
    postings_start: u32,
    postings_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    header: SearchCacheHeader,
    terms: Vec<TermEntry>,
    posting_count: u32,
    postings_checksum: u64,
}

#[derive(Debug, Clone)]
enum PostingStorage {
    Heap(Vec<PackedPosting>),
    Mapped { mmap: Arc<Mmap>, byte_offset: usize, len: usize },
}

#[derive(Debug, Clone)]
pub struct SearchIndex {
    pub header: SearchCacheHeader,
    terms: Vec<TermEntry>,
    postings: PostingStorage,
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self { header: SearchCacheHeader::default(), terms: Vec::new(), postings: PostingStorage::Heap(Vec::new()) }
    }
}

#[derive(Clone, Copy)]
pub struct PostingList<'a> {
    storage: &'a PostingStorage,
    start: usize,
    len: usize,
}

impl<'a> PostingList<'a> {
    pub const fn len(self) -> usize {
        self.len
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    pub fn get(self, index: usize) -> Option<PackedPosting> {
        (index < self.len).then(|| self.storage.get(self.start + index))
    }

    pub const fn iter(self) -> PostingIter<'a> {
        PostingIter { list: self, position: 0 }
    }
}

pub struct PostingIter<'a> {
    list: PostingList<'a>,
    position: usize,
}

impl Iterator for PostingIter<'_> {
    type Item = PackedPosting;
    fn next(&mut self) -> Option<Self::Item> {
        let posting = self.list.get(self.position)?;
        self.position += 1;
        Some(posting)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.list.len.saturating_sub(self.position);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PostingIter<'_> {}

impl PostingStorage {
    fn len(&self) -> usize {
        match self {
            Self::Heap(postings) => postings.len(),
            Self::Mapped { len, .. } => *len,
        }
    }
    fn get(&self, index: usize) -> PackedPosting {
        match self {
            Self::Heap(postings) => postings[index],
            Self::Mapped { mmap, byte_offset, len } => {
                debug_assert!(index < *len);
                let start = byte_offset + index * std::mem::size_of::<PackedPosting>();
                let note_id = u32::from_le_bytes(mmap[start..start + 4].try_into().expect("validated posting ID"));
                let line_number = u32::from_le_bytes(mmap[start + 4..start + 8].try_into().expect("validated posting line"));
                PackedPosting { note_id, line_number }
            }
        }
    }
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Heap(postings) => postings.capacity() * std::mem::size_of::<PackedPosting>(),
            Self::Mapped { .. } => 0,
        }
    }
    fn mapped_bytes(&self) -> usize {
        match self {
            Self::Heap(_) => 0,
            Self::Mapped { mmap, .. } => mmap.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchIndexError {
    LineNumberOverflow { note_id: NoteId, line_number: usize },
    PostingTableOverflow,
}

impl std::fmt::Display for SearchIndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LineNumberOverflow { note_id, line_number } => {
                write!(formatter, "line {line_number} in note {note_id} does not fit in the search cache")
            }
            Self::PostingTableOverflow => formatter.write_str("search posting table exceeds the fixed-width cache format"),
        }
    }
}

impl std::error::Error for SearchIndexError {}

/// Get the platform cache path for a vault search index.
pub fn get_index_path(notes_dir: &Path) -> PathBuf {
    let cache_base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache"));
    get_index_path_in(&cache_base, notes_dir)
}

pub fn get_index_path_in(cache_base: &Path, notes_dir: &Path) -> PathBuf {
    let identity = vault_identity(notes_dir);
    cache_base.join("ekphos").join(format!("{identity:016x}")).join("search_index.bin")
}

/// A stable identity over the normalized vault path. Cache validation also
/// compares every file fingerprint, so an identity collision cannot install a
/// stale index.
pub fn vault_identity(notes_dir: &Path) -> u64 {
    let normalized = notes_dir.to_string_lossy().replace('\\', "/");
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn load_index(path: &Path) -> Option<SearchIndex> {
    let file = File::open(path).ok()?;
    let file_len = usize::try_from(file.metadata().ok()?.len()).ok()?;
    if file_len > MAX_CACHE_BYTES as usize || file_len < 16 {
        return None;
    }
    let mmap = unsafe { MmapOptions::new().map(&file).ok()? };
    let (index, expected_checksum) = index_from_mapping(Arc::new(mmap))?;
    let (byte_offset, posting_bytes) = match &index.postings {
        PostingStorage::Mapped { byte_offset, len, .. } => (*byte_offset, len.checked_mul(std::mem::size_of::<PackedPosting>())?),
        PostingStorage::Heap(_) => return None,
    };
    if checksum_file_range(&file, byte_offset, posting_bytes).ok()? != expected_checksum {
        return None;
    }
    index.validate().then_some(index)
}

/// Benchmark-only heap loader used to compare the exact same cache tables with
/// their production read-only mapping.
#[doc(hidden)]
pub fn load_index_heap(path: &Path) -> Option<SearchIndex> {
    let mut index = load_index(path)?;
    let postings: Vec<_> = (0..index.posting_count()).map(|position| index.postings.get(position)).collect();
    index.postings = PostingStorage::Heap(postings);
    Some(index)
}

pub fn load_index_for(path: &Path, notes_dir: &Path, sources: &[SearchSource]) -> Option<SearchIndex> {
    let index = load_index(path)?;
    (index.header.vault_identity == vault_identity(notes_dir) && index.header.files == cached_files(sources)).then_some(index)
}

/// Transactionally replace a cache file. A crash can leave a harmless sibling
/// temporary file, but never a partially written live cache.
pub fn save_index(index: &SearchIndex, path: &Path) -> io::Result<()> {
    if !index.validate() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid search index"));
    }
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "search cache has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("search_index.bin");
    let mut last_error = None;
    for _ in 0..16 {
        let counter = CACHE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(".{name}.ekphos-{}-{counter}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Ok(file) => {
                let result = (|| {
                    let mut writer = BufWriter::new(file);
                    write_index(&mut writer, index)?;
                    writer.flush()?;
                    writer.get_ref().sync_all()?;
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
    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "could not allocate search cache temporary path")))
}

impl SearchIndex {
    pub fn build_from_loader<F>(notes_dir: &Path, sources: &[SearchSource], mut load: F) -> Result<Self, SearchIndexError>
    where
        F: FnMut(&SearchSource) -> Option<Arc<str>>,
    {
        let mut terms = BTreeMap::<String, Vec<PackedPosting>>::new();
        for source in sources {
            if let Some(body) = load(source) {
                index_body(&mut terms, source.note_id, &body)?;
            }
        }
        Self::from_terms(notes_dir, sources, terms)
    }

    /// Incrementally retain unchanged postings, remove deleted/changed IDs, and
    /// stream only added or changed bodies through the loader.
    pub fn update_from_loader<F>(&self, notes_dir: &Path, sources: &[SearchSource], mut load: F) -> Result<Self, SearchIndexError>
    where
        F: FnMut(&SearchSource) -> Option<Arc<str>>,
    {
        if self.header.vault_identity != vault_identity(notes_dir) {
            return Self::build_from_loader(notes_dir, sources, load);
        }
        let current_files = cached_files(sources);
        let unchanged_ids: HashSet<u32> = current_files.iter().filter_map(|current| self.header.files.iter().any(|cached| cached == current).then_some(current.note_id)).collect();
        let mut terms = self.to_term_map();
        for postings in terms.values_mut() {
            postings.retain(|posting| unchanged_ids.contains(&posting.note_id));
        }
        terms.retain(|_, postings| !postings.is_empty());
        for source in sources {
            if !unchanged_ids.contains(&source.note_id.get()) {
                if let Some(body) = load(source) {
                    index_body(&mut terms, source.note_id, &body)?;
                }
            }
        }
        Self::from_terms(notes_dir, sources, terms)
    }

    pub fn matches_sources(&self, notes_dir: &Path, sources: &[SearchSource]) -> bool {
        self.validate() && self.header.vault_identity == vault_identity(notes_dir) && self.header.files == cached_files(sources)
    }

    pub fn postings_for_exact(&self, term: &str) -> PostingList<'_> {
        let Ok(index) = self.terms.binary_search_by(|entry| entry.term.as_ref().cmp(term)) else {
            return PostingList { storage: &self.postings, start: 0, len: 0 };
        };
        self.postings_for_entry(&self.terms[index])
    }

    /// Deterministically walk prefix terms in lexical order and return their
    /// contiguous posting slices.
    pub fn postings_for_prefix<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (&'a str, PostingList<'a>)> + 'a {
        let start = self.terms.partition_point(|entry| entry.term.as_ref() < prefix);
        self.terms[start..].iter().take_while(move |entry| entry.term.starts_with(prefix)).map(|entry| (entry.term.as_ref(), self.postings_for_entry(entry)))
    }

    pub fn term_count(&self) -> usize {
        self.terms.len()
    }

    pub fn posting_count(&self) -> usize {
        self.postings.len()
    }

    pub fn retained_bytes(&self) -> usize {
        self.header.files.capacity() * std::mem::size_of::<CachedFile>()
            + self.header.files.iter().map(|file| file.relative_path.len()).sum::<usize>()
            + self.terms.capacity() * std::mem::size_of::<TermEntry>()
            + self.terms.iter().map(|entry| entry.term.len()).sum::<usize>()
            + self.postings.heap_bytes()
            + self.postings.mapped_bytes()
    }

    pub fn heap_posting_bytes(&self) -> usize {
        self.postings.heap_bytes()
    }

    pub fn mapped_cache_bytes(&self) -> usize {
        self.postings.mapped_bytes()
    }
    fn from_terms(notes_dir: &Path, sources: &[SearchSource], mut terms: BTreeMap<String, Vec<PackedPosting>>) -> Result<Self, SearchIndexError> {
        let mut dictionary = Vec::with_capacity(terms.len());
        let posting_count = terms.values().map(Vec::len).sum();
        let mut contiguous = Vec::with_capacity(posting_count);
        for (term, postings) in &mut terms {
            postings.sort_unstable();
            postings.dedup();
            let postings_start = u32::try_from(contiguous.len()).map_err(|_| SearchIndexError::PostingTableOverflow)?;
            let postings_len = u32::try_from(postings.len()).map_err(|_| SearchIndexError::PostingTableOverflow)?;
            contiguous.extend_from_slice(postings);
            dictionary.push(TermEntry { term: term.clone().into_boxed_str(), postings_start, postings_len });
        }
        u32::try_from(contiguous.len()).map_err(|_| SearchIndexError::PostingTableOverflow)?;
        let index = Self { header: SearchCacheHeader { vault_identity: vault_identity(notes_dir), files: cached_files(sources), ..SearchCacheHeader::default() }, terms: dictionary, postings: PostingStorage::Heap(contiguous) };
        debug_assert!(index.validate());
        Ok(index)
    }
    fn to_term_map(&self) -> BTreeMap<String, Vec<PackedPosting>> {
        self.terms.iter().map(|entry| (entry.term.to_string(), self.postings_for_entry(entry).iter().collect())).collect()
    }
    fn postings_for_entry(&self, entry: &TermEntry) -> PostingList<'_> {
        let start = entry.postings_start as usize;
        PostingList { storage: &self.postings, start, len: entry.postings_len as usize }
    }
    fn validate(&self) -> bool {
        if self.header.magic != INDEX_MAGIC
            || self.header.format_version != INDEX_VERSION
            || self.header.endian_marker != ENDIAN_MARKER
            || self.header.pointer_width != usize::BITS as u8
            || self.header.note_id_width != u32::BITS as u8
            || self.header.line_number_width != u32::BITS as u8
            || !self.header.files.windows(2).all(|pair| pair[0].relative_path < pair[1].relative_path)
            || !self.terms.windows(2).all(|pair| pair[0].term < pair[1].term)
        {
            return false;
        }
        let mut expected_start = 0usize;
        for entry in &self.terms {
            if entry.term.is_empty() || entry.postings_start as usize != expected_start {
                return false;
            }
            let Some(end) = expected_start.checked_add(entry.postings_len as usize) else {
                return false;
            };
            if end > self.postings.len() {
                return false;
            }
            if matches!(self.postings, PostingStorage::Heap(_)) {
                let mut postings = PostingList { storage: &self.postings, start: expected_start, len: end - expected_start }.iter();
                if let Some(mut previous) = postings.next() {
                    for posting in postings {
                        if previous >= posting {
                            return false;
                        }
                        previous = posting;
                    }
                }
            }
            expected_start = end;
        }
        expected_start == self.postings.len()
    }
}
fn cached_files(sources: &[SearchSource]) -> Vec<CachedFile> {
    let mut files: Vec<_> = sources.iter().map(|source| CachedFile { note_id: source.note_id.get(), relative_path: source.relative_path.clone(), fingerprint: source.fingerprint }).collect();
    files.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
    files
}
fn index_from_mapping(mmap: Arc<Mmap>) -> Option<(SearchIndex, u64)> {
    if mmap.get(..8)? != INDEX_MAGIC {
        return None;
    }
    let metadata_len = usize::try_from(u64::from_le_bytes(mmap.get(8..16)?.try_into().ok()?)).ok()?;
    let metadata_end = 16usize.checked_add(metadata_len)?;
    let mut metadata: CacheMetadata = bincode_options().deserialize(mmap.get(16..metadata_end)?).ok()?;
    let posting_count = metadata.posting_count as usize;
    let posting_bytes = posting_count.checked_mul(std::mem::size_of::<PackedPosting>())?;
    if metadata_end.checked_add(posting_bytes)? != mmap.len() {
        return None;
    }
    metadata.header.files.shrink_to_fit();
    metadata.terms.shrink_to_fit();
    Some((SearchIndex { header: metadata.header, terms: metadata.terms, postings: PostingStorage::Mapped { mmap, byte_offset: metadata_end, len: posting_count } }, metadata.postings_checksum))
}
fn write_index(writer: &mut impl Write, index: &SearchIndex) -> io::Result<()> {
    let posting_count = u32::try_from(index.posting_count()).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "search posting table is too large"))?;
    let metadata = CacheMetadata { header: index.header.clone(), terms: index.terms.clone(), posting_count, postings_checksum: checksum_postings(index) };
    let encoded = bincode_options().serialize(&metadata).map_err(io::Error::other)?;
    writer.write_all(&INDEX_MAGIC)?;
    writer.write_all(&(encoded.len() as u64).to_le_bytes())?;
    writer.write_all(&encoded)?;
    for position in 0..index.posting_count() {
        let posting = index.postings.get(position);
        writer.write_all(&posting.note_id.to_le_bytes())?;
        writer.write_all(&posting.line_number.to_le_bytes())?;
    }
    Ok(())
}
fn checksum_postings(index: &SearchIndex) -> u64 {
    let mut checksum = 0xcbf2_9ce4_8422_2325u64;
    for position in 0..index.posting_count() {
        let posting = index.postings.get(position);
        for byte in posting.note_id.to_le_bytes().into_iter().chain(posting.line_number.to_le_bytes()) {
            checksum ^= u64::from(byte);
            checksum = checksum.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    checksum
}
fn checksum_file_range(file: &File, offset: usize, len: usize) -> io::Result<u64> {
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(offset as u64))?;
    let mut checksum = 0xcbf2_9ce4_8422_2325u64;
    let mut remaining = len;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let chunk = remaining.min(buffer.len());
        file.read_exact(&mut buffer[..chunk])?;
        for byte in &buffer[..chunk] {
            checksum ^= u64::from(*byte);
            checksum = checksum.wrapping_mul(0x0000_0100_0000_01b3);
        }
        remaining -= chunk;
    }
    Ok(checksum)
}
fn index_body(terms: &mut BTreeMap<String, Vec<PackedPosting>>, note_id: NoteId, body: &str) -> Result<(), SearchIndexError> {
    for (line_number, line) in body.lines().enumerate() {
        let line_number = u32::try_from(line_number).map_err(|_| SearchIndexError::LineNumberOverflow { note_id, line_number })?;
        for word in line.split(|character: char| !character.is_alphanumeric()).filter(|word| (1..=50).contains(&word.chars().count())) {
            terms.entry(word.to_lowercase()).or_default().push(PackedPosting { note_id: note_id.get(), line_number });
        }
    }
    Ok(())
}
fn bincode_options() -> impl Options {
    bincode::DefaultOptions::new().with_fixint_encoding().with_limit(MAX_CACHE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn source(id: u32, path: &str, fingerprint: u64) -> SearchSource {
        SearchSource { note_id: NoteId::new(id), relative_path: path.into(), absolute_path: PathBuf::from(path), fingerprint: SearchFileFingerprint { size: fingerprint, modified_nanos: fingerprint } }
    }

    #[test]
    fn postings_are_packed_sorted_and_deduplicated() {
        assert_eq!(std::mem::size_of::<PackedPosting>(), 8);
        let sources = [source(9, "b.md", 1), source(3, "a.md", 1)];
        let bodies = HashMap::from([(9, Arc::<str>::from("alpha alpha alphabet")), (3, Arc::<str>::from("Alpha"))]);
        let index = SearchIndex::build_from_loader(Path::new("vault"), &sources, |source| bodies.get(&source.note_id.get()).cloned()).unwrap();
        let exact = index.postings_for_exact("alpha");
        assert_eq!(exact.len(), 2);
        let first = exact.get(0).unwrap();
        let second = exact.get(1).unwrap();
        assert_eq!((first.note_id(), first.line_number()), (NoteId::new(3), 0));
        assert_eq!((second.note_id(), second.line_number()), (NoteId::new(9), 0));
        let prefix: Vec<_> = index.postings_for_prefix("alph").map(|(term, _)| term).collect();
        assert_eq!(prefix, ["alpha", "alphabet"]);
    }

    #[test]
    fn cache_round_trip_and_all_invalid_forms_rebuild() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ekphos-search-cache-{unique}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("index.bin");
        assert!(load_index(&path).is_none());
        let sources = [source(1, "note.md", 7)];
        let index = SearchIndex::build_from_loader(&root, &sources, |_| Some(Arc::from("searchable"))).unwrap();
        save_index(&index, &path).unwrap();
        let loaded = load_index_for(&path, &root, &sources).unwrap();
        assert_eq!(loaded.posting_count(), 1);
        assert_eq!(loaded.heap_posting_bytes(), 0);
        assert!(loaded.mapped_cache_bytes() > 0);
        assert_eq!(loaded.header.magic, INDEX_MAGIC);
        assert_eq!(loaded.header.format_version, INDEX_VERSION);
        assert_eq!(loaded.header.vault_identity, vault_identity(&root));
        assert_eq!(loaded.header.files.len(), 1);
        drop(loaded);
        let mut raw_corruption = fs::read(&path).unwrap();
        *raw_corruption.last_mut().unwrap() ^= 0xff;
        fs::write(&path, raw_corruption).unwrap();
        assert!(load_index(&path).is_none());
        save_index(&index, &path).unwrap();
        let mut older = index.clone();
        older.header.format_version = INDEX_VERSION - 1;
        let file = File::create(&path).unwrap();
        write_index(&mut BufWriter::new(file), &older).unwrap();
        assert!(load_index(&path).is_none());
        save_index(&index, &path).unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
        assert!(load_index(&path).is_none());
        let stale = [source(1, "note.md", 8)];
        save_index(&index, &path).unwrap();
        assert!(load_index_for(&path, &root, &stale).is_none());
        fs::write(&path, [1, 2, 3]).unwrap();
        assert!(load_index(&path).is_none());
        fs::write(&path, vec![0xff; 128]).unwrap();
        assert!(load_index(&path).is_none());
        assert!(fs::read_dir(&root).unwrap().flatten().all(|entry| !entry.file_name().to_string_lossy().ends_with(".tmp")));
        let _ = fs::remove_dir_all(root);
    }

    #[derive(Serialize)]
    struct ReleasedV02510SearchIndex {
        version: u32,
        terms: HashMap<String, Vec<(usize, usize, usize)>>,
        lines: Vec<Vec<String>>,
        file_meta: HashMap<String, (u64, usize)>,
        notes_dir: String,
    }

    #[test]
    fn released_v02510_cache_is_rejected_and_rebuilt_without_touching_notes() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ekphos-released-search-cache-{unique}"));
        fs::create_dir_all(&root).unwrap();
        let note = root.join("note.md");
        fs::write(&note, "# Source\nsearchable source remains intact\n").unwrap();
        let path = root.join("search_index.bin");
        let released = ReleasedV02510SearchIndex {
            version: 2,
            terms: HashMap::from([("searchable".to_string(), vec![(0, 1, 0)])]),
            lines: vec![vec!["# Source".to_string(), "searchable source remains intact".to_string()]],
            file_meta: HashMap::from([("note.md".to_string(), (1, 0))]),
            notes_dir: root.to_string_lossy().into_owned(),
        };
        bincode::serialize_into(File::create(&path).unwrap(), &released).unwrap();
        assert!(load_index(&path).is_none());

        let sources = [source(0, "note.md", 1)];
        let rebuilt = SearchIndex::build_from_loader(&root, &sources, |_| fs::read_to_string(&note).ok().map(Arc::from)).unwrap();
        save_index(&rebuilt, &path).unwrap();
        assert!(load_index_for(&path, &root, &sources).is_some());
        assert_eq!(fs::read_to_string(&note).unwrap(), "# Source\nsearchable source remains intact\n");

        let orphan = root.join(format!(".search_index.bin.ekphos-{}-killed.tmp", std::process::id()));
        fs::write(&orphan, b"partial replacement").unwrap();
        assert!(load_index_for(&path, &root, &sources).is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incremental_add_change_delete_and_rename_preserve_stable_ids() {
        let root = Path::new("vault");
        let initial_sources = [source(11, "a.md", 1), source(22, "b.md", 1)];
        let initial = HashMap::from([(11, Arc::<str>::from("alpha")), (22, Arc::<str>::from("beta"))]);
        let index = SearchIndex::build_from_loader(root, &initial_sources, |source| initial.get(&source.note_id.get()).cloned()).unwrap();
        let current_sources = [source(11, "a.md", 1), source(33, "renamed.md", 2), source(44, "new.md", 1)];
        let current = HashMap::from([(33, Arc::<str>::from("gamma")), (44, Arc::<str>::from("delta"))]);
        let mut loaded = Vec::new();
        let updated = index
            .update_from_loader(root, &current_sources, |source| {
                loaded.push(source.note_id);
                current.get(&source.note_id.get()).cloned()
            })
            .unwrap();
        assert_eq!(loaded, [NoteId::new(33), NoteId::new(44)]);
        assert_eq!(updated.postings_for_exact("alpha").get(0).unwrap().note_id(), NoteId::new(11));
        assert!(updated.postings_for_exact("beta").is_empty());
        assert_eq!(updated.postings_for_exact("gamma").get(0).unwrap().note_id(), NoteId::new(33));
        assert_eq!(updated.postings_for_exact("delta").get(0).unwrap().note_id(), NoteId::new(44));
    }
}
