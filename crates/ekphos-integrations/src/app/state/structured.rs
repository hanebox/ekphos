use super::*;

use chrono::{DateTime, Local, NaiveDateTime};
use ekphos_bases::{BaseFile, BaseRecord, CompiledBase, Corpus, Value};
use ekphos_canvas::{CanvasEdge, CanvasEnd, CanvasNode, CanvasNodeKind, CanvasSide};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

struct BaseEvaluationRequest {
    generation: u64,
    definition: BaseFile,
    view_index: usize,
    root: PathBuf,
    notes: Vec<Note>,
    now: NaiveDateTime,
}

struct BaseEvaluationResponse {
    generation: u64,
    compiled: CompiledBase,
    corpus: Corpus,
    result: ekphos_bases::BaseResult,
}

enum BaseWorkerCommand {
    Evaluate(Box<BaseEvaluationRequest>),
    Shutdown,
}

pub(crate) struct BaseWorker {
    command_sender: std::sync::mpsc::Sender<BaseWorkerCommand>,
    result_receiver: std::sync::mpsc::Receiver<BaseEvaluationResponse>,
    generation: Arc<AtomicU64>,
    next_generation: u64,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl BaseWorker {
    pub(crate) fn new() -> Self {
        let (command_sender, command_receiver) = std::sync::mpsc::channel();
        let (result_sender, result_receiver) = std::sync::mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = Arc::clone(&generation);
        let thread = std::thread::Builder::new().name("ekphos-base".to_string()).spawn(move || base_worker_loop(command_receiver, result_sender, worker_generation)).ok();
        Self { command_sender, result_receiver, generation, next_generation: 0, thread }
    }

    fn request(&mut self, definition: BaseFile, view_index: usize, root: PathBuf, notes: Vec<Note>, now: NaiveDateTime) -> Result<u64, String> {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        let generation = self.next_generation;
        self.generation.store(generation, Ordering::Release);
        self.command_sender.send(BaseWorkerCommand::Evaluate(Box::new(BaseEvaluationRequest { generation, definition, view_index, root, notes, now }))).map_err(|_| "Could not start Base evaluation".to_string())?;
        Ok(generation)
    }

    fn poll(&self) -> Option<BaseEvaluationResponse> {
        self.result_receiver.try_iter().last()
    }

    fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl Drop for BaseWorker {
    fn drop(&mut self) {
        self.cancel();
        let _ = self.command_sender.send(BaseWorkerCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn base_worker_loop(receiver: std::sync::mpsc::Receiver<BaseWorkerCommand>, sender: std::sync::mpsc::Sender<BaseEvaluationResponse>, generation: Arc<AtomicU64>) {
    while let Ok(command) = receiver.recv() {
        let BaseWorkerCommand::Evaluate(mut request) = command else {
            return;
        };
        loop {
            match receiver.try_recv() {
                Ok(BaseWorkerCommand::Evaluate(newer)) => request = newer,
                Ok(BaseWorkerCommand::Shutdown) | Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
            }
        }
        let Some(corpus) = build_base_corpus(&request.root, &request.notes, request.generation, &generation) else {
            continue;
        };
        let compiled = CompiledBase::compile(request.definition);
        let result = compiled.evaluate_view(&corpus, request.view_index, request.now);
        if generation.load(Ordering::Acquire) != request.generation {
            continue;
        }
        if sender.send(BaseEvaluationResponse { generation: request.generation, compiled, corpus, result }).is_err() {
            return;
        }
    }
}

impl App {
    pub fn active_document_kind(&self) -> Option<ekphos_vault::VaultFileKind> {
        self.current_note().map(|note| note.kind)
    }

    /// Parse the active Base or Canvas document and replace the corresponding
    /// typed view model. Markdown documents continue through `DocumentState`.
    pub(crate) fn refresh_structured_document(&mut self) {
        let Some((note_id, kind)) = self.current_note().map(|note| (note.id, note.kind)) else {
            self.structured.base_worker.cancel();
            self.structured.parse_key = None;
            self.structured.base = BaseViewState::default();
            self.structured.canvas = CanvasViewState::default();
            return;
        };
        if kind.is_markdown() {
            self.structured.base_worker.cancel();
            self.structured.parse_key = None;
            return;
        }
        let parse_key = (note_id, self.document.document_generation, self.vault.catalog_generation);
        if self.structured.parse_key == Some(parse_key) {
            return;
        }
        let reset_canvas_selection = self.structured.parse_key.is_none_or(|key| key.0 != note_id);
        let source = self.current_body().unwrap_or_default().to_string();
        match kind {
            ekphos_vault::VaultFileKind::Markdown => {}
            ekphos_vault::VaultFileKind::Base => self.refresh_base(&source),
            ekphos_vault::VaultFileKind::Canvas => self.refresh_canvas(&source, reset_canvas_selection),
        }
        self.structured.parse_key = Some(parse_key);
        self.structured.vault_signature = vault_signature(self.vault.root());
    }

    fn refresh_base(&mut self, source: &str) {
        self.structured.canvas = CanvasViewState::default();
        let view_index = self.structured.base.view_index;
        let definition = match ekphos_bases::parse_base(source) {
            Ok(definition) => definition,
            Err(error) => {
                self.structured.base = BaseViewState { error: Some(format_yaml_error(&error)), ..BaseViewState::default() };
                return;
            }
        };
        let view_count = definition.views.len();
        let view_index = view_index.min(view_count.saturating_sub(1));
        let request = self.structured.base_worker.request(definition, view_index, self.vault.root().to_path_buf(), self.vault.notes.clone(), Local::now().naive_local());
        self.structured.base = match request {
            Ok(request_generation) => BaseViewState { view_index, view_count, loading: true, request_generation, ..BaseViewState::default() },
            Err(error) => BaseViewState { error: Some(error), view_index, view_count, ..BaseViewState::default() },
        };
    }

    fn refresh_canvas(&mut self, source: &str, reset_selection: bool) {
        self.structured.base_worker.cancel();
        self.structured.base = BaseViewState::default();
        match ekphos_canvas::parse_canvas(source) {
            Ok((document, diagnostics)) => {
                let node_count = document.nodes.len();
                let selected_node = if reset_selection { document.nodes.iter().position(|node| !matches!(node.kind, CanvasNodeKind::Group { .. })).unwrap_or(0) } else { self.structured.canvas.selected_node.min(node_count.saturating_sub(1)) };
                self.structured.canvas.document = Some(document);
                self.structured.canvas.diagnostics = diagnostics.into_iter().map(|diagnostic| diagnostic.to_string()).collect();
                self.structured.canvas.error = None;
                self.structured.canvas.selected_node = selected_node;
                self.structured.canvas.selected_edge = None;
                self.structured.canvas.hovered_node = None;
                self.structured.canvas.hovered_edge = None;
                self.structured.canvas.node_rects.clear();
                self.structured.canvas.edge_cells.clear();
                self.structured.canvas.handle_rects.clear();
                self.structured.canvas.resize_rects.clear();
                self.structured.canvas.hovered_resize = None;
                self.structured.canvas.interaction = CanvasInteraction::Idle;
                self.structured.canvas.editor = None;
                self.structured.canvas.undo.clear();
                self.structured.canvas.redo.clear();
                self.structured.canvas.needs_fit = true;
            }
            Err(error) => {
                self.structured.canvas.document = None;
                self.structured.canvas.diagnostics.clear();
                self.structured.canvas.error = Some(error.to_string());
                self.structured.canvas.node_rects.clear();
                self.structured.canvas.edge_cells.clear();
                self.structured.canvas.handle_rects.clear();
                self.structured.canvas.resize_rects.clear();
                self.structured.canvas.hovered_resize = None;
                self.structured.canvas.interaction = CanvasInteraction::Idle;
                self.structured.canvas.editor = None;
                self.structured.canvas.undo.clear();
                self.structured.canvas.redo.clear();
            }
        }
    }

    pub(crate) fn poll_base_evaluation(&mut self) -> bool {
        let Some(response) = self.structured.base_worker.poll() else {
            return false;
        };
        if response.generation != self.structured.base.request_generation || self.active_document_kind() != Some(ekphos_vault::VaultFileKind::Base) {
            return false;
        }
        let row_count = response.result.groups.iter().map(|group| group.rows.len()).sum::<usize>();
        self.structured.base.selected_row = self.structured.base.selected_row.min(row_count.saturating_sub(1));
        self.structured.base.compiled = Some(response.compiled);
        self.structured.base.corpus = response.corpus;
        self.structured.base.result = Some(response.result);
        self.structured.base.loading = false;
        true
    }

    pub(crate) fn base_evaluation_pending(&self) -> bool {
        self.structured.base.loading
    }

    pub(crate) fn clear_structured_document(&mut self) {
        self.structured.base_worker.cancel();
        self.structured.base = BaseViewState::default();
        self.structured.canvas = CanvasViewState::default();
        self.structured.parse_key = None;
    }

    pub fn base_move_selection(&mut self, delta: isize) {
        let row_count = self.structured.base.result.as_ref().map(|result| result.groups.iter().map(|group| group.rows.len()).sum()).unwrap_or(0);
        if row_count == 0 {
            return;
        }
        self.structured.base.selected_row = self.structured.base.selected_row.saturating_add_signed(delta).min(row_count - 1);
    }

    pub fn base_move_column(&mut self, delta: isize) {
        let column_count = self.structured.base.result.as_ref().map_or(0, |result| result.columns.len());
        if column_count == 0 {
            return;
        }
        self.structured.base.column_offset = self.structured.base.column_offset.saturating_add_signed(delta).min(column_count - 1);
    }

    pub fn base_change_view(&mut self, delta: isize) {
        let view_count = self.structured.base.view_count;
        let Some(compiled) = self.structured.base.compiled.as_ref() else {
            return;
        };
        if view_count < 2 {
            return;
        }
        let next = (self.structured.base.view_index as isize + delta).rem_euclid(view_count as isize) as usize;
        let result = compiled.evaluate_view(&self.structured.base.corpus, next, Local::now().naive_local());
        self.structured.base.view_index = next;
        self.structured.base.result = Some(result);
        self.structured.base.selected_row = 0;
        self.structured.base.row_offset = 0;
        self.structured.base.column_offset = 0;
    }

    pub fn open_selected_base_row(&mut self) -> bool {
        let selected = self.structured.base.selected_row;
        let note_id = self.structured.base.result.as_ref().and_then(|result| result.groups.iter().flat_map(|group| group.rows.iter()).nth(selected)).map(|row| row.id);
        note_id.and_then(|id| self.note_index_for_id(id)).is_some_and(|index| self.navigate_to_note(index))
    }

    pub fn canvas_move_selection(&mut self, dx: f64, dy: f64) {
        let Some(document) = self.structured.canvas.document.as_ref() else {
            return;
        };
        let Some(current) = document.nodes.get(self.structured.canvas.selected_node) else {
            return;
        };
        let (cx, cy) = (current.x as f64 + current.width as f64 / 2.0, current.y as f64 + current.height as f64 / 2.0);
        let direction_len = (dx * dx + dy * dy).sqrt();
        let best = document
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.structured.canvas.selected_node)
            .filter_map(|(index, node)| {
                let vx = node.x as f64 + node.width as f64 / 2.0 - cx;
                let vy = node.y as f64 + node.height as f64 / 2.0 - cy;
                let distance = (vx * vx + vy * vy).sqrt();
                let projection = (vx * dx + vy * dy) / direction_len;
                (projection > 0.0 && distance > 0.0).then_some((index, distance + (distance - projection) * 3.0))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index);
        if let Some(index) = best {
            self.structured.canvas.selected_node = index;
            self.structured.canvas.selected_edge = None;
            let node = &document.nodes[index];
            let area = self.structured.canvas.view_area;
            if area.width > 0 && area.height > 0 {
                let zoom = self.structured.canvas.zoom.max(0.1);
                let visible_width = area.width as f64 * 20.0 / zoom;
                let visible_height = area.height as f64 * 40.0 / zoom;
                let margin_x = 20.0 / zoom;
                let margin_y = 40.0 / zoom;
                let node_left = node.x as f64;
                let node_right = (node.x + node.width) as f64;
                let node_top = node.y as f64;
                let node_bottom = (node.y + node.height) as f64;
                if node_left < self.structured.canvas.viewport_x {
                    self.structured.canvas.viewport_x = node_left - margin_x;
                } else if node_right > self.structured.canvas.viewport_x + visible_width {
                    self.structured.canvas.viewport_x = node_right - visible_width + margin_x;
                }
                if node_top < self.structured.canvas.viewport_y {
                    self.structured.canvas.viewport_y = node_top - margin_y;
                } else if node_bottom > self.structured.canvas.viewport_y + visible_height {
                    self.structured.canvas.viewport_y = node_bottom - visible_height + margin_y;
                }
            }
        }
    }

    pub fn canvas_pan(&mut self, dx: f64, dy: f64) {
        self.structured.canvas.viewport_x += dx / self.structured.canvas.zoom.max(0.1);
        self.structured.canvas.viewport_y += dy / self.structured.canvas.zoom.max(0.1);
        self.structured.canvas.needs_fit = false;
    }

    pub fn canvas_zoom(&mut self, factor: f64) {
        self.canvas_zoom_at(factor, None);
    }

    pub fn canvas_zoom_at(&mut self, factor: f64, pointer: Option<(u16, u16)>) {
        let area = self.structured.canvas.view_area;
        let old_zoom = self.structured.canvas.zoom.max(0.1);
        let new_zoom = (old_zoom * factor).clamp(0.1, 8.0);
        if (new_zoom - old_zoom).abs() < f64::EPSILON {
            return;
        }
        let (anchor_x, anchor_y) = pointer.filter(|(x, y)| *x >= area.x && *x < area.right() && *y >= area.y && *y < area.bottom()).unwrap_or((area.x + area.width / 2, area.y + area.height / 2));
        let screen_x = anchor_x.saturating_sub(area.x) as f64;
        let screen_y = anchor_y.saturating_sub(area.y) as f64;
        let world_x = self.structured.canvas.viewport_x + screen_x * 20.0 / old_zoom;
        let world_y = self.structured.canvas.viewport_y + screen_y * 40.0 / old_zoom;
        self.structured.canvas.viewport_x = world_x - screen_x * 20.0 / new_zoom;
        self.structured.canvas.viewport_y = world_y - screen_y * 40.0 / new_zoom;
        self.structured.canvas.zoom = new_zoom;
        self.structured.canvas.needs_fit = false;
    }

    pub fn canvas_fit(&mut self) {
        self.structured.canvas.needs_fit = true;
    }

    pub fn canvas_select_node(&mut self, index: usize) {
        let node_count = self.structured.canvas.document.as_ref().map_or(0, |document| document.nodes.len());
        if index < node_count {
            self.structured.canvas.selected_node = index;
            self.structured.canvas.selected_edge = None;
            self.state.focus = Focus::Content;
        }
    }

    pub fn canvas_begin_node_drag(&mut self, index: usize, pointer: (u16, u16)) {
        let Some(node) = self.structured.canvas.document.as_ref().and_then(|document| document.nodes.get(index)) else {
            return;
        };
        let origin = (node.x, node.y);
        self.canvas_select_node(index);
        self.structured.canvas.interaction = CanvasInteraction::DraggingNode { node: index, last: pointer, origin, changed: false };
        self.structured.canvas.needs_fit = false;
    }

    pub fn canvas_begin_node_resize(&mut self, index: usize, handle: CanvasResizeHandle, pointer: (u16, u16)) {
        let Some(node) = self.structured.canvas.document.as_ref().and_then(|document| document.nodes.get(index)) else {
            return;
        };
        let origin = (node.x, node.y, node.width.max(1), node.height.max(1));
        let minimum = (origin.2.min(160), origin.3.min(120));
        self.canvas_select_node(index);
        self.structured.canvas.interaction = CanvasInteraction::ResizingNode { node: index, handle, start: pointer, last: pointer, origin, minimum, changed: false };
        self.structured.canvas.needs_fit = false;
        self.state.status_message = Some("Resizing card · hold Shift to keep its aspect ratio".to_string());
    }

    pub fn canvas_begin_pan(&mut self, pointer: (u16, u16)) {
        self.structured.canvas.selected_edge = None;
        self.structured.canvas.interaction = CanvasInteraction::Panning { last: pointer };
        self.structured.canvas.needs_fit = false;
        self.state.focus = Focus::Content;
    }

    pub fn canvas_begin_connect(&mut self, from_side: Option<CanvasSide>, pointer: Option<(u16, u16)>) {
        let node_count = self.structured.canvas.document.as_ref().map_or(0, |document| document.nodes.len());
        if self.structured.canvas.selected_node >= node_count {
            return;
        }
        self.structured.canvas.selected_edge = None;
        self.structured.canvas.interaction = CanvasInteraction::Connecting { from_node: self.structured.canvas.selected_node, from_side, pointer };
        self.state.status_message = Some("Choose a target, then press Enter".to_string());
    }

    pub fn canvas_pointer_drag(&mut self, pointer: (u16, u16)) {
        self.canvas_pointer_drag_with_aspect(pointer, false);
    }

    pub fn canvas_pointer_drag_with_aspect(&mut self, pointer: (u16, u16), preserve_aspect: bool) {
        match self.structured.canvas.interaction {
            CanvasInteraction::DraggingNode { node, last, origin, changed } => {
                let dx = pointer.0 as i32 - last.0 as i32;
                let dy = pointer.1 as i32 - last.1 as i32;
                if dx == 0 && dy == 0 {
                    return;
                }
                let zoom = self.structured.canvas.zoom.max(0.1);
                if let Some(canvas_node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(node)) {
                    canvas_node.x = canvas_node.x.saturating_add((dx as f64 * 20.0 / zoom).round() as i64);
                    canvas_node.y = canvas_node.y.saturating_add((dy as f64 * 40.0 / zoom).round() as i64);
                }
                self.structured.canvas.interaction = CanvasInteraction::DraggingNode { node, last: pointer, origin, changed: changed || dx != 0 || dy != 0 };
            }
            CanvasInteraction::Panning { last } => {
                let dx = pointer.0 as i32 - last.0 as i32;
                let dy = pointer.1 as i32 - last.1 as i32;
                let zoom = self.structured.canvas.zoom.max(0.1);
                self.structured.canvas.viewport_x -= dx as f64 * 20.0 / zoom;
                self.structured.canvas.viewport_y -= dy as f64 * 40.0 / zoom;
                self.structured.canvas.interaction = CanvasInteraction::Panning { last: pointer };
            }
            CanvasInteraction::Connecting { from_node, from_side, .. } => {
                self.structured.canvas.interaction = CanvasInteraction::Connecting { from_node, from_side, pointer: Some(pointer) };
            }
            CanvasInteraction::ResizingNode { node, handle, start, origin, minimum, .. } => {
                let zoom = self.structured.canvas.zoom.max(0.1);
                let dx = ((pointer.0 as i32 - start.0 as i32) as f64 * 20.0 / zoom).round() as i64;
                let dy = ((pointer.1 as i32 - start.1 as i32) as f64 * 40.0 / zoom).round() as i64;
                let resized = resized_canvas_geometry(origin, minimum, handle, dx, dy, preserve_aspect);
                if let Some(canvas_node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(node)) {
                    (canvas_node.x, canvas_node.y, canvas_node.width, canvas_node.height) = resized;
                }
                self.structured.canvas.interaction = CanvasInteraction::ResizingNode { node, handle, start, last: pointer, origin, minimum, changed: resized != origin };
                if let Some(editor) = self.structured.canvas.editor.as_mut().filter(|editor| editor.node == node) {
                    editor.follow_cursor = true;
                }
            }
            CanvasInteraction::Idle => {}
        }
    }

    pub fn canvas_end_pointer_interaction(&mut self, target: Option<(usize, Option<CanvasSide>)>) {
        let interaction = std::mem::take(&mut self.structured.canvas.interaction);
        match interaction {
            CanvasInteraction::DraggingNode { node, origin, changed, .. } if changed => {
                let previous = self.structured.canvas.document.as_ref().map(|document| {
                    let mut previous = document.clone();
                    if let Some(canvas_node) = previous.nodes.get_mut(node) {
                        (canvas_node.x, canvas_node.y) = origin;
                    }
                    previous
                });
                if self.persist_canvas_document("Card moved") {
                    if let Some(previous) = previous {
                        self.push_canvas_undo(previous);
                    }
                } else if let Some(canvas_node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(node)) {
                    (canvas_node.x, canvas_node.y) = origin;
                }
            }
            CanvasInteraction::Connecting { from_node, from_side, .. } => {
                if let Some((to_node, to_side)) = target {
                    self.canvas_connect_nodes(from_node, from_side, to_node, to_side);
                } else {
                    self.state.status_message = Some("Connection canceled".to_string());
                }
            }
            CanvasInteraction::ResizingNode { node, origin, changed, .. } if changed => {
                let previous = self.structured.canvas.document.as_ref().map(|document| {
                    let mut previous = document.clone();
                    if let Some(canvas_node) = previous.nodes.get_mut(node) {
                        (canvas_node.x, canvas_node.y, canvas_node.width, canvas_node.height) = origin;
                    }
                    previous
                });
                if self.persist_canvas_document("Card resized") {
                    if let Some(previous) = previous {
                        self.push_canvas_undo(previous);
                    }
                } else if let Some(canvas_node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(node)) {
                    (canvas_node.x, canvas_node.y, canvas_node.width, canvas_node.height) = origin;
                }
            }
            CanvasInteraction::DraggingNode { .. } | CanvasInteraction::ResizingNode { .. } | CanvasInteraction::Panning { .. } | CanvasInteraction::Idle => {}
        }
    }

    pub fn canvas_cancel_interaction(&mut self) -> bool {
        let interaction = std::mem::take(&mut self.structured.canvas.interaction);
        match interaction {
            CanvasInteraction::DraggingNode { node, origin, changed, .. } => {
                if changed {
                    if let Some(canvas_node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(node)) {
                        (canvas_node.x, canvas_node.y) = origin;
                    }
                }
                true
            }
            CanvasInteraction::Connecting { .. } => {
                self.state.status_message = Some("Connection canceled".to_string());
                true
            }
            CanvasInteraction::ResizingNode { node, origin, changed, .. } => {
                if changed {
                    if let Some(canvas_node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(node)) {
                        (canvas_node.x, canvas_node.y, canvas_node.width, canvas_node.height) = origin;
                    }
                }
                self.state.status_message = Some("Card resize canceled".to_string());
                true
            }
            CanvasInteraction::Panning { .. } => true,
            CanvasInteraction::Idle => false,
        }
    }

    pub fn canvas_finish_keyboard_connect(&mut self) -> bool {
        let CanvasInteraction::Connecting { from_node, from_side, .. } = self.structured.canvas.interaction else {
            return false;
        };
        let to_node = self.structured.canvas.selected_node;
        self.structured.canvas.interaction = CanvasInteraction::Idle;
        self.canvas_connect_nodes(from_node, from_side, to_node, None)
    }

    fn canvas_connect_nodes(&mut self, from_node: usize, from_side: Option<CanvasSide>, to_node: usize, to_side: Option<CanvasSide>) -> bool {
        if from_node == to_node {
            self.state.status_message = Some("Choose a different target".to_string());
            return false;
        }
        let Some(document) = self.structured.canvas.document.as_ref() else {
            return false;
        };
        let (Some(from), Some(to)) = (document.nodes.get(from_node), document.nodes.get(to_node)) else {
            return false;
        };
        let from_side = from_side.unwrap_or_else(|| side_toward(from, to));
        let to_side = to_side.unwrap_or_else(|| side_toward(to, from));
        if document.edges.iter().any(|edge| edge.from_node == from.id && edge.to_node == to.id && edge.from_side == Some(from_side) && edge.to_side == Some(to_side)) {
            self.state.status_message = Some("These handles are already connected".to_string());
            return false;
        }
        let from_id = from.id.clone();
        let to_id = to.id.clone();
        let id = next_canvas_edge_id(document);
        let edge = CanvasEdge { id, from_node: from_id, from_side: Some(from_side), from_end: CanvasEnd::None, to_node: to_id, to_side: Some(to_side), to_end: CanvasEnd::Arrow, color: None, label: None, extra: BTreeMap::new() };
        let previous_document = document.clone();
        let previous_selection = self.structured.canvas.selected_edge;
        let edge_index = document.edges.len();
        self.structured.canvas.document.as_mut().expect("document checked").edges.push(edge);
        self.structured.canvas.selected_edge = Some(edge_index);
        if self.persist_canvas_document("Connection added") {
            self.push_canvas_undo(previous_document);
            true
        } else {
            self.structured.canvas.document.as_mut().expect("document checked").edges.pop();
            self.structured.canvas.selected_edge = previous_selection;
            false
        }
    }

    pub fn canvas_cycle_edge(&mut self, delta: isize) {
        let edge_count = self.structured.canvas.document.as_ref().map_or(0, |document| document.edges.len());
        if edge_count == 0 {
            self.state.status_message = Some("No connections".to_string());
            self.structured.canvas.selected_edge = None;
            return;
        }
        let current = self.structured.canvas.selected_edge.unwrap_or_else(|| if delta < 0 { 0 } else { edge_count - 1 });
        let next = (current as isize + delta).rem_euclid(edge_count as isize) as usize;
        self.structured.canvas.selected_edge = Some(next);
        self.state.status_message = Some(format!("Connection {} of {}", next + 1, edge_count));
    }

    pub fn canvas_select_edge(&mut self, index: usize) {
        let edge_count = self.structured.canvas.document.as_ref().map_or(0, |document| document.edges.len());
        if index < edge_count {
            self.structured.canvas.selected_edge = Some(index);
            self.structured.canvas.interaction = CanvasInteraction::Idle;
            self.state.focus = Focus::Content;
        }
    }

    pub fn canvas_delete_selected_edge(&mut self) -> bool {
        let Some(index) = self.structured.canvas.selected_edge else {
            self.state.status_message = Some("Select a connection first".to_string());
            return false;
        };
        let Some(document) = self.structured.canvas.document.as_mut() else {
            return false;
        };
        if index >= document.edges.len() {
            self.structured.canvas.selected_edge = None;
            return false;
        }
        let previous_document = document.clone();
        let edge = document.edges.remove(index);
        let next_selection = (!document.edges.is_empty()).then_some(index.min(document.edges.len() - 1));
        self.structured.canvas.selected_edge = next_selection;
        if self.persist_canvas_document("Connection removed") {
            self.push_canvas_undo(previous_document);
            true
        } else {
            self.structured.canvas.document.as_mut().expect("document checked").edges.insert(index, edge);
            self.structured.canvas.selected_edge = Some(index);
            false
        }
    }

    pub fn canvas_nudge_selected(&mut self, dx: i64, dy: i64) -> bool {
        let index = self.structured.canvas.selected_node;
        let previous_document = self.structured.canvas.document.clone();
        let Some(node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(index)) else {
            return false;
        };
        let origin = (node.x, node.y);
        node.x = node.x.saturating_add(dx);
        node.y = node.y.saturating_add(dy);
        self.structured.canvas.needs_fit = false;
        if self.persist_canvas_document("Card moved") {
            if let Some(previous_document) = previous_document {
                self.push_canvas_undo(previous_document);
            }
            true
        } else {
            if let Some(node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(index)) {
                (node.x, node.y) = origin;
            }
            false
        }
    }

    pub fn canvas_resize_selected(&mut self, width_delta: i64, height_delta: i64) -> bool {
        let index = self.structured.canvas.selected_node;
        let Some(previous_document) = self.structured.canvas.document.clone() else {
            return false;
        };
        let Some(node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(index)) else {
            return false;
        };
        let minimum_width = node.width.clamp(1, 160);
        let minimum_height = node.height.clamp(1, 120);
        let width = node.width.saturating_add(width_delta).max(minimum_width);
        let height = node.height.saturating_add(height_delta).max(minimum_height);
        if width == node.width && height == node.height {
            self.state.status_message = Some("Card is at its minimum size".to_string());
            return false;
        }
        node.width = width;
        node.height = height;
        self.structured.canvas.needs_fit = false;
        if self.persist_canvas_document("Card resized") {
            self.push_canvas_undo(previous_document);
            true
        } else {
            self.structured.canvas.document = Some(previous_document);
            false
        }
    }

    pub fn canvas_undo(&mut self) -> bool {
        let Some(previous) = self.structured.canvas.undo.pop() else {
            self.state.status_message = Some("Nothing to undo".to_string());
            return false;
        };
        let Some(current) = self.structured.canvas.document.replace(previous) else {
            return false;
        };
        if self.persist_canvas_document("Canvas edit undone") {
            push_bounded_history(&mut self.structured.canvas.redo, current);
            self.reset_canvas_selection_after_history();
            true
        } else {
            let failed = self.structured.canvas.document.replace(current).expect("undo document installed");
            self.structured.canvas.undo.push(failed);
            false
        }
    }

    pub fn canvas_redo(&mut self) -> bool {
        let Some(next) = self.structured.canvas.redo.pop() else {
            self.state.status_message = Some("Nothing to redo".to_string());
            return false;
        };
        let Some(current) = self.structured.canvas.document.replace(next) else {
            return false;
        };
        if self.persist_canvas_document("Canvas edit redone") {
            push_bounded_history(&mut self.structured.canvas.undo, current);
            self.reset_canvas_selection_after_history();
            true
        } else {
            let failed = self.structured.canvas.document.replace(current).expect("redo document installed");
            self.structured.canvas.redo.push(failed);
            false
        }
    }

    fn push_canvas_undo(&mut self, previous: ekphos_canvas::Canvas) {
        push_bounded_history(&mut self.structured.canvas.undo, previous);
        self.structured.canvas.redo.clear();
    }

    fn reset_canvas_selection_after_history(&mut self) {
        let document = self.structured.canvas.document.as_ref();
        let node_count = document.map_or(0, |document| document.nodes.len());
        self.structured.canvas.selected_node = self.structured.canvas.selected_node.min(node_count.saturating_sub(1));
        self.structured.canvas.selected_edge = None;
        self.structured.canvas.interaction = CanvasInteraction::Idle;
        self.structured.canvas.editor = None;
    }

    pub fn canvas_interaction_active(&self) -> bool {
        self.structured.canvas.interaction != CanvasInteraction::Idle
    }

    pub fn canvas_editor_active(&self) -> bool {
        self.structured.canvas.editor.is_some()
    }

    pub fn canvas_begin_node_edit(&mut self) -> bool {
        let node_index = self.structured.canvas.selected_node;
        let Some(node) = self.structured.canvas.document.as_ref().and_then(|document| document.nodes.get(node_index)) else {
            return false;
        };
        let (field, draft) = match &node.kind {
            CanvasNodeKind::Text { text } => (CanvasNodeEditField::Text, text.clone()),
            CanvasNodeKind::Link { url } => (CanvasNodeEditField::Link, url.clone()),
            CanvasNodeKind::Group { label, .. } => (CanvasNodeEditField::GroupLabel, label.clone().unwrap_or_default()),
            CanvasNodeKind::File { .. } => {
                let canvas_note = self.current_note().map(|note| note.id);
                if self.open_selected_canvas_node() {
                    if self.current_note().map(|note| note.id) != canvas_note {
                        self.enter_edit_mode();
                    }
                    return true;
                }
                return false;
            }
            CanvasNodeKind::Unknown { .. } => {
                self.state.status_message = Some("This card type can only be edited in source".to_string());
                return false;
            }
        };
        self.structured.canvas.editor = Some(CanvasNodeEditor::new(node_index, field, draft));
        self.structured.canvas.selected_edge = None;
        self.structured.canvas.interaction = CanvasInteraction::Idle;
        self.state.focus = Focus::Content;
        self.state.status_message = Some(if field.multiline() { "Editing card · Ctrl+Enter saves · Esc cancels" } else { "Editing card · Enter saves · Esc cancels" }.to_string());
        true
    }

    pub fn canvas_activate_selected_node(&mut self) -> bool {
        let kind = self.structured.canvas.document.as_ref().and_then(|document| document.nodes.get(self.structured.canvas.selected_node)).map(|node| node.kind.clone());
        match kind {
            Some(CanvasNodeKind::File { .. }) => self.open_selected_canvas_node(),
            Some(CanvasNodeKind::Text { .. } | CanvasNodeKind::Link { .. } | CanvasNodeKind::Group { .. }) => self.canvas_begin_node_edit(),
            Some(CanvasNodeKind::Unknown { .. }) | None => {
                self.state.status_message = Some("This card type can only be edited in source".to_string());
                false
            }
        }
    }

    pub fn canvas_edit_insert(&mut self, text: &str) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.insert(text);
        true
    }

    pub fn canvas_edit_backspace(&mut self) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.backspace();
        true
    }

    pub fn canvas_edit_delete(&mut self) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.delete();
        true
    }

    pub fn canvas_edit_move_horizontal(&mut self, delta: isize) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.move_horizontal(delta);
        true
    }

    pub fn canvas_edit_move_vertical(&mut self, delta: isize) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.move_vertical(delta);
        true
    }

    pub fn canvas_edit_move_line_boundary(&mut self, end: bool) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.move_row_boundary(end);
        true
    }

    pub fn canvas_edit_move_document_boundary(&mut self, end: bool) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.move_document_boundary(end);
        true
    }

    pub fn canvas_edit_move_page(&mut self, delta: isize) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.move_page(delta);
        true
    }

    pub fn canvas_edit_scroll(&mut self, delta: isize) -> bool {
        let Some(editor) = self.structured.canvas.editor.as_mut() else {
            return false;
        };
        editor.scroll(delta);
        true
    }

    pub fn canvas_edit_place_cursor(&mut self, pointer: ratatui::layout::Position) -> bool {
        self.structured.canvas.editor.as_mut().is_some_and(|editor| editor.place_cursor(pointer))
    }

    pub fn canvas_editor_contains(&self, pointer: ratatui::layout::Position) -> bool {
        self.structured.canvas.editor.as_ref().is_some_and(|editor| editor.editor_area.contains(pointer))
    }

    pub fn canvas_cancel_node_edit(&mut self) -> bool {
        if self.structured.canvas.editor.take().is_none() {
            return false;
        }
        self.state.status_message = Some("Card edit canceled".to_string());
        true
    }

    pub fn canvas_commit_node_edit(&mut self) -> bool {
        let Some(editor) = self.structured.canvas.editor.take() else {
            return false;
        };
        let Some(previous_document) = self.structured.canvas.document.clone() else {
            return false;
        };
        let Some(node) = self.structured.canvas.document.as_mut().and_then(|document| document.nodes.get_mut(editor.node)) else {
            return false;
        };
        let changed = match (&mut node.kind, editor.field) {
            (CanvasNodeKind::Text { text }, CanvasNodeEditField::Text) => {
                let changed = *text != editor.draft;
                text.clone_from(&editor.draft);
                changed
            }
            (CanvasNodeKind::Link { url }, CanvasNodeEditField::Link) => {
                let value = editor.draft.trim();
                if value.is_empty() {
                    self.structured.canvas.editor = Some(editor);
                    self.state.status_message = Some("Enter a link before saving".to_string());
                    return false;
                }
                let changed = url != value;
                *url = value.to_string();
                changed
            }
            (CanvasNodeKind::Group { label, .. }, CanvasNodeEditField::GroupLabel) => {
                let value = (!editor.draft.trim().is_empty()).then(|| editor.draft.trim().to_string());
                let changed = *label != value;
                *label = value;
                changed
            }
            _ => {
                self.state.status_message = Some("The card changed before the edit could be saved".to_string());
                return false;
            }
        };
        if !changed {
            self.state.status_message = Some("No card changes".to_string());
            return true;
        }
        if self.persist_canvas_document("Card updated") {
            self.push_canvas_undo(previous_document);
            true
        } else {
            self.structured.canvas.document = Some(previous_document);
            self.structured.canvas.editor = Some(editor);
            false
        }
    }

    fn persist_canvas_document(&mut self, success_message: &str) -> bool {
        let source = match self.structured.canvas.document.as_ref().map(ekphos_canvas::Canvas::to_json_pretty) {
            Some(Ok(source)) => format!("{source}\n"),
            Some(Err(error)) => {
                self.show_error_toast(format!("Could not serialize Canvas: {error}"));
                return false;
            }
            None => return false,
        };
        if !self.persist_active_body(source) {
            return false;
        }
        if let Some(note_id) = self.current_note().map(|note| note.id) {
            self.structured.parse_key = Some((note_id, self.document.document_generation, self.vault.catalog_generation));
        }
        self.structured.vault_signature = vault_signature(self.vault.root());
        self.state.status_message = Some(success_message.to_string());
        true
    }

    pub fn open_selected_canvas_node(&mut self) -> bool {
        let kind = self.structured.canvas.document.as_ref().and_then(|document| document.nodes.get(self.structured.canvas.selected_node)).map(|node| node.kind.clone());
        match kind {
            Some(CanvasNodeKind::File { file, .. }) => {
                let target = match self.confined_vault_relative_path(&file) {
                    Ok(target) if target.exists() => target,
                    Ok(_) => {
                        self.state.status_message = Some("Canvas file not found".to_string());
                        return false;
                    }
                    Err(error) => {
                        self.state.status_message = Some(error);
                        return false;
                    }
                };
                if self.select_note_by_path(&target) {
                    true
                } else {
                    self.open_path_or_url(&target.to_string_lossy());
                    true
                }
            }
            Some(CanvasNodeKind::Link { url }) => {
                self.open_path_or_url(&url);
                true
            }
            Some(CanvasNodeKind::Text { .. }) | Some(CanvasNodeKind::Group { .. }) | Some(CanvasNodeKind::Unknown { .. }) | None => false,
        }
    }

    /// Polling is deliberately scoped to an active Base. It gives live query
    /// invalidation without keeping a permanent worker alive for vaults that
    /// are only browsing Markdown or Canvas files.
    pub(crate) fn poll_structured_vault(&mut self) -> bool {
        if self.editor.mode != Mode::Normal || self.active_document_kind() != Some(ekphos_vault::VaultFileKind::Base) {
            return false;
        }
        let now = self.dependencies.clock.now();
        if now.saturating_duration_since(self.structured.last_vault_poll) < std::time::Duration::from_millis(750) {
            return false;
        }
        self.structured.last_vault_poll = now;
        let signature = vault_signature(self.vault.root());
        if signature == self.structured.vault_signature {
            return false;
        }
        self.reload_on_focus();
        true
    }
}

fn resized_canvas_geometry(origin: (i64, i64, i64, i64), minimum: (i64, i64), handle: CanvasResizeHandle, dx: i64, dy: i64, preserve_aspect: bool) -> (i64, i64, i64, i64) {
    let (origin_x, origin_y, origin_width, origin_height) = origin;
    let origin_width = origin_width.max(1);
    let origin_height = origin_height.max(1);
    let minimum_width = minimum.0.max(1);
    let minimum_height = minimum.1.max(1);
    let origin_right = origin_x.saturating_add(origin_width);
    let origin_bottom = origin_y.saturating_add(origin_height);

    let mut left = if handle.affects_left() { origin_x.saturating_add(dx) } else { origin_x };
    let mut right = if handle.affects_right() { origin_right.saturating_add(dx) } else { origin_right };
    let mut top = if handle.affects_top() { origin_y.saturating_add(dy) } else { origin_y };
    let mut bottom = if handle.affects_bottom() { origin_bottom.saturating_add(dy) } else { origin_bottom };
    if right.saturating_sub(left) < minimum_width {
        if handle.affects_left() {
            left = right.saturating_sub(minimum_width);
        } else {
            right = left.saturating_add(minimum_width);
        }
    }
    if bottom.saturating_sub(top) < minimum_height {
        if handle.affects_top() {
            top = bottom.saturating_sub(minimum_height);
        } else {
            bottom = top.saturating_add(minimum_height);
        }
    }
    if !preserve_aspect {
        return (left, top, right.saturating_sub(left).max(1), bottom.saturating_sub(top).max(1));
    }

    let width = right.saturating_sub(left).max(1);
    let height = bottom.saturating_sub(top).max(1);
    let (new_width, new_height) = if handle.is_corner() {
        let width_scale = width as f64 / origin_width as f64;
        let height_scale = height as f64 / origin_height as f64;
        let mut scale = if (width_scale - 1.0).abs() >= (height_scale - 1.0).abs() { width_scale } else { height_scale };
        let minimum_scale = (minimum_width as f64 / origin_width as f64).max(minimum_height as f64 / origin_height as f64);
        scale = scale.max(minimum_scale);
        ((origin_width as f64 * scale).round() as i64, (origin_height as f64 * scale).round() as i64)
    } else if handle.affects_left() || handle.affects_right() {
        let width = width.max(minimum_width);
        (width, ((width as f64 * origin_height as f64 / origin_width as f64).round() as i64).max(minimum_height))
    } else {
        let height = height.max(minimum_height);
        (((height as f64 * origin_width as f64 / origin_height as f64).round() as i64).max(minimum_width), height)
    };

    if handle.is_corner() {
        left = if handle.affects_left() { origin_right.saturating_sub(new_width) } else { origin_x };
        top = if handle.affects_top() { origin_bottom.saturating_sub(new_height) } else { origin_y };
    } else if handle.affects_left() || handle.affects_right() {
        left = if handle.affects_left() { origin_right.saturating_sub(new_width) } else { origin_x };
        top = origin_y.saturating_add(origin_height / 2).saturating_sub(new_height / 2);
    } else {
        left = origin_x.saturating_add(origin_width / 2).saturating_sub(new_width / 2);
        top = if handle.affects_top() { origin_bottom.saturating_sub(new_height) } else { origin_y };
    }
    (left, top, new_width.max(1), new_height.max(1))
}

fn side_toward(from: &CanvasNode, to: &CanvasNode) -> CanvasSide {
    let from_center = (from.x as f64 + from.width as f64 / 2.0, from.y as f64 + from.height as f64 / 2.0);
    let to_center = (to.x as f64 + to.width as f64 / 2.0, to.y as f64 + to.height as f64 / 2.0);
    let dx = to_center.0 - from_center.0;
    let dy = to_center.1 - from_center.1;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            CanvasSide::Right
        } else {
            CanvasSide::Left
        }
    } else if dy >= 0.0 {
        CanvasSide::Bottom
    } else {
        CanvasSide::Top
    }
}

fn next_canvas_edge_id(document: &ekphos_canvas::Canvas) -> String {
    let mut sequence = document.edges.len() + 1;
    loop {
        let candidate = format!("ekphos-edge-{sequence}");
        if document.edges.iter().all(|edge| edge.id != candidate) {
            return candidate;
        }
        sequence += 1;
    }
}

fn push_bounded_history(history: &mut Vec<ekphos_canvas::Canvas>, document: ekphos_canvas::Canvas) {
    const MAX_CANVAS_HISTORY: usize = 32;
    if history.len() == MAX_CANVAS_HISTORY {
        history.remove(0);
    }
    history.push(document);
}

fn build_base_corpus(root: &std::path::Path, notes: &[Note], request_generation: u64, generation: &AtomicU64) -> Option<Corpus> {
    let mut records = Vec::with_capacity(notes.len());
    for note in notes {
        if generation.load(Ordering::Acquire) != request_generation {
            return None;
        }
        let Some(path) = note.file_path.as_ref() else {
            continue;
        };
        let Ok(relative_path) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative_path.to_string_lossy().replace('\\', "/");
        let extension = note.kind.extension().to_string();
        let folder = relative_path.parent().filter(|parent| !parent.as_os_str().is_empty()).map(|parent| parent.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        let mut properties = BTreeMap::new();
        let mut tags = Vec::new();
        let mut links = Vec::new();
        if note.kind.is_markdown() {
            if let Ok(body) = std::fs::read_to_string(path) {
                let (frontmatter, frontmatter_end) = ekphos_vault::Frontmatter::parse(&body);
                if let Some(frontmatter) = frontmatter {
                    if let Some(title) = frontmatter.title {
                        properties.insert("title".to_string(), Value::String(title));
                    }
                    if let Some(date) = frontmatter.date {
                        properties.insert("date".to_string(), ekphos_bases::parse_date(&date).map(Value::Date).unwrap_or(Value::String(date)));
                    }
                    if let Some(author) = frontmatter.author {
                        properties.insert("author".to_string(), Value::String(author));
                    }
                    tags = frontmatter.tags;
                    properties.insert("tags".to_string(), Value::List(tags.iter().cloned().map(Value::String).collect()));
                    properties.extend(frontmatter.extra.iter().map(|(key, value)| (key.clone(), Value::from_yaml(value))));
                }
                for tag in inline_tags(&body, frontmatter_end) {
                    if !tags.iter().any(|existing| existing.eq_ignore_ascii_case(&tag)) {
                        tags.push(tag);
                    }
                }
                properties.insert("tags".to_string(), Value::List(tags.iter().cloned().map(Value::String).collect()));
                links = ekphos_core::markdown::document_wiki_links_with_tilde_fences(&body, frontmatter_end.checked_sub(1), true).into_iter().map(|link| link.link.target.to_string()).collect();
            }
        }
        records.push(BaseRecord { id: note.id, path: relative, name: note.title.clone(), extension, folder, size: note.file_size, created: note.created_time.map(system_time_to_naive), modified: note.modified_time.map(system_time_to_naive), tags, links, properties });
    }
    (generation.load(Ordering::Acquire) == request_generation).then_some(Corpus { records })
}

fn inline_tags(body: &str, content_start_line: usize) -> Vec<String> {
    let mut tags = Vec::new();
    let mut fence: Option<&str> = None;
    for line in body.lines().skip(content_start_line) {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = marker {
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let mut in_code = false;
        let mut previous = None;
        let mut chars = line.char_indices().peekable();
        while let Some((start, character)) = chars.next() {
            if character == '`' {
                in_code = !in_code;
                previous = Some(character);
                continue;
            }
            if !in_code && character == '#' && previous != Some('\\') && previous.is_none_or(|value: char| value.is_whitespace() || matches!(value, '(' | '[' | '{' | ',' | ':')) {
                let tag_start = start + character.len_utf8();
                let mut tag_end = tag_start;
                while let Some(&(offset, value)) = chars.peek() {
                    if value.is_alphanumeric() || matches!(value, '_' | '-' | '/') {
                        chars.next();
                        tag_end = offset + value.len_utf8();
                    } else {
                        break;
                    }
                }
                if tag_end > tag_start {
                    tags.push(line[tag_start..tag_end].to_string());
                }
            }
            previous = Some(character);
        }
    }
    tags
}

fn system_time_to_naive(value: std::time::SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(value).naive_local()
}

fn format_yaml_error(error: &serde_yaml::Error) -> String {
    error.location().map_or_else(|| error.to_string(), |location| format!("{} at line {}, column {}", error, location.line(), location.column()))
}

fn vault_signature(root: &std::path::Path) -> u64 {
    fn visit(root: &std::path::Path, path: &std::path::Path, hasher: &mut std::collections::hash_map::DefaultHasher) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
                visit(root, &path, hasher);
            } else if file_type.is_file() && ekphos_vault::VaultFileKind::from_path(&path).is_some() {
                path.strip_prefix(root).unwrap_or(&path).hash(hasher);
                if let Ok(metadata) = entry.metadata() {
                    metadata.len().hash(hasher);
                    metadata.modified().ok().hash(hasher);
                }
            }
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    visit(root, root, &mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        app: App,
        root: PathBuf,
        vault: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-structured-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            std::fs::create_dir_all(&vault).unwrap();
            std::fs::write(vault.join("Alpha.md"), "---\nstatus: open\nscore: 7\ntags: [book]\n---\n# Alpha\n#inline-tag [[Beta]]\n`#not-a-tag`").unwrap();
            std::fs::write(vault.join("Beta.md"), "---\nstatus: done\nscore: 3\n---\n# Beta").unwrap();
            std::fs::write(
                vault.join("Library.base"),
                r#"filters: status == "open"
formulas:
  doubled: score * 2
views:
  - type: table
    name: Open
    order: [file.name, status, formula.doubled]
  - type: cards
    name: Everything
    order: [file.name]
"#,
            )
            .unwrap();
            std::fs::write(vault.join("Board.canvas"), r#"{"nodes":[{"id":"alpha","type":"file","file":"Alpha.md","x":0,"y":0,"width":240,"height":80},{"id":"note","type":"text","text":"Idea","x":320,"y":0,"width":200,"height":80}],"edges":[{"id":"edge","fromNode":"alpha","toNode":"note"}]}"#)
                .unwrap();
            let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, ..Default::default() };
            let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            let app = App::new_injected(config, vault.clone(), None, dependencies);
            Self { app, root, vault }
        }

        fn wait_for_base(&mut self) {
            for _ in 0..100 {
                if self.app.poll_base_evaluation() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            panic!("Base evaluation did not finish");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn base_document_builds_typed_rows_and_switches_views() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Library.base")));
        assert_eq!(fixture.app.active_document_kind(), Some(ekphos_vault::VaultFileKind::Base));
        assert!(fixture.app.document.content_items.is_empty());
        fixture.wait_for_base();
        let result = fixture.app.structured.base.result.as_ref().unwrap();
        assert_eq!(result.view_name, "Open");
        assert_eq!(result.matched_rows, 1);
        assert_eq!(result.groups[0].rows[0].path, "Alpha.md");
        assert_eq!(result.groups[0].rows[0].cells[2], Value::Number(14.0));
        assert!(fixture.app.structured.base.corpus.records.iter().find(|record| record.name == "Alpha").unwrap().tags.contains(&"inline-tag".to_string()));
        fixture.app.base_change_view(1);
        assert_eq!(fixture.app.structured.base.result.as_ref().unwrap().view_name, "Everything");
    }

    #[test]
    fn canvas_file_nodes_open_catalog_documents() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Board.canvas")));
        assert_eq!(fixture.app.structured.canvas.document.as_ref().unwrap().nodes.len(), 2);
        assert!(fixture.app.open_selected_canvas_node());
        assert_eq!(fixture.app.current_note().unwrap().title, "Alpha");
        assert_eq!(fixture.app.active_document_kind(), Some(ekphos_vault::VaultFileKind::Markdown));
    }

    #[test]
    fn canvas_edits_persist_positions_and_connections() {
        let mut fixture = Fixture::new();
        let canvas_path = fixture.vault.join("Board.canvas");
        assert!(fixture.app.select_note_by_path(&canvas_path));
        let original = fixture.app.structured.canvas.document.as_ref().unwrap().nodes[0].clone();

        fixture.app.canvas_begin_node_drag(0, (10, 10));
        fixture.app.canvas_pointer_drag((12, 11));
        fixture.app.canvas_end_pointer_interaction(None);
        let saved = std::fs::read_to_string(&canvas_path).unwrap();
        let (saved, diagnostics) = ekphos_canvas::parse_canvas(&saved).unwrap();
        assert!(diagnostics.is_empty());
        assert_eq!(saved.nodes[0].x, original.x + 40);
        assert_eq!(saved.nodes[0].y, original.y + 40);

        fixture.app.canvas_begin_connect(None, None);
        fixture.app.canvas_move_selection(1.0, 0.0);
        assert!(fixture.app.canvas_finish_keyboard_connect());
        assert_eq!(fixture.app.structured.canvas.document.as_ref().unwrap().edges.len(), 2);
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Connection added"));

        assert!(fixture.app.canvas_delete_selected_edge());
        let saved = std::fs::read_to_string(&canvas_path).unwrap();
        let (saved, diagnostics) = ekphos_canvas::parse_canvas(&saved).unwrap();
        assert!(diagnostics.is_empty());
        assert_eq!(saved.edges.len(), 1);
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Connection removed"));

        assert!(fixture.app.canvas_undo());
        assert_eq!(fixture.app.structured.canvas.document.as_ref().unwrap().edges.len(), 2);
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Canvas edit undone"));
        assert!(fixture.app.canvas_redo());
        assert_eq!(fixture.app.structured.canvas.document.as_ref().unwrap().edges.len(), 1);
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Canvas edit redone"));
    }

    #[test]
    fn canvas_text_cards_edit_in_place_and_round_trip_unicode() {
        let mut fixture = Fixture::new();
        let canvas_path = fixture.vault.join("Board.canvas");
        assert!(fixture.app.select_note_by_path(&canvas_path));
        fixture.app.canvas_select_node(1);

        assert!(fixture.app.canvas_begin_node_edit());
        assert!(fixture.app.canvas_edit_insert(" ✨"));
        assert!(fixture.app.canvas_edit_move_horizontal(-1));
        assert!(fixture.app.canvas_edit_insert("direct"));
        assert!(fixture.app.canvas_commit_node_edit());

        let saved = std::fs::read_to_string(&canvas_path).unwrap();
        let (saved, diagnostics) = ekphos_canvas::parse_canvas(&saved).unwrap();
        assert!(diagnostics.is_empty());
        assert_eq!(saved.nodes[1].kind, CanvasNodeKind::Text { text: "Idea direct✨".to_string() });
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Card updated"));

        assert!(fixture.app.canvas_undo());
        assert_eq!(fixture.app.structured.canvas.document.as_ref().unwrap().nodes[1].kind, CanvasNodeKind::Text { text: "Idea".to_string() });
    }

    #[test]
    fn canvas_card_edits_cancel_without_touching_the_document() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Board.canvas")));
        fixture.app.canvas_select_node(1);
        let original = fixture.app.structured.canvas.document.clone();

        assert!(fixture.app.canvas_begin_node_edit());
        assert!(fixture.app.canvas_edit_insert(" discarded"));
        assert!(fixture.app.canvas_cancel_node_edit());

        assert_eq!(fixture.app.structured.canvas.document, original);
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Card edit canceled"));
    }

    #[test]
    fn canvas_editor_vertical_motion_preserves_terminal_columns() {
        let text = "a界x\n1234".to_string();
        let first_line_cursor = "a界".len();
        let second_line_cursor = "a界x\n123".len();
        let mut editor = CanvasNodeEditor::new(0, CanvasNodeEditField::Text, text);
        editor.viewport_width = 20;
        editor.cursor = first_line_cursor;

        editor.move_vertical(1);
        assert_eq!(editor.cursor, second_line_cursor);
        editor.move_vertical(-1);
        assert_eq!(editor.cursor, first_line_cursor);
    }

    #[test]
    fn canvas_resize_persists_dimensions_and_undo_restores_them() {
        let mut fixture = Fixture::new();
        let canvas_path = fixture.vault.join("Board.canvas");
        assert!(fixture.app.select_note_by_path(&canvas_path));
        fixture.app.canvas_select_node(1);

        fixture.app.canvas_begin_node_resize(1, CanvasResizeHandle::Right, (10, 10));
        fixture.app.canvas_pointer_drag((13, 12));
        fixture.app.canvas_end_pointer_interaction(None);

        let source = std::fs::read_to_string(&canvas_path).unwrap();
        let (saved, diagnostics) = ekphos_canvas::parse_canvas(&source).unwrap();
        assert!(diagnostics.is_empty());
        assert_eq!((saved.nodes[1].x, saved.nodes[1].width, saved.nodes[1].height), (320, 260, 80));
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("Card resized"));

        assert!(fixture.app.canvas_undo());
        let node = &fixture.app.structured.canvas.document.as_ref().unwrap().nodes[1];
        assert_eq!((node.x, node.width, node.height), (320, 200, 80));
    }

    #[test]
    fn canvas_resize_cancel_and_aspect_ratio_keep_stable_geometry() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Board.canvas")));
        fixture.app.canvas_select_node(1);
        let original = fixture.app.structured.canvas.document.as_ref().unwrap().nodes[1].clone();

        fixture.app.canvas_begin_node_resize(1, CanvasResizeHandle::TopLeft, (10, 10));
        fixture.app.canvas_pointer_drag_with_aspect((5, 5), true);
        assert_ne!(fixture.app.structured.canvas.document.as_ref().unwrap().nodes[1], original);
        assert!(fixture.app.canvas_cancel_interaction());
        assert_eq!(fixture.app.structured.canvas.document.as_ref().unwrap().nodes[1], original);

        assert_eq!(resized_canvas_geometry((10, 20, 200, 100), (160, 100), CanvasResizeHandle::BottomRight, 100, 20, true), (10, 20, 300, 150));
    }

    #[test]
    fn canvas_resize_edges_anchor_the_opposite_side_and_respect_minimums() {
        let origin = (10, 20, 200, 120);
        let minimum = (160, 120);

        assert_eq!(resized_canvas_geometry(origin, minimum, CanvasResizeHandle::Left, 40, 0, false), (50, 20, 160, 120));
        assert_eq!(resized_canvas_geometry(origin, minimum, CanvasResizeHandle::Right, 40, 0, false), (10, 20, 240, 120));
        assert_eq!(resized_canvas_geometry(origin, minimum, CanvasResizeHandle::Top, 0, -40, false), (10, -20, 200, 160));
        assert_eq!(resized_canvas_geometry(origin, minimum, CanvasResizeHandle::Bottom, 0, 40, false), (10, 20, 200, 160));
        assert_eq!(resized_canvas_geometry(origin, minimum, CanvasResizeHandle::TopLeft, 500, 500, false), (50, 20, 160, 120));
    }

    #[test]
    fn canvas_editor_scrolls_to_caret_and_moves_by_grapheme() {
        let family = "👨‍👩‍👧‍👦";
        let mut editor = CanvasNodeEditor::new(0, CanvasNodeEditField::Text, format!("first\nsecond\nthird\na{family}b"));
        let layout = editor.layout(Rect::new(4, 2, 8, 2));
        assert!(layout.hidden_before);
        assert!(!layout.hidden_after);
        assert!(layout.caret.is_some());
        assert!(editor.scroll_row > 0);

        editor.move_horizontal(-1);
        assert_eq!(&editor.draft[editor.cursor..], "b");
        editor.move_horizontal(-1);
        assert!(editor.draft[..editor.cursor].ends_with('a'));
    }

    #[test]
    fn canvas_single_line_editor_discloses_overflow_and_supports_click_positioning() {
        let mut editor = CanvasNodeEditor::new(0, CanvasNodeEditField::Link, "https://example.test/a/very/long/path".to_string());
        let layout = editor.layout(Rect::new(10, 4, 14, 1));
        assert!(layout.hidden_before);
        assert!(!layout.hidden_after);
        assert!(layout.caret.is_some());
        let visible_row = editor.hit_rows[0].clone();

        assert!(editor.place_cursor(ratatui::layout::Position::new(visible_row.area.x, visible_row.area.y)));
        assert_eq!(editor.cursor, visible_row.start);
        editor.insert("\nnext");
        assert!(!editor.draft.contains('\n'));
        assert!(editor.draft.contains(" next"));
    }

    #[test]
    fn editing_a_canvas_file_card_opens_the_linked_note_editor() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Board.canvas")));

        assert!(fixture.app.canvas_begin_node_edit());

        assert_eq!(fixture.app.current_note().unwrap().title, "Alpha");
        assert_eq!(fixture.app.active_document_kind(), Some(ekphos_vault::VaultFileKind::Markdown));
        assert_eq!(fixture.app.editor.mode, Mode::Edit);
    }

    #[test]
    fn canvas_pointer_zoom_keeps_the_world_anchor_stable() {
        let mut fixture = Fixture::new();
        fixture.app.structured.canvas.view_area = Rect::new(10, 5, 100, 40);
        fixture.app.structured.canvas.viewport_x = 100.0;
        fixture.app.structured.canvas.viewport_y = 200.0;
        fixture.app.structured.canvas.zoom = 1.0;
        let pointer = (30, 15);
        let world_before = (fixture.app.structured.canvas.viewport_x + f64::from(pointer.0 - 10) * 20.0 / fixture.app.structured.canvas.zoom, fixture.app.structured.canvas.viewport_y + f64::from(pointer.1 - 5) * 40.0 / fixture.app.structured.canvas.zoom);

        fixture.app.canvas_zoom_at(2.0, Some(pointer));
        let world_after = (fixture.app.structured.canvas.viewport_x + f64::from(pointer.0 - 10) * 20.0 / fixture.app.structured.canvas.zoom, fixture.app.structured.canvas.viewport_y + f64::from(pointer.1 - 5) * 40.0 / fixture.app.structured.canvas.zoom);

        assert_eq!(world_before, world_after);
    }

    #[test]
    fn canvas_file_nodes_cannot_escape_the_vault() {
        let mut fixture = Fixture::new();
        std::fs::write(fixture.root.join("outside.txt"), "outside").unwrap();
        std::fs::write(fixture.vault.join("Escape.canvas"), r#"{"nodes":[{"id":"escape","type":"file","file":"../outside.txt","x":0,"y":0,"width":200,"height":80}],"edges":[]}"#).unwrap();
        fixture.app.load_notes_from_dir();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Escape.canvas")));

        assert!(!fixture.app.open_selected_canvas_node());
        assert_eq!(fixture.app.state.status_message.as_deref(), Some("The path must stay inside the vault"));
    }

    #[test]
    fn rename_and_move_keep_structured_document_extensions() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Library.base")));
        assert!(fixture.app.rename_note("Reading"));
        assert!(fixture.vault.join("Reading.base").exists());
        std::fs::create_dir(fixture.vault.join("Archive")).unwrap();
        fixture.app.move_note(&fixture.vault.join("Reading.base"), &fixture.vault.join("Archive"), "Reading").unwrap();
        assert!(fixture.vault.join("Archive/Reading.base").exists());
        assert!(!fixture.vault.join("Archive/Reading.md").exists());
    }

    #[test]
    fn active_base_reloads_when_vault_metadata_changes() {
        let mut fixture = Fixture::new();
        assert!(fixture.app.select_note_by_path(&fixture.vault.join("Library.base")));
        fixture.wait_for_base();
        assert_eq!(fixture.app.structured.base.result.as_ref().unwrap().matched_rows, 1);
        std::fs::write(fixture.vault.join("Alpha.md"), "---\nstatus: done\nscore: 7\ntags: [book]\nchanged: true\n---\n# Alpha").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert!(fixture.app.poll_structured_vault());
        fixture.wait_for_base();
        assert_eq!(fixture.app.structured.base.result.as_ref().unwrap().matched_rows, 0);
    }
}
