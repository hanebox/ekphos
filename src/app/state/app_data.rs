use super::*;

pub struct App {
    pub(crate) dependencies: AppDependencies,
    pub(crate) vault: ekphos_vault::Vault,
    pub(crate) body_cache: ekphos_vault::BodyCache,
    pub(crate) active_note_id: Option<NoteId>,
    pub(crate) active_fingerprint: Option<ekphos_vault::FileFingerprint>,
    pub(crate) active_body: Option<Arc<str>>,
    pub(crate) document_generation: u64,
    pub notes: Vec<Note>,
    pub selected_note: usize,
    #[allow(dead_code)]
    pub list_state: ListState,
    pub focus: Focus,
    pub mode: Mode,
    pub editor: Editor,
    pub picker: Option<Picker>,
    pub image_cache_dir: PathBuf,
    /// Persistent terminal protocol state for each rendered image placement.
    /// Keeping this across frames prevents protocol IDs from changing on scroll.
    pub image_states: HashMap<String, ImageState>,
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
    pub inline_image_rects: Vec<InlineImageRect>,
    pub mouse_hover_inline_image: Option<(usize, usize)>,
    pub selected_link_index: usize,
    pub details_open_states: HashMap<usize, bool>,
    pub heading_fold_states: HashMap<usize, bool>, // content_item index -> is_folded
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
    pub keymap: Keymap,
    pub keybinding_warning: Option<KeybindingWarning>,
    pub status_message: Option<String>, // Status message shown next to path
    pub toast: Option<Toast>,           // Transient error/info notification overlay
    pub buffer_search: BufferSearchState,
    pub help_scroll: usize,
    // Graph view state
    pub graph_view: GraphViewState,
    pub graph_index: Option<Arc<GraphIndex>>,
    pub graph_index_receiver: Receiver<(u64, GraphIndex)>,
    pub graph_index_generation: u64,
    pub graph_indexing: bool,
    pub graph_layout_receiver: Receiver<GraphLayoutWorkerResult>,
    pub graph_layout_generation: u64,
    // Sidebar sorting
    pub sort_mode: SortMode,
    // Navigation history (like browser back/forward)
    pub navigation_history: Vec<NavigationEntry>,
    pub navigation_index: usize,
    // Frontmatter visibility
    pub frontmatter_hidden: bool,
    // Theme selector modal (Ctrl+T)
    pub theme_picker: ThemePicker,
    // Global search picker (file/content search)
    pub search_picker: SearchPickerState,
    pub search_picker_area: ratatui::layout::Rect,
    pub search_picker_results_area: ratatui::layout::Rect,
    pub search_picker_last_click: Option<(std::time::Instant, usize)>, // (time, selected_index)
    pub next_search_id: u64,
    pub(crate) search_generation: u64,
    pub(crate) search_generation_signal: Arc<AtomicU64>,
    pub(crate) search_worker: Option<SearchWorker>,
    // Search index for fast content search
    pub search_index: Option<Arc<SearchIndex>>,
    /// Channel to receive completed index from background thread
    pub index_receiver: Receiver<(u64, SearchIndex)>,
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
    pub note_id: NoteId,
    pub content_cursor: usize,
    pub content_scroll_offset: usize,
}
