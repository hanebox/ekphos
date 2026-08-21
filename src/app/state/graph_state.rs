use super::*;

const RETAINED_GRAPH_INDEX_BUDGET: usize = 16 * 1024 * 1024;

impl App {
    pub fn start_graph_index_build(&mut self) {
        self.graph_index_generation = self.graph_index_generation.wrapping_add(1);
        let generation = self.graph_index_generation;
        let sources: Vec<GraphSourceFile> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(note_index, note)| {
                let absolute_path = note.file_path.clone()?;
                let fingerprint = self.vault.fingerprint(note.id)?;
                Some(GraphSourceFile {
                    metadata: GraphSourceMetadata {
                        note_id: note.id,
                        title: note.title.clone(),
                        path: self.get_wiki_path_for_note(note_index).unwrap_or_else(|| note.title.clone()),
                        tags: note
                            .frontmatter
                            .as_ref()
                            .map(|frontmatter| frontmatter.tags.iter().map(|tag| tag.to_string()).collect())
                            .unwrap_or_default(),
                    },
                    absolute_path,
                    fingerprint: GraphFileFingerprint {
                        size: fingerprint.size,
                        modified_nanos: fingerprint.modified_nanos.map(std::num::NonZeroU64::get).unwrap_or(0),
                    },
                })
            })
            .collect();
        let cache_path = self.graph_cache_path("graph_index.bin");
        self.graph_worker
            .get_or_insert_with(GraphWorker::new)
            .submit_build(generation, sources, cache_path);

        self.graph_index = None;
        self.graph_layout_generation = self.graph_layout_generation.wrapping_add(1);
        self.graph_view.global_positions = Vec::new();
        self.graph_view.global_fingerprint = None;
        self.graph_view.layout_pending = false;
        self.graph_indexing = true;
        self.graph_view.index_pending = true;
    }

    pub fn graph_has_background_work(&self) -> bool {
        self.graph_indexing || self.graph_view.layout_pending || self.graph_worker.as_ref().is_some_and(GraphWorker::is_pending)
    }

    /// Poll the single managed graph worker. Generation checks prevent a reload
    /// or a replaced layout request from installing stale note IDs.
    pub fn poll_graph_workers(&mut self) -> bool {
        let Some(response) = self.graph_worker.as_ref().and_then(GraphWorker::try_take) else {
            return false;
        };
        match response {
            GraphResponse::Index { generation, outcome } => {
                if generation != self.graph_index_generation {
                    return false;
                }
                let fingerprint_changed = self
                    .graph_index
                    .as_ref()
                    .map(|current| current.fingerprint != outcome.index.fingerprint)
                    .or_else(|| self.graph_view.global_fingerprint.map(|fingerprint| fingerprint != outcome.index.fingerprint))
                    .unwrap_or(true);
                self.graph_last_reused_files = outcome.reused_files;
                self.graph_last_parsed_files = outcome.parsed_files;
                self.graph_index = Some(Arc::new(outcome.index));
                self.graph_indexing = false;
                self.graph_view.index_pending = false;
                if fingerprint_changed {
                    self.graph_view.global_positions = Vec::new();
                    self.graph_view.global_fingerprint = None;
                }
                if self.dialog == DialogState::GraphView {
                    self.rebuild_graph_projection(true);
                    if self.graph_view.mode == GraphMode::Global {
                        self.start_global_graph_layout();
                    }
                }
                true
            }
            GraphResponse::Layout {
                generation,
                fingerprint,
                mut positions,
            } => {
                if generation != self.graph_layout_generation || self.graph_index.as_ref().map(|index| index.fingerprint) != Some(fingerprint) {
                    return false;
                }
                positions.sort_unstable_by_key(|(note_id, _, _)| *note_id);
                positions.shrink_to_fit();
                self.graph_view.global_positions = positions;
                self.graph_view.global_fingerprint = Some(fingerprint);
                self.graph_view.layout_pending = false;
                if self.dialog == DialogState::GraphView && self.graph_view.mode == GraphMode::Global {
                    self.rebuild_graph_projection(true);
                }
                true
            }
            GraphResponse::Failed { generation } => {
                if generation == self.graph_index_generation {
                    self.graph_indexing = false;
                    self.graph_view.index_pending = false;
                }
                if generation == self.graph_layout_generation {
                    self.graph_view.layout_pending = false;
                }
                true
            }
        }
    }

    pub(super) fn start_global_graph_layout(&mut self) {
        if self.dialog != DialogState::GraphView || self.graph_view.mode != GraphMode::Global {
            return;
        }
        let Some(index) = self.graph_index.clone() else {
            return;
        };
        if self.graph_view.global_fingerprint == Some(index.fingerprint) || self.graph_view.layout_pending {
            return;
        }
        let root_note_id = self
            .notes
            .get(self.graph_view.root_note_index)
            .map(|note| note.id)
            .unwrap_or_else(|| NoteId::new(0));
        let projection = index.project(GraphMode::Global, root_note_id, 1, GraphLinkScope::All, &GraphFilter::default(), true);
        self.graph_layout_generation = self.graph_layout_generation.wrapping_add(1);
        let generation = self.graph_layout_generation;
        let cache_path = self.graph_cache_path("graph_layout.bin");
        self.graph_worker
            .get_or_insert_with(GraphWorker::new)
            .submit_layout(generation, index, projection, cache_path);
        self.graph_view.layout_pending = true;
    }

    /// Open a fresh Local graph. Global projection and layout remain absent
    /// until the user explicitly switches modes.
    pub fn build_graph(&mut self) {
        self.graph_view.mode = GraphMode::Local;
        self.graph_view.root_note_index = self.selected_note;
        self.graph_view.selected_note_index = Some(self.selected_note);
        self.graph_view.index_pending = self.graph_index.is_none();
        self.graph_view.global_positions = Vec::new();
        self.graph_view.global_fingerprint = None;
        if self.graph_index.is_none() && !self.graph_indexing {
            self.start_graph_index_build();
        }
        self.rebuild_graph_projection(true);
    }

    pub fn rebuild_graph_projection(&mut self, refit: bool) {
        let Some(index) = self.graph_index.clone() else {
            self.graph_view.nodes = Vec::new();
            self.graph_view.edges = Vec::new();
            self.graph_view.total_nodes = 0;
            self.graph_view.total_edges = 0;
            return;
        };
        let filter = GraphFilter::parse(&self.graph_view.filter_query);
        let root_note_id = self
            .notes
            .get(self.graph_view.root_note_index)
            .map(|note| note.id)
            .unwrap_or_else(|| NoteId::new(0));
        let mut projection = index.project(
            self.graph_view.mode,
            root_note_id,
            self.graph_view.depth,
            self.graph_view.link_scope,
            &filter,
            self.graph_view.show_orphans,
        );
        match self.graph_view.mode {
            GraphMode::Local => graph::apply_local_layout(&index, &mut projection.nodes),
            GraphMode::Global => {
                for node in &mut projection.nodes {
                    if let Ok(position) = self.graph_view.global_positions.binary_search_by_key(&node.note_id, |(note_id, _, _)| *note_id) {
                        let (_, x, y) = self.graph_view.global_positions[position];
                        node.x = x;
                        node.y = y;
                        node.home_x = x;
                        node.home_y = y;
                    }
                }
                if self.graph_view.global_positions.is_empty() {
                    graph::apply_global_seed_layout(&index, &mut projection.nodes);
                }
            }
        }
        let preferred = self
            .graph_view
            .selected_note_index
            .or(Some(self.graph_view.root_note_index))
            .and_then(|note_index| self.notes.get(note_index).map(|note| note.id));
        let selected = preferred
            .and_then(|note_id| projection.nodes.iter().position(|node| node.note_id == note_id))
            .or(projection.root_node)
            .or_else(|| (!projection.nodes.is_empty()).then_some(0));
        self.graph_view.selected_node = selected;
        self.graph_view.selected_note_index = selected.and_then(|node| self.note_index_for_id(projection.nodes[node].note_id));
        self.graph_view.total_nodes = projection.total_nodes;
        self.graph_view.total_edges = projection.total_edges;
        self.graph_view.nodes = projection.nodes;
        self.graph_view.edges = projection.edges;
        self.graph_view.dirty = refit;
        self.graph_view.needs_center = false;
    }

    pub fn toggle_graph_mode(&mut self) {
        self.graph_view.mode = match self.graph_view.mode {
            GraphMode::Local => GraphMode::Global,
            GraphMode::Global => {
                if let Some(note_index) = self.graph_view.selected_note_index {
                    self.graph_view.root_note_index = note_index;
                }
                GraphMode::Local
            }
        };
        self.rebuild_graph_projection(true);
        if self.graph_view.mode == GraphMode::Global {
            self.start_global_graph_layout();
        } else if self.graph_view.layout_pending {
            self.graph_layout_generation = self.graph_layout_generation.wrapping_add(1);
            self.graph_view.layout_pending = false;
            if let Some(worker) = &self.graph_worker {
                worker.cancel();
            }
        }
    }

    pub fn change_graph_depth(&mut self, delta: isize) {
        let depth = (self.graph_view.depth as isize + delta).clamp(1, 5) as usize;
        if depth != self.graph_view.depth {
            self.graph_view.depth = depth;
            if self.graph_view.mode == GraphMode::Local {
                self.rebuild_graph_projection(true);
            }
        }
    }

    pub fn cycle_graph_link_scope(&mut self) {
        self.graph_view.link_scope = self.graph_view.link_scope.next();
        if self.graph_view.mode == GraphMode::Local {
            self.rebuild_graph_projection(true);
        }
    }

    pub fn update_graph_filter(&mut self, query: String, refit: bool) {
        self.graph_view.filter_query = query;
        self.rebuild_graph_projection(refit);
    }

    pub fn toggle_graph_orphans(&mut self) {
        self.graph_view.show_orphans = !self.graph_view.show_orphans;
        if self.graph_view.mode == GraphMode::Global {
            self.rebuild_graph_projection(false);
        }
    }

    pub fn reset_graph_view(&mut self) {
        self.graph_view.depth = 1;
        self.graph_view.link_scope = GraphLinkScope::All;
        self.graph_view.show_orphans = true;
        self.graph_view.filter_query.clear();
        self.graph_view.filter_draft.clear();
        self.rebuild_graph_projection(true);
    }

    pub fn reroot_graph_on_selected(&mut self) {
        let Some(note_index) = self.graph_view.selected_note_index else {
            return;
        };
        self.graph_view.root_note_index = note_index;
        self.graph_view.mode = GraphMode::Local;
        self.rebuild_graph_projection(true);
    }

    pub fn close_graph_view(&mut self) {
        self.dialog = DialogState::None;
        self.release_graph_session();
    }

    pub fn release_graph_session(&mut self) {
        self.graph_index_generation = self.graph_index_generation.wrapping_add(1);
        self.graph_layout_generation = self.graph_layout_generation.wrapping_add(1);
        self.graph_indexing = false;
        if let Some(worker) = self.graph_worker.take() {
            worker.cancel();
            drop(worker);
        }
        self.graph_view.nodes = Vec::new();
        self.graph_view.edges = Vec::new();
        self.graph_view.global_positions = Vec::new();
        self.graph_view.global_fingerprint = None;
        self.graph_view.selected_node = None;
        self.graph_view.index_pending = false;
        self.graph_view.layout_pending = false;
        self.graph_view.drag_start = None;
        self.graph_view.dragging_node = None;
        self.graph_view.is_panning = false;
        if self
            .graph_index
            .as_ref()
            .is_some_and(|index| index.retained_bytes() > RETAINED_GRAPH_INDEX_BUDGET)
        {
            self.graph_index = None;
        }
    }

    pub(super) fn invalidate_graph_service(&mut self) {
        self.release_graph_session();
        self.graph_index = None;
    }

    fn graph_cache_path(&self, file_name: &str) -> PathBuf {
        search::get_index_path_in(&self.dependencies.cache_dir, &self.config.notes_path()).with_file_name(file_name)
    }
}
