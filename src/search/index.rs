use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{BufReader, BufWriter};
use serde::{Serialize, Deserialize};

const INDEX_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SearchIndex {
    /// Index format version (for migrations)
    pub version: u32,
    /// Word (lowercase) -> Vec<(note_index, line_number, char_position)>
    pub terms: HashMap<String, Vec<(usize, usize, usize)>>,
    /// Cached lines per note for displaying results
    pub lines: Vec<Vec<String>>,
    /// File metadata for incremental updates: path -> (modified_time, note_index)
    pub file_meta: HashMap<String, (u64, usize)>,
    /// Notes directory this index was built for
    pub notes_dir: String,
    /// Whether the index is ready to use (set after loading/building)
    #[serde(skip)]
    pub ready: bool,
    /// Whether indexing is complete
    #[serde(skip)]
    pub indexing_complete: bool,
}

/// Get cache directory for index
pub fn get_index_path(notes_dir: &Path) -> PathBuf {
    let cache_base = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".cache"));

    // Create 8-char hash of notes directory
    let hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        notes_dir.hash(&mut hasher);
        format!("{:016x}", hasher.finish())[..8].to_string()
    };

    cache_base.join("ekphos").join(hash).join("search_index.bin")
}

/// Load index from disk
pub fn load_index(path: &Path) -> Option<SearchIndex> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut index: SearchIndex = bincode::deserialize_from(reader).ok()?;

    // Check version compatibility
    if index.version != INDEX_VERSION {
        return None;  // Rebuild if version mismatch
    }

    index.ready = true;
    index.indexing_complete = true; // Loaded indexes are complete
    Some(index)
}

/// Save index to disk
pub fn save_index(index: &SearchIndex, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    bincode::serialize_into(writer, index)
        .map_err(std::io::Error::other)
}

impl SearchIndex {
    /// Index a single note's content (public for incremental indexing)
    pub fn index_note_pub(&mut self, note_idx: usize, rel_path: &str, content: &str, mtime: u64) {
        self.index_note(note_idx, rel_path, content, mtime);
    }

    /// Index a single note's content
    fn index_note(&mut self, note_idx: usize, rel_path: &str, content: &str, mtime: u64) {
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        for (line_num, line) in lines.iter().enumerate() {
            let line_lower = line.to_lowercase();
            let line_chars: Vec<char> = line_lower.chars().collect();

            // Tokenize: split on non-alphanumeric, keep words with 1-50 chars
            // Use chars().count() for proper Unicode support
            for word in line.split(|c: char| !c.is_alphanumeric())
                .filter(|w| (1..=50).contains(&w.chars().count()))
            {
                let word_lower = word.to_lowercase();
                // Find character position (not byte position) for Unicode support
                if let Some(char_pos) = find_char_position(&line_chars, &word_lower) {
                    self.terms
                        .entry(word_lower)
                        .or_default()
                        .push((note_idx, line_num, char_pos));
                }
            }
        }

        // Ensure lines vec is large enough
        while self.lines.len() <= note_idx {
            self.lines.push(Vec::new());
        }
        self.lines[note_idx] = lines;
        self.file_meta.insert(rel_path.to_string(), (mtime, note_idx));
    }

    /// Check which files need re-indexing
    pub fn get_stale_files(&self, current_files: &[(String, u64)]) -> Vec<String> {
        let mut stale = Vec::new();

        for (path, mtime) in current_files {
            match self.file_meta.get(path) {
                Some((cached_mtime, _)) if cached_mtime >= mtime => continue,
                _ => stale.push(path.clone()),
            }
        }

        stale
    }

    /// Remove entries for deleted files
    pub fn remove_deleted(&mut self, current_paths: &[String]) {
        let current_set: std::collections::HashSet<_> = current_paths.iter().collect();

        // Find deleted files
        let deleted: Vec<String> = self.file_meta.keys()
            .filter(|p| !current_set.contains(p))
            .cloned()
            .collect();

        for path in deleted {
            if let Some((_, note_idx)) = self.file_meta.remove(&path) {
                // Clear entries for this note (keeping index stable)
                if note_idx < self.lines.len() {
                    self.lines[note_idx].clear();
                }
                // Remove from terms (expensive but necessary)
                for positions in self.terms.values_mut() {
                    positions.retain(|(idx, _, _)| *idx != note_idx);
                }
            }
        }

        // Remove empty term entries
        self.terms.retain(|_, positions| !positions.is_empty());
    }

    /// Remove entries for a specific note (by path) before re-indexing it
    pub fn remove_note(&mut self, rel_path: &str) {
        if let Some((_, note_idx)) = self.file_meta.remove(rel_path) {
            // Clear entries for this note
            if note_idx < self.lines.len() {
                self.lines[note_idx].clear();
            }
            // Remove from terms
            for positions in self.terms.values_mut() {
                positions.retain(|(idx, _, _)| *idx != note_idx);
            }
            // Remove empty term entries
            self.terms.retain(|_, positions| !positions.is_empty());
        }
    }

    /// Update index with changed notes (incremental update)
    pub fn update_with_notes(&mut self, notes: &[(usize, String, String, u64)]) {
        for (note_idx, rel_path, content, mtime) in notes {
            // Remove old entries for this note
            self.remove_note(rel_path);
            // Re-index the note
            self.index_note(*note_idx, rel_path, content, *mtime);
        }
    }
}

/// Find the character position of a substring in a char slice
/// Returns None if not found
fn find_char_position(haystack: &[char], needle: &str) -> Option<usize> {
    let needle_chars: Vec<char> = needle.chars().collect();
    let needle_len = needle_chars.len();

    if needle_len == 0 || needle_len > haystack.len() {
        return None;
    }

    'outer: for i in 0..=(haystack.len() - needle_len) {
        for (j, &nc) in needle_chars.iter().enumerate() {
            if haystack[i + j] != nc {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}
