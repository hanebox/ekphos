use super::*;

impl App {
    pub fn update_outline(&mut self) {
        self.outline.clear();

        for (idx, item) in self.content_items.iter().enumerate() {
            if let ContentItem::TextLine(line) = item {
                if line.starts_with("# ") {
                    self.outline.push(OutlineItem {
                        level: 1,
                        title: line.trim_start_matches("# ").to_string(),
                        line: idx,
                    });
                } else if line.starts_with("## ") {
                    self.outline.push(OutlineItem {
                        level: 2,
                        title: line.trim_start_matches("## ").to_string(),
                        line: idx,
                    });
                } else if line.starts_with("### ") {
                    self.outline.push(OutlineItem {
                        level: 3,
                        title: line.trim_start_matches("### ").to_string(),
                        line: idx,
                    });
                }
            }
        }

        if !self.outline.is_empty() {
            self.outline_state.select(Some(0));
        }
    }

    pub fn update_content_items(&mut self) {
        self.content_items.clear();
        self.content_item_source_lines.clear();
        self.image_states.clear();
        self.inline_image_rects.clear();
        self.mouse_hover_inline_image = None;
        self.details_open_states.clear();
        self.heading_fold_states.clear();

        // Get note data to extract frontmatter info
        let note_data = self.current_note().map(|n| (n.content.clone(), n.frontmatter.clone(), n.content_start_line));

        if let Some((content, frontmatter, content_start_line)) = note_data {
            let mut in_code_block = false;
            let lines: Vec<&str> = content.lines().collect();
            let mut i = 0;

            // Handle frontmatter display
            let has_frontmatter = frontmatter.is_some() && content_start_line > 0;
            if has_frontmatter && !self.frontmatter_hidden {
                self.content_items.push(ContentItem::FrontmatterDelimiter);
                self.content_item_source_lines.push(0);

                // Parse and show frontmatter lines as key-value pairs
                for line_idx in 1..content_start_line.saturating_sub(1) {
                    if line_idx < lines.len() {
                        let line = lines[line_idx];
                        if let Some(colon_pos) = line.find(':') {
                            let key = line[..colon_pos].trim().to_string();
                            let value = line[colon_pos + 1..].trim().to_string();
                            self.content_items.push(ContentItem::FrontmatterLine { key, value });
                        } else {
                            self.content_items.push(ContentItem::FrontmatterLine {
                                key: String::new(),
                                value: line.to_string(),
                            });
                        }
                        self.content_item_source_lines.push(line_idx);
                    }
                }

                // Closing delimiter
                if content_start_line > 0 {
                    let closing_idx = content_start_line.saturating_sub(1);
                    self.content_items.push(ContentItem::FrontmatterDelimiter);
                    self.content_item_source_lines.push(closing_idx);
                }

                i = content_start_line;
            } else if has_frontmatter {
                if self.config.show_tags {
                    if let Some(ref fm) = frontmatter {
                        if !fm.tags.is_empty() || fm.date.is_some() {
                            self.content_items.push(ContentItem::TagBadges {
                                tags: fm.tags.clone(),
                                date: fm.date.clone(),
                            });
                            self.content_item_source_lines.push(0);
                        }
                    }
                }
                i = content_start_line;
            }

            while i < lines.len() {
                let line = lines[i];
                let line_index = i;

                // Check for code fence
                if line.starts_with("```") {
                    let lang = line.trim_start_matches('`').to_string();
                    self.content_items.push(ContentItem::CodeFence(lang));
                    self.content_item_source_lines.push(line_index);
                    in_code_block = !in_code_block;
                    i += 1;
                    continue;
                }

                // If inside code block, add as CodeLine
                if in_code_block {
                    self.content_items.push(ContentItem::CodeLine(line.to_string()));
                    self.content_item_source_lines.push(line_index);
                    i += 1;
                    continue;
                }

                // A line containing exactly one image gets the larger standalone
                // treatment. Multiple images on the same source line remain inline
                // and flow next to one another in the content renderer.
                if let Some(path) = standalone_image_path(line) {
                    self.content_items.push(ContentItem::Image(path.to_string()));
                    self.content_item_source_lines.push(line_index);
                    i += 1;
                    continue;
                }

                let trimmed = line.trim_start();
                if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
                    let checked = trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ");
                    let text = trimmed[6..].to_string();
                    let indent = line.chars().count() - trimmed.chars().count();
                    self.content_items.push(ContentItem::TaskItem {
                        text,
                        checked,
                        line_index,
                        indent,
                    });
                    self.content_item_source_lines.push(line_index);
                    i += 1;
                    continue;
                }

                let trimmed_line = line.trim();
                if trimmed_line.starts_with("<details") && (trimmed_line.ends_with(">") || trimmed_line.contains("><")) {
                    let details_start_line = line_index;
                    let mut summary = String::new();
                    let mut content_lines: Vec<String> = Vec::new();
                    let mut found_end = false;
                    i += 1;

                    while i < lines.len() {
                        let dline = lines[i].trim();

                        if dline.contains("</details>") {
                            found_end = true;
                            i += 1;
                            break;
                        }

                        if dline.starts_with("<summary>") || dline.contains("<summary>") {
                            if dline.contains("</summary>") {
                                if let Some(start) = dline.find("<summary>") {
                                    if let Some(end) = dline.find("</summary>") {
                                        summary = dline[start + 9..end].trim().to_string();
                                    }
                                }
                            } else {
                                summary = dline.trim_start_matches("<summary>").trim().to_string();
                            }
                            i += 1;
                            continue;
                        }

                        if dline == "</summary>" {
                            i += 1;
                            continue;
                        }

                        content_lines.push(lines[i].to_string());
                        i += 1;
                    }

                    if found_end {
                        if summary.is_empty() {
                            summary = "Details".to_string();
                        }
                        self.content_items.push(ContentItem::Details {
                            summary,
                            content_lines,
                            id: details_start_line,
                        });
                        self.content_item_source_lines.push(details_start_line);
                        continue;
                    } else {
                        self.content_items.push(ContentItem::TextLine(line.to_string()));
                        self.content_item_source_lines.push(line_index);
                        continue;
                    }
                }

                if trimmed_line.starts_with('|') && trimmed_line.ends_with('|') {
                    let table_start_line = line_index;
                    let mut table_rows: Vec<(Vec<String>, bool)> = Vec::new();

                    while i < lines.len() {
                        let tline = lines[i].trim();
                        if tline.starts_with('|') && tline.ends_with('|') {
                            let inner = &tline[1..tline.len() - 1];
                            let cells: Vec<String> = inner.split('|').map(|s| s.trim().to_string()).collect();
                            let is_separator = cells.iter().all(|cell| {
                                let c = cell.trim();
                                !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')
                            });
                            table_rows.push((cells, is_separator));
                            i += 1;
                        } else {
                            break;
                        }
                    }

                    let num_cols = table_rows.iter().map(|(cells, _)| cells.len()).max().unwrap_or(0);
                    let mut column_widths: Vec<usize> = vec![0; num_cols];

                    for (cells, is_sep) in &table_rows {
                        if !is_sep {
                            for (col_idx, cell) in cells.iter().enumerate() {
                                if col_idx < column_widths.len() {
                                    column_widths[col_idx] = column_widths[col_idx].max(crate::ui::cell_visible_width(cell));
                                }
                            }
                        }
                    }

                    for w in &mut column_widths {
                        *w = (*w).max(3);
                    }

                    let separator_idx = table_rows.iter().position(|(_, is_sep)| *is_sep);

                    // Derive per-column alignment from the separator row. Tables without
                    // a separator fall back to Left (GFM default).
                    let mut alignments: Vec<Alignment> = vec![Alignment::Left; num_cols];
                    if let Some(sep_idx) = separator_idx {
                        if let Some((sep_cells, _)) = table_rows.get(sep_idx) {
                            for (col_idx, cell) in sep_cells.iter().enumerate() {
                                if col_idx >= alignments.len() {
                                    break;
                                }
                                alignments[col_idx] = Alignment::from_separator_cell(cell);
                            }
                        }
                    }

                    for (row_idx, (cells, is_separator)) in table_rows.into_iter().enumerate() {
                        let is_header = separator_idx.map(|sep_idx| row_idx < sep_idx).unwrap_or(false);
                        self.content_items.push(ContentItem::TableRow {
                            cells,
                            is_separator,
                            is_header,
                            column_widths: column_widths.clone(),
                            alignments: alignments.clone(),
                        });
                        self.content_item_source_lines.push(table_start_line + row_idx);
                    }
                    continue;
                }

                self.content_items.push(ContentItem::TextLine(line.to_string()));
                self.content_item_source_lines.push(line_index);
                i += 1;
            }
        }
        self.content_cursor = 0;
    }

    pub fn next_content_line(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        // Find next visible content item
        let mut next = self.content_cursor + 1;
        while next < self.content_items.len() && !self.is_content_item_visible(next) {
            next += 1;
        }
        if next < self.content_items.len() {
            self.content_cursor = next;
            self.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn previous_content_line(&mut self) {
        if self.content_cursor == 0 {
            return;
        }
        // Find previous visible content item
        let mut prev = self.content_cursor.saturating_sub(1);
        while prev > 0 && !self.is_content_item_visible(prev) {
            prev = prev.saturating_sub(1);
        }
        // Only move if the target is visible
        if self.is_content_item_visible(prev) {
            self.content_cursor = prev;
            self.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn goto_first_content_line(&mut self) {
        // Find first visible item
        self.content_cursor = 0;
        while self.content_cursor < self.content_items.len() && !self.is_content_item_visible(self.content_cursor) {
            self.content_cursor += 1;
        }
        self.selected_link_index = 0;
    }

    pub fn goto_last_content_line(&mut self) {
        if !self.content_items.is_empty() {
            // Find last visible item
            self.content_cursor = self.content_items.len() - 1;
            while self.content_cursor > 0 && !self.is_content_item_visible(self.content_cursor) {
                self.content_cursor -= 1;
            }
            self.selected_link_index = 0;
        }
    }

    pub fn half_page_down_content(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        let content_height = self.content_area.height.saturating_sub(2) as usize;
        let half = content_height / 2;
        let max_cursor = self.content_items.len().saturating_sub(1);

        // Count visible items to move by half page
        let mut moved = 0;
        let mut new_cursor = self.content_cursor;
        while moved < half && new_cursor < max_cursor {
            new_cursor += 1;
            if self.is_content_item_visible(new_cursor) {
                moved += 1;
            }
        }
        self.content_cursor = new_cursor;
        self.selected_link_index = 0;
    }

    pub fn half_page_up_content(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        let content_height = self.content_area.height.saturating_sub(2) as usize;
        let half = content_height / 2;

        // Count visible items to move by half page
        let mut moved = 0;
        let mut new_cursor = self.content_cursor;
        while moved < half && new_cursor > 0 {
            new_cursor -= 1;
            if self.is_content_item_visible(new_cursor) {
                moved += 1;
            }
        }
        self.content_cursor = new_cursor;
        self.selected_link_index = 0;
    }

    pub fn toggle_floating_cursor(&mut self) {
        self.floating_cursor_mode = !self.floating_cursor_mode;
    }

    pub fn floating_move_down(&mut self) {
        if self.content_items.is_empty() || !self.floating_cursor_mode {
            return;
        }

        // Find next visible content item
        let mut next = self.content_cursor + 1;
        while next < self.content_items.len() && !self.is_content_item_visible(next) {
            next += 1;
        }
        if next < self.content_items.len() {
            self.content_cursor = next;
            self.selected_link_index = 0;
        }
    }

    pub fn floating_move_up(&mut self) {
        if !self.floating_cursor_mode {
            return;
        }

        if self.content_cursor == 0 {
            return;
        }
        // Find previous visible content item
        let mut prev = self.content_cursor.saturating_sub(1);
        while prev > 0 && !self.is_content_item_visible(prev) {
            prev = prev.saturating_sub(1);
        }
        if self.is_content_item_visible(prev) {
            self.content_cursor = prev;
            self.selected_link_index = 0;
        }
    }

    pub fn toggle_current_task(&mut self) {
        let saved_cursor = self.content_cursor;

        if let Some(item) = self.content_items.get(self.content_cursor) {
            if let ContentItem::TaskItem { line_index, checked, .. } = item {
                let line_index = *line_index;
                let new_checked = !*checked;

                if let Some(note) = self.notes.get_mut(self.selected_note) {
                    let lines: Vec<&str> = note.content.lines().collect();
                    if line_index < lines.len() {
                        let line = lines[line_index];
                        let new_line = if new_checked {
                            line.replacen("- [ ]", "- [x]", 1)
                        } else {
                            line.replacen("- [x]", "- [ ]", 1).replacen("- [X]", "- [ ]", 1)
                        };

                        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                        new_lines[line_index] = new_line;
                        note.content = new_lines.join("\n");

                        if let Some(ref path) = note.file_path {
                            let _ = fs::write(path, &note.content);
                        }
                    }
                }

                self.update_content_items();
                self.content_cursor = saved_cursor.min(self.content_items.len().saturating_sub(1));
            }
        }
    }

    pub fn toggle_current_details(&mut self) {
        if let Some(item) = self.content_items.get(self.content_cursor) {
            if let ContentItem::Details { id, .. } = item {
                let id = *id;
                let current = self.details_open_states.get(&id).copied().unwrap_or(false);
                self.details_open_states.insert(id, !current);
            }
        }
    }
    pub fn heading_level(line: &str) -> Option<usize> {
        ekphos_core::markdown::heading(line)
            .filter(|heading| heading.level <= 3 && line[heading.level..].starts_with(' '))
            .map(|heading| heading.level)
    }
    pub fn is_heading_at(&self, idx: usize) -> bool {
        if let Some(ContentItem::TextLine(line)) = self.content_items.get(idx) {
            Self::heading_level(line).is_some()
        } else {
            false
        }
    }
    pub fn is_heading_folded(&self, idx: usize) -> bool {
        self.heading_fold_states.get(&idx).copied().unwrap_or(false)
    }
    pub fn toggle_current_heading_fold(&mut self) {
        if self.is_heading_at(self.content_cursor) {
            let idx = self.content_cursor;
            let current = self.heading_fold_states.get(&idx).copied().unwrap_or(false);
            let new_state = !current;
            self.heading_fold_states.insert(idx, new_state);
            let msg = if new_state { "Folded" } else { "Unfolded" };
            self.status_message = Some(msg.to_string());
        }
    }
    pub fn toggle_heading_fold_at(&mut self, idx: usize) {
        if self.is_heading_at(idx) {
            let current = self.heading_fold_states.get(&idx).copied().unwrap_or(false);
            let new_state = !current;
            self.heading_fold_states.insert(idx, new_state);
            let msg = if new_state { "Folded" } else { "Unfolded" };
            self.status_message = Some(msg.to_string());
        }
    }
    pub fn get_heading_children_range(&self, heading_idx: usize) -> std::ops::Range<usize> {
        let heading_level = if let Some(ContentItem::TextLine(line)) = self.content_items.get(heading_idx) {
            Self::heading_level(line).unwrap_or(0)
        } else {
            return heading_idx..heading_idx;
        };

        let mut end_idx = heading_idx + 1;
        while end_idx < self.content_items.len() {
            if let ContentItem::TextLine(line) = &self.content_items[end_idx] {
                if let Some(level) = Self::heading_level(line) {
                    if level <= heading_level {
                        break;
                    }
                }
            }
            end_idx += 1;
        }
        (heading_idx + 1)..end_idx
    }
    pub fn is_content_item_visible(&self, idx: usize) -> bool {
        for (heading_idx, is_folded) in &self.heading_fold_states {
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
        for idx in 0..self.content_items.len() {
            if self.is_heading_at(idx) {
                self.heading_fold_states.insert(idx, true);
                count += 1;
            }
        }
        self.status_message = Some(format!("Folded {} headings", count));
    }
    pub fn unfold_all_headings(&mut self) {
        let count = self.heading_fold_states.len();
        self.heading_fold_states.clear();
        self.status_message = Some(format!("Unfolded {} headings", count));
    }
    pub fn unfold_heading_at(&mut self, idx: usize) {
        if self.is_heading_at(idx) && self.is_heading_folded(idx) {
            self.heading_fold_states.insert(idx, false);
        }
    }

    pub fn sync_outline_to_content(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        // Find the outline item that corresponds to the current content line
        // or the closest heading before the current line
        let mut best_match: Option<usize> = None;
        for (i, item) in self.outline.iter().enumerate() {
            if item.line <= self.content_cursor {
                best_match = Some(i);
            } else {
                break;
            }
        }
        if let Some(idx) = best_match {
            self.outline_state.select(Some(idx));
        }
    }

    pub fn current_item_is_image(&self) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(self.content_cursor) {
            Some(path)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn current_item_link(&self) -> Option<String> {
        let links = self.item_links_at(self.content_cursor);
        if links.is_empty() {
            return None;
        }
        let idx = self.selected_link_index.min(links.len().saturating_sub(1));
        links.get(idx).map(|(_, url, _, _)| url.clone())
    }

    pub fn item_all_links_at(&self, index: usize) -> Vec<LinkInfo> {
        let mut all_links = Vec::new();
        let inline_images = self.inline_image_links_at(index);

        for (text, url, start, end) in self.item_links_at(index) {
            if inline_images.iter().any(|(path, start_col)| path == &url && *start_col == start) {
                all_links.push(LinkInfo::Image {
                    path: url,
                    start_col: start,
                    end_col: end,
                });
            } else {
                all_links.push(LinkInfo::Markdown {
                    text,
                    url,
                    start_col: start,
                    end_col: end,
                });
            }
        }
        for wiki in self.item_wiki_links_at(index) {
            all_links.push(LinkInfo::Wiki {
                target: wiki.target,
                heading: wiki.heading,
                start_col: wiki.start_col,
                end_col: wiki.end_col,
                is_valid: wiki.is_valid,
            });
        }

        all_links.sort_by_key(|link| link.start_col());
        all_links
    }

    /// Inline preview images on a content item, paired with the selection index
    /// used by `[` / `]`. Task items reserve selection zero for the checkbox.
    pub fn item_inline_image_selections_at(&self, index: usize) -> Vec<(String, usize)> {
        let all_links = self.item_all_links_at(index);
        let is_task = matches!(self.content_items.get(index), Some(ContentItem::TaskItem { .. }));
        Self::inline_image_selections_for_links(all_links, is_task)
    }

    pub(super) fn inline_image_selections_for_links(all_links: Vec<LinkInfo>, is_task: bool) -> Vec<(String, usize)> {
        let task_offset = usize::from(is_task && !all_links.is_empty());

        all_links
            .into_iter()
            .enumerate()
            .filter_map(|(link_index, link)| match link {
                LinkInfo::Image { path, .. } => Some((path, link_index + task_offset)),
                _ => None,
            })
            .collect()
    }

    /// Extract single-bang Markdown images which are rendered as previews. The
    /// returned column is the position in the formatted text after Markdown
    /// syntax has been removed; images themselves occupy no text cells.
    pub(super) fn inline_image_links_at(&self, index: usize) -> Vec<(String, usize)> {
        let text = match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => line.as_str(),
            Some(ContentItem::TaskItem { text, .. }) => text.as_str(),
            _ => return Vec::new(),
        };

        Self::inline_image_links_in_text(text)
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

            // `!![alt](url)` is intentionally a text-only link. Markdown-like
            // syntax inside inline code is literal and must not create a preview.
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
        matches!(self.content_items.get(self.content_cursor), Some(ContentItem::TaskItem { .. }))
    }
    pub fn is_task_checkbox_selected(&self) -> bool {
        self.is_current_task_item() && self.selected_link_index == 0
    }

    pub fn current_selected_link(&self) -> Option<LinkInfo> {
        let all_links = self.item_all_links_at(self.content_cursor);
        if all_links.is_empty() {
            return None;
        }

        let idx = if self.is_current_task_item() {
            if self.selected_link_index == 0 {
                return None;
            }
            (self.selected_link_index - 1).min(all_links.len().saturating_sub(1))
        } else {
            self.selected_link_index.min(all_links.len().saturating_sub(1))
        };

        all_links.get(idx).cloned()
    }

    pub fn current_line_link_count(&self) -> usize {
        let link_count = self.item_all_links_at(self.content_cursor).len();
        if self.is_current_task_item() && link_count > 0 {
            link_count + 1
        } else {
            link_count
        }
    }

    pub fn next_link(&mut self) {
        let link_count = self.current_line_link_count();
        if self.is_current_task_item() && link_count > 0 {
            self.selected_link_index = (self.selected_link_index + 1) % link_count;
        } else if link_count > 1 {
            self.selected_link_index = (self.selected_link_index + 1) % link_count;
        }
    }

    pub fn previous_link(&mut self) {
        let link_count = self.current_line_link_count();
        if self.is_current_task_item() && link_count > 0 {
            if self.selected_link_index == 0 {
                self.selected_link_index = link_count - 1;
            } else {
                self.selected_link_index -= 1;
            }
        } else if link_count > 1 {
            if self.selected_link_index == 0 {
                self.selected_link_index = link_count - 1;
            } else {
                self.selected_link_index -= 1;
            }
        }
    }

    /// Check if the current line has any links or wikilinks
    #[allow(dead_code)]
    pub fn current_item_has_link(&self) -> bool {
        !self.item_all_links_at(self.content_cursor).is_empty()
    }

    /// Extract all `[text](url)` and bare URL links from each table cell, mapping positions
    /// into the row's rendered column space. Walks every cell end-to-end so multiple links
    /// per cell are all navigable.
    ///
    /// Rendered positions assume natural column widths and a single-line row. When a table
    /// wraps (capped widths, multi-line rows), keyboard Enter-to-open still works because it
    /// only uses the URL; mouse click accuracy on wrapped lines is not guaranteed by this
    /// method's output.
    pub(super) fn extract_simple_table_links(cells: &[String], column_widths: &[usize], alignments: &[Alignment]) -> Vec<(String, String, usize, usize)> {
        let mut links = Vec::new();
        let mut col_cursor = 0usize; // column within content area (after `  │` prefix)
        for (i, cell) in cells.iter().enumerate() {
            let width = column_widths.get(i).copied().unwrap_or_else(|| crate::ui::cell_visible_width(cell));
            let visible = crate::ui::cell_visible_width(cell);
            let pad = width.saturating_sub(visible);
            let alignment = alignments.get(i).copied().unwrap_or(Alignment::Left);
            let left_pad = match alignment {
                Alignment::Left => 0,
                Alignment::Right => pad,
                Alignment::Center => pad / 2,
            };
            let cell_start = col_cursor + 1 /* leading space */ + left_pad;

            // Walk the cell: at each position, try to recognise a bracket link first (so a
            // bare URL inside its `(url)` portion is not double-emitted), then a bare URL.
            let mut scan = 0;
            while scan < cell.len() {
                if let Some((display, url, raw_start, raw_end)) = Self::bracket_link_at(cell, scan) {
                    let pre_visible = crate::ui::cell_visible_width(&cell[..raw_start]);
                    let start = cell_start + pre_visible;
                    let end = start + display.chars().count();
                    links.push((display, url, start, end));
                    scan = raw_end;
                    continue;
                }
                if let Some(url_len) = crate::ui::detect_bare_url_len(cell, scan) {
                    let url = cell[scan..scan + url_len].to_string();
                    let pre_visible = crate::ui::cell_visible_width(&cell[..scan]);
                    let start = cell_start + pre_visible;
                    let end = start + url.chars().count();
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
        let display = if link.label.is_empty() {
            link.destination.to_string()
        } else {
            link.label.to_string()
        };
        Some((display, link.destination.to_string(), link.range.start, link.range.end))
    }

    /// Extract all links and images from a specific content item as (text, url, start_col, end_col) tuples
    /// The columns are character positions in the rendered line (after prefix like "▶ " or "• ")
    pub fn item_links_at(&self, index: usize) -> Vec<(String, String, usize, usize)> {
        let text = match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => line.as_str(),
            Some(ContentItem::TaskItem { text, .. }) => text.as_str(),
            Some(ContentItem::TableRow {
                cells,
                is_separator,
                column_widths,
                alignments,
                ..
            }) => {
                if *is_separator {
                    return Vec::new();
                }
                return Self::extract_simple_table_links(cells, column_widths, alignments);
            }
            _ => return Vec::new(),
        };

        let mut links = Vec::new();
        let mut search_start = 0;
        // Raw byte ranges claimed by bracket-style links/images. Used to skip bare URLs
        // that fall inside a `(url)` portion so we don't double-emit.
        let mut claimed: Vec<(usize, usize)> = Vec::new();

        while search_start < text.len() {
            let remaining = &text[search_start..];

            // Check for double-bang image !![alt](url) first (text-only, no preview)
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
                                let rendered_end = rendered_start + display_text.chars().count();

                                links.push((display_text, url.to_string(), rendered_start, rendered_end));
                            }

                            search_start = image_end;
                            claimed.push((abs_img_pos, search_start));
                            continue;
                        }
                    }
                }
            }

            // check for single-bang image
            if let Some(img_pos) = remaining.find("![") {
                // skip if this is actually a double-bang
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
                                let display_text = if alt_text.is_empty() {
                                    format!("[img: {}]", url)
                                } else {
                                    format!("[img: {}]", alt_text)
                                };
                                let rendered_start = Self::calc_rendered_pos(text, abs_img_pos);
                                // Preview images do not occupy cells in the prose line;
                                // their selectable region is the thumbnail rect instead.
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

            //check for regular markdown link
            if let Some(bracket_pos) = remaining.find('[') {
                let abs_bracket_pos = search_start + bracket_pos;
                let from_bracket = &text[abs_bracket_pos..];

                // skip if this is part of a wiki link
                if from_bracket.starts_with("[[") {
                    if let Some(close_pos) = from_bracket[2..].find("]]") {
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
                            let rendered_end = rendered_start + display_text.chars().count();

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

        // Bare URL autolink pass. Skips URLs that fall inside already-claimed bracket-link
        // ranges so e.g. `[click](https://x)` doesn't double-emit the URL inside the parens.
        let mut pos = 0;
        while pos < text.len() {
            if let Some(url_len) = crate::ui::detect_bare_url_len(text, pos) {
                let end = pos + url_len;
                let overlaps = claimed.iter().any(|(s, e)| pos < *e && end > *s);
                if !overlaps && !is_inside_inline_code(text, pos) {
                    let url = text[pos..end].to_string();
                    let rendered_start = Self::calc_rendered_pos(text, pos);
                    let rendered_end = rendered_start + url.chars().count();
                    links.push((url.clone(), url, rendered_start, rendered_end));
                }
                pos = end;
            } else {
                pos += 1;
            }
        }

        links
    }

    pub(super) fn calc_rendered_pos(text: &str, target_pos: usize) -> usize {
        let mut rendered_pos = 0;
        let mut i = 0;

        while i < target_pos && i < text.len() {
            let remaining = &text[i..];

            if remaining.starts_with("!![") {
                if let Some(bracket_end) = remaining[2..].find("](") {
                    let after_bracket = &remaining[2 + bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let alt_text = &remaining[3..2 + bracket_end];
                        let url = &after_bracket[..paren_end];
                        let full_link_len = 2 + bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            let display_len = if alt_text.is_empty() { url.chars().count() } else { alt_text.chars().count() };
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
                            // Single-bang images are removed from the prose line and
                            // rendered in the thumbnail flow below it.
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }

            if remaining.starts_with("[[") {
                if let Some(end_pos) = remaining[2..].find("]]") {
                    let target = &remaining[2..2 + end_pos];
                    let full_link_len = 2 + end_pos + 2;

                    if i + full_link_len <= target_pos {
                        rendered_pos += target.chars().count();
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
                            let display_len = if link_text.is_empty() {
                                after_bracket[..paren_end].chars().count()
                            } else {
                                link_text.chars().count()
                            };
                            rendered_pos += display_len;
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            rendered_pos += 1;
            i += remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }

        rendered_pos
    }

    /// Find a Markdown link using a column in the unwrapped rendered line.
    /// Wrapped mouse coordinates are converted to this space by the UI layer.
    pub(crate) fn find_clicked_link_at_col(&self, index: usize, click_col: usize) -> Option<String> {
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
    pub(crate) fn find_clicked_wiki_link_at_col(&self, index: usize, click_col: usize) -> Option<WikiLinkInfo> {
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
        !self.item_links_at(index).is_empty() || !self.item_wiki_links_at(index).is_empty()
    }

    pub(super) fn get_line_prefix_len(&self, index: usize) -> usize {
        match self.content_items.get(index) {
            // Bullet markers are already counted by calc_rendered_pos because
            // they are present in the source line, even though the renderer
            // replaces them with a visual bullet.
            Some(ContentItem::TextLine(_)) => 2,
            Some(ContentItem::TaskItem { indent, .. }) => 6 + indent,
            Some(ContentItem::TableRow { .. }) => 3, // "  " cursor indicator + "│" left border
            _ => 2,
        }
    }

    pub fn item_is_image_at(&self, index: usize) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(index) {
            Some(path)
        } else {
            None
        }
    }

    pub fn item_is_details_at(&self, index: usize) -> bool {
        matches!(self.content_items.get(index), Some(ContentItem::Details { .. }))
    }

    pub fn toggle_details_at(&mut self, index: usize) {
        if let Some(ContentItem::Details { id, .. }) = self.content_items.get(index) {
            let id = *id;
            let current = self.details_open_states.get(&id).copied().unwrap_or(false);
            self.details_open_states.insert(id, !current);
        }
    }

    pub fn is_click_on_task_checkbox(&self, index: usize, col: u16, content_x: u16) -> bool {
        let indent = match self.content_items.get(index) {
            Some(ContentItem::TaskItem { indent, .. }) => *indent,
            _ => return false,
        };
        let click_col = col.saturating_sub(content_x) as usize;
        click_col >= 2 + indent && click_col <= 4 + indent
    }

    pub fn toggle_task_at(&mut self, index: usize) {
        let saved_cursor = self.content_cursor;

        if let Some(item) = self.content_items.get(index) {
            if let ContentItem::TaskItem { line_index, checked, .. } = item {
                let line_index = *line_index;
                let new_checked = !*checked;

                if let Some(note) = self.notes.get_mut(self.selected_note) {
                    let lines: Vec<&str> = note.content.lines().collect();
                    if line_index < lines.len() {
                        let line = lines[line_index];
                        let new_line = if new_checked {
                            line.replacen("- [ ]", "- [x]", 1)
                        } else {
                            line.replacen("- [x]", "- [ ]", 1).replacen("- [X]", "- [ ]", 1)
                        };

                        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                        new_lines[line_index] = new_line;
                        note.content = new_lines.join("\n");

                        if let Some(ref path) = note.file_path {
                            let _ = fs::write(path, &note.content);
                        }
                    }
                }

                self.update_content_items();
                self.content_cursor = saved_cursor.min(self.content_items.len().saturating_sub(1));
            }
        }
    }

    #[allow(dead_code)]
    pub fn open_current_link(&mut self) {
        if let Some(url) = self.current_item_link() {
            self.open_link(&url);
        }
    }

    /// Open a link - navigates internally for .md files, opens externally otherwise
    pub fn open_link(&mut self, url: &str) {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            let (path_part, heading) = if let Some(hash_pos) = url.find('#') {
                (&url[..hash_pos], Some(&url[hash_pos + 1..]))
            } else {
                (url, None)
            };

            // Same-file anchor: [text](#section)
            if path_part.is_empty() {
                if let Some(heading_text) = heading {
                    self.navigate_to_heading(heading_text);
                }
                return;
            }

            if path_part.ends_with(".md") {
                let base_dir = self
                    .current_note()
                    .and_then(|n| n.file_path.as_ref())
                    .and_then(|p| p.parent())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| self.config.notes_path());

                let resolved = base_dir.join(path_part);
                if let Ok(canonical) = resolved.canonicalize() {
                    // Find matching note by canonical path
                    let found = self.notes.iter().enumerate().find_map(|(idx, note)| {
                        note.file_path
                            .as_ref()
                            .and_then(|fp| fp.canonicalize().ok())
                            .filter(|cp| *cp == canonical)
                            .map(|_| idx)
                    });

                    if let Some(note_idx) = found {
                        // Expand parent folders
                        if let Some(note) = self.notes.get(note_idx) {
                            if let Some(ref file_path) = note.file_path {
                                let notes_root = self.config.notes_path();
                                let mut current = file_path.parent();
                                let mut needs_rebuild = false;
                                while let Some(parent) = current {
                                    if parent == notes_root {
                                        break;
                                    }
                                    if !self.folder_states.get(&parent.to_path_buf()).copied().unwrap_or(false) {
                                        self.folder_states.insert(parent.to_path_buf(), true);
                                        needs_rebuild = true;
                                    }
                                    current = parent.parent();
                                }
                                if needs_rebuild {
                                    Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);
                                    self.rebuild_sidebar_items();
                                }
                            }
                        }

                        for (idx, item) in self.sidebar_items.iter().enumerate() {
                            if let SidebarItemKind::Note { note_index } = &item.kind {
                                if *note_index == note_idx {
                                    self.end_buffer_search();
                                    self.selected_sidebar_index = idx;
                                    self.selected_note = note_idx;
                                    self.push_navigation_history(note_idx);
                                    self.content_cursor = 0;
                                    self.content_scroll_offset = 0;
                                    self.selected_link_index = 0;
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
