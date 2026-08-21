use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimMode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
}

/// Context menu state for right-click actions
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ContextMenuState {
    #[default]
    None,
    Open {
        x: u16,
        y: u16,
        selected_index: usize,
    },
}

/// Context menu items
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContextMenuItem {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

impl ContextMenuItem {
    pub fn all() -> &'static [ContextMenuItem] {
        &[ContextMenuItem::Copy, ContextMenuItem::Cut, ContextMenuItem::Paste, ContextMenuItem::SelectAll]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ContextMenuItem::Copy => "Copy",
            ContextMenuItem::Cut => "Cut",
            ContextMenuItem::Paste => "Paste",
            ContextMenuItem::SelectAll => "Select All",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum WikiAutocompleteMode {
    #[default]
    Note,
    Heading,
    Alias,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum WikiAutocompleteState {
    #[default]
    None,
    Open {
        trigger_pos: (usize, usize),
        query: String,
        suggestions: Vec<WikiSuggestion>,
        selected_index: usize,
        mode: WikiAutocompleteMode,
        target_note: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BufferSearchMatch {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SearchDirection {
    #[default]
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BufferSearchState {
    pub active: bool,
    pub query: String,
    pub matches: Vec<BufferSearchMatch>,
    pub current_match_index: usize,
    pub case_sensitive: bool,
    pub direction: SearchDirection,
}

impl BufferSearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_match(&self) -> Option<&BufferSearchMatch> {
        if self.matches.is_empty() {
            None
        } else {
            self.matches.get(self.current_match_index)
        }
    }

    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match_index = (self.current_match_index + 1) % self.matches.len();
        }
    }

    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            if self.current_match_index == 0 {
                self.current_match_index = self.matches.len() - 1;
            } else {
                self.current_match_index -= 1;
            }
        }
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
        self.current_match_index = 0;
        self.direction = SearchDirection::Forward;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SearchPickerMode {
    #[default]
    Files,
    Content,
}
#[derive(Debug, Clone, PartialEq, Default)]
pub enum SearchPickerState {
    #[default]
    Closed,
    Open {
        mode: SearchPickerMode,
        query: String,
        file_results: Vec<FilePickerResult>,
        content_results: Vec<SearchHit>,
        hydrated_content_results: Vec<HydratedSearchResult>,
        content_preview: Option<ContentSearchPreview>,
        hydration_key: Option<(u64, usize, usize)>,
        selected_index: usize,
        scroll_offset: usize,
        search_in_progress: bool,
        search_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilePickerResult {
    pub display_name: String,
    pub folder_hint: Option<String>,
    pub note_index: usize,
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentSearchResult {
    pub display_name: String,
    pub matched_line: String,
    pub line_number: usize,
    pub note_index: usize,
    pub folder_hint: Option<String>,
    pub score: i32,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HydratedSearchResult {
    pub result_index: usize,
    pub result: ContentSearchResult,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentSearchPreview {
    pub note_id: NoteId,
    pub start_line: usize,
    pub lines: Vec<String>,
}

/// A suggestion item for wiki link autocomplete
#[derive(Debug, Clone, PartialEq)]
pub struct WikiSuggestion {
    /// Display name shown in the list (note title)
    pub display_name: String,
    /// Text to insert when selected (full path for nested notes)
    pub insert_text: String,
    /// True if this is a folder, false if it's a note
    pub is_folder: bool,
    /// Full path for reference
    pub path: String,
    /// Fuzzy match score (higher is better)
    pub score: i32,
    /// Optional folder hint for nested notes (shown below title)
    pub folder_hint: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WikiLinkInfo {
    pub target: String,               // The file path (without heading)
    pub heading: Option<String>,      // Optional #heading part
    pub display_text: Option<String>, // Optional |alias part
    pub start_col: usize,
    pub end_col: usize,
    pub is_valid: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum LinkInfo {
    Markdown {
        text: String,
        url: String,
        start_col: usize,
        end_col: usize,
    },
    Image {
        path: String,
        start_col: usize,
        end_col: usize,
    },
    Wiki {
        target: String,
        heading: Option<String>,
        start_col: usize,
        end_col: usize,
        is_valid: bool,
    },
}

impl LinkInfo {
    pub fn start_col(&self) -> usize {
        match self {
            LinkInfo::Markdown { start_col, .. } => *start_col,
            LinkInfo::Image { start_col, .. } => *start_col,
            LinkInfo::Wiki { start_col, .. } => *start_col,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FileTreeItem {
    Folder(Box<FileTreeFolder>),
    Note { note_id: NoteId, depth: usize },
}

#[derive(Debug, Clone)]
pub struct FileTreeFolder {
    pub name: String,
    pub path: PathBuf,
    pub expanded: bool,
    pub children: Vec<FileTreeItem>,
    pub depth: usize,
}

#[derive(Debug, Clone)]
pub struct SidebarItem {
    pub kind: SidebarItemKind,
    pub depth: usize,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub enum SidebarItemKind {
    Folder(Box<SidebarFolder>),
    Note { note_id: NoteId },
}

#[derive(Debug, Clone)]
pub struct SidebarFolder {
    pub path: PathBuf,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub enum CutItem {
    Note { source_path: PathBuf, title: String },
    Folder { source_path: PathBuf, name: String },
}

pub type GraphLayoutWorkerResult = (u64, u64, Vec<(NoteId, f32, f32)>);
