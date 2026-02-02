use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use image::DynamicImage;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, ListState},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use crate::editor::{Editor, Position};
use crate::highlight::Highlighter;
use crate::highlight_worker::{HighlightColors, HighlightResult, HighlightWorker};
use crate::config::{Config, Theme};
use crate::search::{self, SearchIndex};
use crate::vim::VimState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockInsertMode {
    Insert,
    Append,
}

#[derive(Debug, Clone)]
pub struct BlockInsertState {
    pub mode: BlockInsertMode,
    pub rows: (usize, usize),
    pub insert_col: usize,
    pub active_row: usize,
    pub start_col: usize,
}

use super::welcome_notes::{GETTING_STARTED_CONTENT, DEMO_NOTE_CONTENT};

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".cache")
        })
        .join("ekphos")
}

fn last_note_path() -> PathBuf {
    cache_dir().join("last_note")
}

fn read_last_opened_note() -> Option<PathBuf> {
    std::fs::read_to_string(last_note_path())
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.exists())
}

fn save_last_opened_note(path: &PathBuf) {
    let cache = cache_dir();
    let _ = std::fs::create_dir_all(&cache);
    let _ = std::fs::write(last_note_path(), path.to_string_lossy().as_bytes());
}

#[derive(Debug, Clone)]
pub struct Note {
    pub title: String,
    pub content: String,
    pub file_path: Option<PathBuf>,
    pub modified_time: Option<std::time::SystemTime>,
    pub created_time: Option<std::time::SystemTime>,
    pub frontmatter: Option<super::frontmatter::Frontmatter>,
    pub content_start_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DialogState {
    None,
    Onboarding,
    CreateNote,
    CreateFolder,
    CreateNoteInFolder,
    DeleteConfirm,
    DeleteFolderConfirm,
    RenameNote,
    RenameFolder,
    Help,
    EmptyDirectory,
    DirectoryNotFound,
    UnsavedChanges,
    CreateWikiNote,
    GraphView,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SortMode {
    #[default]
    NameAsc,       
    NameDesc,       
    ModifiedOldest, 
    ModifiedNewest, 
    CreatedOldest,  
    CreatedNewest,  
}

impl SortMode {
    pub fn next(self) -> Self {
        match self {
            SortMode::NameAsc => SortMode::NameDesc,
            SortMode::NameDesc => SortMode::ModifiedOldest,
            SortMode::ModifiedOldest => SortMode::ModifiedNewest,
            SortMode::ModifiedNewest => SortMode::CreatedOldest,
            SortMode::CreatedOldest => SortMode::CreatedNewest,
            SortMode::CreatedNewest => SortMode::NameAsc,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SortMode::NameAsc => "A→Z",
            SortMode::NameDesc => "Z→A",
            SortMode::ModifiedOldest => "Mod↑",
            SortMode::ModifiedNewest => "Mod↓",
            SortMode::CreatedOldest => "Cre↑",
            SortMode::CreatedNewest => "Cre↓",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphViewState {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub selected_node: Option<usize>,
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub zoom: f32,
    pub dirty: bool,
    pub drag_start: Option<(u16, u16)>,
    pub is_panning: bool,
    pub dragging_node: Option<usize>,
    pub view_width: f32,
    pub view_height: f32,
    pub needs_center: bool,
}

impl Default for GraphViewState {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            selected_node: None,
            viewport_x: 0.0,
            viewport_y: 0.0,
            zoom: 1.0,
            dirty: true,
            drag_start: None,
            is_panning: false,
            dragging_node: None,
            view_width: 100.0,
            view_height: 50.0,
            needs_center: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub note_index: usize,
    pub title: String,
    pub x: f32,
    pub y: f32,
    pub home_x: f32,  // Original position for snap-back
    pub home_y: f32,
    pub vx: f32,
    pub vy: f32,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    Content,
    Outline,
}

#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub level: usize,
    pub title: String,
    pub line: usize,
}

pub struct ImageState {
    pub image: StatefulProtocol,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum ContentItem {
    TextLine(String),
    Image(String),
    CodeLine(String),
    CodeFence(String),
    TaskItem { text: String, checked: bool, line_index: usize },
    TableRow { cells: Vec<String>, is_separator: bool, is_header: bool, column_widths: Vec<usize> },
    Details { summary: String, content_lines: Vec<String>, id: usize },
    FrontmatterLine { key: String, value: String },
    FrontmatterDelimiter,
    TagBadges { tags: Vec<String>, date: Option<String> },
}

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
    Open { x: u16, y: u16, selected_index: usize },
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
        &[
            ContextMenuItem::Copy,
            ContextMenuItem::Cut,
            ContextMenuItem::Paste,
            ContextMenuItem::SelectAll,
        ]
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
        content_results: Vec<ContentSearchResult>,
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

pub struct ContentSearchResponse {
    pub search_id: u64,
    pub results: Vec<ContentSearchResult>,
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
    pub target: String,           // The file path (without heading)
    pub heading: Option<String>,  // Optional #heading part
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
            LinkInfo::Wiki { start_col, .. } => *start_col,
        }
    }
}

#[derive(Debug, Clone)]
pub enum FileTreeItem {
    Folder {
        name: String,
        path: PathBuf,
        expanded: bool,
        children: Vec<FileTreeItem>,
        depth: usize,
    },
    Note {
        note_index: usize,
        depth: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SidebarItem {
    pub kind: SidebarItemKind,
    pub depth: usize,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub enum SidebarItemKind {
    Folder { path: PathBuf, expanded: bool },
    Note { note_index: usize },
}

#[derive(Debug, Clone)]
pub enum CutItem {
    Note { source_path: PathBuf, title: String },
    Folder { source_path: PathBuf, name: String },
}

pub struct App {
    pub notes: Vec<Note>,
    pub selected_note: usize,
    #[allow(dead_code)]
    pub list_state: ListState,
    pub focus: Focus,
    pub mode: Mode,
    pub editor: Editor,
    pub picker: Option<Picker>,
    pub image_cache: HashMap<String, DynamicImage>,
    pub current_image: Option<ImageState>,
    pub pending_images: HashSet<String>,
    pub image_sender: Sender<(String, DynamicImage)>,
    pub image_receiver: Receiver<(String, DynamicImage)>,
    pub show_welcome: bool,
    pub outline: Vec<OutlineItem>,
    pub outline_state: ListState,
    pub vim_mode: VimMode,
    pub vim: VimState,
    pub visual_line_anchor: Option<usize>,
    pub visual_line_current: Option<usize>,
    pub visual_block_anchor: Option<Position>,
    pub block_insert_state: Option<BlockInsertState>,
    pub content_cursor: usize,
    pub content_scroll_offset: usize,
    pub floating_cursor_mode: bool,
    pub content_items: Vec<ContentItem>,
    pub content_item_source_lines: Vec<usize>,
    pub theme: Theme,
    pub config: Config,
    pub dialog: DialogState,
    pub input_buffer: String,
    pub search_active: bool,
    pub search_query: String,
    pub filtered_indices: Vec<usize>,
    pub editor_scroll_top: usize,
    pub editor_view_height: usize,
    pub pending_operator: Option<char>,
    pub pending_delete: Option<DeleteType>,
    pub file_tree: Vec<FileTreeItem>,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_sidebar_index: usize,
    pub folder_states: HashMap<PathBuf, bool>,
    pub target_folder: Option<PathBuf>,
    pub dialog_error: Option<String>,
    pub search_matched_notes: Vec<usize>,
    pub pre_search_folder_states: Option<HashMap<PathBuf, bool>>,
    pub pre_search_sidebar_index: Option<usize>,
    pub content_area: Rect,
    pub sidebar_area: Rect,
    pub outline_area: Rect,
    pub mouse_hover_item: Option<usize>,
    pub content_item_rects: Vec<(usize, Rect)>,
    pub selected_link_index: usize,
    pub details_open_states: HashMap<usize, bool>,
    pub heading_fold_states: HashMap<usize, bool>,  // content_item index -> is_folded
    pub highlighter: Option<Highlighter>,
    pub highlighter_loading: bool,
    pub highlighter_sender: Sender<Highlighter>,
    pub highlighter_receiver: Receiver<Highlighter>,
    pub sidebar_collapsed: bool,
    pub outline_collapsed: bool,
    pub zen_mode: bool,
    // Mouse selection state
    pub mouse_button_held: bool,
    pub mouse_drag_start: Option<(u16, u16)>,
    pub last_mouse_y: u16,
    pub editor_area: Rect,
    pub context_menu_state: ContextMenuState,
    // Wiki link support
    pub wiki_autocomplete: WikiAutocompleteState,
    pub pending_wiki_target: Option<String>,
    pub needs_full_clear: bool,
    pub pending_g: bool,
    pub pending_z: bool,  // For z-prefixed commands like zM, zR
    pub status_message: Option<String>,  // Status message shown next to path
    pub buffer_search: BufferSearchState,
    pub help_scroll: usize,
    // Graph view state
    pub graph_view: GraphViewState,
    // Sidebar sorting
    pub sort_mode: SortMode,
    // Navigation history (like browser back/forward)
    pub navigation_history: Vec<NavigationEntry>,
    pub navigation_index: usize,
    // Frontmatter visibility
    pub frontmatter_hidden: bool,
    // Global search picker (file/content search)
    pub search_picker: SearchPickerState,
    pub search_picker_area: ratatui::layout::Rect,
    pub search_picker_results_area: ratatui::layout::Rect,
    pub search_picker_last_click: Option<(std::time::Instant, usize)>, // (time, selected_index)
    pub content_search_sender: Sender<ContentSearchResponse>,
    pub content_search_receiver: Receiver<ContentSearchResponse>,
    pub next_search_id: u64,
    // Search index for fast content search
    pub search_index: SearchIndex,
    /// Channel to receive completed index from background thread
    pub index_receiver: Receiver<SearchIndex>,
    pub indexing_in_progress: bool,
    /// Progress counters (updated by background thread, read by main thread)
    pub index_progress: Arc<AtomicUsize>,
    pub index_total: Arc<AtomicUsize>,
    /// Timestamp when indexing started (for timeout detection)
    pub index_started_at: Option<std::time::Instant>,
    /// Cut buffer for file move/relocation operations
    pub cut_buffer: Option<CutItem>,
    // Background highlight worker
    /// Highlight worker for background syntax highlighting
    pub highlight_worker: Option<HighlightWorker>,
    /// Current document version for highlight requests (incremented on edits)
    pub highlight_version: u64,
    /// Whether there's a pending highlight request waiting for results
    pub highlight_pending: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeleteType {
    Word,
    Line,
}

/// Navigation history entry storing note index and cursor/scroll position
#[derive(Debug, Clone)]
pub struct NavigationEntry {
    pub note_idx: usize,
    pub content_cursor: usize,
    pub content_scroll_offset: usize,
}

impl App {
    pub fn new() -> Self {
        // Check if config exists before loading (determines if onboarding is needed)
        // This must be checked before load_or_create() which creates the config
        let config_exists = Config::exists();

        let config = Config::load_or_create();

        // For first launch: config was just created, so notes_dir won't exist yet
        let is_first_launch = !config_exists;

        let theme = Theme::from_name(&config.theme);

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut editor = Editor::default();
        editor.set_line_wrap(config.editor.line_wrap);
        editor.set_tab_width(config.editor.tab_width);
        editor.set_padding(config.editor.left_padding, config.editor.right_padding);
        editor.set_line_number_mode(config.editor.line_numbers);
        editor.set_scrolloff(config.editor.scrolloff as usize);
        editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.primary))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        // No line highlighting in normal mode - only word highlighting via selection
        editor.set_cursor_line_style(Style::default());
        editor.set_selection_style(
            Style::default()
                .fg(theme.foreground)
                .bg(theme.selection)
        );

        // Initialize image picker for terminal graphics
        let picker = Picker::from_query_stdio().ok();

        // Check if notes directory exists
        let notes_dir_exists = config.notes_path().exists();

        // Check if notes directory has any .md files
        let notes_dir_empty = if notes_dir_exists {
            !Self::directory_has_notes(&config.notes_path())
        } else {
            true
        };

        let dialog = if is_first_launch {
            DialogState::Onboarding
        } else if !notes_dir_exists {
            DialogState::DirectoryNotFound
        } else if notes_dir_empty {
            DialogState::EmptyDirectory
        } else {
            DialogState::None
        };

        let input_buffer = config.notes_dir.clone();
        let sidebar_collapsed = config.sidebar_collapsed;
        let outline_collapsed = config.outline_collapsed;
        let frontmatter_hidden = config.frontmatter_hidden;

        let (image_sender, image_receiver) = mpsc::channel();
        let (highlighter_sender, highlighter_receiver) = mpsc::channel();
        let (content_search_sender, content_search_receiver) = mpsc::channel();
        let (_, index_receiver) = mpsc::channel();

        let mut app = Self {
            notes: Vec::new(),
            selected_note: 0,
            list_state,
            focus: Focus::Sidebar,
            mode: Mode::Normal,
            editor,
            picker,
            image_cache: HashMap::new(),
            current_image: None,
            pending_images: HashSet::new(),
            image_sender,
            image_receiver,
            show_welcome: !is_first_launch && config.welcome_shown && notes_dir_exists && !notes_dir_empty,
            outline: Vec::new(),
            outline_state: ListState::default(),
            vim_mode: VimMode::Normal,
            vim: VimState::new(),
            visual_line_anchor: None,
            visual_line_current: None,
            visual_block_anchor: None,
            block_insert_state: None,
            content_cursor: 0,
            content_scroll_offset: 0,
            floating_cursor_mode: false,
            content_items: Vec::new(),
            content_item_source_lines: Vec::new(),
            theme,
            config,
            dialog,
            input_buffer,
            search_active: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            editor_scroll_top: 0,
            editor_view_height: 0,
            pending_operator: None,
            pending_delete: None,
            file_tree: Vec::new(),
            sidebar_items: Vec::new(),
            selected_sidebar_index: 0,
            folder_states: HashMap::new(),
            target_folder: None,
            dialog_error: None,
            search_matched_notes: Vec::new(),
            pre_search_folder_states: None,
            pre_search_sidebar_index: None,
            content_area: Rect::default(),
            sidebar_area: Rect::default(),
            outline_area: Rect::default(),
            mouse_hover_item: None,
            content_item_rects: Vec::new(),
            selected_link_index: 0,
            details_open_states: HashMap::new(),
            heading_fold_states: HashMap::new(),
            highlighter: None,
            highlighter_loading: false,
            highlighter_sender,
            highlighter_receiver,
            sidebar_collapsed,
            outline_collapsed,
            zen_mode: false,
            // Mouse selection state
            mouse_button_held: false,
            mouse_drag_start: None,
            last_mouse_y: 0,
            editor_area: Rect::default(),
            context_menu_state: ContextMenuState::None,
            wiki_autocomplete: WikiAutocompleteState::None,
            pending_wiki_target: None,
            needs_full_clear: false,
            pending_g: false,
            pending_z: false,
            status_message: None,
            buffer_search: BufferSearchState::new(),
            help_scroll: 0,
            graph_view: GraphViewState::default(),
            sort_mode: SortMode::default(),
            navigation_history: Vec::new(),
            navigation_index: 0,
            frontmatter_hidden,
            search_picker: SearchPickerState::Closed,
            search_picker_area: ratatui::layout::Rect::default(),
            search_picker_results_area: ratatui::layout::Rect::default(),
            search_picker_last_click: None,
            content_search_sender,
            content_search_receiver,
            next_search_id: 0,
            search_index: SearchIndex::default(),
            index_receiver,
            indexing_in_progress: false,
            index_progress: Arc::new(AtomicUsize::new(0)),
            index_total: Arc::new(AtomicUsize::new(0)),
            index_started_at: None,
            cut_buffer: None,
            highlight_worker: Some(HighlightWorker::new()),
            highlight_version: 0,
            highlight_pending: false,
        };

        if !is_first_launch && notes_dir_exists {
            app.load_notes_from_dir();
            app.start_index_build();

            if let Some(last_path) = read_last_opened_note() {
                app.select_note_by_path(&last_path);
            }
        }

        app
    }

    /// Create a new App instance with an optional initial path.
    /// If the path is a directory, it becomes the notes directory.
    /// If the path is a file, its parent becomes the notes directory and the file is selected.
    pub fn new_with_path(initial_path: Option<PathBuf>) -> Self {
        let initial_path = match initial_path {
            Some(path) => path,
            None => return Self::new(),
        };
        let (notes_dir, target_file) = if initial_path.is_dir() {
            (initial_path, None)
        } else if initial_path.is_file() {
            let parent = initial_path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| initial_path.clone());
            (parent, Some(initial_path))
        } else {
            return Self::new();
        };
        let _config_exists = Config::exists();
        let mut config = Config::load_or_create();
        config.notes_dir = notes_dir.to_string_lossy().to_string();

        let theme = Theme::from_name(&config.theme);

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut editor = Editor::default();
        editor.set_line_wrap(config.editor.line_wrap);
        editor.set_tab_width(config.editor.tab_width);
        editor.set_padding(config.editor.left_padding, config.editor.right_padding);
        editor.set_line_number_mode(config.editor.line_numbers);
        editor.set_scrolloff(config.editor.scrolloff as usize);
        editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.primary))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        editor.set_cursor_line_style(Style::default());
        editor.set_selection_style(
            Style::default()
                .fg(theme.foreground)
                .bg(theme.selection)
        );

        let picker = Picker::from_query_stdio().ok();

        let notes_dir_exists = config.notes_path().exists();
        let notes_dir_empty = if notes_dir_exists {
            !Self::directory_has_notes(&config.notes_path())
        } else {
            true
        };

        // Skip onboarding when path is explicitly provided
        let dialog = if !notes_dir_exists {
            DialogState::DirectoryNotFound
        } else if notes_dir_empty {
            DialogState::EmptyDirectory
        } else {
            DialogState::None
        };

        let input_buffer = config.notes_dir.clone();
        let sidebar_collapsed = config.sidebar_collapsed;
        let outline_collapsed = config.outline_collapsed;
        let frontmatter_hidden = config.frontmatter_hidden;

        let (image_sender, image_receiver) = mpsc::channel();
        let (highlighter_sender, highlighter_receiver) = mpsc::channel();
        let (content_search_sender, content_search_receiver) = mpsc::channel();
        let (_, index_receiver) = mpsc::channel();

        let mut app = Self {
            notes: Vec::new(),
            selected_note: 0,
            list_state,
            focus: Focus::Sidebar,
            mode: Mode::Normal,
            editor,
            picker,
            image_cache: HashMap::new(),
            current_image: None,
            pending_images: HashSet::new(),
            image_sender,
            image_receiver,
            show_welcome: false, // Don't show welcome when opening via CLI path
            outline: Vec::new(),
            outline_state: ListState::default(),
            vim_mode: VimMode::Normal,
            vim: VimState::new(),
            visual_line_anchor: None,
            visual_line_current: None,
            visual_block_anchor: None,
            block_insert_state: None,
            content_cursor: 0,
            content_scroll_offset: 0,
            floating_cursor_mode: false,
            content_items: Vec::new(),
            content_item_source_lines: Vec::new(),
            theme,
            config,
            dialog,
            input_buffer,
            search_active: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            editor_scroll_top: 0,
            editor_view_height: 0,
            pending_operator: None,
            pending_delete: None,
            file_tree: Vec::new(),
            sidebar_items: Vec::new(),
            selected_sidebar_index: 0,
            folder_states: HashMap::new(),
            target_folder: None,
            dialog_error: None,
            search_matched_notes: Vec::new(),
            pre_search_folder_states: None,
            pre_search_sidebar_index: None,
            content_area: Rect::default(),
            sidebar_area: Rect::default(),
            outline_area: Rect::default(),
            mouse_hover_item: None,
            content_item_rects: Vec::new(),
            selected_link_index: 0,
            details_open_states: HashMap::new(),
            heading_fold_states: HashMap::new(),
            highlighter: None,
            highlighter_loading: false,
            highlighter_sender,
            highlighter_receiver,
            sidebar_collapsed,
            outline_collapsed,
            zen_mode: false,
            mouse_button_held: false,
            mouse_drag_start: None,
            last_mouse_y: 0,
            editor_area: Rect::default(),
            context_menu_state: ContextMenuState::None,
            wiki_autocomplete: WikiAutocompleteState::None,
            pending_wiki_target: None,
            needs_full_clear: false,
            pending_g: false,
            pending_z: false,
            status_message: None,
            buffer_search: BufferSearchState::new(),
            help_scroll: 0,
            graph_view: GraphViewState::default(),
            sort_mode: SortMode::default(),
            navigation_history: Vec::new(),
            navigation_index: 0,
            frontmatter_hidden,
            search_picker: SearchPickerState::Closed,
            search_picker_area: ratatui::layout::Rect::default(),
            search_picker_results_area: ratatui::layout::Rect::default(),
            search_picker_last_click: None,
            content_search_sender,
            content_search_receiver,
            next_search_id: 0,
            search_index: SearchIndex::default(),
            index_receiver,
            indexing_in_progress: false,
            index_progress: Arc::new(AtomicUsize::new(0)),
            index_total: Arc::new(AtomicUsize::new(0)),
            index_started_at: None,
            cut_buffer: None,
            highlight_worker: Some(HighlightWorker::new()),
            highlight_version: 0,
            highlight_pending: false,
        };

        if notes_dir_exists {
            app.load_notes_from_dir();
            app.start_index_build();
            if let Some(ref target_path) = target_file {
                app.select_note_by_path(target_path);
            } else if let Some(last_path) = read_last_opened_note() {
                app.select_note_by_path(&last_path);
            }
        }

        app
    }

    /// Select a note by its file path
    pub fn select_note_by_path(&mut self, target_path: &PathBuf) {
        // Find the matching note first to avoid borrow conflicts
        let found = self.sidebar_items.iter().enumerate().find_map(|(idx, item)| {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if let Some(note) = self.notes.get(*note_index) {
                    if let Some(ref path) = note.file_path {
                        if path == target_path {
                            return Some((idx, *note_index));
                        }
                    }
                }
            }
            None
        });

        if let Some((sidebar_idx, note_idx)) = found {
            // Clear search when switching notes
            if self.selected_note != note_idx {
                self.end_buffer_search();
            }
            self.selected_sidebar_index = sidebar_idx;
            self.selected_note = note_idx;
            self.update_content_items();
            self.update_outline();
        }
    }

    pub fn reload_on_focus(&mut self) {
        if self.mode == Mode::Edit {
            return;
        }
        let current_note_path = self.current_note().and_then(|n| n.file_path.clone());
        let scroll_offset = self.content_scroll_offset;
        let content_cursor = self.content_cursor;
        self.load_notes_from_dir();
        if let Some(path) = current_note_path {
            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    if self.notes.get(*note_index)
                        .and_then(|n| n.file_path.as_ref())
                        .map(|p| p == &path)
                        .unwrap_or(false)
                    {
                        self.selected_sidebar_index = idx;
                        self.selected_note = *note_index;
                        break;
                    }
                }
            }
        }
        // Rebuild content_items for the restored note BEFORE clamping positions,
        // so that content_items.len() reflects the correct note's length
        self.update_content_items();
        let len = self.content_items.len();
        self.content_cursor = content_cursor.min(len.saturating_sub(1));
        self.content_scroll_offset = if len == 0 {
            0
        } else {
            scroll_offset.clamp(1, len)
        };
        self.update_outline();
    }

    pub fn reload_config(&mut self) {
        if self.mode == Mode::Edit {
            return;
        }

        self.config = Config::load();

        self.theme = Theme::from_name(&self.config.theme);

        self.editor.set_line_wrap(self.config.editor.line_wrap);
        self.editor.set_tab_width(self.config.editor.tab_width);
        self.editor.set_padding(self.config.editor.left_padding, self.config.editor.right_padding);
        self.editor.set_line_number_mode(self.config.editor.line_numbers);
        self.editor.set_scrolloff(self.config.editor.scrolloff as usize);
        self.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.primary))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        self.editor.set_selection_style(
            Style::default()
                .fg(self.theme.foreground)
                .bg(self.theme.selection)
        );

        self.highlighter = None;
        self.load_notes_from_dir();
        self.update_content_items();
        self.update_outline();
    }

    fn directory_has_notes(path: &PathBuf) -> bool {
        Self::directory_has_notes_recursive(path)
    }

    fn directory_has_notes_recursive(path: &PathBuf) -> bool {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    if entry_path.file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    if Self::directory_has_notes_recursive(&entry_path) {
                        return true;
                    }
                } else if let Some(ext) = entry_path.extension() {
                    if ext == "md" {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn load_notes_from_dir(&mut self) {
        self.notes.clear();
        self.file_tree.clear();
        let notes_path = self.config.notes_path();

        if !notes_path.exists() {
            let _ = fs::create_dir_all(&notes_path);
        }

        self.file_tree = self.build_tree(&notes_path, 0);

        // Sort the tree according to current sort mode
        self.sort_tree();

        self.rebuild_sidebar_items();

        self.selected_sidebar_index = 0;
        self.sync_selected_note_from_sidebar();

        self.update_content_items();
        self.update_outline();
    }

    fn build_tree(&mut self, dir: &PathBuf, depth: usize) -> Vec<FileTreeItem> {
        let mut items = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    if path.file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    let children = self.build_tree(&path, depth + 1);

                    if self.config.show_empty_dir || Self::tree_has_notes(&children) {
                        let name = path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        let expanded = self.folder_states
                            .get(&path)
                            .copied()
                            .unwrap_or(false);

                        items.push(FileTreeItem::Folder {
                            name,
                            path,
                            expanded,
                            children,
                            depth,
                        });
                    }
                } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let title = path.file_stem()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        let (modified_time, created_time) = fs::metadata(&path)
                            .map(|m| (m.modified().ok(), m.created().ok()))
                            .unwrap_or((None, None));

                        // Parse frontmatter
                        let (frontmatter, content_start_line) = super::frontmatter::Frontmatter::parse(&content);

                        let note_index = self.notes.len();
                        self.notes.push(Note {
                            title,
                            content,
                            file_path: Some(path),
                            modified_time,
                            created_time,
                            frontmatter,
                            content_start_line,
                        });

                        items.push(FileTreeItem::Note {
                            note_index,
                            depth,
                        });
                    }
                }
            }
        }

        items
    }

    fn tree_has_notes(items: &[FileTreeItem]) -> bool {
        items.iter().any(|item| match item {
            FileTreeItem::Note { .. } => true,
            FileTreeItem::Folder { children, .. } => Self::tree_has_notes(children),
        })
    }

    fn sort_tree(&mut self) {
        let sort_mode = self.sort_mode;
        let folders_first = self.config.folders_first;
        Self::sort_tree_items(&mut self.file_tree, &self.notes, sort_mode, folders_first);
    }

    fn sort_tree_items(items: &mut [FileTreeItem], notes: &[Note], sort_mode: SortMode, folders_first: bool) {
        items.sort_by(|a, b| {
            if folders_first {
                let is_folder_a = matches!(a, FileTreeItem::Folder { .. });
                let is_folder_b = matches!(b, FileTreeItem::Folder { .. });

                match (is_folder_a, is_folder_b) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }
            Self::compare_items(a, b, notes, sort_mode)
        });

        for item in items.iter_mut() {
            if let FileTreeItem::Folder { children, .. } = item {
                Self::sort_tree_items(children, notes, sort_mode, folders_first);
            }
        }
    }

    fn compare_items(a: &FileTreeItem, b: &FileTreeItem, notes: &[Note], sort_mode: SortMode) -> std::cmp::Ordering {
        match sort_mode {
            SortMode::NameAsc => {
                let name_a = Self::get_tree_item_name(a, notes);
                let name_b = Self::get_tree_item_name(b, notes);
                name_a.to_lowercase().cmp(&name_b.to_lowercase())
            }
            SortMode::NameDesc => {
                let name_a = Self::get_tree_item_name(a, notes);
                let name_b = Self::get_tree_item_name(b, notes);
                name_b.to_lowercase().cmp(&name_a.to_lowercase())
            }
            SortMode::ModifiedOldest => {
                let time_a = Self::get_tree_item_modified(a, notes);
                let time_b = Self::get_tree_item_modified(b, notes);
                time_a.cmp(&time_b)
            }
            SortMode::ModifiedNewest => {
                let time_a = Self::get_tree_item_modified(a, notes);
                let time_b = Self::get_tree_item_modified(b, notes);
                time_b.cmp(&time_a)
            }
            SortMode::CreatedOldest => {
                let time_a = Self::get_tree_item_created(a, notes);
                let time_b = Self::get_tree_item_created(b, notes);
                time_a.cmp(&time_b)
            }
            SortMode::CreatedNewest => {
                let time_a = Self::get_tree_item_created(a, notes);
                let time_b = Self::get_tree_item_created(b, notes);
                time_b.cmp(&time_a)
            }
        }
    }

    fn get_tree_item_name<'b>(item: &'b FileTreeItem, notes: &'b [Note]) -> &'b str {
        match item {
            FileTreeItem::Folder { name, .. } => name,
            FileTreeItem::Note { note_index, .. } => &notes[*note_index].title,
        }
    }

    fn get_tree_item_modified(item: &FileTreeItem, notes: &[Note]) -> Option<std::time::SystemTime> {
        match item {
            FileTreeItem::Folder { path, .. } => {
                fs::metadata(path).ok().and_then(|m| m.modified().ok())
            }
            FileTreeItem::Note { note_index, .. } => notes[*note_index].modified_time,
        }
    }

    fn get_tree_item_created(item: &FileTreeItem, notes: &[Note]) -> Option<std::time::SystemTime> {
        match item {
            FileTreeItem::Folder { path, .. } => {
                fs::metadata(path).ok().and_then(|m| m.created().ok())
            }
            FileTreeItem::Note { note_index, .. } => notes[*note_index].created_time,
        }
    }

    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.sort_tree();
        self.rebuild_sidebar_items();
    }

    pub fn rebuild_sidebar_items(&mut self) {
        self.sidebar_items.clear();

        // Add root folder first
        let notes_path = self.config.notes_path();
        let root_name = notes_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Notes".to_string());

        let root_expanded = self.folder_states
            .get(&notes_path)
            .copied()
            .unwrap_or(true); // Root expanded by default

        self.sidebar_items.push(SidebarItem {
            kind: SidebarItemKind::Folder {
                path: notes_path,
                expanded: root_expanded,
            },
            depth: 0,
            display_name: root_name,
        });

        // Only add children if root is expanded
        if root_expanded {
            let tree_clone = self.file_tree.clone();
            self.flatten_tree_into_sidebar(&tree_clone, 1); // Start at depth 1
        }
    }

    fn flatten_tree_into_sidebar(&mut self, items: &[FileTreeItem], depth_offset: usize) {
        for item in items {
            match item {
                FileTreeItem::Folder { name, path, expanded, children, depth } => {
                    self.sidebar_items.push(SidebarItem {
                        kind: SidebarItemKind::Folder {
                            path: path.clone(),
                            expanded: *expanded,
                        },
                        depth: *depth + depth_offset,
                        display_name: name.clone(),
                    });

                    if *expanded {
                        self.flatten_tree_into_sidebar(children, depth_offset);
                    }
                }
                FileTreeItem::Note { note_index, depth } => {
                    self.sidebar_items.push(SidebarItem {
                        kind: SidebarItemKind::Note {
                            note_index: *note_index,
                        },
                        depth: *depth + depth_offset,
                        display_name: self.notes[*note_index].title.clone(),
                    });
                }
            }
        }
    }

    pub fn sync_selected_note_from_sidebar(&mut self) {
        let note_index = self.sidebar_items
            .get(self.selected_sidebar_index)
            .and_then(|item| {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    Some(*note_index)
                } else {
                    None
                }
            });

        if let Some(new_note_idx) = note_index {
            if self.selected_note != new_note_idx {
                self.end_buffer_search();
            }
            self.selected_note = new_note_idx;
            self.current_image = None;
        }
    }

    /// find and select the current note in the sidebar after re sorting
    fn select_current_note_in_sidebar(&mut self) {
        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if *note_index == self.selected_note {
                    self.selected_sidebar_index = idx;
                    return;
                }
            }
        }
    }

    pub fn create_note(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }

        let parent_path = self.target_folder.clone()
            .unwrap_or_else(|| self.config.notes_path());
        let file_path = parent_path.join(format!("{}.md", name));

        // Don't overwrite existing files
        if file_path.exists() {
            return;
        }

        let content = format!("# {}\n\n", name);
        if fs::write(&file_path, &content).is_ok() {
            if let Some(ref folder_path) = self.target_folder {
                self.folder_states.insert(folder_path.clone(), true);
            }

            self.load_notes_from_dir();

            let name_owned = name.to_string();
            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    if self.notes[*note_index].title == name_owned {
                        self.selected_sidebar_index = idx;
                        self.selected_note = *note_index;
                        break;
                    }
                }
            }

            self.update_content_items();
            self.update_outline();
            self.focus = Focus::Content;
        }

        self.target_folder = None;
    }

    pub fn create_folder(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }

        let parent_path = self.target_folder.clone()
            .unwrap_or_else(|| self.config.notes_path());
        let folder_path = parent_path.join(name);

        if folder_path.exists() {
            self.dialog_error = Some(format!("Folder '{}' already exists", name));
            return false;
        }

        if fs::create_dir(&folder_path).is_ok() {
            self.target_folder = Some(folder_path);
            self.dialog_error = None;
            true
        } else {
            self.dialog_error = Some("Failed to create folder".to_string());
            false
        }
    }

    pub fn get_current_context_folder(&self) -> Option<PathBuf> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => Some(path.clone()),
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
                        if let Some(ref file_path) = note.file_path {
                            return file_path.parent().map(|p| p.to_path_buf());
                        }
                    }
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_selected_folder_path(&self) -> Option<PathBuf> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Folder { path, .. } = &item.kind {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn get_selected_folder_name(&self) -> Option<String> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Folder { .. } = &item.kind {
                return Some(item.display_name.clone());
            }
        }
        None
    }

    pub fn delete_current_note(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if let Some(ref path) = self.notes[*note_index].file_path {
                    let _ = fs::remove_file(path);
                }

                self.load_notes_from_dir();

                if self.selected_sidebar_index >= self.sidebar_items.len() {
                    self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
                }
                self.sync_selected_note_from_sidebar();

                self.update_content_items();
                self.update_outline();
            }
        }
    }

    pub fn delete_current_folder(&mut self) {
        if let Some(path) = self.get_selected_folder_path() {
            if fs::remove_dir_all(&path).is_ok() {
                self.folder_states.remove(&path);

                self.load_notes_from_dir();

                if self.selected_sidebar_index >= self.sidebar_items.len() {
                    self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
                }
                self.sync_selected_note_from_sidebar();

                self.update_content_items();
                self.update_outline();
            }
        }
    }

    pub fn rename_note(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                let note_index = *note_index;

                if self.notes[note_index].title == new_name {
                    return;
                }

                let new_file_path = if let Some(ref old_path) = self.notes[note_index].file_path {
                    if let Some(parent) = old_path.parent() {
                        parent.join(format!("{}.md", new_name))
                    } else {
                        return;
                    }
                } else {
                    return;
                };

                if new_file_path.exists() {
                    return;
                }

                if let Some(ref old_path) = self.notes[note_index].file_path {
                    if fs::rename(old_path, &new_file_path).is_ok() {
                        self.load_notes_from_dir();

                        let new_name_owned = new_name.to_string();
                        for (idx, item) in self.sidebar_items.iter().enumerate() {
                            if let SidebarItemKind::Note { note_index } = &item.kind {
                                if self.notes[*note_index].title == new_name_owned {
                                    self.selected_sidebar_index = idx;
                                    self.selected_note = *note_index;
                                    break;
                                }
                            }
                        }

                        self.update_content_items();
                        self.update_outline();
                    }
                }
            }
        }
    }

    pub fn rename_folder(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if let Some(old_path) = self.get_selected_folder_path() {
            let old_name = old_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if old_name == new_name {
                return;
            }

            if let Some(parent) = old_path.parent() {
                let new_path = parent.join(new_name);

                if new_path.exists() {
                    self.dialog_error = Some(format!("Folder '{}' already exists", new_name));
                    return;
                }

                if fs::rename(&old_path, &new_path).is_ok() {
                    if let Some(expanded) = self.folder_states.remove(&old_path) {
                        self.folder_states.insert(new_path.clone(), expanded);
                    }

                    self.load_notes_from_dir();

                    let new_name_owned = new_name.to_string();
                    for (idx, item) in self.sidebar_items.iter().enumerate() {
                        if let SidebarItemKind::Folder { path, .. } = &item.kind {
                            if path == &new_path {
                                self.selected_sidebar_index = idx;
                                break;
                            }
                        }
                        if item.display_name == new_name_owned {
                            if let SidebarItemKind::Folder { .. } = &item.kind {
                                self.selected_sidebar_index = idx;
                                break;
                            }
                        }
                    }

                    self.update_content_items();
                    self.update_outline();
                }
            }
        }
    }

    // ==================== Cut/Paste/Move Operations ====================
    pub fn cut_selected_item(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
                        if let Some(ref path) = note.file_path {
                            self.cut_buffer = Some(CutItem::Note {
                                source_path: path.clone(),
                                title: note.title.clone(),
                            });
                            self.status_message = Some(format!("Cut: {}", note.title));
                        }
                    }
                }
                SidebarItemKind::Folder { path, .. } => {
                    let name = item.display_name.clone();
                    self.cut_buffer = Some(CutItem::Folder {
                        source_path: path.clone(),
                        name: name.clone(),
                    });
                    self.status_message = Some(format!("Cut: {}/", name));
                }
            }
        }
    }
    pub fn clear_cut_buffer(&mut self) {
        if self.cut_buffer.is_some() {
            self.cut_buffer = None;
            self.status_message = Some("Cut cancelled".to_string());
        }
    }
    pub fn paste_cut_item(&mut self) -> Result<(), String> {
        let cut_item = match self.cut_buffer.take() {
            Some(item) => item,
            None => return Err("Nothing to paste".to_string()),
        };
        let dest_folder = self.get_paste_destination_folder();

        match cut_item {
            CutItem::Note { source_path, title } => {
                self.move_note(&source_path, &dest_folder, &title)
            }
            CutItem::Folder { source_path, name } => {
                self.move_folder(&source_path, &dest_folder, &name)
            }
        }
    }
    fn get_paste_destination_folder(&self) -> PathBuf {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => {
                    return path.clone();
                }
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
                        if let Some(ref file_path) = note.file_path {
                            if let Some(parent) = file_path.parent() {
                                return parent.to_path_buf();
                            }
                        }
                    }
                }
            }
        }
        self.config.notes_path()
    }
    fn move_note(&mut self, source: &std::path::Path, dest_folder: &std::path::Path, title: &str) -> Result<(), String> {
        if !source.exists() {
            return Err("Source file no longer exists".to_string());
        }
        let dest_path = dest_folder.join(format!("{}.md", title));
        if source == &dest_path {
            return Err("Already in this location".to_string());
        }
        if source.parent() == Some(dest_folder) {
            return Err("Already in this location".to_string());
        }
        if dest_path.exists() {
            return Err(format!("'{}' already exists in destination", title));
        }
        let notes_root = self.config.notes_path();
        let old_wiki_path = Self::calculate_wiki_path(source, &notes_root);
        let new_wiki_path = Self::calculate_wiki_path(&dest_path, &notes_root);
        fs::rename(source, &dest_path)
            .map_err(|e| format!("Failed to move file: {}", e))?;
        self.update_wiki_links_after_move(&old_wiki_path, &new_wiki_path, title);
        self.load_notes_from_dir();
        self.start_index_build();
        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if let Some(note) = self.notes.get(*note_index) {
                    if note.file_path.as_ref() == Some(&dest_path) {
                        self.selected_sidebar_index = idx;
                        self.selected_note = *note_index;
                        break;
                    }
                }
            }
        }
        self.update_content_items();
        self.update_outline();
        self.status_message = Some(format!("Moved: {}", title));

        Ok(())
    }
    fn move_folder(&mut self, source: &std::path::Path, dest_folder: &std::path::Path, name: &str) -> Result<(), String> {
        if !source.exists() {
            return Err("Source folder no longer exists".to_string());
        }
        let dest_path = dest_folder.join(name);
        if dest_folder.starts_with(source) {
            return Err("Cannot move folder into itself".to_string());
        }
        if source == &dest_path {
            return Err("Already in this location".to_string());
        }
        if source.parent() == Some(dest_folder) {
            return Err("Already in this location".to_string());
        }
        if dest_path.exists() {
            return Err(format!("Folder '{}' already exists in destination", name));
        }

        let notes_root = self.config.notes_path();
        let mut old_new_paths: Vec<(String, String, String)> = Vec::new(); // (old_wiki, new_wiki, title)

        for note in &self.notes {
            if let Some(ref file_path) = note.file_path {
                if file_path.starts_with(source) {
                    let old_wiki = Self::calculate_wiki_path(file_path, &notes_root);
                    // Calculate new path by replacing source prefix with dest
                    let relative = file_path.strip_prefix(source).unwrap_or(file_path.as_path());
                    let new_file_path = dest_path.join(relative);
                    let new_wiki = Self::calculate_wiki_path(&new_file_path, &notes_root);
                    old_new_paths.push((old_wiki, new_wiki, note.title.clone()));
                }
            }
        }

        fs::rename(source, &dest_path)
            .map_err(|e| format!("Failed to move folder: {}", e))?;

        let keys_to_update: Vec<PathBuf> = self.folder_states.keys()
            .filter(|k| k.starts_with(source))
            .cloned()
            .collect();

        for old_key in keys_to_update {
            if let Some(expanded) = self.folder_states.remove(&old_key) {
                let relative = old_key.strip_prefix(source).unwrap_or(&old_key);
                let new_key = dest_path.join(relative);
                self.folder_states.insert(new_key, expanded);
            }
        }

        for (old_wiki, new_wiki, title) in old_new_paths {
            self.update_wiki_links_after_move(&old_wiki, &new_wiki, &title);
        }

        self.load_notes_from_dir();
        self.start_index_build();
        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Folder { path, .. } = &item.kind {
                if path == &dest_path {
                    self.selected_sidebar_index = idx;
                    break;
                }
            }
        }

        self.update_content_items();
        self.update_outline();
        self.status_message = Some(format!("Moved: {}/", name));

        Ok(())
    }

    fn update_wiki_links_after_move(&mut self, old_path: &str, new_path: &str, title: &str) {
        let notes_root = self.config.notes_path();
        let md_files = Self::collect_markdown_files(&notes_root);

        for file_path in md_files {
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let modified_content = self.replace_wiki_links_in_content(
                &content,
                old_path,
                new_path,
                title,
            );

            if modified_content != content {
                let _ = fs::write(&file_path, modified_content);
            }
        }
    }
    fn collect_markdown_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(Self::collect_markdown_files(&path));
                } else if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                    files.push(path);
                }
            }
        }
        files
    }

    fn replace_wiki_links_in_content(
        &self,
        content: &str,
        old_path: &str,
        new_path: &str,
        old_title: &str,
    ) -> String {
        let mut result = String::new();
        let mut remaining = content;

        while let Some(start) = remaining.find("[[") {
            result.push_str(&remaining[..start]);
            remaining = &remaining[start + 2..];

            if let Some(end) = remaining.find("]]") {
                let link_content = &remaining[..end];

                let (target, suffix) = if let Some(hash_pos) = link_content.find('#') {
                    (&link_content[..hash_pos], &link_content[hash_pos..])
                } else if let Some(pipe_pos) = link_content.find('|') {
                    (&link_content[..pipe_pos], &link_content[pipe_pos..])
                } else {
                    (link_content, "")
                };

                let target_lower = target.to_lowercase();
                let old_path_lower = old_path.to_lowercase();
                let old_title_lower = old_title.to_lowercase();

                let should_replace = target_lower == old_path_lower
                    || target_lower == old_title_lower;

                if should_replace {
                    let new_target = if new_path.contains('/') {
                        new_path.to_string()
                    } else {
                        old_title.to_string()
                    };
                    result.push_str("[[");
                    result.push_str(&new_target);
                    result.push_str(suffix);
                    result.push_str("]]");
                } else {
                    // Keep original
                    result.push_str("[[");
                    result.push_str(link_content);
                    result.push_str("]]");
                }

                remaining = &remaining[end + 2..];
            } else {
                result.push_str("[[");
            }
        }

        result.push_str(remaining);
        result
    }

    fn calculate_wiki_path(file_path: &std::path::Path, notes_root: &std::path::Path) -> String {
        if let Ok(relative) = file_path.strip_prefix(notes_root) {
            let path_str = relative.to_string_lossy();
            if let Some(stripped) = path_str.strip_suffix(".md") {
                return stripped.to_string();
            }
            path_str.to_string()
        } else {
            file_path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        }
    }

    pub fn complete_onboarding(&mut self) {
        // 1. Save config
        self.config.notes_dir = self.input_buffer.clone();
        let _ = self.config.save();

        let notes_path = self.config.notes_path();
        let _ = fs::create_dir_all(&notes_path);

        let _ = fs::write(notes_path.join("01-Getting Started.md"), GETTING_STARTED_CONTENT);
        let _ = fs::write(notes_path.join("02-Demo Note.md"), DEMO_NOTE_CONTENT);
        self.dialog = DialogState::None;
        self.load_notes_from_dir();

        self.show_welcome = true;
        self.needs_full_clear = true;
    }

    /// Create the notes directory when it doesn't exist
    pub fn create_notes_directory(&mut self) {
        let notes_path = self.config.notes_path();
        if fs::create_dir_all(&notes_path).is_ok() {
            self.load_notes_from_dir();
            // Show empty directory dialog since we just created an empty directory
            if self.notes.is_empty() {
                self.dialog = DialogState::EmptyDirectory;
            } else {
                self.dialog = DialogState::None;
            }
        }
    }

    pub fn dismiss_welcome(&mut self) {
        self.show_welcome = false;
    }

    pub fn update_outline(&mut self) {
        self.outline.clear();

        for (idx, item) in self.content_items.iter().enumerate() {
            if let ContentItem::TextLine(line) = item {
                if line.starts_with("# ") {
                    self.outline.push(OutlineItem {
                        level: 1,
                        title: line.trim_start_matches("# ").to_string(),
                        line: idx,
                    });
                } else if line.starts_with("## ") {
                    self.outline.push(OutlineItem {
                        level: 2,
                        title: line.trim_start_matches("## ").to_string(),
                        line: idx,
                    });
                } else if line.starts_with("### ") {
                    self.outline.push(OutlineItem {
                        level: 3,
                        title: line.trim_start_matches("### ").to_string(),
                        line: idx,
                    });
                }
            }
        }

        if !self.outline.is_empty() {
            self.outline_state.select(Some(0));
        }
    }

    pub fn update_content_items(&mut self) {
        self.content_items.clear();
        self.content_item_source_lines.clear();
        self.details_open_states.clear();
        self.heading_fold_states.clear();

        // Get note data to extract frontmatter info
        let note_data = self.current_note().map(|n| {
            (n.content.clone(), n.frontmatter.clone(), n.content_start_line)
        });

        if let Some((content, frontmatter, content_start_line)) = note_data {
            let mut in_code_block = false;
            let lines: Vec<&str> = content.lines().collect();
            let mut i = 0;

            // Handle frontmatter display
            let has_frontmatter = frontmatter.is_some() && content_start_line > 0;
            if has_frontmatter && !self.frontmatter_hidden {
                self.content_items.push(ContentItem::FrontmatterDelimiter);
                self.content_item_source_lines.push(0);

                // Parse and show frontmatter lines as key-value pairs
                for line_idx in 1..content_start_line.saturating_sub(1) {
                    if line_idx < lines.len() {
                        let line = lines[line_idx];
                        if let Some(colon_pos) = line.find(':') {
                            let key = line[..colon_pos].trim().to_string();
                            let value = line[colon_pos + 1..].trim().to_string();
                            self.content_items.push(ContentItem::FrontmatterLine {
                                key,
                                value,
                            });
                        } else {
                            self.content_items.push(ContentItem::FrontmatterLine {
                                key: String::new(),
                                value: line.to_string(),
                            });
                        }
                        self.content_item_source_lines.push(line_idx);
                    }
                }

                // Closing delimiter
                if content_start_line > 0 {
                    let closing_idx = content_start_line.saturating_sub(1);
                    self.content_items.push(ContentItem::FrontmatterDelimiter);
                    self.content_item_source_lines.push(closing_idx);
                }

                i = content_start_line;
            } else if has_frontmatter {
                if self.config.show_tags {
                    if let Some(ref fm) = frontmatter {
                        if !fm.tags.is_empty() || fm.date.is_some() {
                            self.content_items.push(ContentItem::TagBadges {
                                tags: fm.tags.clone(),
                                date: fm.date.clone(),
                            });
                            self.content_item_source_lines.push(0);
                        }
                    }
                }
                i = content_start_line;
            }

            while i < lines.len() {
                let line = lines[i];
                let line_index = i;

                // Check for code fence
                if line.starts_with("```") {
                    let lang = line.trim_start_matches('`').to_string();
                    self.content_items.push(ContentItem::CodeFence(lang));
                    self.content_item_source_lines.push(line_index);
                    in_code_block = !in_code_block;
                    i += 1;
                    continue;
                }

                // If inside code block, add as CodeLine
                if in_code_block {
                    self.content_items.push(ContentItem::CodeLine(line.to_string()));
                    self.content_item_source_lines.push(line_index);
                    i += 1;
                    continue;
                }

                // Check for image
                if line.starts_with("![") && line.contains("](") && line.contains(')') {
                    if let Some(start) = line.find("](") {
                        if let Some(end) = line[start..].find(')') {
                            let path = &line[start + 2..start + end];
                            if !path.is_empty() {
                                self.content_items.push(ContentItem::Image(path.to_string()));
                                self.content_item_source_lines.push(line_index);
                                i += 1;
                                continue;
                            }
                        }
                    }
                }

                let trimmed = line.trim_start();
                if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
                    let checked = trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ");
                    let text = trimmed[6..].to_string();
                    self.content_items.push(ContentItem::TaskItem { text, checked, line_index });
                    self.content_item_source_lines.push(line_index);
                    i += 1;
                    continue;
                }

                let trimmed_line = line.trim();
                if trimmed_line.starts_with("<details") && (trimmed_line.ends_with(">") || trimmed_line.contains("><")) {
                    let details_start_line = line_index;
                    let mut summary = String::new();
                    let mut content_lines: Vec<String> = Vec::new();
                    let mut found_end = false;
                    i += 1;

                    while i < lines.len() {
                        let dline = lines[i].trim();

                        if dline.contains("</details>") {
                            found_end = true;
                            i += 1;
                            break;
                        }

                        if dline.starts_with("<summary>") || dline.contains("<summary>") {
                            if dline.contains("</summary>") {
                                if let Some(start) = dline.find("<summary>") {
                                    if let Some(end) = dline.find("</summary>") {
                                        summary = dline[start + 9..end].trim().to_string();
                                    }
                                }
                            } else {
                                summary = dline.trim_start_matches("<summary>").trim().to_string();
                            }
                            i += 1;
                            continue;
                        }

                        if dline == "</summary>" {
                            i += 1;
                            continue;
                        }

                        content_lines.push(lines[i].to_string());
                        i += 1;
                    }

                    if found_end {
                        if summary.is_empty() {
                            summary = "Details".to_string();
                        }
                        self.content_items.push(ContentItem::Details {
                            summary,
                            content_lines,
                            id: details_start_line,
                        });
                        self.content_item_source_lines.push(details_start_line);
                        continue;
                    } else {
                        self.content_items.push(ContentItem::TextLine(line.to_string()));
                        self.content_item_source_lines.push(line_index);
                        continue;
                    }
                }

                if trimmed_line.starts_with('|') && trimmed_line.ends_with('|') {
                    let table_start_line = line_index;
                    let mut table_rows: Vec<(Vec<String>, bool)> = Vec::new();

                    while i < lines.len() {
                        let tline = lines[i].trim();
                        if tline.starts_with('|') && tline.ends_with('|') {
                            let inner = &tline[1..tline.len()-1];
                            let cells: Vec<String> = inner.split('|').map(|s| s.trim().to_string()).collect();
                            let is_separator = cells.iter().all(|cell| {
                                let c = cell.trim();
                                !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')
                            });
                            table_rows.push((cells, is_separator));
                            i += 1;
                        } else {
                            break;
                        }
                    }

                    let num_cols = table_rows.iter().map(|(cells, _)| cells.len()).max().unwrap_or(0);
                    let mut column_widths: Vec<usize> = vec![0; num_cols];

                    for (cells, is_sep) in &table_rows {
                        if !is_sep {
                            for (col_idx, cell) in cells.iter().enumerate() {
                                if col_idx < column_widths.len() {
                                    column_widths[col_idx] = column_widths[col_idx].max(cell.chars().count());
                                }
                            }
                        }
                    }

                    for w in &mut column_widths {
                        *w = (*w).max(3);
                    }

                    let separator_idx = table_rows.iter().position(|(_, is_sep)| *is_sep);

                    for (row_idx, (cells, is_separator)) in table_rows.into_iter().enumerate() {
                        let is_header = separator_idx.map(|sep_idx| row_idx < sep_idx).unwrap_or(false);
                        self.content_items.push(ContentItem::TableRow {
                            cells,
                            is_separator,
                            is_header,
                            column_widths: column_widths.clone(),
                        });
                        self.content_item_source_lines.push(table_start_line + row_idx);
                    }
                    continue;
                }

                self.content_items.push(ContentItem::TextLine(line.to_string()));
                self.content_item_source_lines.push(line_index);
                i += 1;
            }
        }
        self.content_cursor = 0;
    }

    pub fn next_content_line(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        // Find next visible content item
        let mut next = self.content_cursor + 1;
        while next < self.content_items.len() && !self.is_content_item_visible(next) {
            next += 1;
        }
        if next < self.content_items.len() {
            self.content_cursor = next;
            self.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn previous_content_line(&mut self) {
        if self.content_cursor == 0 {
            return;
        }
        // Find previous visible content item
        let mut prev = self.content_cursor.saturating_sub(1);
        while prev > 0 && !self.is_content_item_visible(prev) {
            prev = prev.saturating_sub(1);
        }
        // Only move if the target is visible
        if self.is_content_item_visible(prev) {
            self.content_cursor = prev;
            self.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn goto_first_content_line(&mut self) {
        // Find first visible item
        self.content_cursor = 0;
        while self.content_cursor < self.content_items.len() && !self.is_content_item_visible(self.content_cursor) {
            self.content_cursor += 1;
        }
        self.selected_link_index = 0;
    }

    pub fn goto_last_content_line(&mut self) {
        if !self.content_items.is_empty() {
            // Find last visible item
            self.content_cursor = self.content_items.len() - 1;
            while self.content_cursor > 0 && !self.is_content_item_visible(self.content_cursor) {
                self.content_cursor -= 1;
            }
            self.selected_link_index = 0;
        }
    }

    pub fn half_page_down_content(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        let content_height = self.content_area.height.saturating_sub(2) as usize;
        let half = content_height / 2;
        let max_cursor = self.content_items.len().saturating_sub(1);

        // Count visible items to move by half page
        let mut moved = 0;
        let mut new_cursor = self.content_cursor;
        while moved < half && new_cursor < max_cursor {
            new_cursor += 1;
            if self.is_content_item_visible(new_cursor) {
                moved += 1;
            }
        }
        self.content_cursor = new_cursor;
        self.selected_link_index = 0;
    }

    pub fn half_page_up_content(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        let content_height = self.content_area.height.saturating_sub(2) as usize;
        let half = content_height / 2;

        // Count visible items to move by half page
        let mut moved = 0;
        let mut new_cursor = self.content_cursor;
        while moved < half && new_cursor > 0 {
            new_cursor -= 1;
            if self.is_content_item_visible(new_cursor) {
                moved += 1;
            }
        }
        self.content_cursor = new_cursor;
        self.selected_link_index = 0;
    }

    pub fn toggle_floating_cursor(&mut self) {
        self.floating_cursor_mode = !self.floating_cursor_mode;
    }

    pub fn floating_move_down(&mut self) {
        if self.content_items.is_empty() || !self.floating_cursor_mode {
            return;
        }

        // Find next visible content item
        let mut next = self.content_cursor + 1;
        while next < self.content_items.len() && !self.is_content_item_visible(next) {
            next += 1;
        }
        if next < self.content_items.len() {
            self.content_cursor = next;
            self.selected_link_index = 0;
        }
    }

    pub fn floating_move_up(&mut self) {
        if !self.floating_cursor_mode {
            return;
        }

        if self.content_cursor == 0 {
            return;
        }
        // Find previous visible content item
        let mut prev = self.content_cursor.saturating_sub(1);
        while prev > 0 && !self.is_content_item_visible(prev) {
            prev = prev.saturating_sub(1);
        }
        if self.is_content_item_visible(prev) {
            self.content_cursor = prev;
            self.selected_link_index = 0;
        }
    }

    pub fn toggle_current_task(&mut self) {
        let saved_cursor = self.content_cursor;

        if let Some(item) = self.content_items.get(self.content_cursor) {
            if let ContentItem::TaskItem { line_index, checked, .. } = item {
                let line_index = *line_index;
                let new_checked = !*checked;

                if let Some(note) = self.notes.get_mut(self.selected_note) {
                    let lines: Vec<&str> = note.content.lines().collect();
                    if line_index < lines.len() {
                        let line = lines[line_index];
                        let new_line = if new_checked {
                            line.replacen("- [ ]", "- [x]", 1)
                        } else {
                            line.replacen("- [x]", "- [ ]", 1)
                                .replacen("- [X]", "- [ ]", 1)
                        };

                        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                        new_lines[line_index] = new_line;
                        note.content = new_lines.join("\n");

                        if let Some(ref path) = note.file_path {
                            let _ = fs::write(path, &note.content);
                        }
                    }
                }

                self.update_content_items();
                self.content_cursor = saved_cursor.min(self.content_items.len().saturating_sub(1));
            }
        }
    }

    pub fn toggle_current_details(&mut self) {
        if let Some(item) = self.content_items.get(self.content_cursor) {
            if let ContentItem::Details { id, .. } = item {
                let id = *id;
                let current = self.details_open_states.get(&id).copied().unwrap_or(false);
                self.details_open_states.insert(id, !current);
            }
        }
    }
    pub fn heading_level(line: &str) -> Option<usize> {
        if line.starts_with("### ") {
            Some(3)
        } else if line.starts_with("## ") {
            Some(2)
        } else if line.starts_with("# ") {
            Some(1)
        } else {
            None
        }
    }
    pub fn is_heading_at(&self, idx: usize) -> bool {
        if let Some(ContentItem::TextLine(line)) = self.content_items.get(idx) {
            Self::heading_level(line).is_some()
        } else {
            false
        }
    }
    pub fn is_heading_folded(&self, idx: usize) -> bool {
        self.heading_fold_states.get(&idx).copied().unwrap_or(false)
    }
    pub fn toggle_current_heading_fold(&mut self) {
        if self.is_heading_at(self.content_cursor) {
            let idx = self.content_cursor;
            let current = self.heading_fold_states.get(&idx).copied().unwrap_or(false);
            let new_state = !current;
            self.heading_fold_states.insert(idx, new_state);
            let msg = if new_state { "Folded" } else { "Unfolded" };
            self.status_message = Some(msg.to_string());
        }
    }
    pub fn toggle_heading_fold_at(&mut self, idx: usize) {
        if self.is_heading_at(idx) {
            let current = self.heading_fold_states.get(&idx).copied().unwrap_or(false);
            let new_state = !current;
            self.heading_fold_states.insert(idx, new_state);
            let msg = if new_state { "Folded" } else { "Unfolded" };
            self.status_message = Some(msg.to_string());
        }
    }
    pub fn get_heading_children_range(&self, heading_idx: usize) -> std::ops::Range<usize> {
        let heading_level = if let Some(ContentItem::TextLine(line)) = self.content_items.get(heading_idx) {
            Self::heading_level(line).unwrap_or(0)
        } else {
            return heading_idx..heading_idx;
        };

        let mut end_idx = heading_idx + 1;
        while end_idx < self.content_items.len() {
            if let ContentItem::TextLine(line) = &self.content_items[end_idx] {
                if let Some(level) = Self::heading_level(line) {
                    if level <= heading_level {
                        break;
                    }
                }
            }
            end_idx += 1;
        }
        (heading_idx + 1)..end_idx
    }
    pub fn is_content_item_visible(&self, idx: usize) -> bool {
        for (heading_idx, is_folded) in &self.heading_fold_states {
            if *is_folded && *heading_idx < idx {
                let children_range = self.get_heading_children_range(*heading_idx);
                if children_range.contains(&idx) {
                    return false;
                }
            }
        }
        true
    }
    pub fn fold_all_headings(&mut self) {
        let mut count = 0;
        for idx in 0..self.content_items.len() {
            if self.is_heading_at(idx) {
                self.heading_fold_states.insert(idx, true);
                count += 1;
            }
        }
        self.status_message = Some(format!("Folded {} headings", count));
    }
    pub fn unfold_all_headings(&mut self) {
        let count = self.heading_fold_states.len();
        self.heading_fold_states.clear();
        self.status_message = Some(format!("Unfolded {} headings", count));
    }
    pub fn unfold_heading_at(&mut self, idx: usize) {
        if self.is_heading_at(idx) && self.is_heading_folded(idx) {
            self.heading_fold_states.insert(idx, false);
        }
    }

    pub fn sync_outline_to_content(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        // Find the outline item that corresponds to the current content line
        // or the closest heading before the current line
        let mut best_match: Option<usize> = None;
        for (i, item) in self.outline.iter().enumerate() {
            if item.line <= self.content_cursor {
                best_match = Some(i);
            } else {
                break;
            }
        }
        if let Some(idx) = best_match {
            self.outline_state.select(Some(idx));
        }
    }

    pub fn current_item_is_image(&self) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(self.content_cursor) {
            Some(path)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn current_item_link(&self) -> Option<String> {
        let links = self.item_links_at(self.content_cursor);
        if links.is_empty() {
            return None;
        }
        let idx = self.selected_link_index.min(links.len().saturating_sub(1));
        links.get(idx).map(|(_, url, _, _)| url.clone())
    }

    pub fn item_all_links_at(&self, index: usize) -> Vec<LinkInfo> {
        let mut all_links = Vec::new();

        for (text, url, start, end) in self.item_links_at(index) {
            all_links.push(LinkInfo::Markdown {
                text,
                url,
                start_col: start,
                end_col: end,
            });
        }
        for wiki in self.item_wiki_links_at(index) {
            all_links.push(LinkInfo::Wiki {
                target: wiki.target,
                heading: wiki.heading,
                start_col: wiki.start_col,
                end_col: wiki.end_col,
                is_valid: wiki.is_valid,
            });
        }

        all_links.sort_by_key(|link| link.start_col());
        all_links
    }

    fn is_current_task_item(&self) -> bool {
        matches!(
            self.content_items.get(self.content_cursor),
            Some(ContentItem::TaskItem { .. })
        )
    }
    pub fn is_task_checkbox_selected(&self) -> bool {
        self.is_current_task_item() && self.selected_link_index == 0
    }

    pub fn current_selected_link(&self) -> Option<LinkInfo> {
        let all_links = self.item_all_links_at(self.content_cursor);
        if all_links.is_empty() {
            return None;
        }

        let idx = if self.is_current_task_item() {
            if self.selected_link_index == 0 {
                return None; 
            }
            (self.selected_link_index - 1).min(all_links.len().saturating_sub(1))
        } else {
            self.selected_link_index.min(all_links.len().saturating_sub(1))
        };

        all_links.get(idx).cloned()
    }

    pub fn current_line_link_count(&self) -> usize {
        let link_count = self.item_all_links_at(self.content_cursor).len();
        if self.is_current_task_item() && link_count > 0 {
            link_count + 1
        } else {
            link_count
        }
    }


    pub fn next_link(&mut self) {
        let link_count = self.current_line_link_count();
        if self.is_current_task_item() && link_count > 0 {
            self.selected_link_index = (self.selected_link_index + 1) % link_count;
        } else if link_count > 1 {
            self.selected_link_index = (self.selected_link_index + 1) % link_count;
        }
    }

    pub fn previous_link(&mut self) {
        let link_count = self.current_line_link_count();
        if self.is_current_task_item() && link_count > 0 {
            if self.selected_link_index == 0 {
                self.selected_link_index = link_count - 1;
            } else {
                self.selected_link_index -= 1;
            }
        } else if link_count > 1 {
            if self.selected_link_index == 0 {
                self.selected_link_index = link_count - 1;
            } else {
                self.selected_link_index -= 1;
            }
        }
    }

    pub fn item_link_at(&self, index: usize) -> Option<String> {
        self.item_links_at(index).first().map(|(_, url, _, _)| url.clone())
    }

    /// Check if the current line has any links or wikilinks
    #[allow(dead_code)]
    pub fn current_item_has_link(&self) -> bool {
        !self.item_all_links_at(self.content_cursor).is_empty()
    }

    /// Extract all links and images from a specific content item as (text, url, start_col, end_col) tuples
    /// The columns are character positions in the rendered line (after prefix like "▶ " or "• ")
    pub fn item_links_at(&self, index: usize) -> Vec<(String, String, usize, usize)> {
        let text = match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => line.as_str(),
            Some(ContentItem::TaskItem { text, .. }) => text.as_str(),
            _ => return Vec::new(),
        };

        let mut links = Vec::new();
        let mut search_start = 0;

        while search_start < text.len() {
            let remaining = &text[search_start..];

            // Check for double-bang image !![alt](url) first (text-only, no preview)
            if let Some(dbl_img_pos) = remaining.find("!![") {
                let single_img_pos = remaining.find("![");
                let bracket_pos = remaining.find('[');

                let is_first = single_img_pos.map(|s| dbl_img_pos <= s).unwrap_or(true)
                    && bracket_pos.map(|b| dbl_img_pos < b).unwrap_or(true);

                if is_first {
                    let abs_img_pos = search_start + dbl_img_pos;
                    let from_img = &text[abs_img_pos..];

                    if let Some(bracket_end) = from_img[2..].find("](") {
                        let after_bracket = &from_img[2 + bracket_end + 2..];
                        if let Some(paren_end) = after_bracket.find(')') {
                            let alt_text = &from_img[3..2 + bracket_end];
                            let url = &after_bracket[..paren_end];

                            if !url.is_empty() {
                                let display_text = if alt_text.is_empty() {
                                    url.to_string()
                                } else {
                                    alt_text.to_string()
                                };
                                let rendered_start = Self::calc_rendered_pos(text, abs_img_pos);
                                let rendered_end = rendered_start + display_text.chars().count();

                                links.push((
                                    display_text,
                                    url.to_string(),
                                    rendered_start,
                                    rendered_end,
                                ));
                            }

                            search_start = abs_img_pos + 2 + bracket_end + 2 + paren_end + 1;
                            continue;
                        }
                    }
                }
            }

            // check for single-bang image
            if let Some(img_pos) = remaining.find("![") {
                // skip if this is actually a double-bang
                if img_pos > 0 && remaining.as_bytes().get(img_pos.saturating_sub(1)) == Some(&b'!') {
                    search_start = search_start + img_pos + 2;
                    continue;
                }

                let bracket_pos = remaining.find('[');

                if bracket_pos.is_none() || img_pos < bracket_pos.unwrap() {
                    let abs_img_pos = search_start + img_pos;
                    let from_img = &text[abs_img_pos..];

                    if let Some(bracket_end) = from_img[1..].find("](") {
                        let after_bracket = &from_img[1 + bracket_end + 2..];
                        if let Some(paren_end) = after_bracket.find(')') {
                            let alt_text = &from_img[2..1 + bracket_end];
                            let url = &after_bracket[..paren_end];

                            if !url.is_empty() {
                                let display_text = if alt_text.is_empty() {
                                    format!("[img: {}]", url)
                                } else {
                                    format!("[img: {}]", alt_text)
                                };
                                let rendered_start = Self::calc_rendered_pos(text, abs_img_pos);
                                let rendered_end = rendered_start + display_text.chars().count();

                                links.push((
                                    display_text,
                                    url.to_string(),
                                    rendered_start,
                                    rendered_end,
                                ));
                            }

                            search_start = abs_img_pos + 1 + bracket_end + 2 + paren_end + 1;
                            continue;
                        }
                    }
                }
            }

            //check for regular markdown link
            if let Some(bracket_pos) = remaining.find('[') {
                let abs_bracket_pos = search_start + bracket_pos;
                let from_bracket = &text[abs_bracket_pos..];

                // skip if this is part of a wiki link
                if from_bracket.starts_with("[[") {
                    if let Some(close_pos) = from_bracket[2..].find("]]") {
                        search_start = abs_bracket_pos + 2 + close_pos + 2;
                        continue;
                    }
                }

                if let Some(bracket_end) = from_bracket.find("](") {
                    let after_bracket = &from_bracket[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &from_bracket[1..bracket_end];
                        let url = &after_bracket[..paren_end];

                        if !url.is_empty() {
                            let display_text = if link_text.is_empty() {
                                url.to_string()
                            } else {
                                link_text.to_string()
                            };
                            let rendered_start = Self::calc_rendered_pos(text, abs_bracket_pos);
                            let rendered_end = rendered_start + display_text.chars().count();

                            links.push((
                                display_text,
                                url.to_string(),
                                rendered_start,
                                rendered_end,
                            ));
                        }

                        search_start = abs_bracket_pos + bracket_end + 2 + paren_end + 1;
                        continue;
                    }
                }
            }
            break;
        }

        links
    }

    fn calc_rendered_pos(text: &str, target_pos: usize) -> usize {
        let mut rendered_pos = 0;
        let mut i = 0;

        while i < target_pos && i < text.len() {
            let remaining = &text[i..];

            if remaining.starts_with("!![") {
                if let Some(bracket_end) = remaining[2..].find("](") {
                    let after_bracket = &remaining[2 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[3..2 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 2 + bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() {
                                url.chars().count()
                            } else {
                                alt_text.chars().count()
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            if remaining.starts_with("![") {
                if let Some(bracket_end) = remaining[1..].find("](") {
                    let after_bracket = &remaining[1 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[2..1 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 1 + bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() {
                                6 + url.chars().count() + 1 
                            } else {
                                6 + alt_text.chars().count() + 1 
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            if remaining.starts_with("[[") {
                if let Some(end_pos) = remaining[2..].find("]]") {
                    let target = &remaining[2..2 + end_pos];
                    let full_link_len = 2 + end_pos + 2;

                    if i + full_link_len <= target_pos {
                        rendered_pos += target.chars().count();
                        i += full_link_len;
                        continue;
                    } else {
                        break;
                    }
                }
            }

            if remaining.starts_with('[') {
                if let Some(bracket_end) = remaining.find("](") {
                    let after_bracket = &remaining[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &remaining[1..bracket_end];
                        let full_link_len = bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if link_text.is_empty() {
                                after_bracket[..paren_end].chars().count()
                            } else {
                                link_text.chars().count()
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            rendered_pos += 1;
            i += remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }

        rendered_pos
    }

    /// find which link was clicked based on column position within the content area
    /// Returns the URL if a link was clicked, None otherwise
    /// `col` is the column relative to the content area start
    pub fn find_clicked_link(&self, index: usize, col: u16, content_x: u16) -> Option<String> {
        let links = self.item_links_at(index);
        if links.is_empty() {
            return None;
        }

        let prefix_len = self.get_line_prefix_len(index);
        let click_col = (col.saturating_sub(content_x)) as usize;

        for (_, url, start, end) in &links {
            let adjusted_start = prefix_len + *start;
            let adjusted_end = prefix_len + *end;
            if click_col >= adjusted_start && click_col < adjusted_end {
                return Some(url.clone());
            }
        }

        None
    }
    pub fn find_clicked_wiki_link(&self, index: usize, col: u16, content_x: u16) -> Option<WikiLinkInfo> {
        let wiki_links = self.item_wiki_links_at(index);
        if wiki_links.is_empty() {
            return None;
        }

        let prefix_len = self.get_line_prefix_len(index);
        let click_col = (col.saturating_sub(content_x)) as usize;

        for wiki_link in wiki_links {
            let adjusted_start = prefix_len + wiki_link.start_col;
            let adjusted_end = prefix_len + wiki_link.end_col;
            if click_col >= adjusted_start && click_col < adjusted_end {
                return Some(wiki_link);
            }
        }

        None
    }

    pub fn item_has_link_at(&self, index: usize) -> bool {
        !self.item_links_at(index).is_empty() || !self.item_wiki_links_at(index).is_empty()
    }

    fn get_line_prefix_len(&self, index: usize) -> usize {
        match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => {
                let mut len = 2; 
                if line.starts_with("- ") || line.starts_with("* ") {
                    len += 2; 
                }
                len
            }
            Some(ContentItem::TaskItem { .. }) => 6, 
            _ => 2,
        }
    }

    pub fn item_is_image_at(&self, index: usize) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(index) {
            Some(path)
        } else {
            None
        }
    }

    pub fn item_is_details_at(&self, index: usize) -> bool {
        matches!(self.content_items.get(index), Some(ContentItem::Details { .. }))
    }

    pub fn toggle_details_at(&mut self, index: usize) {
        if let Some(ContentItem::Details { id, .. }) = self.content_items.get(index) {
            let id = *id;
            let current = self.details_open_states.get(&id).copied().unwrap_or(false);
            self.details_open_states.insert(id, !current);
        }
    }

    pub fn item_is_task_at(&self, index: usize) -> bool {
        matches!(self.content_items.get(index), Some(ContentItem::TaskItem { .. }))
    }

    pub fn is_click_on_task_checkbox(&self, index: usize, col: u16, content_x: u16) -> bool {
        if !self.item_is_task_at(index) {
            return false;
        }
        let click_col = col.saturating_sub(content_x) as usize;
        click_col >= 2 && click_col <= 4
    }

    pub fn toggle_task_at(&mut self, index: usize) {
        let saved_cursor = self.content_cursor;

        if let Some(item) = self.content_items.get(index) {
            if let ContentItem::TaskItem { line_index, checked, .. } = item {
                let line_index = *line_index;
                let new_checked = !*checked;

                if let Some(note) = self.notes.get_mut(self.selected_note) {
                    let lines: Vec<&str> = note.content.lines().collect();
                    if line_index < lines.len() {
                        let line = lines[line_index];
                        let new_line = if new_checked {
                            line.replacen("- [ ]", "- [x]", 1)
                        } else {
                            line.replacen("- [x]", "- [ ]", 1)
                                .replacen("- [X]", "- [ ]", 1)
                        };

                        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                        new_lines[line_index] = new_line;
                        note.content = new_lines.join("\n");

                        if let Some(ref path) = note.file_path {
                            let _ = fs::write(path, &note.content);
                        }
                    }
                }

                self.update_content_items();
                self.content_cursor = saved_cursor.min(self.content_items.len().saturating_sub(1));
            }
        }
    }

    #[allow(dead_code)]
    pub fn open_current_link(&self) {
        if let Some(url) = self.current_item_link() {
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(&url).spawn();
            #[cfg(target_os = "linux")]
            let _ = Command::new("xdg-open").arg(&url).spawn();
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd").args(["/c", "start", "", &url]).spawn();
        }
    }

    // ==================== Wiki Link Support ====================

    /// Resolve a wiki link target to a note index
    /// "note" -> searches all notes recursively for matching title (root first, then subfolders)
    /// "folder/note" -> searches for note in specific folder
    pub fn resolve_wiki_link(&self, target: &str) -> Option<usize> {
        if target.is_empty() {
            return None;
        }

        let notes_path = self.config.notes_path();

        if target.contains('/') {
            let expected_path = notes_path.join(format!("{}.md", target));
            let expected_str = expected_path.to_string_lossy();
            for (idx, note) in self.notes.iter().enumerate() {
                if let Some(file_path) = &note.file_path {
                    if file_path.to_string_lossy() == expected_str {
                        return Some(idx);
                    }
                }
            }
        } else {
            // First, try to find in root directory (for backwards compatibility)
            for (idx, note) in self.notes.iter().enumerate() {
                if note.title.eq_ignore_ascii_case(target) {
                    if let Some(file_path) = &note.file_path {
                        if file_path.parent() == Some(notes_path.as_path()) {
                            return Some(idx);
                        }
                    }
                }
            }
            // If not found in root, search recursively in all subdirectories
            // all notes in self.notes are already from the notes directory
            for (idx, note) in self.notes.iter().enumerate() {
                if note.title.eq_ignore_ascii_case(target) {
                    return Some(idx);
                }
            }
        }
        None
    }

    /// Check if a wiki link target exists
    pub fn wiki_link_exists(&self, target: &str) -> bool {
        self.resolve_wiki_link(target).is_some()
    }

    /// Check if cursor position is inside code (inline code or code block)
    pub fn is_cursor_in_code(&self, row: usize, col: usize) -> bool {
        let lines = self.editor.lines();

        // Check if we're inside a code block by counting ``` fences before this row
        let mut in_code_block = false;
        for (i, line) in lines.iter().enumerate() {
            if i >= row {
                break;
            }
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
            }
        }

        // If current line starts with ```, we're on the fence line
        if let Some(current_line) = lines.get(row) {
            if current_line.trim_start().starts_with("```") {
                return true;
            }
        }

        if in_code_block {
            return true;
        }

        // Check for inline code on the current line
        if let Some(line) = lines.get(row) {
            let chars: Vec<char> = line.chars().collect();

            let mut i = 0;
            while i < col {
                if chars.get(i) == Some(&'`') {
                    let mut count = 0;
                    while i < col && chars.get(i) == Some(&'`') {
                        count += 1;
                        i += 1;
                    }

                    let mut found_closing = false;
                    let mut j = i;
                    while j < col {
                        if chars.get(j) == Some(&'`') {
                            let mut close_count = 0;
                            while j < chars.len() && chars.get(j) == Some(&'`') {
                                close_count += 1;
                                j += 1;
                            }
                            if close_count == count {
                                found_closing = true;
                                i = j; 
                                break;
                            }
                        } else {
                            j += 1;
                        }
                    }
                    if !found_closing {
                        return true;
                    }
                } else {
                    i += 1;
                }
            }
        }

        false
    }

    /// Check if cursor is inside an unclosed wikilink and return the current state
    /// Returns: Option<(note_query, heading_query, alias_query, mode)>
    /// - note_query: the part before # or |
    /// - heading_query: the part after # (if present)
    /// - alias_query: the part after | (if present)
    /// - mode: WikiAutocompleteMode indicating current position
    pub fn detect_unclosed_wikilink(&self, row: usize, col: usize) -> Option<(String, Option<String>, Option<String>, WikiAutocompleteMode)> {
        let lines = self.editor.lines();
        let line = lines.get(row)?;
        let chars: Vec<char> = line.chars().collect();
        let mut open_pos = None;
        let mut i = col.saturating_sub(1);
        while i > 0 {
            if i >= 1 && chars.get(i.saturating_sub(1)) == Some(&'[') && chars.get(i) == Some(&'[') {
                open_pos = Some(i.saturating_sub(1));
                break;
            }
            if i >= 1 && chars.get(i.saturating_sub(1)) == Some(&']') && chars.get(i) == Some(&']') {
                return None;
            }
            i = i.saturating_sub(1);
        }
        if open_pos.is_none() && i == 0 && col >= 2 {
            if chars.get(0) == Some(&'[') && chars.get(1) == Some(&'[') {
                open_pos = Some(0);
            }
        }

        let start = open_pos? + 2;

        if self.is_cursor_in_code(row, start) {
            return None;
        } 

        for j in start..col.saturating_sub(1) {
            if chars.get(j) == Some(&']') && chars.get(j + 1) == Some(&']') {
                return None;
            }
        }

        let content: String = chars[start..col].iter().collect();

        if let Some(pipe_pos) = content.find('|') {
            let before_pipe = &content[..pipe_pos];
            let alias_query = content[pipe_pos + 1..].to_string();

            if let Some(hash_pos) = before_pipe.find('#') {
                let note_query = before_pipe[..hash_pos].to_string();
                let heading_query = before_pipe[hash_pos + 1..].to_string();
                Some((note_query, Some(heading_query), Some(alias_query), WikiAutocompleteMode::Alias))
            } else {
                Some((before_pipe.to_string(), None, Some(alias_query), WikiAutocompleteMode::Alias))
            }
        } else if let Some(hash_pos) = content.find('#') {
            let note_query = content[..hash_pos].to_string();
            let heading_query = content[hash_pos + 1..].to_string();
            Some((note_query, Some(heading_query), None, WikiAutocompleteMode::Heading))
        } else {
            Some((content, None, None, WikiAutocompleteMode::Note))
        }
    }

    pub fn get_wiki_path_for_note(&self, note_idx: usize) -> Option<String> {
        let note = self.notes.get(note_idx)?;
        let file_path = note.file_path.as_ref()?;
        let notes_path = self.config.notes_path();
        if let Ok(relative) = file_path.strip_prefix(&notes_path) {
            let path_str = relative.to_string_lossy();
            if let Some(stripped) = path_str.strip_suffix(".md") {
                return Some(stripped.to_string());
            }
        }
        Some(note.title.clone())
    }

    pub fn item_wiki_links_at(&self, index: usize) -> Vec<WikiLinkInfo> {
        let text = match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => line.as_str(),
            Some(ContentItem::TaskItem { text, .. }) => text.as_str(),
            _ => return Vec::new(),
        };

        self.extract_wiki_links_from_text(text)
    }

    pub fn extract_wiki_links_from_text(&self, text: &str) -> Vec<WikiLinkInfo> {
        let mut links = Vec::new();
        let mut search_start = 0;

        while search_start < text.len() {
            let remaining = &text[search_start..];

            // Check for inline code first - skip wikilinks inside backticks
            if let Some(backtick_pos) = remaining.find('`') {
                let wiki_pos = remaining.find("[[");

                // If backtick comes before wikilink, we need to skip past the inline code
                if wiki_pos.is_none() || backtick_pos < wiki_pos.unwrap() {
                    let abs_backtick = search_start + backtick_pos;
                    let after_backtick = &text[abs_backtick + 1..];

                    if let Some(close_backtick) = after_backtick.find('`') {
                        // Skip past the inline code
                        search_start = abs_backtick + 1 + close_backtick + 1;
                        continue;
                    } else {
                        // No closing backtick, rest of text is code
                        break;
                    }
                }
            }

            if let Some(start_pos) = remaining.find("[[") {
                let abs_start = search_start + start_pos;
                let after_brackets = &text[abs_start + 2..];

                if let Some(end_pos) = after_brackets.find("]]") {
                    let raw_content = &after_brackets[..end_pos];
                    if !raw_content.is_empty() && !raw_content.contains('[') && !raw_content.contains(']') {
                        // Parse: [[target#heading|display]]
                        // First split by | to get display text (alias)
                        let (content, display_text) = if let Some(pipe_pos) = raw_content.find('|') {
                            (&raw_content[..pipe_pos], Some(raw_content[pipe_pos + 1..].to_string()))
                        } else {
                            (raw_content, None)
                        };

                        // Then split by # to get heading
                        let (target, heading) = if let Some(hash_pos) = content.find('#') {
                            (&content[..hash_pos], Some(content[hash_pos + 1..].to_string()))
                        } else {
                            (content, None)
                        };

                        let rendered_start = Self::calc_wiki_rendered_pos(text, abs_start);
                        // Display text determines rendered length if present (use unicode width for CJK support)
                        use unicode_width::UnicodeWidthStr;
                        let display_len = display_text.as_ref().map_or(raw_content.width(), |d| d.width());
                        let rendered_end = rendered_start + display_len;
                        // Validate against target file (without heading)
                        let is_valid = self.wiki_link_exists(target);

                        links.push(WikiLinkInfo {
                            target: target.to_string(),
                            heading,
                            display_text,
                            start_col: rendered_start,
                            end_col: rendered_end,
                            is_valid,
                        });
                    }

                    search_start = abs_start + 2 + end_pos + 2;
                    continue;
                }
            }
            break;
        }

        links
    }

    fn calc_wiki_rendered_pos(text: &str, target_pos: usize) -> usize {
        use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};
        let mut rendered_pos = 0;
        let mut i = 0;

        while i < target_pos && i < text.len() {
            let remaining = &text[i..];

            if remaining.starts_with("!![") {
                if let Some(bracket_end) = remaining[2..].find("](") {
                    let after_bracket = &remaining[2 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[3..2 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 2 + bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() {
                                url.width()
                            } else {
                                alt_text.width()
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            if remaining.starts_with("![") {
                if let Some(bracket_end) = remaining[1..].find("](") {
                    let after_bracket = &remaining[1 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[2..1 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 1 + bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() {
                                6 + url.width() + 1
                            } else {
                                6 + alt_text.width() + 1
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            if remaining.starts_with("[[") {
                if let Some(end_pos) = remaining[2..].find("]]") {
                    let target = &remaining[2..2 + end_pos];
                    let full_link_len = 2 + end_pos + 2;

                    if i + full_link_len <= target_pos {
                        rendered_pos += target.width();
                        i += full_link_len;
                        continue;
                    } else {
                        break;
                    }
                }
            }

            if remaining.starts_with('[') {
                if let Some(bracket_end) = remaining.find("](") {
                    let after_bracket = &remaining[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &remaining[1..bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if link_text.is_empty() {
                                url.width()
                            } else {
                                link_text.width()
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            // Use unicode widh for individual characters (CJK = 2, ASCII = 1)
            rendered_pos += remaining.chars().next().map(|c| c.width().unwrap_or(1)).unwrap_or(1);
            i += remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }

        rendered_pos
    }

    #[allow(dead_code)]
    pub fn current_wiki_link_target(&self) -> Option<String> {
        let wiki_links = self.item_wiki_links_at(self.content_cursor);
        wiki_links.get(self.selected_link_index).map(|info| info.target.clone())
    }

    pub fn navigate_to_wiki_link(&mut self, target: &str) -> bool {
        self.navigate_to_wiki_link_with_heading(target, None)
    }

    pub fn navigate_to_wiki_link_with_heading(&mut self, target: &str, heading: Option<&str>) -> bool {
        if let Some(note_idx) = self.resolve_wiki_link(target) {
            if let Some(note) = self.notes.get(note_idx) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.config.notes_path();
                    let mut current = file_path.parent();
                    let mut needs_rebuild = false;
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        if !self.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                            self.folder_states.insert(parent.to_path_buf(), true);
                            needs_rebuild = true;
                        }
                        current = parent.parent();
                    }
                    if needs_rebuild {
                        Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
                        self.rebuild_sidebar_items();
                    }
                }
            }

            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    if *note_index == note_idx {
                        // Clear search when navigating to wiki link
                        self.end_buffer_search();
                        self.selected_sidebar_index = idx;
                        self.selected_note = note_idx;
                        self.push_navigation_history(note_idx);
                        self.content_cursor = 0;
                        self.content_scroll_offset = 0;
                        self.selected_link_index = 0;
                        self.update_content_items();
                        self.update_outline();

                        // If heading is specified, navigate to it
                        if let Some(heading_text) = heading {
                            self.navigate_to_heading(heading_text);
                        }

                        return true;
                    }
                }
            }
        }
        false
    }

    /// Navigate to a heading in the current note's content
    fn navigate_to_heading(&mut self, heading: &str) {
        let heading_lower = heading.to_lowercase();

        for (idx, item) in self.content_items.iter().enumerate() {
            if let ContentItem::TextLine(line) = item {
                let title = if line.starts_with("### ") {
                    Some(line.trim_start_matches("### "))
                } else if line.starts_with("## ") {
                    Some(line.trim_start_matches("## "))
                } else if line.starts_with("# ") {
                    Some(line.trim_start_matches("# "))
                } else {
                    None
                };

                if let Some(title) = title {
                    if title.to_lowercase() == heading_lower {
                        self.content_cursor = idx;
                        self.content_scroll_offset = idx.saturating_sub(2);
                        return;
                    }
                }
            }
        }
    }

    // ==================== Navigation History ====================

    /// push a note to navigation history
    /// called when navigating to a new note
    pub fn push_navigation_history(&mut self, note_idx: usize) {
        if let Some(current) = self.navigation_history.get(self.navigation_index) {
            if current.note_idx == note_idx {
                return;
            }
        }
        if let Some(current) = self.navigation_history.get_mut(self.navigation_index) {
            current.content_cursor = self.content_cursor;
            current.content_scroll_offset = self.content_scroll_offset;
        }

        if self.navigation_index + 1 < self.navigation_history.len() {
            self.navigation_history.truncate(self.navigation_index + 1);
        }

        self.navigation_history.push(NavigationEntry {
            note_idx,
            content_cursor: 0,
            content_scroll_offset: 0,
        });
        self.navigation_index = self.navigation_history.len().saturating_sub(1);

        // limit history size to prevent memory bloat
        const MAX_HISTORY: usize = 100;
        if self.navigation_history.len() > MAX_HISTORY {
            let remove_count = self.navigation_history.len() - MAX_HISTORY;
            self.navigation_history.drain(0..remove_count);
            self.navigation_index = self.navigation_index.saturating_sub(remove_count);
        }
    }

    pub fn navigate_back(&mut self) -> bool {
        if self.navigation_index == 0 || self.navigation_history.is_empty() {
            return false;
        }
        if let Some(current) = self.navigation_history.get_mut(self.navigation_index) {
            current.content_cursor = self.content_cursor;
            current.content_scroll_offset = self.content_scroll_offset;
        }

        self.navigation_index -= 1;
        if let Some(entry) = self.navigation_history.get(self.navigation_index).cloned() {
            self.go_to_note_without_history(entry.note_idx, Some(entry.content_cursor), Some(entry.content_scroll_offset));
            return true;
        }
        false
    }

    /// navigate to next note in history
    pub fn navigate_forward(&mut self) -> bool {
        if self.navigation_index + 1 >= self.navigation_history.len() {
            return false;
        }
        if let Some(current) = self.navigation_history.get_mut(self.navigation_index) {
            current.content_cursor = self.content_cursor;
            current.content_scroll_offset = self.content_scroll_offset;
        }

        self.navigation_index += 1;
        if let Some(entry) = self.navigation_history.get(self.navigation_index).cloned() {
            self.go_to_note_without_history(entry.note_idx, Some(entry.content_cursor), Some(entry.content_scroll_offset));
            return true;
        }
        false
    }

    /// go to a note without pushing to history used by back/forward to prevent infinite loop
    fn go_to_note_without_history(&mut self, note_idx: usize, cursor: Option<usize>, scroll: Option<usize>) {
        if note_idx >= self.notes.len() {
            return;
        }

        if let Some(note) = self.notes.get(note_idx) {
            if let Some(ref file_path) = note.file_path {
                let notes_root = self.config.notes_path();
                let mut current = file_path.parent();
                let mut needs_rebuild = false;
                while let Some(parent) = current {
                    if parent == notes_root {
                        break;
                    }
                    if !self.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                        self.folder_states.insert(parent.to_path_buf(), true);
                        needs_rebuild = true;
                    }
                    current = parent.parent();
                }
                if needs_rebuild {
                    Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
                    self.rebuild_sidebar_items();
                }
            }
        }

        for (idx, item) in self.sidebar_items.iter().enumerate() {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if *note_index == note_idx {
                    self.end_buffer_search();
                    self.selected_sidebar_index = idx;
                    self.selected_note = note_idx;
                    self.selected_link_index = 0;
                    self.update_content_items();
                    self.update_outline();
                    let max_cursor = self.content_items.len().saturating_sub(1);
                    self.content_cursor = cursor.unwrap_or(0).min(max_cursor);
                    self.content_scroll_offset = scroll.unwrap_or(0).min(max_cursor);
                    return;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn can_navigate_back(&self) -> bool {
        self.navigation_index > 0 && !self.navigation_history.is_empty()
    }

    #[allow(dead_code)]
    pub fn can_navigate_forward(&self) -> bool {
        self.navigation_index + 1 < self.navigation_history.len()
    }

    pub fn build_graph(&mut self) {
        use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};

        let mut nodes: Vec<GraphNode> = Vec::new();
        let mut edges: Vec<GraphEdge> = Vec::new();
        let mut note_to_node: HashMap<usize, usize> = HashMap::new();
        for (note_idx, note) in self.notes.iter().enumerate() {
            let node_idx = nodes.len();
            note_to_node.insert(note_idx, node_idx);

            let title = {
                let display_width = note.title.width();
                if display_width > 20 {
                    let mut truncated = String::new();
                    let mut current_width = 0;
                    for ch in note.title.chars() {
                        let ch_width = ch.width().unwrap_or(1);
                        if current_width + ch_width > 17 {
                            break;
                        }
                        truncated.push(ch);
                        current_width += ch_width;
                    }
                    truncated + "..."
                } else {
                    note.title.clone()
                }
            };

            nodes.push(GraphNode {
                note_index: note_idx,
                title,
                x: 0.0,
                y: 0.0,
                home_x: 0.0,
                home_y: 0.0,
                vx: 0.0,
                vy: 0.0,
            });
        }

        for (note_idx, note) in self.notes.iter().enumerate() {
            let wiki_targets = self.extract_wiki_targets_from_content(&note.content);

            for target in wiki_targets {
                if let Some(target_note_idx) = self.resolve_wiki_link(&target) {
                    if let (Some(&from_node), Some(&to_node)) =
                        (note_to_node.get(&note_idx), note_to_node.get(&target_note_idx))
                    {
                        let existing = edges.iter_mut().find(|e| e.from == to_node && e.to == from_node);

                        if let Some(edge) = existing {
                            edge.bidirectional = true;
                        } else {
                            let already_exists = edges.iter().any(|e| e.from == from_node && e.to == to_node);
                            if !already_exists {
                                edges.push(GraphEdge {
                                    from: from_node,
                                    to: to_node,
                                    bidirectional: false,
                                });
                            }
                        }
                    }
                }
            }
        }

        self.graph_view.nodes = nodes;
        self.graph_view.edges = edges;
        self.graph_view.dirty = true;

        if let Some(&node_idx) = note_to_node.get(&self.selected_note) {
            self.graph_view.selected_node = Some(node_idx);
            self.graph_view.needs_center = true;
        } else {
            self.graph_view.selected_node = if !self.graph_view.nodes.is_empty() { Some(0) } else { None };
        }
    }

    fn extract_wiki_targets_from_content(&self, content: &str) -> Vec<String> {
        let mut targets = Vec::new();
        for line in content.lines() {
            for wiki_link in self.extract_wiki_links_from_text(line) {
                if !targets.contains(&wiki_link.target) {
                    targets.push(wiki_link.target);
                }
            }
        }
        targets
    }

    pub fn build_wiki_suggestions(&self, query: &str) -> Vec<WikiSuggestion> {
        let mut suggestions = Vec::new();
        let notes_path = self.config.notes_path();
        let (folder_prefix, note_query) = if let Some(last_slash) = query.rfind('/') {
            (&query[..=last_slash], &query[last_slash + 1..])
        } else {
            ("", query)
        };

        for (idx, note) in self.notes.iter().enumerate() {
            if let Some(wiki_path) = self.get_wiki_path_for_note(idx) {
                if !folder_prefix.is_empty() {
                    if !wiki_path.to_lowercase().starts_with(&folder_prefix.to_lowercase()) {
                        continue;
                    }
                }

                if let Some(score) = fuzzy_match(&note.title, note_query) {
                    let folder_hint = if let Some(last_slash) = wiki_path.rfind('/') {
                        Some(wiki_path[..last_slash].to_string())
                    } else {
                        None
                    };
                    suggestions.push(WikiSuggestion {
                        display_name: note.title.clone(),
                        insert_text: note.title.clone(),
                        is_folder: false,
                        path: note.file_path.as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        score,
                        folder_hint,
                    });
                }
            }
        }

        for item in &self.sidebar_items {
            if let SidebarItemKind::Folder { path, .. } = &item.kind {
                if let Ok(relative) = path.strip_prefix(&notes_path) {
                    let folder_path = relative.to_string_lossy().to_string();

                    if folder_path.is_empty() {
                        continue;
                    }

                    if !folder_prefix.is_empty() {
                        if !folder_path.to_lowercase().starts_with(&folder_prefix.to_lowercase().trim_end_matches('/')) {
                            continue;
                        }
                    }

                    if let Some(score) = fuzzy_match(&item.display_name, note_query) {
                        suggestions.push(WikiSuggestion {
                            display_name: item.display_name.clone(),
                            insert_text: format!("{}/", folder_path),
                            is_folder: true,
                            path: path.display().to_string(),
                            score,
                            folder_hint: None,
                        });
                    }
                }
            }
        }

        suggestions.sort_by(|a, b| {
            match (a.is_folder, b.is_folder) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => b.score.cmp(&a.score)
                    .then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())),
            }
        });

        suggestions
    }

    /// Build heading suggestions for a note target
    /// This extracts headings from the note's content and filters by query
    pub fn build_heading_suggestions(&self, note_target: &str, query: &str) -> Vec<WikiSuggestion> {
        let mut suggestions = Vec::new();

        for (idx, note) in self.notes.iter().enumerate() {
            if let Some(wiki_path) = self.get_wiki_path_for_note(idx) {
                if wiki_path.to_lowercase() == note_target.to_lowercase()
                   || note.title.to_lowercase() == note_target.to_lowercase() {
                    for line in note.content.lines() {
                        let heading: Option<(usize, String)> = if line.starts_with("### ") {
                            Some((3, line.trim_start_matches("### ").to_string()))
                        } else if line.starts_with("## ") {
                            Some((2, line.trim_start_matches("## ").to_string()))
                        } else if line.starts_with("# ") {
                            Some((1, line.trim_start_matches("# ").to_string()))
                        } else {
                            None
                        };

                        if let Some((level, title)) = heading {
                            let score = if query.is_empty() {
                                1000 
                            } else if let Some(s) = fuzzy_match(&title, query) {
                                s
                            } else {
                                continue; 
                            };

                            let prefix = "  ".repeat(level.saturating_sub(1));
                            suggestions.push(WikiSuggestion {
                                display_name: format!("{}{}", prefix, title),
                                insert_text: title.clone(), // Just the heading text for insertion
                                is_folder: false,
                                path: format!("{}#{}", wiki_path, title),
                                score,
                                folder_hint: None,
                            });
                        }
                    }
                    break; 
                }
            }
        }

        suggestions.sort_by(|a, b| b.score.cmp(&a.score));

        suggestions
    }

    pub fn create_note_from_wiki_target(&mut self, target: &str) -> bool {
        if target.starts_with('/') || target.starts_with('\\') {
            return false;
        }

        let notes_path = self.config.notes_path();
        let file_path = notes_path.join(format!("{}.md", target));

        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if fs::create_dir_all(parent).is_err() {
                    return false;
                }
            }
        }

        let title = target.rsplit('/').next().unwrap_or(target);

        let content = format!("# {}\n\n", title);
        if fs::write(&file_path, &content).is_err() {
            return false;
        }

        self.load_notes_from_dir();

        self.navigate_to_wiki_link(target)
    }

    pub fn open_current_image(&self) {
        if let Some(path) = self.current_item_is_image() {
            self.open_path_or_url(path);
        }
    }

    pub fn open_path_or_url(&self, path: &str) {
        let is_url = path.starts_with("http://") || path.starts_with("https://");

        let open_path = if is_url {
            path.to_string()
        } else if let Some(resolved) = self.resolve_image_path(path) {
            resolved.to_string_lossy().to_string()
        } else {
            path.to_string()
        };

        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(&open_path).spawn();
        #[cfg(target_os = "linux")]
        let _ = Command::new("xdg-open").arg(&open_path).spawn();
        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd").args(["/c", "start", "", &open_path]).spawn();
    }

    pub fn next_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = (self.selected_sidebar_index + 1) % self.sidebar_items.len();
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn previous_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = if self.selected_sidebar_index == 0 {
            self.sidebar_items.len() - 1
        } else {
            self.selected_sidebar_index - 1
        };
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn goto_first_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = 0;
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn goto_last_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = self.sidebar_items.len() - 1;
        self.sync_selected_note_from_sidebar();
        self.update_content_items();
        self.update_outline();
    }

    pub fn handle_sidebar_enter(&mut self) {
        let item_info = self.sidebar_items.get(self.selected_sidebar_index).map(|item| {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => (true, path.clone(), 0),
                SidebarItemKind::Note { note_index } => (false, PathBuf::new(), *note_index),
            }
        });

        if let Some((is_folder, path, note_index)) = item_info {
            if is_folder {
                self.toggle_folder(path);
            } else {
                self.toggle_focus(false);
                self.push_navigation_history(note_index);
            }
        }
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        let new_state = !self.folder_states.get(&path).copied().unwrap_or(false);
        self.folder_states.insert(path.clone(), new_state);

        Self::update_folder_in_tree(&mut self.file_tree, &path, new_state);

        self.rebuild_sidebar_items();

        if self.selected_sidebar_index >= self.sidebar_items.len() {
            self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
        }

        self.sync_selected_note_from_sidebar();
    }

    fn update_folder_in_tree(items: &mut [FileTreeItem], target_path: &PathBuf, new_state: bool) {
        for item in items {
            if let FileTreeItem::Folder { path, expanded, children, .. } = item {
                if path == target_path {
                    *expanded = new_state;
                    return;
                }
                Self::update_folder_in_tree(children, target_path, new_state);
            }
        }
    }

    pub fn toggle_focus(&mut self, backwards: bool) {
        self.focus = match self.focus {
            Focus::Sidebar => if backwards { Focus::Outline } else { Focus::Content },
            Focus::Content => if backwards { Focus::Sidebar } else { Focus::Outline },
            Focus::Outline => if backwards {Focus::Content} else {Focus::Sidebar},
        };
    }

    pub fn toggle_sidebar_collapsed(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    pub fn toggle_outline_collapsed(&mut self) {
        self.outline_collapsed = !self.outline_collapsed;
    }

    pub fn toggle_zen_mode(&mut self) {
        self.zen_mode = !self.zen_mode;
        if self.zen_mode {
            self.focus = Focus::Content;
        }
    }

    pub fn toggle_frontmatter_hidden(&mut self) {
        self.frontmatter_hidden = !self.frontmatter_hidden;
        self.update_content_items();
    }

    pub fn update_filtered_indices(&mut self) {
        if self.search_query.is_empty() {
            self.search_matched_notes.clear();
            self.filtered_indices.clear();
            return;
        }

        let query = self.search_query.to_lowercase();

        self.search_matched_notes = self.notes
            .iter()
            .enumerate()
            .filter(|(_, note)| note.title.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        for &note_index in &self.search_matched_notes {
            if let Some(note) = self.notes.get(note_index) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.config.notes_path();
                    let mut current = file_path.parent();
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        self.folder_states.insert(parent.to_path_buf(), true);
                        current = parent.parent();
                    }
                }
            }
        }

        Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);

        self.rebuild_sidebar_items();

        self.filtered_indices = self.sidebar_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    self.search_matched_notes.contains(note_index)
                } else {
                    false
                }
            })
            .map(|(i, _)| i)
            .collect();

        if !self.filtered_indices.is_empty() {
            self.selected_sidebar_index = self.filtered_indices[0];
            self.sync_selected_note_from_sidebar();
            self.update_content_items();
            self.update_outline();
        }
    }

    fn update_tree_expanded_states(items: &mut [FileTreeItem], folder_states: &HashMap<PathBuf, bool>) {
        for item in items {
            if let FileTreeItem::Folder { path, expanded, children, .. } = item {
                if let Some(&state) = folder_states.get(path) {
                    *expanded = state;
                }
                Self::update_tree_expanded_states(children, folder_states);
            }
        }
    }

    pub fn activate_sidebar_search(&mut self) {
        self.pre_search_folder_states = Some(self.folder_states.clone());
        self.pre_search_sidebar_index = Some(self.selected_sidebar_index);
        self.search_active = true;
        self.search_query.clear();
    }

    pub fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.filtered_indices.clear();
        self.search_matched_notes.clear();
        if let Some(saved_states) = self.pre_search_folder_states.take() {
            self.folder_states = saved_states;
            Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
            self.rebuild_sidebar_items();
        }
        if let Some(saved_index) = self.pre_search_sidebar_index.take() {
            self.selected_sidebar_index = saved_index.min(self.sidebar_items.len().saturating_sub(1));
        }
    }

    pub fn start_buffer_search(&mut self) {
        self.start_buffer_search_with_direction(SearchDirection::Forward);
    }

    #[allow(dead_code)]
    pub fn start_buffer_search_backward(&mut self) {
        self.start_buffer_search_with_direction(SearchDirection::Backward);
    }

    pub fn start_buffer_search_with_direction(&mut self, direction: SearchDirection) {
        self.buffer_search.active = true;
        self.buffer_search.query.clear();
        self.buffer_search.matches.clear();
        self.buffer_search.current_match_index = 0;
        self.buffer_search.direction = direction;
    }

    pub fn end_buffer_search(&mut self) {
        self.buffer_search.clear();
    }

    pub fn perform_buffer_search(&mut self) {
        self.buffer_search.matches.clear();
        self.buffer_search.current_match_index = 0;

        if self.buffer_search.query.is_empty() {
            return;
        }

        let query = if self.buffer_search.case_sensitive {
            self.buffer_search.query.clone()
        } else {
            self.buffer_search.query.to_lowercase()
        };

        let lines: Vec<String> = if self.mode == Mode::Edit {
            self.editor.lines().iter().map(|s| s.to_string()).collect()
        } else if let Some(note) = self.notes.get(self.selected_note) {
            note.content.lines().map(|s| s.to_string()).collect()
        } else {
            return;
        };

        for (row, line) in lines.iter().enumerate() {
            let search_line = if self.buffer_search.case_sensitive {
                line.clone()
            } else {
                line.to_lowercase()
            };

            let chars: Vec<char> = search_line.chars().collect();
            let query_chars: Vec<char> = query.chars().collect();
            let query_len = query_chars.len();

            if query_len == 0 {
                continue;
            }

            let mut col = 0;
            while col + query_len <= chars.len() {
                let matches = chars[col..col + query_len]
                    .iter()
                    .zip(query_chars.iter())
                    .all(|(a, b)| a == b);

                if matches {
                    self.buffer_search.matches.push(BufferSearchMatch {
                        row,
                        start_col: col,
                        end_col: col + query_len,
                    });
                    col += 1; 
                } else {
                    col += 1;
                }
            }
        }
    }

    pub fn scroll_to_current_match(&mut self) {
        if let Some(m) = self.buffer_search.current_match() {
            let target_row = m.row;

            if self.mode == Mode::Edit {
                let start_col = m.start_col;
                self.editor.set_cursor(target_row, start_col);
                let half_height = self.editor_view_height / 2;
                if target_row > half_height {
                    self.editor_scroll_top = target_row - half_height;
                } else {
                    self.editor_scroll_top = 0;
                }
            } else {
                for (idx, &source_line) in self.content_item_source_lines.iter().enumerate() {
                    if source_line >= target_row {
                        self.content_cursor = idx;
                        let content_height = self.content_area.height.saturating_sub(2) as usize;
                        let half_height = content_height / 2;
                        if idx > half_height {
                            self.content_scroll_offset = idx - half_height;
                        } else {
                            self.content_scroll_offset = 0;
                        }
                        break;
                    }
                }
            }
        }
    }

    pub fn buffer_search_next(&mut self) {
        self.buffer_search.next_match();
        self.scroll_to_current_match();
    }

    pub fn buffer_search_prev(&mut self) {
        self.buffer_search.prev_match();
        self.scroll_to_current_match();
    }

    pub fn get_visible_sidebar_indices(&self) -> Vec<usize> {
        if self.search_active && !self.search_query.is_empty() {
            self.filtered_indices.clone()
        } else {
            (0..self.sidebar_items.len()).collect()
        }
    }

    pub fn next_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => (i + 1) % self.outline.len(),
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    pub fn previous_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.outline.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    pub fn goto_first_outline(&mut self) {
        if !self.outline.is_empty() {
            self.outline_state.select(Some(0));
        }
    }

    pub fn goto_last_outline(&mut self) {
        if !self.outline.is_empty() {
            self.outline_state.select(Some(self.outline.len() - 1));
        }
    }

    pub fn jump_to_outline(&mut self) {
        if let Some(selected) = self.outline_state.selected() {
            if let Some(outline_item) = self.outline.get(selected) {
                let target_line = outline_item.line;
                // Set content cursor to the target line
                if target_line < self.content_items.len() {
                    self.unfold_heading_at(target_line);
                    self.content_cursor = target_line;
                }
                // Switch focus to content
                self.focus = Focus::Content;
            }
        }
    }

    pub fn current_note(&self) -> Option<&Note> {
        self.notes.get(self.selected_note)
    }

    pub fn save_last_opened_note_to_cache(&self) {
        if let Some(note) = self.current_note() {
            if let Some(ref path) = note.file_path {
                save_last_opened_note(path);
            }
        }
    }

    pub fn resolve_image_path(&self, path: &str) -> Option<PathBuf> {
        if path.starts_with("http://") || path.starts_with("https://") {
            return Some(PathBuf::from(path));
        }

        let path_buf = if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&path[2..])
            } else {
                PathBuf::from(path)
            }
        } else if path == "~" {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(path))
        } else {
            PathBuf::from(path)
        };

        if path_buf.is_absolute() && path_buf.exists() {
            return Some(path_buf);
        }

        if let Some(note) = self.current_note() {
            if let Some(ref file_path) = note.file_path {
                if let Some(note_dir) = file_path.parent() {
                    let resolved = note_dir.join(&path_buf);
                    if resolved.exists() {
                        return Some(resolved);
                    }
                }
            }
        }

        if path_buf.exists() {
            return Some(path_buf);
        }

        None
    }

    /// Find the content item index for a given source line.
    /// Returns the index of the content item that starts at or before the given line.
    fn content_cursor_for_source_line(&self, source_line: usize) -> usize {
        let mut best_idx = 0;
        for (idx, &line) in self.content_item_source_lines.iter().enumerate() {
            if line <= source_line {
                best_idx = idx;
            } else {
                break;
            }
        }
        best_idx
    }

    pub fn enter_edit_mode(&mut self) {
        // Drain any old highlight results before starting fresh
        if let Some(ref worker) = self.highlight_worker {
            worker.drain_results();
        }
        self.highlight_pending = false;

        if let Some(note) = self.current_note() {
            let lines: Vec<String> = note.content.lines().map(String::from).collect();
            let line_count = lines.len();
            let content_start_line = note.content_start_line;

            let target_row = self.content_item_source_lines
                .get(self.content_cursor)
                .copied()
                .unwrap_or(0)
                .min(line_count.saturating_sub(1));

            self.editor = Editor::new(lines);
            self.editor.set_line_wrap(self.config.editor.line_wrap);
            self.editor.set_tab_width(self.config.editor.tab_width);
            self.editor.set_padding(self.config.editor.left_padding, self.config.editor.right_padding);
            self.editor.set_line_number_mode(self.config.editor.line_numbers);
            self.editor.set_scrolloff(self.config.editor.scrolloff as usize);

            self.vim_mode = VimMode::Normal;
            self.vim.mode = crate::vim::VimMode::Normal;
            self.vim.reset_pending();
            self.vim.command_buffer.clear();

            // Set wiki link styles from theme
            self.editor.set_wiki_link_styles(
                ratatui::style::Style::default().fg(self.theme.info),
                ratatui::style::Style::default().fg(self.theme.error),
            );

            // Set markdown highlighting colors from theme
            self.editor.set_markdown_colors(
                [
                    self.theme.editor.heading1,
                    self.theme.editor.heading2,
                    self.theme.editor.heading3,
                    self.theme.editor.heading4,
                    self.theme.editor.heading5,
                    self.theme.editor.heading6,
                ],
                self.theme.editor.code,
                self.theme.editor.link,
                self.theme.editor.blockquote,
                self.theme.editor.list_marker,
                Some(self.theme.editor.bold),
                Some(self.theme.editor.italic),
            );
            self.editor.set_frontmatter_color(self.theme.content.frontmatter);

            self.editor.set_cursor(target_row, 0);

            // Calculate scroll position:
            // - If frontmatter was hidden and we're near the top, start from line 0
            //   to show frontmatter in edit mode (unless it would push cursor off screen)
            // - Otherwise, try to maintain similar viewport position
            let view_height = self.editor_view_height.max(10);
            // content_scroll_offset is 1-indexed, so <= 1 means at the top
            let editor_scroll = if self.frontmatter_hidden && content_start_line > 0 && self.content_scroll_offset <= 1 {
                // Frontmatter was hidden, user was at/near top of content
                // Start from line 0 unless cursor would be off screen
                if target_row < view_height {
                    0
                } else {
                    target_row.saturating_sub(view_height / 2)
                }
            } else {
                // Normal case: try to preserve relative cursor position
                let preview_scroll_top = self.content_scroll_offset.saturating_sub(1);
                let cursor_offset_from_top = self.content_cursor.saturating_sub(preview_scroll_top);
                target_row.saturating_sub(cursor_offset_from_top)
            };

            self.editor.set_scroll_offset(editor_scroll.min(line_count.saturating_sub(1)));
            self.editor_scroll_top = self.editor.scroll_offset();

            self.update_editor_block();
            self.mode = Mode::Edit;
            self.focus = Focus::Content;

            self.request_highlight_update();
        }
    }

    pub fn update_editor_highlights(&mut self) {
        self.request_highlight_update();
    }

    pub fn update_editor_highlights_incremental(&mut self) {
        self.request_highlight_update();
    }

    pub fn update_editor_scroll(&mut self, view_height: usize) {
        self.editor_view_height = view_height;
        self.editor.update_scroll(view_height);
        self.editor_scroll_top = self.editor.scroll_offset();
    }

    pub fn update_editor_block(&mut self) {
        // Check for command mode first (from new vim state)
        let is_command_mode = self.vim.mode.is_command();

        let mode_str = if is_command_mode {
            "COMMAND"
        } else if let Some(ref block_state) = self.block_insert_state {
            match block_state.mode {
                BlockInsertMode::Insert => "V-BLK INSERT",
                BlockInsertMode::Append => "V-BLK APPEND",
            }
        } else {
            match self.vim_mode {
                VimMode::Normal => "NORMAL",
                VimMode::Insert => "INSERT",
                VimMode::Replace => "REPLACE",
                VimMode::Visual => "VISUAL",
                VimMode::VisualLine => "V-LINE",
                VimMode::VisualBlock => "V-BLOCK",
            }
        };
        let pending_str = match (&self.pending_delete, self.pending_operator) {
            (Some(_), _) => " [DEL]",
            (None, Some('d')) => " d-",
            _ => "",
        };
        let color = if is_command_mode {
            self.theme.info
        } else if self.block_insert_state.is_some() {
            self.theme.secondary // Use secondary color for block insert mode
        } else {
            match (&self.pending_delete, self.vim_mode) {
                (Some(_), _) => self.theme.error,
                (None, VimMode::Normal) if self.pending_operator.is_some() => self.theme.warning,
                (None, VimMode::Normal) => self.theme.primary,
                (None, VimMode::Insert) => self.theme.success,
                (None, VimMode::Replace) => self.theme.warning,
                (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => {
                    self.theme.secondary
                }
            }
        };
        let hint = if is_command_mode {
            "Enter: Execute, Esc: Cancel"
        } else if self.block_insert_state.is_some() {
            "Type text, Esc: Apply to all lines"
        } else {
            match (&self.pending_delete, self.vim_mode) {
                (Some(_), _) => "d: Confirm, Esc: Cancel",
                (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => {
                    "y: Yank, d: Delete, Esc: Cancel"
                }
                (None, _) if self.pending_operator == Some('d') => "d: Line, w: Word→, b: Word←",
                _ => "Ctrl+S: Save, Esc: Exit",
            }
        };
        if self.zen_mode {
            self.editor.set_block(Block::default());
        } else {
            self.editor.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color))
                    .title(format!(" {}{} | {} ", mode_str, pending_str, hint)),
            );
        }
        self.editor.set_selection_style(
            Style::default()
                .fg(self.theme.foreground)
                .bg(self.theme.selection)
        );
        self.editor.set_cursor_line_style(Style::default());
    }

    pub fn save_edit(&mut self) {
        // Clear search state and vim state when exiting edit mode
        self.end_buffer_search();
        self.vim.reset_pending();
        self.vim.command_buffer.clear();
        self.vim.mode = crate::vim::VimMode::Normal;
        self.vim_mode = VimMode::Normal;
        self.highlight_pending = false;

        let (cursor_row, _) = self.editor.cursor();
        let editor_scroll = self.editor.scroll_offset();

        let cursor_offset_from_top = cursor_row.saturating_sub(editor_scroll);

        if let Some(note) = self.notes.get_mut(self.selected_note) {
            note.content = self.editor.lines().join("\n");
            // Re-parse frontmatter after content change
            let (frontmatter, content_start_line) = super::frontmatter::Frontmatter::parse(&note.content);
            note.frontmatter = frontmatter;
            note.content_start_line = content_start_line;
            // Save to file
            if let Some(ref path) = note.file_path {
                let _ = fs::write(path, &note.content);
                // Update modified time after save
                note.modified_time = fs::metadata(path).ok().and_then(|m| m.modified().ok());
            }
        }

        // Re-sort and rebuild sidebar to reflect updated modified time
        self.sort_tree();
        self.rebuild_sidebar_items();
        // Re-select the current note in the sidebar after re-sorting
        self.select_current_note_in_sidebar();

        self.mode = Mode::Normal;
        self.update_content_items();
        self.update_outline();

        // Map editor row to content_cursor using source line mapping
        self.content_cursor = self.content_cursor_for_source_line(cursor_row);
        let preview_scroll = self.content_cursor.saturating_sub(cursor_offset_from_top);
        self.content_scroll_offset = preview_scroll + 1;
    }

    pub fn cancel_edit(&mut self) {
        self.end_buffer_search();
        self.vim.reset_pending();
        self.vim.command_buffer.clear();
        self.vim.mode = crate::vim::VimMode::Normal;
        self.vim_mode = VimMode::Normal;
        self.highlight_pending = false;

        let (cursor_row, _) = self.editor.cursor();
        let editor_scroll = self.editor.scroll_offset();

        let cursor_offset_from_top = cursor_row.saturating_sub(editor_scroll);
        self.mode = Mode::Normal;

        self.content_cursor = self.content_cursor_for_source_line(cursor_row);
        let preview_scroll = self.content_cursor.saturating_sub(cursor_offset_from_top);
        self.content_scroll_offset = preview_scroll + 1;
    }

    pub fn has_unsaved_changes(&self) -> bool {
        if let Some(note) = self.notes.get(self.selected_note) {
            let current_content = self.editor.lines().join("\n");
            current_content != note.content
        } else {
            false
        }
    }

    pub fn poll_pending_images(&mut self) {
        while let Ok((url, img)) = self.image_receiver.try_recv() {
            self.pending_images.remove(&url);
            const MAX_CACHED_IMAGES: usize = 20;
            if self.image_cache.len() >= MAX_CACHED_IMAGES {
                if let Some(key) = self.image_cache.keys().next().cloned() {
                    self.image_cache.remove(&key);
                }
            }

            self.image_cache.insert(url, img);
        }
    }

    pub fn is_image_pending(&self, url: &str) -> bool {
        self.pending_images.contains(url)
    }

    pub fn start_remote_image_fetch(&mut self, url: &str) {
        if self.pending_images.contains(url) || self.image_cache.contains_key(url) {
            return;
        }

        self.pending_images.insert(url.to_string());
        let url_owned = url.to_string();
        let sender = self.image_sender.clone();

        std::thread::spawn(move || {
            if let Some(img) = fetch_remote_image_blocking(&url_owned) {
                let _ = sender.send((url_owned, img));
            }
        });
    }

    // ==================== Highlighter Lazy Loading ====================

    // Syntect syntax highlighter takes around extra 30mb of memory, which I think it should be considered
    // as quite bloated, the threshold of ekphos should be no more than 15mb if possible
    // but unfortunately still can't find a better syntax highlighter than syntect for now
    // I will enable this lazy load by default so markdown file without code syntax won't need to take extra 30mb of memory
    
    pub fn poll_highlighter(&mut self) {
        if let Ok(highlighter) = self.highlighter_receiver.try_recv() {
            self.highlighter = Some(highlighter);
            self.highlighter_loading = false;
        }
    }

    pub fn ensure_highlighter(&mut self) {
        if self.highlighter.is_some() || self.highlighter_loading {
            return;
        }

        self.highlighter_loading = true;
        let syntax_theme = self.config.syntax_theme.clone();
        let sender = self.highlighter_sender.clone();

        std::thread::spawn(move || {
            let highlighter = Highlighter::new(&syntax_theme);
            let _ = sender.send(highlighter);
        });
    }

    // Background Highlight Worker

    pub fn request_highlight_update(&mut self) {
        self.highlight_version += 1;
        self.highlight_pending = true;

        if let Some(ref worker) = self.highlight_worker {
            let content = self.editor.lines().join("\n");
            let colors = self.get_highlight_colors();
            worker.request(content, self.highlight_version, colors);
        }
    }

    fn get_highlight_colors(&self) -> HighlightColors {
        HighlightColors {
            heading_colors: [
                self.theme.editor.heading1,
                self.theme.editor.heading2,
                self.theme.editor.heading3,
                self.theme.editor.heading4,
                self.theme.editor.heading5,
                self.theme.editor.heading6,
            ],
            code_color: self.theme.editor.code,
            link_color: self.theme.editor.link,
            blockquote_color: self.theme.editor.blockquote,
            list_marker_color: self.theme.editor.list_marker,
            bold_color: Some(self.theme.editor.bold),
            italic_color: Some(self.theme.editor.italic),
            frontmatter_color: self.theme.content.frontmatter,
            details_color: self.theme.editor.link, // Use link color for HTML details tags
            horizontal_rule_color: self.theme.editor.blockquote, // Use blockquote color for horizontal rules
        }
    }

    pub fn poll_highlight_worker(&mut self) -> bool {
        let result = if let Some(ref worker) = self.highlight_worker {
            worker.try_recv()
        } else {
            return false;
        };

        if let Some(result) = result {
            let applied = self.apply_highlight_result(result);
            if applied {
                self.highlight_pending = false;
            }
            applied
        } else {
            false
        }
    }

    fn apply_highlight_result(&mut self, result: HighlightResult) -> bool {
        if result.version != self.highlight_version {
            return false;
        }

        self.editor.clear_highlights();
        self.editor.add_highlights(result.highlights);
        self.update_editor_wiki_links_with_ranges(&result.wiki_links);
        self.editor.invalidate_all_styles();
        true
    }

    fn update_editor_wiki_links_with_ranges(&mut self, ranges: &[crate::editor::WikiLinkRange]) {
        let notes_path = self.config.notes_path();
        let mut valid_targets: HashSet<String> = HashSet::new();

        for note in &self.notes {
            if let Some(file_path) = &note.file_path {
                if let Ok(relative) = file_path.strip_prefix(&notes_path) {
                    let path_str = relative.to_string_lossy();
                    if let Some(stripped) = path_str.strip_suffix(".md") {
                        valid_targets.insert(stripped.to_string());
                        valid_targets.insert(note.title.clone());
                        valid_targets.insert(note.title.to_lowercase());
                    }
                }
            }
        }

        let validated_ranges: Vec<crate::editor::WikiLinkRange> = ranges
            .iter()
            .map(|range| {
                // Extract target from the wiki link at this position
                let is_valid = self.validate_wiki_link_at(range.row, range.start_col, &valid_targets);
                crate::editor::WikiLinkRange {
                    row: range.row,
                    start_col: range.start_col,
                    end_col: range.end_col,
                    is_valid,
                }
            })
            .collect();

        self.editor.set_wiki_link_ranges(validated_ranges);
    }

    fn validate_wiki_link_at(&self, row: usize, start_col: usize, valid_targets: &HashSet<String>) -> bool {
        let line = match self.editor.lines().get(row) {
            Some(l) => *l,
            None => return false,
        };

        let chars: Vec<char> = line.chars().collect();
        if start_col + 2 >= chars.len() {
            return false;
        }

        let after_open: String = chars[start_col + 2..].iter().collect();
        if let Some(end_pos) = after_open.find("]]") {
            let raw_content = &after_open[..end_pos];

            let content = if let Some(pipe_pos) = raw_content.find('|') {
                &raw_content[..pipe_pos]
            } else {
                raw_content
            };
            let target = if let Some(hash_pos) = content.find('#') {
                &content[..hash_pos]
            } else {
                content
            };

            if valid_targets.contains(target) {
                return true;
            }
            if !target.contains('/') {
                return valid_targets.contains(&target.to_lowercase());
            }
        }
        false
    }

    pub fn has_highlight_work(&self) -> bool {
        self.highlight_pending
    }

    pub fn get_highlighter(&self) -> Option<&Highlighter> {
        self.highlighter.as_ref()
    }

    // ==================== Search Index ====================

    pub fn start_index_build(&mut self) {
        if self.indexing_in_progress {
            return;
        }

        let notes_dir = self.config.notes_path();
        let index_path = search::get_index_path(&notes_dir);
        let notes_dir_str = notes_dir.to_string_lossy().to_string();

        let note_data: Vec<(usize, String, String, u64)> = self.notes
            .iter()
            .enumerate()
            .filter_map(|(idx, note)| {
                let path = note.file_path.as_ref()?;
                let rel_path = path.strip_prefix(&notes_dir).ok()?
                    .to_string_lossy().to_string();
                let mtime = note.modified_time?
                    .duration_since(std::time::UNIX_EPOCH).ok()?
                    .as_secs();
                Some((idx, rel_path, note.content.clone(), mtime))
            })
            .collect();

        if note_data.is_empty() {
            self.index_progress.store(0, Ordering::Relaxed);
            self.index_total.store(0, Ordering::Relaxed);
            self.search_index = SearchIndex {
                version: 2,
                notes_dir: notes_dir_str,
                ready: true,
                indexing_complete: true,
                ..Default::default()
            };
            return;
        }

        self.indexing_in_progress = true;
        self.index_started_at = Some(std::time::Instant::now());

        self.index_progress.store(0, Ordering::Relaxed);
        self.index_total.store(note_data.len(), Ordering::Relaxed);
        let progress = Arc::clone(&self.index_progress);
        let total = Arc::clone(&self.index_total);
        let (sender, receiver) = mpsc::channel();
        self.index_receiver = receiver;

        std::thread::spawn(move || {
            let build_full_with_progress = |note_data: &[(usize, String, String, u64)],
                                            notes_dir: &str,
                                            progress: &Arc<AtomicUsize>| -> SearchIndex {
                let mut index = SearchIndex {
                    version: 2,
                    notes_dir: notes_dir.to_string(),
                    ..Default::default()
                };
                for (i, (note_idx, rel_path, content, mtime)) in note_data.iter().enumerate() {
                    index.index_note_pub(*note_idx, rel_path, content, *mtime);
                    progress.store(i + 1, Ordering::Relaxed);
                }
                index
            };

            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let existing_index = search::load_index(&index_path);

                let mut index = if let Some(mut cached) = existing_index {
                    if cached.notes_dir == notes_dir_str {
                        let current_files: Vec<(String, u64)> = note_data
                            .iter()
                            .map(|(_, path, _, mtime)| (path.clone(), *mtime))
                            .collect();
                        let current_paths: Vec<String> = current_files.iter().map(|(p, _)| p.clone()).collect();

                        cached.remove_deleted(&current_paths);
                        let stale = cached.get_stale_files(&current_files);

                        if stale.is_empty() {
                            progress.store(note_data.len(), Ordering::Relaxed);
                            total.store(note_data.len(), Ordering::Relaxed);
                            cached
                        } else {
                            total.store(stale.len(), Ordering::Relaxed);
                            progress.store(0, Ordering::Relaxed);

                            let stale_notes: Vec<_> = note_data
                                .iter()
                                .filter(|(_, path, _, _)| stale.contains(path))
                                .cloned()
                                .collect();

                            for (i, note) in stale_notes.iter().enumerate() {
                                cached.update_with_notes(&[note.clone()]);
                                progress.store(i + 1, Ordering::Relaxed);
                            }
                            cached
                        }
                    } else {
                        build_full_with_progress(&note_data, &notes_dir_str, &progress)
                    }
                } else {
                    build_full_with_progress(&note_data, &notes_dir_str, &progress)
                };

                index.ready = true;
                index.indexing_complete = true;
                let _ = search::save_index(&index, &index_path);
                let _ = sender.send(index);
            }));

            if result.is_err() {
                let mut empty = SearchIndex::default();
                empty.ready = true;
                empty.indexing_complete = true;
                let _ = sender.send(empty);
            }
        });
    }

    pub fn poll_index_build(&mut self) {
        // Early return if not indexing
        if !self.indexing_in_progress {
            return;
        }

        if let Ok(index) = self.index_receiver.try_recv() {
            self.search_index = index;
            self.indexing_in_progress = false;
            self.index_started_at = None;
            // Reset progress counters
            self.index_progress.store(0, Ordering::Relaxed);
            self.index_total.store(0, Ordering::Relaxed);
            return;
        }

        const INDEXING_TIMEOUT_SECS: u64 = 60;
        if let Some(started) = self.index_started_at {
            if started.elapsed().as_secs() > INDEXING_TIMEOUT_SECS {
                self.indexing_in_progress = false;
                self.index_started_at = None;
                self.index_progress.store(0, Ordering::Relaxed);
                self.index_total.store(0, Ordering::Relaxed);
                self.search_index.ready = true;
                self.search_index.indexing_complete = true;
            }
        }
    }

    /// Search using the index (fast path)
    fn search_with_index(&self, query: &str) -> Vec<ContentSearchResult> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // i think most people should be fine with 15k limits
        const MAX_RESULTS: usize = 15000;
        const MAX_EXACT_MATCHES: usize = 15000;
        const MAX_PREFIX_MATCHES: usize = 15000;
        const MAX_PREFIX_TERMS_SCANNED: usize = 15000;
        const MAX_LINE_SCAN_NOTES: usize = 15000;

        let create_result = |note_idx: usize, line_num: usize, line: &str, query_lower: &str| -> Option<ContentSearchResult> {
            let note = self.notes.get(note_idx)?;
            let wiki_path = self.get_wiki_path_for_note(note_idx);
            let folder_hint = wiki_path.as_ref().and_then(|wp| {
                wp.rfind('/').map(|pos| wp[..pos].to_string())
            });

            let line_lower = line.to_lowercase();
            let match_byte_pos = line_lower.find(query_lower)?;
            let line_chars: Vec<char> = line.chars().collect();
            let match_start_char = line_lower[..match_byte_pos].chars().count();
            let match_end_char = match_start_char + query_lower.chars().count();

            let mut score = 100;
            let title_lower = note.title.to_lowercase();
            if title_lower.contains(query_lower) {
                score += 50;
            }
            if match_start_char == 0 {
                score += 20;
            }
            if match_start_char == 0 || !line_chars.get(match_start_char.saturating_sub(1))
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false) {
                score += 10;
            }

            let context_size = 25;
            let start = match_start_char.saturating_sub(context_size);
            let end = (match_end_char + context_size).min(line_chars.len());

            let mut matched_line: String = line_chars[start..end].iter().collect();
            let display_match_start = match_start_char - start;
            let display_match_end = match_end_char - start;

            if start > 0 {
                matched_line = format!("...{}", matched_line);
            }
            if end < line_chars.len() {
                matched_line.push_str("...");
            }

            Some(ContentSearchResult {
                display_name: note.title.clone(),
                matched_line,
                line_number: line_num + 1,
                note_index: note_idx,
                folder_hint,
                score,
                match_start: display_match_start + if start > 0 { 3 } else { 0 },
                match_end: display_match_end + if start > 0 { 3 } else { 0 },
            })
        };

        if let Some(positions) = self.search_index.terms.get(&query_lower) {
            for &(note_idx, line_num, _) in positions.iter().take(MAX_EXACT_MATCHES) {
                if seen.insert((note_idx, line_num)) {
                    if let Some(lines) = self.search_index.lines.get(note_idx) {
                        if let Some(line) = lines.get(line_num) {
                            if let Some(result) = create_result(note_idx, line_num, line, &query_lower) {
                                results.push(result);
                            }
                        }
                    }
                }
            }
        }

        // Phase 2 Prefix matches - limit terms scanned to prevent freeze
        if results.len() < MAX_RESULTS {
            let mut terms_scanned = 0;
            let mut prefix_matches = 0;

            for (word, positions) in &self.search_index.terms {
                // Early exit conditions
                if terms_scanned >= MAX_PREFIX_TERMS_SCANNED || prefix_matches >= MAX_PREFIX_MATCHES {
                    break;
                }
                terms_scanned += 1;

                if word.starts_with(&query_lower) && word != &query_lower {
                    for &(note_idx, line_num, _) in positions.iter().take(50) {
                        if prefix_matches >= MAX_PREFIX_MATCHES {
                            break;
                        }
                        if seen.insert((note_idx, line_num)) {
                            if let Some(lines) = self.search_index.lines.get(note_idx) {
                                if let Some(line) = lines.get(line_num) {
                                    if let Some(result) = create_result(note_idx, line_num, line, &query_lower) {
                                        results.push(result);
                                        prefix_matches += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Phase 3 Line scan fallback for substring matches
        if results.len() < MAX_RESULTS {
            let mut notes_scanned = 0;
            'outer: for (note_idx, lines) in self.search_index.lines.iter().enumerate() {
                if notes_scanned >= MAX_LINE_SCAN_NOTES || results.len() >= MAX_RESULTS {
                    break;
                }
                notes_scanned += 1;

                for (line_num, line) in lines.iter().enumerate() {
                    if seen.contains(&(note_idx, line_num)) {
                        continue;
                    }
                    if line.to_lowercase().contains(&query_lower) {
                        if let Some(result) = create_result(note_idx, line_num, line, &query_lower) {
                            seen.insert((note_idx, line_num));
                            results.push(result);
                            if results.len() >= MAX_RESULTS {
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            b.score.cmp(&a.score)
                .then_with(|| a.display_name.cmp(&b.display_name))
                .then_with(|| a.line_number.cmp(&b.line_number))
        });
        results.truncate(MAX_RESULTS);
        results
    }

    // ==================== Mouse Selection Helpers ====================

    /// Convert mouse screen coordinates to editor row/col.
    /// Returns None if mouse is outside the editor area.
    pub fn screen_to_editor_coords(&self, mouse_x: u16, mouse_y: u16) -> Option<(usize, usize)> {
        let (inner_x, inner_y, inner_width, inner_height) = if self.zen_mode {
            (
                self.editor_area.x,
                self.editor_area.y,
                self.editor_area.width,
                self.editor_area.height,
            )
        } else {
            (
                self.editor_area.x + 1,
                self.editor_area.y + 1,
                self.editor_area.width.saturating_sub(2),
                self.editor_area.height.saturating_sub(2),
            )
        };

        if mouse_x < inner_x || mouse_x >= inner_x + inner_width ||
           mouse_y < inner_y || mouse_y >= inner_y + inner_height {
            return None;
        }

        let content_x_offset = self.editor.content_x_offset();
        let content_start_x = inner_x + content_x_offset;
        let rel_x = if mouse_x >= content_start_x {
            (mouse_x - content_start_x) as usize
        } else {
            0
        };
        let rel_y = (mouse_y - inner_y) as usize;

        let (row, col) = self.editor.visual_to_logical_coords(rel_y, rel_x);

        Some((row, col))
    }

    /// Check if mouse is in the auto-scroll zone (top or bottom edge).
    /// Returns scroll direction: -1 for up, 1 for down, 0 for no scroll.
    pub fn get_auto_scroll_direction(&self, mouse_y: u16) -> i8 {
        const SCROLL_THRESHOLD: u16 = 2;

        let (inner_y, inner_height) = if self.zen_mode {
            (
                self.editor_area.y,
                self.editor_area.height,
            )
        } else {
            (
                self.editor_area.y + 1,
                self.editor_area.height.saturating_sub(2),
            )
        };

        if mouse_y < inner_y + SCROLL_THRESHOLD && self.editor_scroll_top > 0 {
            -1 // Scroll up
        } else if mouse_y >= inner_y + inner_height - SCROLL_THRESHOLD {
            1 // Scroll down
        } else {
            0
        }
    }

    pub fn open_search_picker(&mut self) {
        self.search_picker = SearchPickerState::Open {
            mode: SearchPickerMode::Files,
            query: String::new(),
            file_results: Vec::new(),
            content_results: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            search_in_progress: false,
            search_id: 0,
        };
    }

    pub fn close_search_picker(&mut self) {
        self.search_picker = SearchPickerState::Closed;
    }

    pub fn toggle_search_picker_mode(&mut self) {
        let (new_mode, query) = if let SearchPickerState::Open {
            mode,
            query,
            selected_index,
            scroll_offset,
            ..
        } = &mut self.search_picker {
            *mode = match *mode {
                SearchPickerMode::Files => SearchPickerMode::Content,
                SearchPickerMode::Content => SearchPickerMode::Files,
            };
            // Reset selection and scroll
            *selected_index = 0;
            *scroll_offset = 0;
            (*mode, query.clone())
        } else {
            return;
        };

        match new_mode {
            SearchPickerMode::Content => {
                if !query.is_empty() {
                    self.start_content_search();
                }
            }
            SearchPickerMode::Files => {
                if query.is_empty() {
                    if let SearchPickerState::Open { file_results, .. } = &mut self.search_picker {
                        file_results.clear();
                    }
                } else {
                    let new_results = self.build_file_picker_results(&query);
                    if let SearchPickerState::Open { file_results, .. } = &mut self.search_picker {
                        *file_results = new_results;
                    }
                }
            }
        }
    }

    fn build_file_picker_results(&self, query: &str) -> Vec<FilePickerResult> {
        let query_lower = query.to_lowercase();

        let mut results: Vec<FilePickerResult> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(idx, note)| {
                let wiki_path = self.get_wiki_path_for_note(idx);

                let score = fuzzy_match(&note.title, query)
                    .or_else(|| wiki_path.as_ref().and_then(|p| fuzzy_match(p, query)))
                    .or_else(|| {
                        let title_lower = note.title.to_lowercase();
                        if title_lower.contains(&query_lower) {
                            Some(100)
                        } else if let Some(ref wp) = wiki_path {
                            if wp.to_lowercase().contains(&query_lower) {
                                Some(50)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });
                let score = score?;

                let folder_hint = wiki_path.and_then(|wp| {
                    wp.rfind('/').map(|pos| wp[..pos].to_string())
                });

                Some(FilePickerResult {
                    display_name: note.title.clone(),
                    folder_hint,
                    note_index: idx,
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score.cmp(&a.score).then_with(|| a.display_name.cmp(&b.display_name))
        });

        results
    }

    pub fn start_content_search(&mut self) {
        let query = if let SearchPickerState::Open { query, mode, .. } = &self.search_picker {
            if *mode != SearchPickerMode::Content || query.is_empty() {
                return;
            }
            query.clone()
        } else {
            return;
        };

        if self.search_index.ready {
            let results = self.search_with_index(&query);
            if let SearchPickerState::Open {
                content_results,
                search_in_progress,
                selected_index,
                scroll_offset,
                ..
            } = &mut self.search_picker {
                *content_results = results;
                *search_in_progress = false;
                *selected_index = 0;
                *scroll_offset = 0;
            }
            return;
        }

        self.next_search_id += 1;
        let search_id = self.next_search_id;

        if let SearchPickerState::Open {
            search_in_progress,
            search_id: state_search_id,
            ..
        } = &mut self.search_picker {
            *search_in_progress = true;
            *state_search_id = search_id;
        }

        let notes: Vec<(usize, String, String, Option<String>)> = self.notes
            .iter()
            .enumerate()
            .map(|(idx, note)| {
                let wiki_path = self.get_wiki_path_for_note(idx);
                let folder_hint = wiki_path.as_ref().and_then(|wp| {
                    wp.rfind('/').map(|pos| wp[..pos].to_string())
                });
                (idx, note.title.clone(), note.content.clone(), folder_hint)
            })
            .collect();

        let sender = self.content_search_sender.clone();

        // Spawn background thread for content search
        std::thread::spawn(move || {
            let query_lower = query.to_lowercase();
            let mut results: Vec<ContentSearchResult> = Vec::new();

            for (note_idx, title, content, folder_hint) in notes {
                let title_lower = title.to_lowercase();
                let title_matches = title_lower.contains(&query_lower);

                for (line_num, line) in content.lines().enumerate() {
                    let line_lower = line.to_lowercase();
                    if let Some(match_byte_pos) = line_lower.find(&query_lower) {
                        // Convert byte position to character position for Unicode support
                        let line_chars: Vec<char> = line.chars().collect();
                        let match_start_char = line_lower[..match_byte_pos].chars().count();
                        let match_end_char = match_start_char + query_lower.chars().count();

                        // Calculate score
                        let mut score = 100;
                        if title_matches {
                            score += 50; 
                        }
                        if match_start_char == 0 {
                            score += 20; 
                        }
                        // Word boundary bonus - use char position, not byte position
                        if match_start_char == 0 || !line_chars.get(match_start_char.saturating_sub(1))
                            .map(|c| c.is_alphanumeric())
                            .unwrap_or(false) {
                            score += 10;
                        }

                        // Get context around match (max 60 chars total)
                        let context_size = 25;
                        let start = match_start_char.saturating_sub(context_size);
                        let end = (match_end_char + context_size).min(line_chars.len());

                        let mut matched_line: String = line_chars[start..end].iter().collect();
                        let display_match_start = match_start_char - start;
                        let display_match_end = match_end_char - start;

                        // Add ellipsis if truncated
                        if start > 0 {
                            matched_line = format!("...{}", matched_line);
                        }
                        if end < line_chars.len() {
                            matched_line.push_str("...");
                        }

                        results.push(ContentSearchResult {
                            display_name: title.clone(),
                            matched_line,
                            line_number: line_num + 1, 
                            note_index: note_idx,
                            folder_hint: folder_hint.clone(),
                            score,
                            match_start: display_match_start + if start > 0 { 3 } else { 0 },
                            match_end: display_match_end + if start > 0 { 3 } else { 0 },
                        });
                    }
                }
            }

            results.sort_by(|a, b| {
                b.score.cmp(&a.score)
                    .then_with(|| a.display_name.cmp(&b.display_name))
                    .then_with(|| a.line_number.cmp(&b.line_number))
            });

            results.truncate(500);

            let _ = sender.send(ContentSearchResponse { search_id, results });
        });
    }

    /// Polls for content search results (call in main loop)
    pub fn poll_content_search(&mut self) {
        while let Ok(response) = self.content_search_receiver.try_recv() {
            if let SearchPickerState::Open {
                search_id,
                content_results,
                search_in_progress,
                selected_index,
                scroll_offset,
                ..
            } = &mut self.search_picker {
                if response.search_id == *search_id {
                    *content_results = response.results;
                    *search_in_progress = false;
                    *selected_index = 0;
                    *scroll_offset = 0;
                }
            }
        }
    }

    pub fn is_content_search_in_progress(&self) -> bool {
        if let SearchPickerState::Open { search_in_progress, .. } = &self.search_picker {
            *search_in_progress
        } else {
            false
        }
    }

    pub fn update_search_picker_results(&mut self) {
        let (query, mode) = if let SearchPickerState::Open { query, mode, .. } = &self.search_picker {
            (query.clone(), *mode)
        } else {
            return;
        };

        match mode {
            SearchPickerMode::Files => {
                if query.is_empty() {
                    if let SearchPickerState::Open { file_results, selected_index, scroll_offset, .. } = &mut self.search_picker {
                        file_results.clear();
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                } else {
                    let new_results = self.build_file_picker_results(&query);
                    if let SearchPickerState::Open { file_results, selected_index, scroll_offset, .. } = &mut self.search_picker {
                        *file_results = new_results;
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                }
            }
            SearchPickerMode::Content => {
                if query.is_empty() {
                    if let SearchPickerState::Open { content_results, selected_index, scroll_offset, search_in_progress, .. } = &mut self.search_picker {
                        content_results.clear();
                        *selected_index = 0;
                        *scroll_offset = 0;
                        *search_in_progress = false;
                    }
                } else {
                    self.start_content_search();
                }
            }
        }
    }

    pub fn select_search_picker_result(&mut self) {
        let result_info = if let SearchPickerState::Open {
            mode, file_results, content_results, selected_index, ..
        } = &self.search_picker {
            match mode {
                SearchPickerMode::Files => {
                    file_results.get(*selected_index).map(|r| (r.note_index, None))
                }
                SearchPickerMode::Content => {
                    content_results.get(*selected_index).map(|r| (r.note_index, Some(r.line_number)))
                }
            }
        } else {
            None
        };

        let Some((note_index, line_number)) = result_info else {
            self.search_picker = SearchPickerState::Closed;
            return;
        };

        if note_index < self.notes.len() {
            if let Some(note) = self.notes.get(note_index) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.config.notes_path();
                    let mut current = file_path.parent();
                    let mut needs_rebuild = false;
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        if !self.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                            self.folder_states.insert(parent.to_path_buf(), true);
                            needs_rebuild = true;
                        }
                        current = parent.parent();
                    }
                    if needs_rebuild {
                        Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
                        self.rebuild_sidebar_items();
                    }
                }
            }

            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index: idx_note } = &item.kind {
                    if *idx_note == note_index {
                        self.end_buffer_search();
                        self.selected_sidebar_index = idx;
                        self.selected_note = note_index;
                        self.push_navigation_history(note_index);
                        self.content_cursor = 0;
                        self.content_scroll_offset = 0;
                        self.update_content_items();
                        self.update_outline();

                        if let Some(target_line) = line_number {
                            let target_line_0indexed = target_line.saturating_sub(1);
                            let mut best_match_idx = 0;
                            let mut best_match_diff = usize::MAX;

                            for (i, &source_line) in self.content_item_source_lines.iter().enumerate() {
                                if source_line == target_line_0indexed {
                                    best_match_idx = i;
                                    break;
                                } else if source_line < target_line_0indexed {
                                    let diff = target_line_0indexed - source_line;
                                    if diff < best_match_diff {
                                        best_match_diff = diff;
                                        best_match_idx = i;
                                    }
                                } else {
                                    let diff = source_line - target_line_0indexed;
                                    if diff < best_match_diff {
                                        best_match_idx = i;
                                    }
                                    break;
                                }
                            }

                            self.content_cursor = best_match_idx.min(self.content_items.len().saturating_sub(1));

                            let visible_height = 20usize; // Approximate visible lines
                            let target_scroll = self.content_cursor.saturating_sub(visible_height / 3);
                            self.content_scroll_offset = target_scroll;
                        }

                        self.focus = Focus::Content;
                        break;
                    }
                }
            }
        }

        self.search_picker = SearchPickerState::Closed;
    }

    pub fn search_picker_select_prev(&mut self) {
        // Must match POPUP_MAX_VISIBLE_ITEMS / POPUP_MAX_VISIBLE_ITEMS_CONTENT in ui/file_picker.rs
        const MAX_VISIBLE_FILES: usize = 10;
        const MAX_VISIBLE_CONTENT: usize = 18;
        if let SearchPickerState::Open { mode, file_results, content_results, selected_index, scroll_offset, .. } = &mut self.search_picker {
            let (results_len, max_visible) = match mode {
                SearchPickerMode::Files => (file_results.len(), MAX_VISIBLE_FILES),
                SearchPickerMode::Content => (content_results.len(), MAX_VISIBLE_CONTENT),
            };

            if results_len == 0 {
                return;
            }

            if *selected_index > 0 {
                *selected_index -= 1;
            } else {
                *selected_index = results_len - 1;
                *scroll_offset = results_len.saturating_sub(max_visible);
                return;
            }

            if *selected_index < *scroll_offset {
                *scroll_offset = *selected_index;
            }
        }
    }

    pub fn search_picker_select_next(&mut self) {
        // Must match POPUP_MAX_VISIBLE_ITEMS / POPUP_MAX_VISIBLE_ITEMS_CONTENT in ui/file_picker.rs
        const MAX_VISIBLE_FILES: usize = 10;
        const MAX_VISIBLE_CONTENT: usize = 18;
        if let SearchPickerState::Open { mode, file_results, content_results, selected_index, scroll_offset, .. } = &mut self.search_picker {
            let (results_len, max_visible) = match mode {
                SearchPickerMode::Files => (file_results.len(), MAX_VISIBLE_FILES),
                SearchPickerMode::Content => (content_results.len(), MAX_VISIBLE_CONTENT),
            };

            if results_len == 0 {
                return;
            }

            if *selected_index < results_len - 1 {
                *selected_index += 1;
            } else {
                *selected_index = 0;
                *scroll_offset = 0;
                return;
            }

            let visible_end = *scroll_offset + max_visible;
            if *selected_index >= visible_end {
                *scroll_offset = *selected_index - max_visible + 1;
            }
        }
    }

    pub fn search_picker_push_char(&mut self, c: char) {
        if let SearchPickerState::Open { query, .. } = &mut self.search_picker {
            query.push(c);
        }
        self.update_search_picker_results();
    }

    pub fn search_picker_pop_char(&mut self) {
        if let SearchPickerState::Open { query, .. } = &mut self.search_picker {
            query.pop();
        }
        self.update_search_picker_results();
    }
    pub fn is_inside_search_picker(&self, x: u16, y: u16) -> bool {
        let area = self.search_picker_area;
        x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
    }
    /// Handle mouse click on search picker results
    pub fn search_picker_click(&mut self, x: u16, y: u16) -> u8 {
        let results_area = self.search_picker_results_area;

        // Check if click is in results area
        if x < results_area.x || x >= results_area.x + results_area.width
            || y < results_area.y || y >= results_area.y + results_area.height
        {
            return 0;
        }

        // Calculate which row was clicked (relative to results area)
        let clicked_row = (y - results_area.y) as usize;

        if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            selected_index,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            let clicked_index = match mode {
                SearchPickerMode::Content => {
                    *scroll_offset + clicked_row
                }
                SearchPickerMode::Files => {
                    let mut accumulated_lines = 0;
                    let mut target_index = None;

                    for (i, result) in file_results.iter().enumerate().skip(*scroll_offset) {
                        let item_lines = if result.folder_hint.is_some() { 2 } else { 1 };
                        if clicked_row < accumulated_lines + item_lines {
                            target_index = Some(i);
                            break;
                        }
                        accumulated_lines += item_lines;
                    }

                    target_index.unwrap_or(*scroll_offset + clicked_row)
                }
            };

            let results_len = match mode {
                SearchPickerMode::Files => file_results.len(),
                SearchPickerMode::Content => content_results.len(),
            };

            if clicked_index < results_len {
                *selected_index = clicked_index;
                let now = std::time::Instant::now();
                let is_double_click = if let Some((last_time, last_index)) = self.search_picker_last_click {
                    last_index == clicked_index && now.duration_since(last_time).as_millis() < 400
                } else {
                    false
                };

                self.search_picker_last_click = Some((now, clicked_index));

                return if is_double_click { 2 } else { 1 };
            }
        }
        0
    }

    pub fn search_picker_scroll_up(&mut self) {
        if let SearchPickerState::Open { scroll_offset, .. } = &mut self.search_picker {
            if *scroll_offset > 0 {
                *scroll_offset -= 1;
            }
        }
    }
    pub fn search_picker_scroll_down(&mut self) {
        const MAX_VISIBLE_FILES: usize = 10; // Must match POPUP_MAX_VISIBLE_ITEMS
        const MAX_VISIBLE_CONTENT: usize = 18; // Must match POPUP_MAX_VISIBLE_ITEMS_CONTENT
        if let SearchPickerState::Open {
            mode,
            file_results,
            content_results,
            scroll_offset,
            ..
        } = &mut self.search_picker
        {
            let (results_len, max_visible) = match mode {
                SearchPickerMode::Files => (file_results.len(), MAX_VISIBLE_FILES),
                SearchPickerMode::Content => (content_results.len(), MAX_VISIBLE_CONTENT),
            };

            if *scroll_offset + max_visible < results_len {
                *scroll_offset += 1;
            }
        }
    }
}

fn fetch_remote_image_blocking(url: &str) -> Option<DynamicImage> {
    use std::io::Read;

    let response = ureq::get(url)
        .set("User-Agent", "ekphos/0.4")
        .call()
        .ok()?;

    let content_type = response
        .header("Content-Type")
        .unwrap_or("")
        .to_lowercase();

    if !content_type.starts_with("image/") {
        return None;
    }

    let mut bytes = Vec::new();
    response.into_reader().take(10 * 1024 * 1024).read_to_end(&mut bytes).ok()?;

    image::load_from_memory(&bytes).ok()
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// fuzzy matching algorithm that scores matches based on:
/// - empty query matches everything with base score
/// - exact match: highest score
/// - prefix match: high score
/// - consecutive character matches: bonus points
/// - earlier matches in the string: bonus points
/// returns None if no match, Some(score) if matched
fn fuzzy_match(text: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    let text_chars: Vec<char> = text_lower.chars().collect();
    let query_chars: Vec<char> = query_lower.chars().collect();

    if text_lower == query_lower {
        return Some(1000);
    }

    if text_lower.starts_with(&query_lower) {
        return Some(900 + (100 - text.len() as i32).max(0));
    }

    if text_lower.contains(&query_lower) {
        let pos = text_lower.find(&query_lower).unwrap_or(0);
        return Some(500 + (50 - pos as i32).max(0));
    }

    let mut text_idx = 0;
    let mut query_idx = 0;
    let mut score: i32 = 0;
    let mut prev_matched = false;
    let mut consecutive_bonus = 0;

    while text_idx < text_chars.len() && query_idx < query_chars.len() {
        if text_chars[text_idx] == query_chars[query_idx] {
            score += (100 - text_idx as i32).max(1);
            if prev_matched {
                consecutive_bonus += 20;
            }

            if text_idx == 0 || matches!(text_chars.get(text_idx.saturating_sub(1)), Some(' ' | '_' | '-')) {
                score += 30;
            }

            prev_matched = true;
            query_idx += 1;
        } else {
            prev_matched = false;
        }
        text_idx += 1;
    }

    if query_idx == query_chars.len() {
        Some(score + consecutive_bonus)
    } else {
        None
    }
}
