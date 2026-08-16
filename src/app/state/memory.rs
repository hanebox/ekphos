use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppMemorySnapshot {
    pub catalog_count: usize,
    pub loaded_body_bytes: usize,
    pub parsed_document_bytes: usize,
    pub editor_and_undo_bytes: usize,
    pub search_index_bytes: usize,
    pub search_result_bytes: usize,
    pub graph_bytes: usize,
    pub image_bytes: usize,
    pub highlight_cache_bytes: usize,
    pub live_workers: usize,
    pub pending_requests: usize,
}

impl AppMemorySnapshot {
    pub fn attributed_bytes(&self) -> usize {
        self.loaded_body_bytes
            + self.parsed_document_bytes
            + self.editor_and_undo_bytes
            + self.search_index_bytes
            + self.search_result_bytes
            + self.graph_bytes
            + self.image_bytes
            + self.highlight_cache_bytes
    }
}

impl App {
    pub fn memory_snapshot(&self) -> AppMemorySnapshot {
        let parsed_document_bytes =
            self.content_items.capacity() * std::mem::size_of::<ContentItem>() + self.content_items.iter().map(content_item_bytes).sum::<usize>();
        let search_result_bytes = match &self.search_picker {
            SearchPickerState::Closed => 0,
            SearchPickerState::Open {
                query,
                file_results,
                content_results,
                ..
            } => {
                query.capacity()
                    + file_results.capacity() * std::mem::size_of::<FilePickerResult>()
                    + file_results
                        .iter()
                        .map(|result| result.display_name.capacity() + result.folder_hint.as_ref().map_or(0, String::capacity))
                        .sum::<usize>()
                    + content_results.capacity() * std::mem::size_of::<ContentSearchResult>()
                    + content_results
                        .iter()
                        .map(|result| result.display_name.capacity() + result.matched_line.capacity() + result.folder_hint.as_ref().map_or(0, String::capacity))
                        .sum::<usize>()
            }
        };
        let graph_projection_bytes = self.graph_view.nodes.capacity() * std::mem::size_of::<GraphNode>()
            + self
                .graph_view
                .nodes
                .iter()
                .map(|node| node.title.capacity() + node.full_title.capacity() + node.path.capacity())
                .sum::<usize>()
            + self.graph_view.edges.capacity() * std::mem::size_of::<GraphEdge>();

        AppMemorySnapshot {
            catalog_count: self.notes.len(),
            loaded_body_bytes: self.notes.iter().map(|note| note.content.capacity()).sum(),
            parsed_document_bytes,
            editor_and_undo_bytes: self.editor.retained_bytes(),
            search_index_bytes: self.search_index.retained_bytes(),
            search_result_bytes,
            graph_bytes: graph_projection_bytes + self.graph_index.as_ref().map_or(0, |index| index.retained_bytes()),
            image_bytes: self
                .image_states
                .values()
                .map(|state| usize::from(state.size.width) * usize::from(state.size.height) * 4)
                .sum(),
            highlight_cache_bytes: self.highlighter.as_ref().map_or(0, Highlighter::retained_cache_bytes),
            live_workers: usize::from(self.highlight_worker.is_some())
                + usize::from(self.indexing_in_progress)
                + usize::from(self.graph_indexing)
                + usize::from(self.graph_view.layout_pending)
                + usize::from(self.highlighter_loading),
            pending_requests: usize::from(self.highlight_pending)
                + usize::from(self.indexing_in_progress)
                + usize::from(self.graph_view.index_pending)
                + usize::from(self.graph_view.layout_pending)
                + self.pending_images.len(),
        }
    }
}

fn content_item_bytes(item: &ContentItem) -> usize {
    match item {
        ContentItem::TextLine(text) | ContentItem::Image(text) | ContentItem::CodeLine(text) | ContentItem::CodeFence(text) => text.capacity(),
        ContentItem::TaskItem { text, .. } => text.capacity(),
        ContentItem::TableRow {
            cells,
            column_widths,
            alignments,
            ..
        } => {
            cells.capacity() * std::mem::size_of::<String>()
                + cells.iter().map(String::capacity).sum::<usize>()
                + column_widths.capacity() * std::mem::size_of::<usize>()
                + alignments.capacity() * std::mem::size_of::<Alignment>()
        }
        ContentItem::Details { summary, content_lines, .. } => {
            summary.capacity() + content_lines.capacity() * std::mem::size_of::<String>() + content_lines.iter().map(String::capacity).sum::<usize>()
        }
        ContentItem::FrontmatterLine { key, value } => key.capacity() + value.capacity(),
        ContentItem::TagBadges { tags, date } => {
            tags.capacity() * std::mem::size_of::<String>() + tags.iter().map(String::capacity).sum::<usize>() + date.as_ref().map_or(0, String::capacity)
        }
        ContentItem::FrontmatterDelimiter => 0,
    }
}
