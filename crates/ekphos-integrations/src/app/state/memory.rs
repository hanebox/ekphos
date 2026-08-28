use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppMemorySnapshot {
    pub catalog_count: usize,
    pub loaded_body_bytes: usize,
    pub body_cache_hits: u64,
    pub body_cache_misses: u64,
    pub body_cache_evictions: u64,
    pub parsed_document_bytes: usize,
    pub editor_and_undo_bytes: usize,
    pub history_payload_bytes: usize,
    pub search_index_bytes: usize,
    pub search_result_bytes: usize,
    pub graph_bytes: usize,
    pub graph_index_bytes: usize,
    pub graph_session_bytes: usize,
    pub graph_cache_reused_files: usize,
    pub graph_parsed_files: usize,
    pub image_bytes: usize,
    pub image_decoded_bytes: usize,
    pub image_protocol_bytes: usize,
    pub syntax_definition_bytes: usize,
    pub syntax_result_cache_bytes: usize,
    pub highlight_cache_bytes: usize,
    pub highlight_worker_bytes: usize,
    pub live_workers: usize,
    pub pending_requests: usize,
}

impl AppMemorySnapshot {
    pub fn attributed_bytes(&self) -> usize {
        self.loaded_body_bytes + self.parsed_document_bytes + self.editor_and_undo_bytes + self.search_index_bytes + self.search_result_bytes + self.graph_bytes + self.image_bytes + self.syntax_definition_bytes + self.highlight_cache_bytes
    }
}

impl App {
    pub(super) fn request_memory_reclaim(&mut self) {
        self.memory_reclaim_pending = true;
    }

    /// Run deferred allocator maintenance after the released UI state has been
    /// redrawn, keeping it out of measured input/close latency.
    #[doc(hidden)]
    pub fn reclaim_memory_if_requested(&mut self) {
        if !std::mem::take(&mut self.memory_reclaim_pending) {
            return;
        }
        self.graph.retired_session = None;
        self.workers.retired_graph = None;
        // SAFETY: both operations accept null value pointers and no input;
        // they only flush/purge the linked process allocator.
        unsafe {
            let _ = tikv_jemalloc_sys::mallctl(c"thread.tcache.flush".as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
            let _ = tikv_jemalloc_sys::mallctl(c"arena.4096.purge".as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
        }
    }

    pub fn memory_snapshot(&self) -> AppMemorySnapshot {
        let body_cache = self.vault.body_cache.stats();
        let parsed_document_bytes = self.document.active_document.as_ref().map_or(0, DocumentSnapshot::offset_bytes)
            + self.document.content_items.capacity() * std::mem::size_of::<ContentItem>()
            + self.document.content_items.iter().map(content_item_bytes).sum::<usize>()
            + self.document.document_tables.capacity() * std::mem::size_of::<TableMetadata>()
            + self.document.document_tables.iter().map(|table| table.column_widths.len() * std::mem::size_of::<u16>() + table.alignments.len() * std::mem::size_of::<Alignment>()).sum::<usize>()
            + self.document.document_link_ranges.capacity() * std::mem::size_of::<DocumentLinkRange>()
            + self.document.document_links.capacity() * std::mem::size_of::<LinkInfo>()
            + self.document.document_links.iter().map(link_info_bytes).sum::<usize>()
            + self.document.content_render_scratch.item_text_heights.capacity() * std::mem::size_of::<u16>()
            + self.document.content_render_scratch.constraints.capacity() * std::mem::size_of::<Constraint>()
            + self.document.content_render_scratch.visible_indices.capacity() * std::mem::size_of::<usize>();
        let search_result_bytes = match &self.search.search_picker {
            SearchPickerState::Closed => 0,
            SearchPickerState::Open { query, file_results, content_results, hydrated_content_results, content_preview, .. } => {
                query.capacity()
                    + file_results.capacity() * std::mem::size_of::<FilePickerResult>()
                    + file_results.iter().map(|result| result.display_name.capacity() + result.folder_hint.as_ref().map_or(0, String::capacity)).sum::<usize>()
                    + content_results.capacity() * std::mem::size_of::<SearchHit>()
                    + hydrated_content_results.capacity() * std::mem::size_of::<HydratedSearchResult>()
                    + hydrated_content_results.iter().map(|entry| entry.result.display_name.capacity() + entry.result.matched_line.capacity() + entry.result.folder_hint.as_ref().map_or(0, String::capacity)).sum::<usize>()
                    + content_preview.as_ref().map_or(0, |preview| preview.lines.capacity() * std::mem::size_of::<String>() + preview.lines.iter().map(String::capacity).sum::<usize>())
            }
        };
        let graph_session = self.graph.session.as_deref().or(self.graph.retired_session.as_deref());
        let graph_projection_bytes =
            graph_session.map_or(0, |session| session.graph_view.nodes.capacity() * std::mem::size_of::<GraphNode>() + session.graph_view.edges.capacity() * std::mem::size_of::<GraphEdge>() + session.graph_view.global_positions.capacity() * std::mem::size_of::<(NoteId, f32, f32)>());
        let graph_index_bytes = graph_session.and_then(|session| session.graph_index.as_ref()).map_or(0, |index| index.retained_bytes());
        let history_payload_bytes = self.editor.history_stats().total_payload_bytes();
        let highlight_worker_bytes = self.workers.highlight.as_ref().map_or(0, HighlightWorker::retained_bytes);
        let wiki_target_cache_bytes = self.document.wiki_target_cache.capacity() * std::mem::size_of::<String>() + self.document.wiki_target_cache.iter().map(String::capacity).sum::<usize>();
        let image_stats = self.images.worker.stats();
        let syntax_definition_bytes = self.state.syntax_service.definition_bytes();
        let syntax_result_cache_bytes = self.state.syntax_service.result_cache_bytes();
        AppMemorySnapshot {
            catalog_count: self.vault.notes.len(),
            loaded_body_bytes: self.document.active_document.as_ref().map_or(0, |document| document.body().len()) + body_cache.bytes,
            body_cache_hits: body_cache.hits,
            body_cache_misses: body_cache.misses,
            body_cache_evictions: body_cache.evictions,
            parsed_document_bytes,
            editor_and_undo_bytes: self.editor.retained_bytes(),
            history_payload_bytes,
            search_index_bytes: self.search.search_index.as_ref().map_or(0, |index| index.retained_bytes()),
            search_result_bytes,
            graph_bytes: graph_projection_bytes + graph_index_bytes,
            graph_index_bytes,
            graph_session_bytes: graph_projection_bytes,
            graph_cache_reused_files: self.graph.last_reused_files,
            graph_parsed_files: self.graph.last_parsed_files,
            image_bytes: image_stats.decoded_bytes + self.images.protocol_bytes,
            image_decoded_bytes: image_stats.decoded_bytes,
            image_protocol_bytes: self.images.protocol_bytes,
            syntax_definition_bytes,
            syntax_result_cache_bytes,
            highlight_cache_bytes: syntax_result_cache_bytes + highlight_worker_bytes + wiki_target_cache_bytes,
            highlight_worker_bytes,
            live_workers: usize::from(self.workers.highlight.is_some())
                + usize::from(self.workers.search.is_some())
                + usize::from(self.search.indexing_in_progress)
                + usize::from(self.workers.graph.is_some())
                + usize::from(self.workers.retired_graph.is_some())
                + self.state.syntax_service.live_workers()
                + image_stats.live_workers,
            pending_requests: usize::from(self.editor.highlight_pending || self.workers.highlight.as_ref().is_some_and(HighlightWorker::is_pending))
                + usize::from(self.workers.search.as_ref().is_some_and(SearchWorker::is_pending))
                + usize::from(self.search.indexing_in_progress)
                + usize::from(self.workers.graph.as_ref().is_some_and(GraphWorker::is_pending))
                + image_stats.pending_requests,
        }
    }
}
fn link_info_bytes(link: &LinkInfo) -> usize {
    match link {
        LinkInfo::Markdown { text, url, .. } => text.capacity() + url.capacity(),
        LinkInfo::Image { path, .. } => path.capacity(),
        LinkInfo::Wiki { target, heading, .. } => target.capacity() + heading.as_ref().map_or(0, String::capacity),
    }
}
fn content_item_bytes(item: &ContentItem) -> usize {
    match item {
        ContentItem::TableRow { cells, .. } => cells.len() * std::mem::size_of::<DocumentRange>(),
        ContentItem::Details { content_lines, .. } => content_lines.len() * std::mem::size_of::<u32>(),
        ContentItem::TextLine { .. } | ContentItem::Image { .. } | ContentItem::CodeLine { .. } | ContentItem::CodeFence { .. } | ContentItem::TaskItem { .. } | ContentItem::FrontmatterLine { .. } | ContentItem::TagBadges | ContentItem::FrontmatterDelimiter { .. } => 0,
    }
}
