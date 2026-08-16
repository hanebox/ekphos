use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockInsertMode {
    Insert,
    Append,
}

/// Severity of a transient [`Toast`] notification, used to pick its accent color.
///
/// `Info`/`Success` round out the notification API for future callers; only
/// `Error` is raised today (see [`App::show_error_toast`]).
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ToastKind {
    Error,
    Info,
    Success,
}

/// A short-lived, non-blocking notification shown as a floating overlay.
///
/// Toasts are how recoverable errors (e.g. a clipboard read failing) reach the
/// user without writing to stdout/stderr, which would corrupt the TUI.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub shown_at: std::time::Instant,
}

impl Toast {
    /// How long a toast stays on screen before auto-dismissing.
    const TTL: std::time::Duration = std::time::Duration::from_secs(4);

    pub fn is_expired_at(&self, now: std::time::Instant) -> bool {
        now.saturating_duration_since(self.shown_at) >= Self::TTL
    }
}

#[derive(Debug, Clone)]
pub struct BlockInsertState {
    pub mode: BlockInsertMode,
    pub rows: (usize, usize),
    pub insert_col: usize,
    pub active_row: usize,
    pub start_col: usize,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub id: NoteId,
    pub title: String,
    pub content: String,
    pub file_path: Option<PathBuf>,
    pub modified_time: Option<std::time::SystemTime>,
    pub created_time: Option<std::time::SystemTime>,
    pub frontmatter: Option<ekphos_vault::Frontmatter>,
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
    ThemeSelector,
}

/// State for the theme selector modal (opened with Ctrl+T). Live-previews the
/// highlighted theme as the user navigates; the original theme is restored on
/// cancel and the selected one is persisted to config on confirm.
#[derive(Debug, Clone, Default)]
pub struct ThemePicker {
    pub themes: Vec<ThemeEntry>,
    pub selected: usize,
    pub scroll_offset: usize,
    /// Theme name active when the picker was opened, restored on Esc.
    pub original_theme_name: String,
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
    pub selected_note_index: Option<usize>,
    pub root_note_index: usize,
    pub mode: GraphMode,
    pub depth: usize,
    pub link_scope: GraphLinkScope,
    pub filter_query: String,
    pub filter_draft: String,
    pub filter_before_edit: String,
    pub filter_editing: bool,
    pub show_orphans: bool,
    pub help_visible: bool,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub index_pending: bool,
    pub layout_pending: bool,
    pub global_positions: HashMap<NoteId, (f32, f32)>,
    pub global_fingerprint: Option<u64>,
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub zoom: f32,
    pub dirty: bool,
    pub drag_start: Option<(u16, u16)>,
    pub is_panning: bool,
    pub dragging_node: Option<usize>,
    pub view_width: f32,
    pub view_height: f32,
    pub graph_area: Rect,
    pub needs_center: bool,
    pub last_click: Option<(std::time::Instant, usize)>,
}

impl Default for GraphViewState {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            selected_node: None,
            selected_note_index: None,
            root_note_index: 0,
            mode: GraphMode::Local,
            depth: 1,
            link_scope: GraphLinkScope::All,
            filter_query: String::new(),
            filter_draft: String::new(),
            filter_before_edit: String::new(),
            filter_editing: false,
            show_orphans: true,
            help_visible: false,
            total_nodes: 0,
            total_edges: 0,
            index_pending: false,
            layout_pending: false,
            global_positions: HashMap::new(),
            global_fingerprint: None,
            viewport_x: 0.0,
            viewport_y: 0.0,
            zoom: 1.0,
            dirty: true,
            drag_start: None,
            is_panning: false,
            dragging_node: None,
            view_width: 100.0,
            view_height: 50.0,
            graph_area: Rect::default(),
            needs_center: false,
            last_click: None,
        }
    }
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
    pub image: SlicedProtocol,
    pub size: Size,
}

#[derive(Debug, Clone)]
pub struct InlineImageRect {
    pub item_index: usize,
    pub selection_index: usize,
    pub rect: Rect,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

impl Alignment {
    /// Classify a GFM table separator cell (e.g. `:---`, `---:`, `:---:`, `---`)
    /// into its alignment. Any cell without a leading `:` is treated as Left
    /// (matches GFM's default-left convention).
    pub fn from_separator_cell(cell: &str) -> Alignment {
        let t = cell.trim();
        match (t.starts_with(':'), t.ends_with(':')) {
            (true, true) => Alignment::Center,
            (false, true) => Alignment::Right,
            _ => Alignment::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ContentItem {
    TextLine(String),
    Image(String),
    CodeLine(String),
    CodeFence(String),
    TaskItem {
        text: String,
        checked: bool,
        line_index: usize,
        indent: usize,
    },
    TableRow {
        cells: Vec<String>,
        is_separator: bool,
        is_header: bool,
        column_widths: Vec<usize>,
        alignments: Vec<Alignment>,
    },
    Details {
        summary: String,
        content_lines: Vec<String>,
        id: usize,
    },
    FrontmatterLine {
        key: String,
        value: String,
    },
    FrontmatterDelimiter,
    TagBadges {
        tags: Vec<String>,
        date: Option<String>,
    },
}
