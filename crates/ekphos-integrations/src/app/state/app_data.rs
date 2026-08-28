use super::*;

use std::ops::{Deref, DerefMut};

pub struct App {
    pub(crate) dependencies: AppDependencies,
    pub vault: VaultState,
    pub document: DocumentState,
    pub editor: EditorSession,
    pub search: SearchState,
    pub graph: GraphState,
    pub workers: WorkerSet,
    pub images: ImageService,
    pub state: UiState,
    pub(crate) memory_reclaim_pending: bool,
}

/// Filesystem catalog ownership. Additional catalog-facing state moves here as
/// callers are migrated away from the former flat `App` aggregate.
pub struct VaultState {
    pub(crate) inner: ekphos_vault::Vault,
    pub(crate) body_cache: ekphos_vault::BodyCache,
    pub(crate) catalog_generation: u64,
    pub notes: Vec<Note>,
    pub selected_note: usize,
    pub list_state: ListState,
    pub file_tree: Vec<FileTreeItem>,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_sidebar_index: usize,
    pub folder_states: HashMap<PathBuf, bool>,
    pub target_folder: Option<PathBuf>,
    pub sort_mode: SortMode,
    pub cut_buffer: Option<CutItem>,
}

impl VaultState {
    pub(crate) fn new(inner: ekphos_vault::Vault, list_state: ListState) -> Self {
        Self {
            inner,
            body_cache: ekphos_vault::BodyCache::default(),
            catalog_generation: 0,
            notes: Vec::new(),
            selected_note: 0,
            list_state,
            file_tree: Vec::new(),
            sidebar_items: Vec::new(),
            selected_sidebar_index: 0,
            folder_states: HashMap::new(),
            target_folder: None,
            sort_mode: SortMode::default(),
            cut_buffer: None,
        }
    }
    pub(crate) fn replace(&mut self, vault: ekphos_vault::Vault) {
        self.inner = vault;
    }
}

impl Deref for VaultState {
    type Target = ekphos_vault::Vault;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for VaultState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// The immutable active document and its derived render/navigation model.
pub struct DocumentState {
    pub(crate) active_note_id: Option<NoteId>,
    pub(crate) active_fingerprint: Option<ekphos_vault::FileFingerprint>,
    pub active_document: Option<DocumentSnapshot>,
    pub document_generation: u64,
    pub(crate) document_parse_key: Option<(u64, u64, bool, bool)>,
    #[doc(hidden)]
    pub document_parse_count: u64,
    pub outline: Vec<OutlineItem>,
    pub outline_state: ListState,
    pub content_cursor: usize,
    pub content_scroll_offset: usize,
    pub content_items: Vec<ContentItem>,
    pub document_tables: Vec<TableMetadata>,
    pub document_links: Vec<LinkInfo>,
    pub document_link_ranges: Vec<DocumentLinkRange>,
    pub content_render_scratch: ContentRenderScratch,
    pub selected_link_index: usize,
    pub details_open_states: HashMap<usize, bool>,
    pub heading_fold_states: HashMap<usize, bool>,
    pub(crate) wiki_target_cache_generation: u64,
    pub(crate) wiki_target_cache: HashSet<String>,
    pub navigation_history: Vec<NavigationEntry>,
    pub navigation_index: usize,
    pub frontmatter_hidden: bool,
}

impl DocumentState {
    pub(crate) fn new(frontmatter_hidden: bool) -> Self {
        Self {
            active_note_id: None,
            active_fingerprint: None,
            active_document: None,
            document_generation: 0,
            document_parse_key: None,
            document_parse_count: 0,
            outline: Vec::new(),
            outline_state: ListState::default(),
            content_cursor: 0,
            content_scroll_offset: 0,
            content_items: Vec::new(),
            document_tables: Vec::new(),
            document_links: Vec::new(),
            document_link_ranges: Vec::new(),
            content_render_scratch: ContentRenderScratch::default(),
            selected_link_index: 0,
            details_open_states: HashMap::new(),
            heading_fold_states: HashMap::new(),
            wiki_target_cache_generation: u64::MAX,
            wiki_target_cache: HashSet::new(),
            navigation_history: Vec::new(),
            navigation_index: 0,
            frontmatter_hidden,
        }
    }
}

/// Mutable editing state. Wrapping the editor keeps its public API intact while
/// making the application/editor ownership boundary explicit.
pub struct EditorSession {
    inner: Editor,
    pub mode: Mode,
    pub vim: VimState,
    pub visual_line_anchor: Option<usize>,
    pub visual_line_current: Option<usize>,
    pub visual_block_anchor: Option<Position>,
    pub block_insert_state: Option<BlockInsertState>,
    pub edit_preview_position: Option<(usize, usize)>,
    pub floating_cursor_mode: bool,
    pub editor_scroll_top: usize,
    pub editor_view_height: usize,
    pub pending_operator: Option<char>,
    pub pending_delete: Option<DeleteType>,
    pub mouse_button_held: bool,
    pub mouse_drag_start: Option<(u16, u16)>,
    pub last_mouse_y: u16,
    pub editor_area: Rect,
    pub context_menu_state: ContextMenuState,
    pub wiki_autocomplete: WikiAutocompleteState,
    pub pending_wiki_target: Option<String>,
    pub highlight_version: u64,
    pub(crate) highlight_requested_rows: Option<(usize, usize)>,
    pub highlight_pending: bool,
}

impl EditorSession {
    pub(crate) fn new(inner: Editor, floating_cursor_mode: bool) -> Self {
        Self {
            inner,
            mode: Mode::Normal,
            vim: VimState::new(),
            visual_line_anchor: None,
            visual_line_current: None,
            visual_block_anchor: None,
            block_insert_state: None,
            edit_preview_position: None,
            floating_cursor_mode,
            editor_scroll_top: 0,
            editor_view_height: 0,
            pending_operator: None,
            pending_delete: None,
            mouse_button_held: false,
            mouse_drag_start: None,
            last_mouse_y: 0,
            editor_area: Rect::default(),
            context_menu_state: ContextMenuState::None,
            wiki_autocomplete: WikiAutocompleteState::None,
            pending_wiki_target: None,
            highlight_version: 0,
            highlight_requested_rows: None,
            highlight_pending: false,
        }
    }
    pub(crate) fn replace(&mut self, editor: Editor) {
        self.inner = editor;
    }

    pub fn sync_scroll_offset(&mut self) {
        self.inner.set_scroll_offset(self.editor_scroll_top);
    }
}

impl Deref for EditorSession {
    type Target = Editor;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for EditorSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Sidebar, buffer, and global-search state. Closing a picker drops its result
/// vectors because `SearchPickerState::Closed` carries no result payload.
pub struct SearchState {
    pub search_active: bool,
    pub search_query: String,
    pub filtered_indices: Vec<usize>,
    pub search_matched_notes: Vec<usize>,
    pub pre_search_folder_states: Option<HashMap<PathBuf, bool>>,
    pub pre_search_sidebar_index: Option<usize>,
    pub buffer_search: BufferSearchState,
    pub search_picker: SearchPickerState,
    pub search_picker_area: Rect,
    pub search_picker_results_area: Rect,
    pub search_picker_last_click: Option<(std::time::Instant, usize)>,
    pub next_search_id: u64,
    pub(crate) search_generation: u64,
    pub(crate) search_generation_signal: Arc<AtomicU64>,
    pub search_index: Option<Arc<SearchIndex>>,
    pub indexing_in_progress: bool,
    pub index_progress: Arc<AtomicUsize>,
    pub index_total: Arc<AtomicUsize>,
    pub index_started_at: Option<std::time::Instant>,
}

impl SearchState {
    pub(crate) fn new() -> Self {
        Self {
            search_active: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            search_matched_notes: Vec::new(),
            pre_search_folder_states: None,
            pre_search_sidebar_index: None,
            buffer_search: BufferSearchState::new(),
            search_picker: SearchPickerState::Closed,
            search_picker_area: Rect::default(),
            search_picker_results_area: Rect::default(),
            search_picker_last_click: None,
            next_search_id: 0,
            search_generation: 0,
            search_generation_signal: Arc::new(AtomicU64::new(0)),
            search_index: None,
            indexing_in_progress: false,
            index_progress: Arc::new(AtomicUsize::new(0)),
            index_total: Arc::new(AtomicUsize::new(0)),
            index_started_at: None,
        }
    }
}

/// Joinable background services owned by the application lifecycle.
pub struct WorkerSet {
    pub graph: Option<GraphWorker>,
    pub(crate) retired_graph: Option<GraphWorker>,
    pub search: Option<SearchWorker>,
    pub index_receiver: Receiver<(u64, SearchIndex)>,
    pub highlight: Option<HighlightWorker>,
}

impl WorkerSet {
    pub(crate) fn new(index_receiver: Receiver<(u64, SearchIndex)>) -> Self {
        Self { graph: None, retired_graph: None, search: None, index_receiver, highlight: Some(HighlightWorker::new()) }
    }
}

/// Bounded decode/fetch service plus terminal-protocol placements for the
/// active document.
pub struct ImageService {
    pub(crate) worker: ImageWorkerService,
    pub picker: Option<Picker>,
    pub image_states: HashMap<String, ImageState>,
    pub(crate) render_epoch: u64,
    pub(crate) protocol_bytes: usize,
}

impl ImageService {
    pub(crate) fn new(worker: ImageWorkerService, picker: Option<Picker>) -> Self {
        Self { worker, picker, image_states: HashMap::new(), render_epoch: 0, protocol_bytes: 0 }
    }
}

/// Lazily allocated graph interaction state. A closed session survives only
/// until the next redraw so close input remains constant-time.
#[derive(Default)]
pub struct GraphState {
    pub session: Option<Box<GraphSession>>,
    pub(crate) retired_session: Option<Box<GraphSession>>,
    #[doc(hidden)]
    pub last_reused_files: usize,
    #[doc(hidden)]
    pub last_parsed_files: usize,
}

pub struct GraphSession {
    pub graph_view: GraphViewState,
    pub graph_index: Option<Arc<GraphIndex>>,
    pub graph_index_generation: u64,
    pub graph_indexing: bool,
    pub graph_layout_generation: u64,
}

impl GraphSession {
    fn new(graph_index: Option<Arc<GraphIndex>>) -> Self {
        Self { graph_view: GraphViewState::default(), graph_index, graph_index_generation: 0, graph_indexing: false, graph_layout_generation: 0 }
    }
}

impl GraphState {
    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn is_indexing(&self) -> bool {
        self.session.as_deref().is_some_and(|session| session.graph_indexing)
    }

    pub fn is_layout_pending(&self) -> bool {
        self.session.as_deref().is_some_and(|session| session.graph_view.layout_pending)
    }
    pub(crate) fn activate(&mut self) {
        let _ = self.deref_mut();
    }
    pub(crate) fn release(&mut self) {
        self.retired_session = self.session.take();
    }
    pub(crate) fn invalidate(&mut self) {
        self.session = None;
        self.retired_session = None;
    }
}

impl Deref for GraphState {
    type Target = GraphSession;
    fn deref(&self) -> &Self::Target {
        self.session.as_deref().expect("graph session is inactive")
    }
}

impl DerefMut for GraphState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session.get_or_insert_with(|| self.retired_session.take().unwrap_or_else(|| Box::new(GraphSession::new(None))))
    }
}

/// Presentation and interaction state retained by the application shell.
/// Feature-owned fields are removed from this structure as their callers move
/// to `VaultState`, `DocumentState`, `EditorSession`, `SearchState`, and
/// `WorkerSet`.
pub struct UiState {
    pub focus: Focus,
    pub show_welcome: bool,
    pub theme: Theme,
    pub config: Config,
    pub dialog: DialogState,
    pub input_buffer: String,
    pub dialog_error: Option<String>,
    pub content_area: Rect,
    pub sidebar_area: Rect,
    pub outline_area: Rect,
    pub mouse_hover_item: Option<usize>,
    pub content_item_rects: Vec<(usize, Rect)>,
    pub inline_image_rects: Vec<InlineImageRect>,
    pub mouse_hover_inline_image: Option<(usize, usize)>,
    pub(crate) syntax_service: SyntaxService,
    pub sidebar_collapsed: bool,
    pub outline_collapsed: bool,
    pub zen_mode: bool,
    pub needs_full_clear: bool,
    pub keymap: Keymap,
    pub keybinding_warning: Option<KeybindingWarning>,
    pub status_message: Option<String>, // Status message shown next to path
    pub toast: Option<Toast>,           // Transient error/info notification overlay
    pub help_scroll: usize,
    pub theme_picker: Option<ThemePicker>,
}

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
