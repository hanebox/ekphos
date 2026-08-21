use super::*;

impl App {
    pub fn start_graph_index_build(&mut self) {
        self.graph_index_generation = self.graph_index_generation.wrapping_add(1);
        let generation = self.graph_index_generation;
        let sources: Vec<(GraphSourceMetadata, PathBuf)> = self
            .notes
            .iter()
            .enumerate()
            .filter_map(|(note_index, note)| {
                let path = note.file_path.clone()?;
                Some((
                    GraphSourceMetadata {
                        note_id: note.id,
                        title: note.title.clone(),
                        path: self.get_wiki_path_for_note(note_index).unwrap_or_else(|| note.title.clone()),
                        tags: note
                            .frontmatter
                            .as_ref()
                            .map(|frontmatter| frontmatter.tags.iter().map(|tag| tag.to_string()).collect())
                            .unwrap_or_default(),
                    },
                    path,
                ))
            })
            .collect();
        let (sender, receiver) = mpsc::channel();
        self.graph_index_receiver = receiver;
        // Note indices can be reassigned by a reload. Keep the last rendered
        // projection on screen, but do not use an old index for new actions.
        self.graph_index = None;
        self.graph_layout_generation = self.graph_layout_generation.wrapping_add(1);
        self.graph_view.layout_pending = false;
        self.graph_indexing = true;
        self.graph_view.index_pending = true;
        std::thread::spawn(move || {
            let paths: HashMap<NoteId, PathBuf> = sources.iter().map(|(source, path)| (source.note_id, path.clone())).collect();
            let metadata = sources.into_iter().map(|(source, _)| source).collect();
            let index = std::panic::catch_unwind(|| {
                GraphIndex::build_from_loader(metadata, |note_id| paths.get(&note_id).and_then(|path| fs::read_to_string(path).ok()))
            })
            .unwrap_or_default();
            let _ = sender.send((generation, index));
        });
    }

    pub fn graph_has_background_work(&self) -> bool {
        self.graph_indexing || self.graph_view.layout_pending
    }

    /// Poll graph workers. Returns true when graph UI state changed.
    pub fn poll_graph_workers(&mut self) -> bool {
        let mut changed = false;
        while let Ok((generation, index)) = self.graph_index_receiver.try_recv() {
            if generation != self.graph_index_generation {
                continue;
            }
            let fingerprint_changed = self
                .graph_index
                .as_ref()
                .map(|current| current.fingerprint != index.fingerprint)
                .or_else(|| self.graph_view.global_fingerprint.map(|fingerprint| fingerprint != index.fingerprint))
                .unwrap_or(true);
            self.graph_index = Some(Arc::new(index));
            self.graph_indexing = false;
            self.graph_view.index_pending = false;
            if fingerprint_changed {
                self.graph_view.global_positions.clear();
                self.graph_view.global_fingerprint = None;
            }
            if self.dialog == DialogState::GraphView {
                self.rebuild_graph_projection(true);
                self.start_global_graph_layout();
            }
            changed = true;
        }

        while let Ok((generation, fingerprint, positions)) = self.graph_layout_receiver.try_recv() {
            if generation != self.graph_layout_generation {
                continue;
            }
            if self.graph_index.as_ref().map(|index| index.fingerprint) != Some(fingerprint) {
                continue;
            }
            self.graph_view.global_positions = positions.into_iter().map(|(note_index, x, y)| (note_index, (x, y))).collect();
            self.graph_view.global_fingerprint = Some(fingerprint);
            self.graph_view.layout_pending = false;
            if self.dialog == DialogState::GraphView && self.graph_view.mode == GraphMode::Global {
                self.rebuild_graph_projection(true);
            }
            changed = true;
        }
        changed
    }

    pub(super) fn start_global_graph_layout(&mut self) {
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
        let mut projection = index.project(GraphMode::Global, root_note_id, 1, GraphLinkScope::All, &GraphFilter::default(), true);
        graph::apply_global_seed_layout(&mut projection.nodes);
        self.graph_view.global_positions = projection.nodes.iter().map(|node| (node.note_id, (node.x, node.y))).collect();
        self.graph_layout_generation = self.graph_layout_generation.wrapping_add(1);
        let generation = self.graph_layout_generation;
        let fingerprint = index.fingerprint;
        let cache_path = search::get_index_path_in(&self.dependencies.cache_dir, &self.config.notes_path()).with_file_name("graph_layout.bin");
        let (sender, receiver) = mpsc::channel();
        self.graph_layout_receiver = receiver;
        self.graph_view.layout_pending = true;
        std::thread::spawn(move || {
            if let Some(positions) = graph::load_layout_cache(&cache_path, fingerprint, &projection.nodes) {
                let _ = sender.send((generation, fingerprint, positions));
                return;
            }
            graph::apply_global_layout(&mut projection.nodes, &projection.edges);
            graph::save_layout_cache(&cache_path, fingerprint, &projection.nodes);
            let positions = projection.nodes.into_iter().map(|node| (node.note_id, node.x, node.y)).collect();
            let _ = sender.send((generation, fingerprint, positions));
        });
    }

    /// Open a fresh Local graph. Session preferences remain, but the active
    /// note always becomes the root as promised by Ctrl+G.
    pub fn build_graph(&mut self) {
        self.graph_view.mode = GraphMode::Local;
        self.graph_view.root_note_index = self.selected_note;
        self.graph_view.selected_note_index = Some(self.selected_note);
        self.graph_view.index_pending = self.graph_index.is_none();
        if self.graph_index.is_none() && !self.graph_indexing {
            self.start_graph_index_build();
        }
        self.rebuild_graph_projection(true);
        self.start_global_graph_layout();
    }

    pub fn rebuild_graph_projection(&mut self, refit: bool) {
        let Some(index) = self.graph_index.clone() else {
            self.graph_view.nodes.clear();
            self.graph_view.edges.clear();
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
            GraphMode::Local => graph::apply_local_layout(&mut projection.nodes),
            GraphMode::Global => {
                if !self.graph_view.global_positions.is_empty() {
                    for node in &mut projection.nodes {
                        if let Some(&(x, y)) = self.graph_view.global_positions.get(&node.note_id) {
                            node.x = x;
                            node.y = y;
                            node.home_x = x;
                            node.home_y = y;
                        }
                    }
                }
                if self.graph_view.global_positions.is_empty() {
                    graph::apply_global_seed_layout(&mut projection.nodes);
                }
            }
        }
        let preferred = self
            .graph_view
            .selected_note_index
            .or(Some(self.graph_view.root_note_index))
            .and_then(|index| self.notes.get(index).map(|note| note.id));
        let selected = preferred
            .and_then(|note_id| projection.nodes.iter().position(|node| node.note_id == note_id))
            .or(projection.root_node)
            .or_else(|| (!projection.nodes.is_empty()).then_some(0));
        self.graph_view.selected_node = selected;
        self.graph_view.selected_note_index = selected.and_then(|idx| self.note_index_for_id(projection.nodes[idx].note_id));
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
}
