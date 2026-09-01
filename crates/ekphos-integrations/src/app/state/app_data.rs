use super::*;

use std::ops::{Deref, DerefMut};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct App {
    pub(crate) dependencies: AppDependencies,
    pub vault: VaultState,
    pub document: DocumentState,
    pub structured: StructuredDocumentState,
    pub editor: EditorSession,
    pub search: SearchState,
    pub graph: GraphState,
    pub workers: WorkerSet,
    pub images: ImageService,
    pub state: UiState,
    pub(crate) memory_reclaim_pending: bool,
}

/// Parsed state for non-Markdown documents. It is kept separate from
/// `DocumentState` so Bases and Canvas never masquerade as Markdown content.
pub struct StructuredDocumentState {
    pub base: BaseViewState,
    pub canvas: CanvasViewState,
    pub(crate) parse_key: Option<(NoteId, u64, u64)>,
    pub(crate) last_vault_poll: std::time::Instant,
    pub(crate) vault_signature: u64,
    pub(crate) base_worker: super::structured::BaseWorker,
}

impl StructuredDocumentState {
    pub(crate) fn new(now: std::time::Instant) -> Self {
        Self { base: BaseViewState::default(), canvas: CanvasViewState::default(), parse_key: None, last_vault_poll: now, vault_signature: 0, base_worker: super::structured::BaseWorker::new() }
    }
}

#[derive(Debug, Default)]
pub struct BaseViewState {
    pub(crate) compiled: Option<ekphos_bases::CompiledBase>,
    pub(crate) corpus: ekphos_bases::Corpus,
    pub result: Option<ekphos_bases::BaseResult>,
    pub error: Option<String>,
    pub selected_row: usize,
    pub row_offset: usize,
    pub column_offset: usize,
    pub view_index: usize,
    pub view_count: usize,
    pub loading: bool,
    pub(crate) request_generation: u64,
    pub row_rects: Vec<(usize, Rect)>,
    pub column_left_rect: Option<Rect>,
    pub column_right_rect: Option<Rect>,
}

#[derive(Debug)]
pub struct CanvasViewState {
    pub document: Option<ekphos_canvas::Canvas>,
    pub diagnostics: Vec<String>,
    pub error: Option<String>,
    pub selected_node: usize,
    pub selected_edge: Option<usize>,
    pub hovered_node: Option<usize>,
    pub hovered_edge: Option<usize>,
    pub viewport_x: f64,
    pub viewport_y: f64,
    pub zoom: f64,
    pub needs_fit: bool,
    pub view_area: Rect,
    pub node_rects: Vec<(usize, Rect)>,
    pub edge_cells: Vec<(usize, ratatui::layout::Position)>,
    pub handle_rects: Vec<(ekphos_canvas::CanvasSide, Rect)>,
    pub resize_rects: Vec<(CanvasResizeHandle, Rect)>,
    pub hovered_resize: Option<(CanvasResizeHandle, ratatui::layout::Position)>,
    pub interaction: CanvasInteraction,
    pub editor: Option<CanvasNodeEditor>,
    pub last_click: Option<(std::time::Instant, usize)>,
    pub(crate) undo: Vec<ekphos_canvas::Canvas>,
    pub(crate) redo: Vec<ekphos_canvas::Canvas>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasNodeEditField {
    Text,
    Link,
    GroupLabel,
}

impl CanvasNodeEditField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Link => "link",
            Self::GroupLabel => "group name",
        }
    }

    pub fn multiline(self) -> bool {
        self == Self::Text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasNodeEditor {
    pub node: usize,
    pub field: CanvasNodeEditField,
    pub draft: String,
    pub cursor: usize,
    pub viewport_width: usize,
    pub viewport_height: usize,
    pub scroll_row: usize,
    pub scroll_column: usize,
    pub preferred_column: Option<usize>,
    pub follow_cursor: bool,
    pub editor_area: Rect,
    pub hit_rows: Vec<CanvasEditorHitRow>,
    pub total_rows: usize,
    pub caret_row: usize,
    pub caret_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasEditorHitRow {
    pub area: Rect,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasEditorRenderRow {
    pub area: Rect,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasEditorLayout {
    pub rows: Vec<CanvasEditorRenderRow>,
    pub caret: Option<ratatui::layout::Position>,
    pub hidden_before: bool,
    pub hidden_after: bool,
    pub caret_before: bool,
    pub caret_after: bool,
    pub multiline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanvasVisualRow {
    start: usize,
    end: usize,
}

impl CanvasNodeEditor {
    pub fn new(node: usize, field: CanvasNodeEditField, draft: String) -> Self {
        let cursor = draft.len();
        Self { node, field, draft, cursor, viewport_width: 1, viewport_height: 1, scroll_row: 0, scroll_column: 0, preferred_column: None, follow_cursor: true, editor_area: Rect::default(), hit_rows: Vec::new(), total_rows: 1, caret_row: 0, caret_column: 0 }
    }

    pub fn insert(&mut self, text: &str) {
        self.normalize_cursor();
        let mut text = text.replace("\r\n", "\n").replace('\r', "\n");
        if !self.field.multiline() {
            text = text.replace('\n', " ");
        }
        self.draft.insert_str(self.cursor, &text);
        self.cursor += text.len();
        self.follow_cursor = true;
        self.preferred_column = None;
    }

    pub fn backspace(&mut self) {
        self.normalize_cursor();
        let previous = previous_grapheme_boundary(&self.draft, self.cursor);
        if previous < self.cursor {
            self.draft.replace_range(previous..self.cursor, "");
            self.cursor = previous;
        }
        self.follow_cursor = true;
        self.preferred_column = None;
    }

    pub fn delete(&mut self) {
        self.normalize_cursor();
        let next = next_grapheme_boundary(&self.draft, self.cursor);
        if next > self.cursor {
            self.draft.replace_range(self.cursor..next, "");
        }
        self.follow_cursor = true;
        self.preferred_column = None;
    }

    pub fn move_horizontal(&mut self, delta: isize) {
        self.normalize_cursor();
        self.cursor = if delta < 0 { previous_grapheme_boundary(&self.draft, self.cursor) } else { next_grapheme_boundary(&self.draft, self.cursor) };
        self.follow_cursor = true;
        self.preferred_column = None;
    }

    pub fn move_vertical(&mut self, delta: isize) {
        if !self.field.multiline() {
            return;
        }
        self.normalize_cursor();
        let rows = visual_rows(&self.draft, self.viewport_width.max(1));
        let (row, column) = visual_cursor(&self.draft, self.cursor, &rows);
        let preferred = self.preferred_column.unwrap_or(column);
        let target = if delta < 0 { row.saturating_sub(1) } else { (row + 1).min(rows.len().saturating_sub(1)) };
        if target != row {
            self.cursor = cursor_at_visual_column(&self.draft, rows[target], preferred);
        }
        self.preferred_column = Some(preferred);
        self.follow_cursor = true;
    }

    pub fn move_page(&mut self, delta: isize) {
        if !self.field.multiline() {
            return;
        }
        self.normalize_cursor();
        let rows = visual_rows(&self.draft, self.viewport_width.max(1));
        let (row, column) = visual_cursor(&self.draft, self.cursor, &rows);
        let preferred = self.preferred_column.unwrap_or(column);
        let distance = self.viewport_height.max(1);
        let target = if delta < 0 { row.saturating_sub(distance) } else { row.saturating_add(distance).min(rows.len().saturating_sub(1)) };
        self.cursor = cursor_at_visual_column(&self.draft, rows[target], preferred);
        self.preferred_column = Some(preferred);
        self.follow_cursor = true;
    }

    pub fn move_row_boundary(&mut self, end: bool) {
        self.normalize_cursor();
        if self.field.multiline() {
            let rows = visual_rows(&self.draft, self.viewport_width.max(1));
            let (row, _) = visual_cursor(&self.draft, self.cursor, &rows);
            self.cursor = if end { rows[row].end } else { rows[row].start };
        } else {
            self.cursor = if end { self.draft.len() } else { 0 };
        }
        self.follow_cursor = true;
        self.preferred_column = None;
    }

    pub fn move_document_boundary(&mut self, end: bool) {
        self.cursor = if end { self.draft.len() } else { 0 };
        self.follow_cursor = true;
        self.preferred_column = None;
    }

    pub fn scroll(&mut self, delta: isize) {
        if !self.field.multiline() || self.total_rows <= self.viewport_height {
            return;
        }
        let maximum = self.total_rows.saturating_sub(self.viewport_height.max(1));
        self.scroll_row = if delta < 0 { self.scroll_row.saturating_sub(delta.unsigned_abs()) } else { self.scroll_row.saturating_add(delta as usize).min(maximum) };
        self.follow_cursor = false;
    }

    pub fn place_cursor(&mut self, pointer: ratatui::layout::Position) -> bool {
        let Some(row) = self.hit_rows.iter().find(|row| row.area.contains(pointer)).cloned() else {
            return false;
        };
        let target_column = pointer.x.saturating_sub(row.area.x) as usize;
        self.cursor = nearest_boundary_at_column(&self.draft, row.start, row.end, target_column);
        self.follow_cursor = true;
        self.preferred_column = None;
        true
    }

    pub fn layout(&mut self, area: Rect) -> CanvasEditorLayout {
        self.editor_area = area;
        self.hit_rows.clear();
        self.normalize_cursor();
        if self.field.multiline() {
            self.layout_multiline(area)
        } else {
            self.layout_single_line(area)
        }
    }

    fn layout_multiline(&mut self, area: Rect) -> CanvasEditorLayout {
        let rail_width = u16::from(area.width >= 2);
        let content = Rect::new(area.x, area.y, area.width.saturating_sub(rail_width), area.height);
        self.viewport_width = content.width.max(1) as usize;
        self.viewport_height = content.height.max(1) as usize;
        let visual = visual_rows(&self.draft, self.viewport_width);
        let (caret_row, caret_column) = visual_cursor(&self.draft, self.cursor, &visual);
        self.total_rows = visual.len();
        self.caret_row = caret_row;
        self.caret_column = caret_column;

        let maximum = visual.len().saturating_sub(self.viewport_height);
        self.scroll_row = self.scroll_row.min(maximum);
        if self.follow_cursor {
            let context = usize::from(self.viewport_height >= 3);
            if caret_row < self.scroll_row.saturating_add(context) {
                self.scroll_row = caret_row.saturating_sub(context);
            } else if caret_row.saturating_add(context) >= self.scroll_row.saturating_add(self.viewport_height) {
                self.scroll_row = caret_row.saturating_add(context + 1).saturating_sub(self.viewport_height);
            }
            self.scroll_row = self.scroll_row.min(maximum);
        }

        let mut rows = Vec::new();
        for (visible_index, row) in visual.iter().skip(self.scroll_row).take(self.viewport_height).enumerate() {
            let row_area = Rect::new(content.x, content.y.saturating_add(visible_index as u16), content.width, 1);
            rows.push(CanvasEditorRenderRow { area: row_area, text: self.draft[row.start..row.end].to_string() });
            self.hit_rows.push(CanvasEditorHitRow { area: row_area, start: row.start, end: row.end });
        }

        let caret = caret_row.checked_sub(self.scroll_row).filter(|row| *row < self.viewport_height && content.width > 0 && content.height > 0).map(|row| {
            let x = content.x.saturating_add((caret_column.min(content.width.saturating_sub(1) as usize)) as u16);
            ratatui::layout::Position::new(x, content.y.saturating_add(row as u16))
        });
        let visible_end = self.scroll_row.saturating_add(self.viewport_height);
        CanvasEditorLayout { rows, caret, hidden_before: self.scroll_row > 0, hidden_after: visible_end < visual.len(), caret_before: caret_row < self.scroll_row, caret_after: caret_row >= visible_end, multiline: true }
    }

    fn layout_single_line(&mut self, area: Rect) -> CanvasEditorLayout {
        let gutters = area.width >= 3;
        let content = if gutters { Rect::new(area.x.saturating_add(1), area.y, area.width.saturating_sub(2), area.height.min(1)) } else { Rect::new(area.x, area.y, area.width, area.height.min(1)) };
        self.viewport_width = content.width.max(1) as usize;
        self.viewport_height = 1;
        let total_columns = self.draft.width();
        let caret_column = self.draft[..self.cursor].width();
        self.total_rows = 1;
        self.caret_row = 0;
        self.caret_column = caret_column;

        let width = self.viewport_width;
        let maximum = total_columns.saturating_sub(width.saturating_sub(1));
        self.scroll_column = self.scroll_column.min(maximum);
        if self.follow_cursor {
            if caret_column < self.scroll_column {
                self.scroll_column = caret_column;
            } else if caret_column >= self.scroll_column.saturating_add(width) {
                self.scroll_column = caret_column.saturating_add(1).saturating_sub(width);
            }
            self.scroll_column = self.scroll_column.min(maximum);
        }

        let (start, end, normalized_scroll) = horizontal_slice(&self.draft, self.scroll_column, width);
        self.scroll_column = normalized_scroll;
        let row_area = Rect::new(content.x, content.y, content.width, content.height);
        let rows = if content.width > 0 && content.height > 0 { vec![CanvasEditorRenderRow { area: row_area, text: self.draft[start..end].to_string() }] } else { Vec::new() };
        if content.width > 0 && content.height > 0 {
            self.hit_rows.push(CanvasEditorHitRow { area: row_area, start, end });
        }
        let relative_caret = caret_column.saturating_sub(self.scroll_column);
        let caret = (caret_column >= self.scroll_column && relative_caret < width && content.width > 0 && content.height > 0).then(|| ratatui::layout::Position::new(content.x.saturating_add(relative_caret as u16), content.y));
        CanvasEditorLayout { rows, caret, hidden_before: self.scroll_column > 0, hidden_after: total_columns > self.scroll_column.saturating_add(width), caret_before: caret_column < self.scroll_column, caret_after: caret_column >= self.scroll_column.saturating_add(width), multiline: false }
    }

    fn normalize_cursor(&mut self) {
        self.cursor = nearest_grapheme_boundary(&self.draft, self.cursor);
    }
}

fn visual_rows(text: &str, width: usize) -> Vec<CanvasVisualRow> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut line_start = 0usize;
    loop {
        let newline = text[line_start..].find('\n').map(|offset| line_start + offset);
        let line_end = newline.unwrap_or(text.len());
        if line_start == line_end {
            rows.push(CanvasVisualRow { start: line_start, end: line_end });
        } else {
            let mut row_start = line_start;
            let mut used = 0usize;
            for (offset, grapheme) in text[line_start..line_end].grapheme_indices(true) {
                let absolute = line_start + offset;
                let grapheme_width = grapheme.width().max(1);
                if used > 0 && used.saturating_add(grapheme_width) > width {
                    rows.push(CanvasVisualRow { start: row_start, end: absolute });
                    row_start = absolute;
                    used = 0;
                }
                used = used.saturating_add(grapheme_width);
            }
            rows.push(CanvasVisualRow { start: row_start, end: line_end });
        }
        let Some(newline) = newline else {
            break;
        };
        line_start = newline + 1;
        if line_start == text.len() {
            rows.push(CanvasVisualRow { start: line_start, end: line_start });
            break;
        }
    }
    rows
}

fn visual_cursor(text: &str, cursor: usize, rows: &[CanvasVisualRow]) -> (usize, usize) {
    let cursor = nearest_grapheme_boundary(text, cursor);
    for (index, row) in rows.iter().enumerate() {
        if cursor < row.end {
            return (index, text[row.start..cursor].width());
        }
        if cursor == row.end {
            let soft_wrap_continues = rows.get(index + 1).is_some_and(|next| next.start == cursor);
            if !soft_wrap_continues {
                return (index, text[row.start..cursor].width());
            }
        }
    }
    let index = rows.len().saturating_sub(1);
    let row = rows[index];
    (index, text[row.start..cursor.min(row.end)].width())
}

fn cursor_at_visual_column(text: &str, row: CanvasVisualRow, column: usize) -> usize {
    nearest_boundary_at_column(text, row.start, row.end, column)
}

fn nearest_boundary_at_column(text: &str, start: usize, end: usize, column: usize) -> usize {
    let mut used = 0usize;
    for (offset, grapheme) in text[start..end].grapheme_indices(true) {
        let width = grapheme.width().max(1);
        if column <= used {
            return start + offset;
        }
        if column < used.saturating_add(width) {
            return if column - used < width.saturating_sub(column - used) { start + offset } else { start + offset + grapheme.len() };
        }
        used = used.saturating_add(width);
    }
    end
}

fn horizontal_slice(text: &str, requested_scroll: usize, width: usize) -> (usize, usize, usize) {
    if width == 0 || text.is_empty() {
        return (0, 0, 0);
    }
    let mut used = 0usize;
    let mut start = text.len();
    let mut normalized_scroll = requested_scroll;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let next = used.saturating_add(grapheme.width().max(1));
        if next > requested_scroll {
            start = offset;
            normalized_scroll = used;
            break;
        }
        used = next;
    }
    if start == text.len() {
        return (text.len(), text.len(), used);
    }
    let mut visible = 0usize;
    let mut end = start;
    for (offset, grapheme) in text[start..].grapheme_indices(true) {
        let grapheme_width = grapheme.width().max(1);
        if visible > 0 && visible.saturating_add(grapheme_width) > width {
            break;
        }
        visible = visible.saturating_add(grapheme_width);
        end = start + offset + grapheme.len();
        if visible >= width {
            break;
        }
    }
    (start, end, normalized_scroll)
}

fn nearest_grapheme_boundary(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor == text.len() {
        return text.len();
    }
    text.grapheme_indices(true).take_while(|(index, _)| *index <= cursor).map(|(index, _)| index).last().unwrap_or(0)
}

fn previous_grapheme_boundary(text: &str, cursor: usize) -> usize {
    let cursor = nearest_grapheme_boundary(text, cursor);
    text[..cursor].grapheme_indices(true).next_back().map_or(0, |(index, _)| index)
}

fn next_grapheme_boundary(text: &str, cursor: usize) -> usize {
    let cursor = nearest_grapheme_boundary(text, cursor);
    text[cursor..].grapheme_indices(true).nth(1).map_or(text.len(), |(index, _)| cursor + index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasResizeHandle {
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
}

impl CanvasResizeHandle {
    pub fn affects_left(self) -> bool {
        matches!(self, Self::Left | Self::TopLeft | Self::BottomLeft)
    }

    pub fn affects_right(self) -> bool {
        matches!(self, Self::Right | Self::TopRight | Self::BottomRight)
    }

    pub fn affects_top(self) -> bool {
        matches!(self, Self::Top | Self::TopLeft | Self::TopRight)
    }

    pub fn affects_bottom(self) -> bool {
        matches!(self, Self::Bottom | Self::BottomLeft | Self::BottomRight)
    }

    pub fn is_corner(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight | Self::BottomRight | Self::BottomLeft)
    }

    pub fn glyph(self) -> char {
        match self {
            Self::Top | Self::Bottom => '↕',
            Self::Left | Self::Right => '↔',
            Self::TopLeft | Self::BottomRight => '╲',
            Self::TopRight | Self::BottomLeft => '╱',
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CanvasInteraction {
    #[default]
    Idle,
    DraggingNode {
        node: usize,
        last: (u16, u16),
        origin: (i64, i64),
        changed: bool,
    },
    Panning {
        last: (u16, u16),
    },
    Connecting {
        from_node: usize,
        from_side: Option<ekphos_canvas::CanvasSide>,
        pointer: Option<(u16, u16)>,
    },
    ResizingNode {
        node: usize,
        handle: CanvasResizeHandle,
        start: (u16, u16),
        last: (u16, u16),
        origin: (i64, i64, i64, i64),
        minimum: (i64, i64),
        changed: bool,
    },
}

impl Default for CanvasViewState {
    fn default() -> Self {
        Self {
            document: None,
            diagnostics: Vec::new(),
            error: None,
            selected_node: 0,
            selected_edge: None,
            hovered_node: None,
            hovered_edge: None,
            viewport_x: 0.0,
            viewport_y: 0.0,
            zoom: 1.0,
            needs_fit: true,
            view_area: Rect::default(),
            node_rects: Vec::new(),
            edge_cells: Vec::new(),
            handle_rects: Vec::new(),
            resize_rects: Vec::new(),
            hovered_resize: None,
            interaction: CanvasInteraction::Idle,
            editor: None,
            last_click: None,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
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
