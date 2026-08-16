use super::*;

impl Editor {
    pub fn set_wiki_link_styles(&mut self, valid_style: Style, invalid_style: Style) {
        self.wiki_link_valid_style = valid_style;
        self.wiki_link_invalid_style = invalid_style;
    }

    pub fn set_markdown_colors(
        &mut self,
        heading_colors: [Color; 6],
        code_color: Color,
        link_color: Color,
        blockquote_color: Color,
        list_marker_color: Color,
        bold_color: Option<Color>,
        italic_color: Option<Color>,
    ) {
        self.heading_colors = heading_colors;
        self.code_color = code_color;
        self.link_color = link_color;
        self.blockquote_color = blockquote_color;
        self.list_marker_color = list_marker_color;
        self.bold_color = bold_color;
        self.italic_color = italic_color;
    }

    pub fn set_frontmatter_color(&mut self, color: Color) {
        self.frontmatter_color = color;
    }

    pub fn update_wiki_links<F>(&mut self, validator: F)
    where
        F: Fn(&str) -> bool,
    {
        self.wiki_link_ranges.clear();
        let mut in_code_block = false;

        for (row, line) in self.buffer.lines().iter().enumerate() {
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            for link in ekphos_core::markdown::wiki_links(line) {
                let columns = link.char_range(line);
                self.wiki_link_ranges.push(WikiLinkRange {
                    row,
                    start_col: columns.start,
                    end_col: columns.end,
                    is_valid: validator(link.target),
                });
            }
        }
    }
    pub(super) fn wiki_link_style_at(&self, row: usize, col: usize) -> Option<Style> {
        for range in &self.wiki_link_ranges {
            if range.row == row && col >= range.start_col && col < range.end_col {
                return if range.is_valid {
                    Some(self.wiki_link_valid_style)
                } else {
                    Some(self.wiki_link_invalid_style)
                };
            }
        }
        None
    }

    pub fn set_wiki_link_ranges(&mut self, ranges: Vec<WikiLinkRange>) {
        self.wiki_link_ranges = ranges;
        self.row_style_cache.borrow_mut().invalidate_all();
    }

    pub fn invalidate_all_styles(&mut self) {
        self.row_style_cache.borrow_mut().invalidate_all();
    }

    // ==================== Highlight Management ====================

    pub fn add_highlight(&mut self, highlight: HighlightRange) {
        let row = highlight.row;
        self.highlight_index.insert(highlight);
        self.row_style_cache.borrow_mut().invalidate_row(row);
    }

    pub fn add_highlights(&mut self, highlights: impl IntoIterator<Item = HighlightRange>) {
        for highlight in highlights {
            let row = highlight.row;
            self.highlight_index.insert(highlight);
            self.row_style_cache.borrow_mut().invalidate_row(row);
        }
    }

    pub fn clear_highlights(&mut self) {
        self.highlight_index.clear();
        self.row_style_cache.borrow_mut().invalidate_all();
    }

    pub fn clear_highlights_of_type(&mut self, highlight_type: HighlightType) {
        self.highlight_index.retain(|h| h.highlight_type != highlight_type);
        self.row_style_cache.borrow_mut().invalidate_all();
    }

    pub fn clear_highlights_for_row(&mut self, row: usize) {
        self.highlight_index.clear_row(row);
        self.row_style_cache.borrow_mut().invalidate_row(row);
    }

    pub fn clear_highlights_for_row_and_type(&mut self, row: usize, highlight_type: HighlightType) {
        self.highlight_index.clear_row_of_type(row, highlight_type);
        self.row_style_cache.borrow_mut().invalidate_row(row);
    }

    pub(super) fn highlight_style_at(&self, row: usize, col: usize) -> Option<Style> {
        let mut best_match: Option<&HighlightRange> = None;

        // O(log n) lookup to get highlights for this row, then scan only that row's highlights
        for highlight in self.highlight_index.get_row(row) {
            if highlight.contains(row, col) {
                match best_match {
                    None => best_match = Some(highlight),
                    Some(current) if highlight.priority > current.priority => {
                        best_match = Some(highlight);
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|h| h.style)
    }

    pub fn get_row_styles_cached(&self, row: usize) -> Vec<Style> {
        {
            let cache = self.row_style_cache.borrow();
            if let Some(styles) = cache.get_row_styles(row) {
                if !cache.is_dirty(row) {
                    return styles.to_vec();
                }
            }
        }
        let styles = self.compute_row_styles_readonly(row);

        self.row_style_cache.borrow_mut().set_row_styles(row, styles.clone());

        styles
    }

    pub(super) fn compute_row_styles_readonly(&self, row: usize) -> Vec<Style> {
        let line_len = self.buffer.line_len(row);
        let mut styles = Vec::with_capacity(line_len);

        for col in 0..line_len {
            let style = self
                .highlight_style_at(row, col)
                .or_else(|| self.wiki_link_style_at(row, col))
                .unwrap_or_default();
            styles.push(style);
        }

        styles
    }
    pub fn get_row_styles(&mut self, row: usize) -> Vec<Style> {
        self.get_row_styles_cached(row)
    }
    pub fn invalidate_row_styles(&mut self, row: usize) {
        self.row_style_cache.borrow_mut().invalidate_row(row);
    }
    pub fn invalidate_styles_from(&mut self, row: usize) {
        self.row_style_cache.borrow_mut().invalidate_from(row);
    }

    #[allow(dead_code)]
    pub fn highlights_for_row(&self, row: usize) -> Vec<&HighlightRange> {
        self.highlight_index.get_row(row).iter().collect()
    }

    #[allow(dead_code)]
    pub fn highlights_of_type(&self, highlight_type: HighlightType) -> Vec<&HighlightRange> {
        self.highlight_index.iter().filter(|h| h.highlight_type == highlight_type).collect()
    }

    #[allow(dead_code)]
    pub fn has_highlights(&self) -> bool {
        !self.highlight_index.is_empty()
    }

    #[allow(dead_code)]
    pub fn highlight_count(&self) -> usize {
        self.highlight_index.len()
    }

    // ==================== Markdown Syntax Highlighting ====================

    pub fn update_markdown_highlights(&mut self) {
        self.highlight_index.retain(|h| h.highlight_type == HighlightType::WikiLink);
        self.code_block_rows.clear();
        self.row_style_cache.borrow_mut().invalidate_all();

        let line_count = self.buffer.line_count();
        let mut in_code_block = false;
        self.frontmatter_end = self.detect_frontmatter_end();

        for row in 0..line_count {
            let line = self.buffer.line(row).unwrap_or("").to_string();

            if let Some(fm_end) = self.frontmatter_end {
                if row <= fm_end {
                    self.highlight_index.insert(HighlightRange::new(
                        row,
                        0,
                        line.chars().count(),
                        Style::default().fg(self.frontmatter_color),
                        HighlightType::Frontmatter,
                    ));
                    continue;
                }
            }

            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                self.code_block_rows.insert(row);
                let byte_start = line.find("```").unwrap_or(0);
                let start = line[..byte_start].chars().count();
                self.highlight_index.insert(HighlightRange::new(
                    row,
                    start,
                    line.chars().count(),
                    Style::default().fg(self.code_color),
                    HighlightType::CodeBlock,
                ));
                continue;
            }

            if in_code_block {
                self.code_block_rows.insert(row);
                self.highlight_index.insert(HighlightRange::new(
                    row,
                    0,
                    line.chars().count(),
                    Style::default().fg(self.code_color),
                    HighlightType::CodeBlock,
                ));
                continue;
            }

            self.highlight_line_markdown(row, &line);
        }
    }

    pub fn update_row_highlights(&mut self, row: usize) {
        self.highlight_index.clear_row_of_type(row, HighlightType::Frontmatter);
        self.highlight_index.clear_row_of_type(row, HighlightType::CodeBlock);
        self.highlight_index.clear_row_of_type(row, HighlightType::Header);
        self.highlight_index.clear_row_of_type(row, HighlightType::Blockquote);
        self.highlight_index.clear_row_of_type(row, HighlightType::ListMarker);
        self.highlight_index.clear_row_of_type(row, HighlightType::InlineCode);
        self.highlight_index.clear_row_of_type(row, HighlightType::Link);
        self.highlight_index.clear_row_of_type(row, HighlightType::Bold);
        self.highlight_index.clear_row_of_type(row, HighlightType::Italic);

        self.row_style_cache.borrow_mut().invalidate_row(row);

        let line = match self.buffer.line(row) {
            Some(l) => l.to_string(),
            None => return,
        };

        if let Some(fm_end) = self.frontmatter_end {
            if row <= fm_end {
                self.highlight_index.insert(HighlightRange::new(
                    row,
                    0,
                    line.chars().count(),
                    Style::default().fg(self.frontmatter_color),
                    HighlightType::Frontmatter,
                ));
                return;
            }
        }

        let is_code_fence = line.trim_start().starts_with("```");
        let was_in_code_block = self.code_block_rows.contains(&row);

        if is_code_fence {
            self.code_block_rows.insert(row);
            let byte_start = line.find("```").unwrap_or(0);
            let start = line[..byte_start].chars().count();
            self.highlight_index.insert(HighlightRange::new(
                row,
                start,
                line.chars().count(),
                Style::default().fg(self.code_color),
                HighlightType::CodeBlock,
            ));
            if !was_in_code_block || self.is_in_code_block(row) != self.is_in_code_block(row.saturating_sub(1)) {
                self.recalc_code_blocks_from(row);
            }
            return;
        }

        if self.is_in_code_block(row) {
            self.code_block_rows.insert(row);
            self.highlight_index.insert(HighlightRange::new(
                row,
                0,
                line.chars().count(),
                Style::default().fg(self.code_color),
                HighlightType::CodeBlock,
            ));
            return;
        }

        self.code_block_rows.remove(&row);
        self.highlight_line_markdown(row, &line);
    }

    pub(super) fn is_in_code_block(&self, row: usize) -> bool {
        let mut in_block = false;
        for r in 0..=row {
            if let Some(line) = self.buffer.line(r) {
                if line.trim_start().starts_with("```") {
                    in_block = !in_block;
                }
            }
        }
        in_block
    }

    pub(super) fn recalc_code_blocks_from(&mut self, start_row: usize) {
        let line_count = self.buffer.line_count();
        let mut in_code_block = if start_row > 0 { self.is_in_code_block(start_row - 1) } else { false };

        for row in start_row..line_count {
            let line = match self.buffer.line(row) {
                Some(l) => l.to_string(),
                None => continue,
            };

            if let Some(fm_end) = self.frontmatter_end {
                if row <= fm_end {
                    continue;
                }
            }

            self.highlight_index.clear_row_of_type(row, HighlightType::CodeBlock);
            self.highlight_index.clear_row_of_type(row, HighlightType::Header);
            self.highlight_index.clear_row_of_type(row, HighlightType::Blockquote);
            self.highlight_index.clear_row_of_type(row, HighlightType::ListMarker);
            self.highlight_index.clear_row_of_type(row, HighlightType::InlineCode);
            self.highlight_index.clear_row_of_type(row, HighlightType::Link);
            self.highlight_index.clear_row_of_type(row, HighlightType::Bold);
            self.highlight_index.clear_row_of_type(row, HighlightType::Italic);
            self.row_style_cache.borrow_mut().invalidate_row(row);

            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                self.code_block_rows.insert(row);
                let byte_start = line.find("```").unwrap_or(0);
                let start = line[..byte_start].chars().count();
                self.highlight_index.insert(HighlightRange::new(
                    row,
                    start,
                    line.chars().count(),
                    Style::default().fg(self.code_color),
                    HighlightType::CodeBlock,
                ));
                continue;
            }

            if in_code_block {
                self.code_block_rows.insert(row);
                self.highlight_index.insert(HighlightRange::new(
                    row,
                    0,
                    line.chars().count(),
                    Style::default().fg(self.code_color),
                    HighlightType::CodeBlock,
                ));
            } else {
                self.code_block_rows.remove(&row);
                self.highlight_line_markdown(row, &line);
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn update_frontmatter_boundary(&mut self) {
        let old_end = self.frontmatter_end;
        self.frontmatter_end = self.detect_frontmatter_end();

        if old_end != self.frontmatter_end {
            let max_row = old_end.unwrap_or(0).max(self.frontmatter_end.unwrap_or(0));
            for row in 0..=max_row.min(self.buffer.line_count().saturating_sub(1)) {
                self.update_row_highlights(row);
            }
        }
    }

    /// detect the line index where frontmatter ends, returns None if no valid frontmatter is found.
    pub(super) fn detect_frontmatter_end(&self) -> Option<usize> {
        ekphos_core::markdown::frontmatter_end_in_lines(self.buffer.lines())
    }

    pub(super) fn highlight_line_markdown(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let line_len = chars.len();

        if line_len == 0 {
            return;
        }

        if let Some(header_end) = self.detect_header(line) {
            let level = line.chars().take_while(|&c| c == '#').count();
            let color = self.heading_colors[level.saturating_sub(1).min(5)];
            self.highlight_index.insert(HighlightRange::new(
                row,
                0,
                header_end.min(line_len),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
                HighlightType::Header,
            ));
            return;
        }

        if line.trim_start().starts_with('>') {
            let byte_start = line.find('>').unwrap_or(0);
            let start = line[..byte_start].chars().count();
            self.highlight_index.insert(HighlightRange::new(
                row,
                start,
                start + 1,
                Style::default().fg(self.blockquote_color),
                HighlightType::Blockquote,
            ));
        }

        self.highlight_list_marker(row, line);

        self.highlight_inline_code(row, line);
        self.highlight_links(row, line);
        self.highlight_bold(row, line);
        self.highlight_italic(row, line);
    }

    pub(super) fn detect_header(&self, line: &str) -> Option<usize> {
        ekphos_core::markdown::heading(line.trim_start()).map(|_| line.chars().count())
    }

    pub(super) fn highlight_list_marker(&mut self, row: usize, line: &str) {
        let trimmed = line.trim_start();
        let indent_chars = line.chars().take_while(|c| c.is_whitespace()).count();

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            self.highlight_index.insert(HighlightRange::new(
                row,
                indent_chars,
                indent_chars + 1,
                Style::default().fg(self.list_marker_color),
                HighlightType::ListMarker,
            ));

            if trimmed.len() >= 5 {
                let after_marker = &trimmed[2..];
                if after_marker.starts_with("[ ] ") || after_marker.starts_with("[x] ") || after_marker.starts_with("[X] ") {
                    self.highlight_index.insert(HighlightRange::new(
                        row,
                        indent_chars + 2,
                        indent_chars + 5,
                        Style::default().fg(self.link_color),
                        HighlightType::ListMarker,
                    ));
                }
            }
        } else if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
                self.highlight_index.insert(HighlightRange::new(
                    row,
                    indent_chars,
                    indent_chars + dot_pos + 1,
                    Style::default().fg(self.list_marker_color),
                    HighlightType::ListMarker,
                ));
            }
        }
    }

    pub(super) fn highlight_inline_code(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '`' && (i + 1 >= chars.len() || chars[i + 1] != '`') {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                    let end_pos = i + 1 + end;
                    self.highlight_index
                        .insert(HighlightRange::new(row, i, end_pos + 1, Style::default().fg(self.code_color), HighlightType::InlineCode).with_priority(2));
                    i = end_pos + 1;
                    continue;
                }
            }
            i += 1;
        }
    }

    pub(super) fn highlight_links(&mut self, row: usize, line: &str) {
        let mut cursor = 0;
        while let Some(relative_start) = line[cursor..].find('[') {
            let start = cursor + relative_start;
            let Some(link) = ekphos_core::markdown::markdown_link_at(line, start) else {
                cursor = start + 1;
                continue;
            };
            let start_col = line[..link.range.start].chars().count();
            let end_col = start_col + line[link.range.clone()].chars().count();
            self.highlight_index.insert(
                HighlightRange::new(
                    row,
                    start_col,
                    end_col,
                    Style::default().fg(self.link_color).add_modifier(Modifier::UNDERLINED),
                    HighlightType::Link,
                )
                .with_priority(1),
            );
            cursor = link.range.end;
        }
    }

    pub(super) fn highlight_bold(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len().saturating_sub(1) {
            if (chars[i] == '*' && chars[i + 1] == '*') || (chars[i] == '_' && chars[i + 1] == '_') {
                let marker = chars[i];
                let mut j = i + 2;
                while j < chars.len().saturating_sub(1) {
                    if chars[j] == marker && chars[j + 1] == marker {
                        if !self.is_position_highlighted(row, i) {
                            let mut style = Style::default().add_modifier(Modifier::BOLD);
                            if let Some(color) = self.bold_color {
                                style = style.fg(color);
                            }
                            self.highlight_index.insert(HighlightRange::new(row, i, j + 2, style, HighlightType::Bold));
                        }
                        i = j + 2;
                        break;
                    }
                    j += 1;
                }
                if j >= chars.len().saturating_sub(1) {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }

    pub(super) fn highlight_italic(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '*' || chars[i] == '_' {
                let marker = chars[i];
                if i + 1 < chars.len() && chars[i + 1] == marker {
                    i += 2;
                    continue;
                }
                if i > 0 && chars[i - 1] == marker {
                    i += 1;
                    continue;
                }

                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == marker {
                        if j + 1 < chars.len() && chars[j + 1] == marker {
                            j += 2;
                            continue;
                        }
                        if !self.is_position_highlighted(row, i) {
                            let mut style = Style::default().add_modifier(Modifier::ITALIC);
                            if let Some(color) = self.italic_color {
                                style = style.fg(color);
                            }
                            self.highlight_index.insert(HighlightRange::new(row, i, j + 1, style, HighlightType::Italic));
                        }
                        i = j + 1;
                        break;
                    }
                    j += 1;
                }
                if j >= chars.len() {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
    }

    pub(super) fn is_position_highlighted(&self, row: usize, col: usize) -> bool {
        self.highlight_index
            .get_row(row)
            .iter()
            .any(|h| col >= h.start_col && col < h.end_col && (h.highlight_type == HighlightType::InlineCode || h.highlight_type == HighlightType::Link))
    }

    pub fn clear_search_highlights(&mut self) {
        self.highlight_index
            .retain(|h| h.highlight_type != HighlightType::SearchMatch && h.highlight_type != HighlightType::SearchMatchCurrent);
        self.row_style_cache.borrow_mut().invalidate_all();
    }

    pub fn set_search_highlights(&mut self, matches: &[(usize, usize, usize)], current_idx: usize, match_color: Color, current_color: Color) {
        self.clear_search_highlights();

        for (idx, (row, start_col, end_col)) in matches.iter().enumerate() {
            let is_current = idx == current_idx;
            let (color, highlight_type) = if is_current {
                (current_color, HighlightType::SearchMatchCurrent)
            } else {
                (match_color, HighlightType::SearchMatch)
            };

            self.highlight_index.insert(HighlightRange {
                row: *row,
                start_col: *start_col,
                end_col: *end_col,
                style: Style::default().bg(color).fg(Color::Black),
                highlight_type,
                priority: 200,
            });
            self.row_style_cache.borrow_mut().invalidate_row(*row);
        }
    }
}
