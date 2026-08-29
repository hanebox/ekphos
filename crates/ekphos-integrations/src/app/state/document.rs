use super::*;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

struct ParsedDocument {
    items: Vec<ContentItem>,
    tables: Vec<TableMetadata>,
    outline: Vec<OutlineItem>,
    links: Vec<LinkInfo>,
    link_ranges: Vec<DocumentLinkRange>,
}

impl ParsedDocument {
    fn push_item(&mut self, item: ContentItem, document: &DocumentSnapshot, wiki_exists: &dyn Fn(&str) -> bool) {
        let mut links = match &item {
            ContentItem::TextLine { range, .. } => parse_text_links(document.slice(*range), wiki_exists),
            ContentItem::TaskItem { text, .. } => parse_text_links(document.slice(*text), wiki_exists),
            ContentItem::TableRow { cells, table, is_separator: false, .. } => self.tables.get(*table as usize).map_or_else(Vec::new, |metadata| {
                let cells: Vec<&str> = cells.iter().map(|range| document.slice(*range)).collect();
                let widths: Vec<usize> = metadata.column_widths.iter().map(|width| *width as usize).collect();
                App::extract_table_links(&cells, &widths, &metadata.alignments).into_iter().map(|(text, url, start_col, end_col)| LinkInfo::Markdown { text, url, start_col, end_col }).collect()
            }),
            _ => Vec::new(),
        };
        links.sort_by_key(LinkInfo::start_col);
        let start = self.links.len();
        self.links.append(&mut links);
        self.link_ranges.push(DocumentLinkRange {
            start: u32::try_from(start).unwrap_or(u32::MAX),
            len: u16::try_from(self.links.len() - start).unwrap_or(u16::MAX),
            image_count: u16::try_from(self.links[start..].iter().filter(|link| matches!(link, LinkInfo::Image { .. })).count()).unwrap_or(u16::MAX),
        });
        self.items.push(item);
    }
}
fn parse_text_links(text: &str, wiki_exists: &dyn Fn(&str) -> bool) -> Vec<LinkInfo> {
    let inline_images = App::inline_image_links_in_text(text);
    let mut links: Vec<LinkInfo> = App::parse_markdown_links_in_text(text)
        .into_iter()
        .map(|(label, url, start_col, end_col)| if inline_images.iter().any(|(path, image_start)| path == &url && *image_start == start_col) { LinkInfo::Image { path: url, start_col, end_col } } else { LinkInfo::Markdown { text: label, url, start_col, end_col } })
        .collect();
    links.extend(ekphos_core::markdown::wiki_links(text).into_iter().map(|link| {
        let start_col = App::calc_wiki_rendered_pos(text, link.range.start);
        LinkInfo::Wiki { target: link.target.to_owned(), heading: link.heading.map(str::to_owned), start_col, end_col: start_col + link.display_text().width(), is_valid: wiki_exists(link.target) }
    }));
    links
}
fn range_for_slice(document: &DocumentSnapshot, source_line: usize, slice: &str) -> DocumentRange {
    let line = document.line(source_line).unwrap_or("");
    let relative_start = slice.as_ptr() as usize - line.as_ptr() as usize;
    document.range_within_line(source_line, relative_start..relative_start + slice.len()).unwrap_or_default()
}
fn push_text_line(parsed: &mut ParsedDocument, document: &DocumentSnapshot, source_line: usize, wiki_exists: &dyn Fn(&str) -> bool) {
    let line = document.line(source_line).unwrap_or("");
    let heading = ekphos_core::markdown::heading(line).filter(|heading| heading.level <= 3 && line[heading.level..].starts_with(' '));
    let heading_level = heading.as_ref().map_or(0, |heading| heading.level as u8);
    let item_index = parsed.items.len();
    if heading.is_some() {
        parsed.outline.push(OutlineItem { level: heading_level, source_line: source_line as u32, line: item_index });
    }
    parsed.push_item(ContentItem::TextLine { range: document.line_range(source_line).unwrap_or_default(), source_line: source_line as u32, heading_level }, document, wiki_exists);
}
fn parse_document(document: &DocumentSnapshot, frontmatter: Option<&CompactFrontmatter>, content_start_line: usize, frontmatter_hidden: bool, show_tags: bool, wiki_exists: &dyn Fn(&str) -> bool) -> ParsedDocument {
    let mut parsed = ParsedDocument { items: Vec::with_capacity(document.line_count()), tables: Vec::new(), outline: Vec::new(), links: Vec::new(), link_ranges: Vec::with_capacity(document.line_count()) };
    let mut line_index = 0usize;
    let has_frontmatter = frontmatter.is_some() && content_start_line > 0;
    if has_frontmatter && !frontmatter_hidden {
        parsed.push_item(ContentItem::FrontmatterDelimiter { source_line: 0 }, document, wiki_exists);
        for source_line in 1..content_start_line.saturating_sub(1).min(document.line_count()) {
            let line = document.line(source_line).unwrap_or("");
            let (key, value) = if let Some(colon) = line.find(':') { (line[..colon].trim(), line[colon + 1..].trim()) } else { (&line[..0], line) };
            parsed.push_item(ContentItem::FrontmatterLine { key: range_for_slice(document, source_line, key), value: range_for_slice(document, source_line, value), source_line: source_line as u32 }, document, wiki_exists);
        }
        parsed.push_item(ContentItem::FrontmatterDelimiter { source_line: content_start_line.saturating_sub(1) as u32 }, document, wiki_exists);
        line_index = content_start_line;
    } else if has_frontmatter {
        if show_tags && frontmatter.is_some_and(|metadata| !metadata.tags.is_empty() || metadata.date.is_some()) {
            parsed.push_item(ContentItem::TagBadges, document, wiki_exists);
        }
        line_index = content_start_line;
    }
    let mut in_code_block = false;
    while line_index < document.line_count() {
        let line = document.line(line_index).unwrap_or("");
        if line.starts_with("```") {
            let language = line.trim_start_matches('`');
            parsed.push_item(ContentItem::CodeFence { language: range_for_slice(document, line_index, language), source_line: line_index as u32 }, document, wiki_exists);
            in_code_block = !in_code_block;
            line_index += 1;
            continue;
        }
        if in_code_block {
            parsed.push_item(ContentItem::CodeLine { range: document.line_range(line_index).unwrap_or_default(), source_line: line_index as u32 }, document, wiki_exists);
            line_index += 1;
            continue;
        }
        if let Some(body) = ekphos_core::markdown::display_math_body(line) {
            parsed.push_item(ContentItem::MathBlock { range: range_for_slice(document, line_index, body), source_line: line_index as u32, end_line: line_index as u32 }, document, wiki_exists);
            line_index += 1;
            continue;
        }
        if ekphos_core::markdown::is_display_math_delimiter(line) {
            let opening_line = line_index;
            let closing_line = ((opening_line + 1)..document.line_count()).find(|candidate| document.line(*candidate).is_some_and(ekphos_core::markdown::is_display_math_delimiter));
            if let Some(closing_line) = closing_line {
                let start = document.line_range(opening_line + 1).map_or_else(|| document.line_range(opening_line).map_or(0, DocumentRange::end), DocumentRange::start);
                let end = closing_line.checked_sub(1).and_then(|line| document.line_range(line)).map_or(start, DocumentRange::end);
                let range = DocumentRange::new(start, end);
                if !document.slice(range).trim().is_empty() {
                    parsed.push_item(ContentItem::MathBlock { range, source_line: opening_line as u32, end_line: closing_line as u32 }, document, wiki_exists);
                    line_index = closing_line + 1;
                    continue;
                }
            }
        }
        if let Some(path) = standalone_image_path(line) {
            parsed.push_item(ContentItem::Image { path: range_for_slice(document, line_index, path), source_line: line_index as u32 }, document, wiki_exists);
            line_index += 1;
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            let checked = trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ");
            let leading_bytes = line.len().saturating_sub(trimmed.len());
            let indent: usize = line[..leading_bytes].chars().map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) }).sum();
            parsed.push_item(ContentItem::TaskItem { text: range_for_slice(document, line_index, &trimmed[6..]), checked, source_line: line_index as u32, indent: u16::try_from(indent).unwrap_or(u16::MAX) }, document, wiki_exists);
            line_index += 1;
            continue;
        }
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("<details") && (trimmed_line.ends_with('>') || trimmed_line.contains("><")) {
            let details_start_line = line_index;
            let mut summary = None;
            let mut content_lines = Vec::new();
            let mut found_end = false;
            line_index += 1;
            while line_index < document.line_count() {
                let source = document.line(line_index).unwrap_or("");
                let detail_line = source.trim();
                if detail_line.contains("</details>") {
                    found_end = true;
                    line_index += 1;
                    break;
                }
                if detail_line.starts_with("<summary>") || detail_line.contains("<summary>") {
                    let summary_text = if let (Some(start), Some(end)) = (detail_line.find("<summary>"), detail_line.find("</summary>")) { detail_line[start + 9..end].trim() } else { detail_line.trim_start_matches("<summary>").trim() };
                    if !summary_text.is_empty() {
                        summary = Some(range_for_slice(document, line_index, summary_text));
                    }
                    line_index += 1;
                    continue;
                }
                if detail_line == "</summary>" {
                    line_index += 1;
                    continue;
                }
                content_lines.push(line_index as u32);
                line_index += 1;
            }
            if found_end {
                parsed.push_item(ContentItem::Details { summary, content_lines: content_lines.into_boxed_slice(), source_line: details_start_line as u32 }, document, wiki_exists);
            } else {
                push_text_line(&mut parsed, document, details_start_line, wiki_exists);
            }
            continue;
        }
        if trimmed_line.starts_with('|') && trimmed_line.ends_with('|') {
            let table_id = parsed.tables.len() as u32;
            let mut rows: Vec<(Box<[DocumentRange]>, bool, u32)> = Vec::new();
            while line_index < document.line_count() {
                let source = document.line(line_index).unwrap_or("");
                let table_line = source.trim();
                if !table_line.starts_with('|') || !table_line.ends_with('|') {
                    break;
                }
                let inner = &table_line[1..table_line.len() - 1];
                let cells: Box<[DocumentRange]> = inner.split('|').map(str::trim).map(|cell| range_for_slice(document, line_index, cell)).collect();
                let is_separator = cells.iter().all(|range| {
                    let cell = document.slice(*range);
                    !cell.is_empty() && cell.chars().all(|character| character == '-' || character == ':')
                });
                rows.push((cells, is_separator, line_index as u32));
                line_index += 1;
            }
            let column_count = rows.iter().map(|(cells, _, _)| cells.len()).max().unwrap_or(0);
            let mut widths = vec![0u16; column_count];
            for (cells, is_separator, _) in &rows {
                if !is_separator {
                    for (column, cell) in cells.iter().enumerate() {
                        let width = crate::text::cell_visible_width(document.slice(*cell)).max(3);
                        widths[column] = widths[column].max(u16::try_from(width).unwrap_or(u16::MAX));
                    }
                }
            }
            for width in &mut widths {
                *width = (*width).max(3);
            }
            let separator = rows.iter().position(|(_, is_separator, _)| *is_separator);
            let mut alignments = vec![Alignment::Left; column_count];
            if let Some(separator_index) = separator {
                for (column, cell) in rows[separator_index].0.iter().enumerate() {
                    alignments[column] = Alignment::from_separator_cell(document.slice(*cell));
                }
            }
            parsed.tables.push(TableMetadata { column_widths: widths.into_boxed_slice(), alignments: alignments.into_boxed_slice() });
            for (row_index, (cells, is_separator, source_line)) in rows.into_iter().enumerate() {
                parsed.push_item(ContentItem::TableRow { cells, table: table_id, source_line, is_separator, is_header: separator.is_some_and(|separator_index| row_index < separator_index) }, document, wiki_exists);
            }
            continue;
        }
        push_text_line(&mut parsed, document, line_index, wiki_exists);
        line_index += 1;
    }
    parsed
}

impl App {
    pub fn document(&self) -> Option<&DocumentSnapshot> {
        self.document.active_document.as_ref()
    }
    pub fn document_slice(&self, range: DocumentRange) -> &str {
        self.document.active_document.as_ref().map_or("", |document| document.slice(range))
    }
    pub fn table_metadata(&self, table: u32) -> Option<&TableMetadata> {
        self.document.document_tables.get(table as usize)
    }

    pub fn update_outline(&mut self) {
        if !self.document.outline.is_empty() {
            self.document.outline_state.select(Some(0));
        }
    }

    pub fn update_content_items(&mut self) {
        let parse_key = (self.document.document_generation, self.vault.catalog_generation, self.document.frontmatter_hidden, self.state.config.show_tags);
        if self.document.active_document.is_some() && self.document.document_parse_key == Some(parse_key) {
            return;
        }
        self.document.content_items.clear();
        self.document.document_tables.clear();
        self.document.document_links.clear();
        self.document.document_link_ranges.clear();
        self.document.outline.clear();
        self.evict_document_services();
        self.state.inline_image_rects.clear();
        self.state.mouse_hover_inline_image = None;
        self.document.details_open_states.clear();
        self.document.heading_fold_states.clear();
        if let Some(document) = self.document.active_document.as_ref() {
            let (frontmatter, content_start_line) = self.current_note().map(|note| (note.frontmatter.as_ref(), note.content_start_line)).unwrap_or((None, 0));
            let parsed = parse_document(document, frontmatter, content_start_line, self.document.frontmatter_hidden, self.state.config.show_tags, &|target| self.wiki_link_exists(target));
            self.document.content_items = parsed.items;
            self.document.document_tables = parsed.tables;
            self.document.outline = parsed.outline;
            self.document.document_links = parsed.links;
            self.document.document_link_ranges = parsed.link_ranges;
            self.document.document_parse_key = Some(parse_key);
            self.document.document_parse_count = self.document.document_parse_count.saturating_add(1);
        } else {
            self.document.document_parse_key = None;
        }
        self.document.content_cursor = 0;
        self.update_outline();
    }

    pub fn next_content_line(&mut self) {
        if self.document.content_items.is_empty() {
            return;
        }
        let mut next = self.document.content_cursor + 1;
        while next < self.document.content_items.len() && !self.is_content_item_visible(next) {
            next += 1;
        }
        if next < self.document.content_items.len() {
            self.document.content_cursor = next;
            self.document.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn previous_content_line(&mut self) {
        if self.document.content_cursor == 0 {
            return;
        }
        let mut prev = self.document.content_cursor.saturating_sub(1);
        while prev > 0 && !self.is_content_item_visible(prev) {
            prev = prev.saturating_sub(1);
        }
        if self.is_content_item_visible(prev) {
            self.document.content_cursor = prev;
            self.document.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn goto_first_content_line(&mut self) {
        self.document.content_cursor = 0;
        while self.document.content_cursor < self.document.content_items.len() && !self.is_content_item_visible(self.document.content_cursor) {
            self.document.content_cursor += 1;
        }
        self.document.selected_link_index = 0;
    }

    pub fn goto_last_content_line(&mut self) {
        if !self.document.content_items.is_empty() {
            self.document.content_cursor = self.document.content_items.len() - 1;
            while self.document.content_cursor > 0 && !self.is_content_item_visible(self.document.content_cursor) {
                self.document.content_cursor -= 1;
            }
            self.document.selected_link_index = 0;
        }
    }

    pub fn half_page_down_content(&mut self) {
        if self.document.content_items.is_empty() {
            return;
        }
        let content_height = self.state.content_area.height.saturating_sub(2) as usize;
        let half = content_height / 2;
        let max_cursor = self.document.content_items.len().saturating_sub(1);
        let mut moved = 0;
        let mut new_cursor = self.document.content_cursor;
        while moved < half && new_cursor < max_cursor {
            new_cursor += 1;
            if self.is_content_item_visible(new_cursor) {
                moved += 1;
            }
        }
        self.document.content_cursor = new_cursor;
        self.document.selected_link_index = 0;
    }

    pub fn half_page_up_content(&mut self) {
        if self.document.content_items.is_empty() {
            return;
        }
        let content_height = self.state.content_area.height.saturating_sub(2) as usize;
        let half = content_height / 2;
        let mut moved = 0;
        let mut new_cursor = self.document.content_cursor;
        while moved < half && new_cursor > 0 {
            new_cursor -= 1;
            if self.is_content_item_visible(new_cursor) {
                moved += 1;
            }
        }
        self.document.content_cursor = new_cursor;
        self.document.selected_link_index = 0;
    }

    pub fn toggle_floating_cursor(&mut self) {
        self.editor.floating_cursor_mode = !self.editor.floating_cursor_mode;
    }

    pub fn floating_move_down(&mut self) {
        if self.document.content_items.is_empty() || !self.editor.floating_cursor_mode {
            return;
        }
        let mut next = self.document.content_cursor + 1;
        while next < self.document.content_items.len() && !self.is_content_item_visible(next) {
            next += 1;
        }
        if next < self.document.content_items.len() {
            self.document.content_cursor = next;
            self.document.selected_link_index = 0;
        }
    }

    pub fn floating_move_up(&mut self) {
        if !self.editor.floating_cursor_mode {
            return;
        }
        if self.document.content_cursor == 0 {
            return;
        }
        let mut prev = self.document.content_cursor.saturating_sub(1);
        while prev > 0 && !self.is_content_item_visible(prev) {
            prev = prev.saturating_sub(1);
        }
        if self.is_content_item_visible(prev) {
            self.document.content_cursor = prev;
            self.document.selected_link_index = 0;
        }
    }

    pub fn toggle_current_task(&mut self) {
        self.toggle_task_at(self.document.content_cursor);
    }

    pub fn toggle_current_details(&mut self) {
        if let Some(ContentItem::Details { source_line, .. }) = self.document.content_items.get(self.document.content_cursor) {
            let id = *source_line as usize;
            let current = self.document.details_open_states.get(&id).copied().unwrap_or(false);
            self.document.details_open_states.insert(id, !current);
        }
    }
    pub fn heading_level(line: &str) -> Option<usize> {
        ekphos_core::markdown::heading(line).filter(|heading| heading.level <= 3 && line[heading.level..].starts_with(' ')).map(|heading| heading.level)
    }
    pub fn is_heading_at(&self, idx: usize) -> bool {
        matches!(self.document.content_items.get(idx), Some(ContentItem::TextLine { heading_level, .. }) if *heading_level > 0)
    }
    pub fn is_heading_folded(&self, idx: usize) -> bool {
        self.document.heading_fold_states.get(&idx).copied().unwrap_or(false)
    }
    pub fn toggle_current_heading_fold(&mut self) {
        if self.is_heading_at(self.document.content_cursor) {
            let idx = self.document.content_cursor;
            let current = self.document.heading_fold_states.get(&idx).copied().unwrap_or(false);
            let new_state = !current;
            self.document.heading_fold_states.insert(idx, new_state);
            let msg = if new_state { "Folded" } else { "Unfolded" };
            self.state.status_message = Some(msg.to_string());
        }
    }
    pub fn toggle_heading_fold_at(&mut self, idx: usize) {
        if self.is_heading_at(idx) {
            let current = self.document.heading_fold_states.get(&idx).copied().unwrap_or(false);
            let new_state = !current;
            self.document.heading_fold_states.insert(idx, new_state);
            let msg = if new_state { "Folded" } else { "Unfolded" };
            self.state.status_message = Some(msg.to_string());
        }
    }
    pub fn get_heading_children_range(&self, heading_idx: usize) -> std::ops::Range<usize> {
        let heading_level = if let Some(ContentItem::TextLine { heading_level, .. }) = self.document.content_items.get(heading_idx) {
            *heading_level as usize
        } else {
            return heading_idx..heading_idx;
        };
        let mut end_idx = heading_idx + 1;
        while end_idx < self.document.content_items.len() {
            if let ContentItem::TextLine { heading_level: level, .. } = &self.document.content_items[end_idx] {
                if *level > 0 && *level as usize <= heading_level {
                    break;
                }
            }
            end_idx += 1;
        }
        (heading_idx + 1)..end_idx
    }
    pub fn is_content_item_visible(&self, idx: usize) -> bool {
        for (heading_idx, is_folded) in &self.document.heading_fold_states {
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
        for idx in 0..self.document.content_items.len() {
            if self.is_heading_at(idx) {
                self.document.heading_fold_states.insert(idx, true);
                count += 1;
            }
        }
        self.state.status_message = Some(format!("Folded {} headings", count));
    }
    pub fn unfold_all_headings(&mut self) {
        let count = self.document.heading_fold_states.len();
        self.document.heading_fold_states.clear();
        self.state.status_message = Some(format!("Unfolded {} headings", count));
    }
    pub fn unfold_heading_at(&mut self, idx: usize) {
        if self.is_heading_at(idx) && self.is_heading_folded(idx) {
            self.document.heading_fold_states.insert(idx, false);
        }
    }

    pub fn sync_outline_to_content(&mut self) {
        if self.document.outline.is_empty() {
            return;
        }
        let mut best_match: Option<usize> = None;
        for (i, item) in self.document.outline.iter().enumerate() {
            if item.line <= self.document.content_cursor {
                best_match = Some(i);
            } else {
                break;
            }
        }
        if let Some(idx) = best_match {
            self.document.outline_state.select(Some(idx));
        }
    }

    pub fn current_item_is_image(&self) -> Option<&str> {
        if let Some(ContentItem::Image { path, .. }) = self.document.content_items.get(self.document.content_cursor) {
            Some(self.document_slice(*path))
        } else {
            None
        }
    }

    pub fn current_item_link(&self) -> Option<String> {
        let links = self.item_links_at(self.document.content_cursor);
        if links.is_empty() {
            return None;
        }
        let idx = self.document.selected_link_index.min(links.len().saturating_sub(1));
        links.get(idx).map(|(_, url, _, _)| url.clone())
    }

    pub fn item_all_links_at(&self, index: usize) -> &[LinkInfo] {
        let Some(range) = self.document.document_link_ranges.get(index) else {
            return &[];
        };
        let start = range.start as usize;
        self.document.document_links.get(start..start + range.len as usize).unwrap_or(&[])
    }

    /// Inline preview images on a content item, paired with the selection index
    /// used by `[` / `]`. Task items reserve selection zero for the checkbox.
    pub fn item_inline_image_selections_at(&self, index: usize) -> Vec<(String, usize)> {
        let all_links = self.item_all_links_at(index);
        let is_task = matches!(self.document.content_items.get(index), Some(ContentItem::TaskItem { .. }));
        Self::inline_image_selections_for_links(all_links, is_task)
    }
    pub fn inline_image_count_at(&self, index: usize) -> usize {
        self.document.document_link_ranges.get(index).map_or(0, |range| range.image_count as usize)
    }
    pub(super) fn inline_image_selections_for_links(all_links: &[LinkInfo], is_task: bool) -> Vec<(String, usize)> {
        let task_offset = usize::from(is_task && !all_links.is_empty());
        all_links
            .iter()
            .enumerate()
            .filter_map(|(link_index, link)| match link {
                LinkInfo::Image { path, .. } => Some((path.clone(), link_index + task_offset)),
                _ => None,
            })
            .collect()
    }
    pub(super) fn inline_image_links_in_text(text: &str) -> Vec<(String, usize)> {
        let mut images = Vec::new();
        let mut search_start = 0;
        while search_start < text.len() {
            let remaining = &text[search_start..];
            let Some(image_offset) = remaining.find("![") else {
                break;
            };
            let image_start = search_start + image_offset;
            let is_double_bang = image_start > 0 && text.as_bytes().get(image_start - 1) == Some(&b'!');
            let is_inline_code = is_inside_inline_code(text, image_start);
            if is_double_bang || is_inline_code {
                search_start = image_start + 2;
                continue;
            }
            let from_image = &text[image_start..];
            let Some(bracket_end) = from_image[1..].find("](") else {
                search_start = image_start + 2;
                continue;
            };
            let destination = &from_image[1 + bracket_end + 2..];
            let Some(paren_end) = destination.find(')') else {
                search_start = image_start + 2;
                continue;
            };
            let path = &destination[..paren_end];
            if !path.is_empty() {
                images.push((path.to_string(), Self::calc_rendered_pos(text, image_start)));
            }
            search_start = image_start + 1 + bracket_end + 2 + paren_end + 1;
        }
        images
    }
    pub(super) fn is_current_task_item(&self) -> bool {
        matches!(self.document.content_items.get(self.document.content_cursor), Some(ContentItem::TaskItem { .. }))
    }
    pub fn is_task_checkbox_selected(&self) -> bool {
        self.is_current_task_item() && self.document.selected_link_index == 0
    }

    pub fn current_selected_link(&self) -> Option<LinkInfo> {
        let all_links = self.item_all_links_at(self.document.content_cursor);
        if all_links.is_empty() {
            return None;
        }
        let idx = if self.is_current_task_item() {
            if self.document.selected_link_index == 0 {
                return None;
            }
            (self.document.selected_link_index - 1).min(all_links.len().saturating_sub(1))
        } else {
            self.document.selected_link_index.min(all_links.len().saturating_sub(1))
        };
        all_links.get(idx).cloned()
    }

    pub fn current_line_link_count(&self) -> usize {
        let link_count = self.item_all_links_at(self.document.content_cursor).len();
        if self.is_current_task_item() && link_count > 0 {
            link_count + 1
        } else {
            link_count
        }
    }

    pub fn next_link(&mut self) {
        let link_count = self.current_line_link_count();
        if (self.is_current_task_item() && link_count > 0) || link_count > 1 {
            self.document.selected_link_index = (self.document.selected_link_index + 1) % link_count;
        }
    }

    pub fn previous_link(&mut self) {
        let link_count = self.current_line_link_count();
        if (self.is_current_task_item() && link_count > 0) || link_count > 1 {
            if self.document.selected_link_index == 0 {
                self.document.selected_link_index = link_count - 1;
            } else {
                self.document.selected_link_index -= 1;
            }
        }
    }

    /// Check if the current line has any links or wikilinks
    pub fn current_item_has_link(&self) -> bool {
        !self.item_all_links_at(self.document.content_cursor).is_empty()
    }

    /// Extract all `[text](url)` and bare URL links from each table cell, mapping positions
    /// into the row's rendered column space. Walks every cell end-to-end so multiple links
    /// per cell are all navigable.
    ///
    /// Rendered positions assume natural column widths and a single-line row. When a table
    /// wraps (capped widths, multi-line rows), keyboard Enter-to-open still works because it
    /// only uses the URL; mouse click accuracy on wrapped lines is not guaranteed by this
    /// method's output.
    #[cfg(test)]
    pub(super) fn extract_simple_table_links(cells: &[String], column_widths: &[usize], alignments: &[Alignment]) -> Vec<(String, String, usize, usize)> {
        let cells: Vec<&str> = cells.iter().map(String::as_str).collect();
        Self::extract_table_links(&cells, column_widths, alignments)
    }
    fn extract_table_links(cells: &[&str], column_widths: &[usize], alignments: &[Alignment]) -> Vec<(String, String, usize, usize)> {
        let mut links = Vec::new();
        let mut col_cursor = 0usize; // column within content area (after `  │` prefix)
        for (i, cell) in cells.iter().enumerate() {
            let width = column_widths.get(i).copied().unwrap_or_else(|| crate::text::cell_visible_width(cell));
            let visible = crate::text::cell_visible_width(cell);
            let pad = width.saturating_sub(visible);
            let alignment = alignments.get(i).copied().unwrap_or(Alignment::Left);
            let left_pad = match alignment {
                Alignment::Left => 0,
                Alignment::Right => pad,
                Alignment::Center => pad / 2,
            };
            let cell_start = col_cursor + 1 /* leading space */ + left_pad;
            let mut scan = 0;
            while scan < cell.len() {
                if let Some((display, url, raw_start, raw_end)) = Self::bracket_link_at(cell, scan) {
                    let pre_visible = crate::text::cell_visible_width(&cell[..raw_start]);
                    let start = cell_start + pre_visible;
                    let end = start + display.width();
                    links.push((display, url, start, end));
                    scan = raw_end;
                    continue;
                }
                if let Some(url_len) = crate::text::detect_bare_url_len(cell, scan) {
                    let url = cell[scan..scan + url_len].to_string();
                    let pre_visible = crate::text::cell_visible_width(&cell[..scan]);
                    let start = cell_start + pre_visible;
                    let end = start + url.width();
                    links.push((url.clone(), url, start, end));
                    scan += url_len;
                    continue;
                }
                scan += 1;
            }
            col_cursor += 1 + width + 1; // " " + width + " "
            if i + 1 < cells.len() {
                col_cursor += 1; // "│" between cells
            }
        }
        links
    }

    /// Parse `[label](url)` anchored at byte offset `at` in `s`, skipping wiki-link form `[[...]]`.
    /// Returns `(display, url, raw_start, raw_end_exclusive)` where display is the label (or url
    /// if label is empty). Returns None if no bracket link starts exactly at `at`.
    pub(super) fn bracket_link_at(s: &str, at: usize) -> Option<(String, String, usize, usize)> {
        let link = ekphos_core::markdown::markdown_link_at(s, at)?;
        if link.kind != ekphos_core::markdown::MarkdownLinkKind::Link || link.destination.is_empty() {
            return None;
        }
        let display = if link.label.is_empty() { link.destination.to_string() } else { link.label.to_string() };
        Some((display, link.destination.to_string(), link.range.start, link.range.end))
    }
    fn parse_markdown_links_in_text(text: &str) -> Vec<(String, String, usize, usize)> {
        let mut links = Vec::new();
        let mut search_start = 0;
        let mut claimed: Vec<(usize, usize)> = Vec::new();
        while search_start < text.len() {
            let remaining = &text[search_start..];
            if let Some(dbl_img_pos) = remaining.find("!![") {
                let single_img_pos = remaining.find("![");
                let bracket_pos = remaining.find('[');
                let is_first = single_img_pos.map(|s| dbl_img_pos <= s).unwrap_or(true) && bracket_pos.map(|b| dbl_img_pos < b).unwrap_or(true);
                if is_first {
                    let abs_img_pos = search_start + dbl_img_pos;
                    let from_img = &text[abs_img_pos..];
                    if let Some(bracket_end) = from_img[2..].find("](") {
                        let after_bracket = &from_img[2 + bracket_end + 2..];
                        if let Some(paren_end) = after_bracket.find(')') {
                            let alt_text = &from_img[3..2 + bracket_end];
                            let url = &after_bracket[..paren_end];
                            let image_end = abs_img_pos + 2 + bracket_end + 2 + paren_end + 1;
                            if is_inside_inline_code(text, abs_img_pos) {
                                search_start = image_end;
                                claimed.push((abs_img_pos, search_start));
                                continue;
                            }
                            if !url.is_empty() {
                                let display_text = if alt_text.is_empty() { url.to_string() } else { alt_text.to_string() };
                                let rendered_start = Self::calc_rendered_pos(text, abs_img_pos);
                                let rendered_end = rendered_start + display_text.width();
                                links.push((display_text, url.to_string(), rendered_start, rendered_end));
                            }
                            search_start = image_end;
                            claimed.push((abs_img_pos, search_start));
                            continue;
                        }
                    }
                }
            }
            if let Some(img_pos) = remaining.find("![") {
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
                            let image_end = abs_img_pos + 1 + bracket_end + 2 + paren_end + 1;
                            if is_inside_inline_code(text, abs_img_pos) {
                                search_start = image_end;
                                claimed.push((abs_img_pos, search_start));
                                continue;
                            }
                            if !url.is_empty() {
                                let display_text = if alt_text.is_empty() { format!("[img: {}]", url) } else { format!("[img: {}]", alt_text) };
                                let rendered_start = Self::calc_rendered_pos(text, abs_img_pos);
                                let rendered_end = rendered_start;
                                links.push((display_text, url.to_string(), rendered_start, rendered_end));
                            }
                            search_start = image_end;
                            claimed.push((abs_img_pos, search_start));
                            continue;
                        }
                    }
                }
            }
            if let Some(bracket_pos) = remaining.find('[') {
                let abs_bracket_pos = search_start + bracket_pos;
                let from_bracket = &text[abs_bracket_pos..];
                if let Some(wiki) = from_bracket.strip_prefix("[[") {
                    if let Some(close_pos) = wiki.find("]]") {
                        search_start = abs_bracket_pos + 2 + close_pos + 2;
                        continue;
                    }
                }
                if let Some(bracket_end) = from_bracket.find("](") {
                    let after_bracket = &from_bracket[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &from_bracket[1..bracket_end];
                        let url = &after_bracket[..paren_end];
                        let link_end = abs_bracket_pos + bracket_end + 2 + paren_end + 1;
                        if is_inside_inline_code(text, abs_bracket_pos) {
                            search_start = link_end;
                            claimed.push((abs_bracket_pos, search_start));
                            continue;
                        }
                        if !url.is_empty() {
                            let display_text = if link_text.is_empty() { url.to_string() } else { link_text.to_string() };
                            let rendered_start = Self::calc_rendered_pos(text, abs_bracket_pos);
                            let rendered_end = rendered_start + display_text.width();
                            links.push((display_text, url.to_string(), rendered_start, rendered_end));
                        }
                        search_start = link_end;
                        claimed.push((abs_bracket_pos, search_start));
                        continue;
                    }
                }
            }
            break;
        }
        let mut pos = 0;
        while pos < text.len() {
            if let Some(url_len) = crate::text::detect_bare_url_len(text, pos) {
                let end = pos + url_len;
                let overlaps = claimed.iter().any(|(s, e)| pos < *e && end > *s);
                if !overlaps && !is_inside_inline_code(text, pos) {
                    let url = text[pos..end].to_string();
                    let rendered_start = Self::calc_rendered_pos(text, pos);
                    let rendered_end = rendered_start + url.width();
                    links.push((url.clone(), url, rendered_start, rendered_end));
                }
                pos = end;
            } else {
                pos += 1;
            }
        }
        links
    }

    pub fn item_links_at(&self, index: usize) -> Vec<(String, String, usize, usize)> {
        self.item_all_links_at(index)
            .iter()
            .filter_map(|link| match link {
                LinkInfo::Markdown { text, url, start_col, end_col } => Some((text.clone(), url.clone(), *start_col, *end_col)),
                LinkInfo::Image { path, start_col, end_col } => Some((path.clone(), path.clone(), *start_col, *end_col)),
                LinkInfo::Wiki { .. } => None,
            })
            .collect()
    }
    pub(super) fn calc_rendered_pos(text: &str, target_pos: usize) -> usize {
        let mut rendered_pos = 0;
        let mut i = 0;
        while i < target_pos && i < text.len() {
            let remaining = &text[i..];
            if remaining.starts_with('$') {
                if let Some(math) = ekphos_core::markdown::inline_math_at(text, i) {
                    if math.range.end <= target_pos {
                        rendered_pos += math.source.width();
                        i = math.range.end;
                        continue;
                    }
                    break;
                }
            }
            if remaining.starts_with("!![") {
                if let Some(bracket_end) = remaining[2..].find("](") {
                    let after_bracket = &remaining[2 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[3..2 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 2 + bracket_end + 2 + paren_end + 1;
                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() { url.width() } else { alt_text.width() };
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
                        let full_link_len = 1 + bracket_end + 2 + paren_end + 1;
                        if i + full_link_len <= target_pos {
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            if let Some(wiki) = remaining.strip_prefix("[[") {
                if let Some(end_pos) = wiki.find("]]") {
                    let target = &wiki[..end_pos];
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
                        let full_link_len = bracket_end + 2 + paren_end + 1;
                        if i + full_link_len <= target_pos {
                            let display_len = if link_text.is_empty() { after_bracket[..paren_end].width() } else { link_text.width() };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            let character = remaining.chars().next();
            rendered_pos += character.map_or(0, |character| if character == '\t' { 4 } else { unicode_width::UnicodeWidthChar::width(character).unwrap_or(0) });
            i += character.map_or(1, char::len_utf8);
        }
        rendered_pos
    }

    /// Find a Markdown link using a column in the unwrapped rendered line.
    /// Wrapped mouse coordinates are converted to this space by the UI layer.
    pub fn find_clicked_link_at_col(&self, index: usize, click_col: usize) -> Option<String> {
        let links = self.item_links_at(index);
        if links.is_empty() {
            return None;
        }
        let prefix_len = self.get_line_prefix_len(index);
        for (_, url, start, end) in &links {
            let adjusted_start = prefix_len + *start;
            let adjusted_end = prefix_len + *end;
            if click_col >= adjusted_start && click_col < adjusted_end {
                return Some(url.clone());
            }
        }
        None
    }

    /// Find a wiki link using a column in the unwrapped rendered line.
    pub fn find_clicked_wiki_link_at_col(&self, index: usize, click_col: usize) -> Option<WikiLinkInfo> {
        let wiki_links = self.item_wiki_links_at(index);
        if wiki_links.is_empty() {
            return None;
        }
        let prefix_len = self.get_line_prefix_len(index);
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
        !self.item_all_links_at(index).is_empty()
    }
    pub(super) fn get_line_prefix_len(&self, index: usize) -> usize {
        match self.document.content_items.get(index) {
            Some(ContentItem::TextLine { .. }) => 2,
            Some(ContentItem::TaskItem { indent, .. }) => 6 + *indent as usize,
            Some(ContentItem::TableRow { .. }) => 3, // "  " cursor indicator + "│" left border
            _ => 2,
        }
    }

    pub fn item_is_image_at(&self, index: usize) -> Option<&str> {
        if let Some(ContentItem::Image { path, .. }) = self.document.content_items.get(index) {
            Some(self.document_slice(*path))
        } else {
            None
        }
    }

    pub fn item_is_details_at(&self, index: usize) -> bool {
        matches!(self.document.content_items.get(index), Some(ContentItem::Details { .. }))
    }

    pub fn toggle_details_at(&mut self, index: usize) {
        if let Some(ContentItem::Details { source_line, .. }) = self.document.content_items.get(index) {
            let id = *source_line as usize;
            let current = self.document.details_open_states.get(&id).copied().unwrap_or(false);
            self.document.details_open_states.insert(id, !current);
        }
    }

    pub fn is_click_on_task_checkbox(&self, index: usize, col: u16, content_x: u16) -> bool {
        let indent = match self.document.content_items.get(index) {
            Some(ContentItem::TaskItem { indent, .. }) => *indent,
            _ => return false,
        };
        let click_col = col.saturating_sub(content_x) as usize;
        click_col >= 2 + indent as usize && click_col <= 4 + indent as usize
    }

    pub fn toggle_task_at(&mut self, index: usize) {
        let saved_cursor = self.document.content_cursor;
        let Some((source_line, checked)) = self.document.content_items.get(index).and_then(|item| match item {
            ContentItem::TaskItem { source_line, checked, .. } => Some((*source_line as usize, *checked)),
            _ => None,
        }) else {
            return;
        };
        let Some(document) = self.document() else {
            return;
        };
        let Some(line_range) = document.line_range(source_line) else {
            return;
        };
        let line = document.slice(line_range);
        let marker = if checked { line.find("- [x]").or_else(|| line.find("- [X]")) } else { line.find("- [ ]") };
        let Some(marker) = marker else {
            return;
        };
        let checkbox = line_range.start() + marker + 3;
        let mut body = document.body().to_owned();
        body.replace_range(checkbox..checkbox + 1, if checked { " " } else { "x" });
        if self.persist_active_body(body) {
            self.update_content_items();
            self.document.content_cursor = saved_cursor.min(self.document.content_items.len().saturating_sub(1));
        }
    }

    pub fn open_current_link(&mut self) {
        if let Some(url) = self.current_item_link() {
            self.open_link(&url);
        }
    }

    /// Open a link - navigates internally for .md files, opens externally otherwise
    pub fn open_link(&mut self, url: &str) {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            let (path_part, heading) = if let Some(hash_pos) = url.find('#') { (&url[..hash_pos], Some(&url[hash_pos + 1..])) } else { (url, None) };
            if path_part.is_empty() {
                if let Some(heading_text) = heading {
                    self.navigate_to_heading(heading_text);
                }
                return;
            }
            if path_part.ends_with(".md") {
                let base_dir = self.current_note().and_then(|n| n.file_path.as_ref()).and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or_else(|| self.state.config.notes_path());
                let resolved = base_dir.join(path_part);
                if let Ok(canonical) = resolved.canonicalize() {
                    let found = self.vault.notes.iter().enumerate().find_map(|(idx, note)| note.file_path.as_ref().and_then(|fp| fp.canonicalize().ok()).filter(|cp| *cp == canonical).map(|_| idx));
                    if let Some(note_idx) = found {
                        if let Some(note) = self.vault.notes.get(note_idx) {
                            if let Some(ref file_path) = note.file_path {
                                let notes_root = self.state.config.notes_path();
                                let mut current = file_path.parent();
                                let mut needs_rebuild = false;
                                while let Some(parent) = current {
                                    if parent == notes_root {
                                        break;
                                    }
                                    if !self.vault.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                                        self.vault.folder_states.insert(parent.to_path_buf(), true);
                                        needs_rebuild = true;
                                    }
                                    current = parent.parent();
                                }
                                if needs_rebuild {
                                    Self::update_tree_expanded_states(&mut self.vault.file_tree, &self.vault.folder_states);
                                    self.rebuild_sidebar_items();
                                }
                            }
                        }
                        let target_id = self.vault.notes[note_idx].id;
                        for (idx, item) in self.vault.sidebar_items.iter().enumerate() {
                            if let SidebarItemKind::Note { note_id } = &item.kind {
                                if *note_id == target_id {
                                    if !self.load_note_body(target_id) {
                                        return;
                                    }
                                    self.end_buffer_search();
                                    self.vault.selected_sidebar_index = idx;
                                    self.vault.selected_note = note_idx;
                                    self.push_navigation_history(note_idx);
                                    self.document.content_cursor = 0;
                                    self.document.content_scroll_offset = 0;
                                    self.document.selected_link_index = 0;
                                    self.update_content_items();
                                    self.update_outline();
                                    if let Some(heading_text) = heading {
                                        self.navigate_to_heading(heading_text);
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
                return;
            }
        }
        #[cfg(target_os = "macos")]
        let _ = Command::new("open").arg(url).spawn();
        #[cfg(any(target_os = "android", target_os = "freebsd", target_os = "linux"))]
        let _ = Command::new("xdg-open").arg(url).spawn();
        #[cfg(target_os = "windows")]
        let _ = Command::new("cmd").args(["/c", "start", "", url]).spawn();
    }
}

#[cfg(test)]
mod phase6_tests {
    use super::*;

    #[test]
    fn shared_pass_emits_ranges_outline_links_and_one_table_metadata_owner() {
        let source = "# Head e\u{301}\nText [link](https://example.test) and [[Head]].\n- [x] task 😀\n| name | value |\n|:-----|------:|\n| 日本 | [open](target.md) |\n```rust\nlet x = 1;\n```\n<details>\n<summary>More</summary>\ninside\n</details>\n![image](image.png)\n";
        let document = DocumentSnapshot::new(Arc::from(source));
        let parsed = parse_document(&document, None, 0, true, true, &|target| target == "Head");
        assert_eq!(parsed.outline.len(), 1);
        assert_eq!(parsed.outline[0].source_line, 0);
        assert_eq!(parsed.tables.len(), 1);
        let table_ids: Vec<u32> = parsed
            .items
            .iter()
            .filter_map(|item| match item {
                ContentItem::TableRow { table, .. } => Some(*table),
                _ => None,
            })
            .collect();
        assert_eq!(table_ids, [0, 0, 0]);
        assert_eq!(parsed.tables[0].alignments.as_ref(), [Alignment::Left, Alignment::Right]);
        assert!(parsed.items.iter().any(|item| matches!(item, ContentItem::TaskItem { checked: true, .. })));
        assert!(parsed.items.iter().any(|item| matches!(item, ContentItem::CodeFence { .. })));
        assert!(parsed.items.iter().any(|item| matches!(item, ContentItem::Details { .. })));
        assert!(parsed.items.iter().any(|item| matches!(item, ContentItem::Image { .. })));
        assert!(parsed.links.iter().any(|link| matches!(link, LinkInfo::Markdown { url, .. } if url == "https://example.test")));
        assert!(parsed.links.iter().any(|link| matches!(link, LinkInfo::Wiki { target, is_valid: true, .. } if target == "Head")));
        assert_eq!(parsed.items.len(), parsed.link_ranges.len());
        assert!(std::mem::size_of::<ContentItem>() <= 40);
        assert!(std::mem::size_of::<DocumentLinkRange>() <= 8);
    }

    #[test]
    fn frontmatter_items_are_ranges_into_the_snapshot() {
        let source = "---\ntags: [one]\ndate: 2026-08-21\n---\n# Body\n";
        let document = DocumentSnapshot::new(Arc::from(source));
        let frontmatter = CompactFrontmatter { tags: vec![Box::<str>::from("one")].into_boxed_slice(), date: Some(Box::from("2026-08-21")) };
        let parsed = parse_document(&document, Some(&frontmatter), 4, false, true, &|_| false);
        let values: Vec<(&str, &str)> = parsed
            .items
            .iter()
            .filter_map(|item| match item {
                ContentItem::FrontmatterLine { key, value, .. } => Some((document.slice(*key), document.slice(*value))),
                _ => None,
            })
            .collect();
        assert_eq!(values, [("tags", "[one]"), ("date", "2026-08-21")]);
    }

    #[test]
    fn display_math_blocks_keep_compact_snapshot_ranges_and_skip_code() {
        let source = "Before\n$$\n\\int_0^1 x^2 \\, dx\n= \\frac{1}{3}\n$$\n$$e^{i\\pi}+1=0$$\n```md\n$$not math$$\n```\nAfter\n";
        let document = DocumentSnapshot::new(Arc::from(source));
        let parsed = parse_document(&document, None, 0, true, true, &|_| false);
        let blocks: Vec<(&str, u32, u32)> = parsed
            .items
            .iter()
            .filter_map(|item| match item {
                ContentItem::MathBlock { range, source_line, end_line } => Some((document.slice(*range), *source_line, *end_line)),
                _ => None,
            })
            .collect();
        assert_eq!(blocks, [("\\int_0^1 x^2 \\, dx\n= \\frac{1}{3}", 1, 4), ("e^{i\\pi}+1=0", 5, 5)]);
        assert!(parsed.items.iter().any(|item| matches!(item, ContentItem::CodeLine { range, .. } if document.slice(*range) == "$$not math$$")));
    }
}
