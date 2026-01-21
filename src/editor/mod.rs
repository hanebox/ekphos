mod buffer;
mod cursor;
mod history;
mod input;
mod wrap;

pub use cursor::{CursorMove, Position};
pub use input::{process_key, InputAction};

// Re-export LineNumberMode for use in other modules
pub use crate::config::LineNumberMode;

use buffer::TextBuffer;
use cursor::Cursor;
use history::{EditOperation, History};
use wrap::WrapCache;

use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer as RatatuiBuffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Widget},
};
use unicode_width::UnicodeWidthChar;

#[inline]
fn char_display_width(ch: char, tab_width: u16) -> u16 {
    if ch == '\t' {
        tab_width
    } else {
        ch.width().unwrap_or(1) as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightType {
    WikiLink,
    Header,
    Bold,
    Italic,
    InlineCode,
    CodeBlock,
    Link,
    Blockquote,
    ListMarker,
    HorizontalRule,
    SearchMatch,
    SearchMatchCurrent,
    Frontmatter,
    Custom(u8),
}

#[derive(Debug, Clone)]
pub struct HighlightRange {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub style: Style,
    pub highlight_type: HighlightType,
    pub priority: u8,
}

impl HighlightRange {
    pub fn new(
        row: usize,
        start_col: usize,
        end_col: usize,
        style: Style,
        highlight_type: HighlightType,
    ) -> Self {
        Self {
            row,
            start_col,
            end_col,
            style,
            highlight_type,
            priority: 0,
        }
    }

    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.row == row && col >= self.start_col && col < self.end_col
    }
}

#[derive(Debug, Clone)]
pub struct WikiLinkRange {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub is_valid: bool,
}

#[derive(Debug, Clone)]
enum ListPrefix {
    Unordered { indent: String, marker: char },
    Task { indent: String, marker: char },
    Ordered { indent: String, number: usize },
}

impl ListPrefix {
    fn detect(line: &str) -> Option<Self> {
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            let marker = trimmed.chars().next().unwrap();

            if trimmed.len() >= 5 {
                let after_marker = &trimmed[2..];
                if after_marker.starts_with("[ ] ")
                    || after_marker.starts_with("[x] ")
                    || after_marker.starts_with("[X] ")
                {
                    return Some(ListPrefix::Task { indent, marker });
                }
            }

            return Some(ListPrefix::Unordered { indent, marker });
        }

        // Check for ordered lists (1. 2. etc.)
        if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(number) = num_part.parse::<usize>() {
                    return Some(ListPrefix::Ordered { indent, number });
                }
            }
        }

        None
    }

    fn next_prefix(&self) -> String {
        match self {
            ListPrefix::Unordered { indent, marker } => {
                format!("{}{} ", indent, marker)
            }
            ListPrefix::Task { indent, marker } => {
                format!("{}{} [ ] ", indent, marker)
            }
            ListPrefix::Ordered { indent, number } => {
                format!("{}{}. ", indent, number + 1)
            }
        }
    }

    fn prefix_len(&self, line: &str) -> usize {
        let trimmed = line.trim_start();
        let indent_len = line.chars().count() - trimmed.chars().count();

        match self {
            ListPrefix::Unordered { .. } => indent_len + 2, // "- " or "* " or "+ "
            ListPrefix::Task { .. } => indent_len + 6,      // "- [ ] "
            ListPrefix::Ordered { number, .. } => {
                indent_len + number.to_string().len() + 2 // "N. "
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Bar,
    Underline,
}

pub struct Editor {
    buffer: TextBuffer,
    cursor: Cursor,
    history: History,
    wrap_cache: WrapCache,
    scroll_offset: usize,
    h_scroll_offset: usize,
    view_height: usize,
    view_width: usize,
    line_wrap_enabled: bool,
    tab_width: u16,
    left_padding: u16,
    right_padding: u16,
    block: Option<Block<'static>>,
    cursor_line_style: Style,
    selection_style: Style,
    clipboard: Option<String>,
    clipboard_linewise: bool,
    // General highlighting system
    highlights: Vec<HighlightRange>,
    // Wiki link highlighting (legacy, kept for compatibility)
    wiki_link_ranges: Vec<WikiLinkRange>,
    wiki_link_valid_style: Style,
    wiki_link_invalid_style: Style,
    visual_line_selection: Option<(usize, usize)>,
    visual_block_selection: Option<(Position, Position)>,
    // Markdown highlighting colors
    heading_colors: [Color; 6],
    code_color: Color,
    link_color: Color,
    blockquote_color: Color,
    list_marker_color: Color,
    bold_color: Option<Color>,
    italic_color: Option<Color>,
    frontmatter_color: Color,
    // Line number display
    line_number_mode: LineNumberMode,
    line_number_style: Style,
    line_number_width: u16,
    // scrolloff, minimum lines above/below cursor
    scrolloff: usize,
    // Cursor shape for visual mode feedback
    cursor_shape: CursorShape,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(vec![String::new()])
    }
}

impl Editor {
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            buffer: TextBuffer::from_lines(lines),
            cursor: Cursor::new(),
            history: History::new(),
            wrap_cache: WrapCache::new(),
            scroll_offset: 0,
            h_scroll_offset: 0,
            view_height: 0,
            view_width: 0,
            line_wrap_enabled: true,
            tab_width: 4,
            left_padding: 0,
            right_padding: 1,
            block: None,
            cursor_line_style: Style::default(),
            selection_style: Style::default().bg(ratatui::style::Color::DarkGray),
            clipboard: None,
            clipboard_linewise: false,
            highlights: Vec::new(),
            wiki_link_ranges: Vec::new(),
            wiki_link_valid_style: Style::default().fg(Color::Cyan),
            wiki_link_invalid_style: Style::default().fg(Color::Red),
            visual_line_selection: None,
            visual_block_selection: None,
            heading_colors: [Color::Blue, Color::Green, Color::Yellow, Color::Magenta, Color::Cyan, Color::Gray],
            code_color: Color::Green,
            link_color: Color::Cyan,
            blockquote_color: Color::Cyan,
            list_marker_color: Color::Yellow,
            bold_color: None,
            italic_color: None,
            frontmatter_color: Color::DarkGray,
            line_number_mode: LineNumberMode::Absolute,
            line_number_style: Style::default().fg(Color::DarkGray),
            line_number_width: 4, // Default width for line numbers
            scrolloff: 0,
            cursor_shape: CursorShape::Block,
        }
    }

    pub fn set_line_number_mode(&mut self, mode: LineNumberMode) {
        self.line_number_mode = mode;
        // Update width based on line count
        self.update_line_number_width();
    }

    fn update_line_number_width(&mut self) {
        if self.line_number_mode == LineNumberMode::None {
            self.line_number_width = 0;
        } else {
            let line_count = self.buffer.line_count();
            self.line_number_width = (line_count.to_string().len() as u16).max(2) + 1; // +1 for spacing
        }
    }

    fn get_line_number_str(&self, row: usize, cursor_row: usize) -> Option<String> {
        match self.line_number_mode {
            LineNumberMode::None => None,
            LineNumberMode::Absolute => Some(format!("{:>width$}", row + 1, width = (self.line_number_width - 1) as usize)),
            LineNumberMode::Relative => {
                let rel = (row as isize - cursor_row as isize).unsigned_abs();
                Some(format!("{:>width$}", rel, width = (self.line_number_width - 1) as usize))
            }
            LineNumberMode::Hybrid => {
                if row == cursor_row {
                    Some(format!("{:>width$}", row + 1, width = (self.line_number_width - 1) as usize))
                } else {
                    let rel = (row as isize - cursor_row as isize).unsigned_abs();
                    Some(format!("{:>width$}", rel, width = (self.line_number_width - 1) as usize))
                }
            }
        }
    }

    pub fn from_str(text: &str) -> Self {
        let lines: Vec<String> = text.lines().map(String::from).collect();
        if lines.is_empty() {
            Self::default()
        } else {
            Self::new(lines)
        }
    }

    // Line wrap
    pub fn set_line_wrap(&mut self, enabled: bool) {
        self.line_wrap_enabled = enabled;
        if enabled {
            self.h_scroll_offset = 0;
        }
    }

    pub fn set_tab_width(&mut self, width: u16) {
        self.tab_width = width.max(1);
    }

    pub fn set_padding(&mut self, left: u16, right: u16) {
        self.left_padding = left;
        self.right_padding = right;
    }

    pub fn set_scrolloff(&mut self, scrolloff: usize) {
        self.scrolloff = scrolloff;
    }

    pub fn line_wrap_enabled(&self) -> bool {
        self.line_wrap_enabled
    }

    // Styling
    pub fn set_block(&mut self, block: Block<'static>) {
        self.block = Some(block);
    }

    pub fn set_cursor_line_style(&mut self, style: Style) {
        self.cursor_line_style = style;
    }

    pub fn set_selection_style(&mut self, style: Style) {
        self.selection_style = style;
    }

    pub fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.cursor_shape = shape;
    }

    pub fn set_visual_line_selection(&mut self, anchor_row: usize, current_row: usize) {
        self.visual_line_selection = Some((anchor_row, current_row));
    }

    pub fn clear_visual_line_selection(&mut self) {
        self.visual_line_selection = None;
    }

    pub fn set_visual_block_selection(&mut self, anchor: Position, current: Position) {
        self.visual_block_selection = Some((anchor, current));
    }

    pub fn clear_visual_block_selection(&mut self) {
        self.visual_block_selection = None;
    }

    pub fn visual_line_selected_text(&self) -> Option<String> {
        let (anchor_row, current_row) = self.visual_line_selection?;
        let (start_row, end_row) = if anchor_row <= current_row {
            (anchor_row, current_row)
        } else {
            (current_row, anchor_row)
        };

        let mut result = String::new();
        for row in start_row..=end_row {
            if let Some(line) = self.buffer.line(row) {
                result.push_str(line);
                result.push('\n');
            }
        }
        Some(result)
    }

    pub fn copy_visual_lines(&mut self) {
        if let Some(text) = self.visual_line_selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = true;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&text);
            }
        }
    }

    pub fn cut_visual_lines(&mut self) {
        if let Some((anchor_row, current_row)) = self.visual_line_selection {
            let (start_row, end_row) = if anchor_row <= current_row {
                (anchor_row, current_row)
            } else {
                (current_row, anchor_row)
            };

            // Collect lines for undo and clipboard
            let mut deleted_lines: Vec<String> = Vec::new();
            for row in start_row..=end_row {
                if let Some(line) = self.buffer.line(row) {
                    deleted_lines.push(line.to_string());
                }
            }

            // Set clipboard (with newlines for vim compatibility)
            let clipboard_text = deleted_lines.join("\n") + "\n";
            self.clipboard = Some(clipboard_text.clone());
            self.clipboard_linewise = true;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&clipboard_text);
            }

            let cursor_before = self.cursor.pos();

            // Delete lines from end to start to preserve row indices
            for row in (start_row..=end_row).rev() {
                self.buffer.delete_line(row);
                self.wrap_cache.remove_line(row);
            }

            // Move cursor to start of deleted region
            let new_row = start_row.min(self.buffer.line_count().saturating_sub(1));
            self.cursor.move_to(new_row, 0);
            self.cursor.cancel_selection();

            self.history.record(
                EditOperation::LineDelete {
                    row: start_row,
                    lines: deleted_lines,
                },
                cursor_before,
                Position { row: new_row, col: 0 },
            );
        }
    }

    pub fn visual_block_selected_text(&self) -> Option<String> {
        let (anchor, current) = self.visual_block_selection?;
        let (start_row, end_row) = if anchor.row <= current.row {
            (anchor.row, current.row)
        } else {
            (current.row, anchor.row)
        };
        let (start_col, end_col) = if anchor.col <= current.col {
            (anchor.col, current.col)
        } else {
            (current.col, anchor.col)
        };

        let mut result = Vec::new();
        for row in start_row..=end_row {
            if let Some(line) = self.buffer.line(row) {
                let chars: Vec<char> = line.chars().collect();
                let line_len = chars.len();
                // Extract only the columns within the block
                let actual_start = start_col.min(line_len);
                let actual_end = (end_col + 1).min(line_len);
                if actual_start < actual_end {
                    let block_text: String = chars[actual_start..actual_end].iter().collect();
                    result.push(block_text);
                } else {
                    result.push(String::new());
                }
            }
        }
        Some(result.join("\n"))
    }

    pub fn copy_visual_block(&mut self) {
        if let Some(text) = self.visual_block_selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = false;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&text);
            }
        }
    }

    pub fn cut_visual_block(&mut self) {
        if let Some((anchor, current)) = self.visual_block_selection {
            let (start_row, end_row) = if anchor.row <= current.row {
                (anchor.row, current.row)
            } else {
                (current.row, anchor.row)
            };
            let (start_col, end_col) = if anchor.col <= current.col {
                (anchor.col, current.col)
            } else {
                (current.col, anchor.col)
            };

            // Collect deleted text for each line (for undo)
            let mut deleted_lines = Vec::new();
            for row in start_row..=end_row {
                if let Some(line) = self.buffer.line(row) {
                    let chars: Vec<char> = line.chars().collect();
                    let line_len = chars.len();
                    let actual_start = start_col.min(line_len);
                    let actual_end = (end_col + 1).min(line_len);
                    if actual_start < actual_end {
                        let block_text: String = chars[actual_start..actual_end].iter().collect();
                        deleted_lines.push(block_text);
                    } else {
                        deleted_lines.push(String::new());
                    }
                }
            }

            // Get the text for clipboard (newline-separated)
            let clipboard_text = deleted_lines.join("\n");
            self.clipboard = Some(clipboard_text.clone());
            self.clipboard_linewise = false;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&clipboard_text);
            }

            let cursor_before = self.cursor.pos();

            // Delete block from each line (process from end to preserve indices)
            for row in (start_row..=end_row).rev() {
                if let Some(line) = self.buffer.line(row) {
                    let chars: Vec<char> = line.chars().collect();
                    let line_len = chars.len();
                    let actual_start = start_col.min(line_len);
                    let actual_end = (end_col + 1).min(line_len);
                    if actual_start < actual_end {
                        // Build new line without the block
                        let new_line: String = chars[..actual_start]
                            .iter()
                            .chain(chars[actual_end..].iter())
                            .collect();
                        if let Some(line_ref) = self.buffer.line_mut(row) {
                            *line_ref = new_line;
                        }
                    }
                }
            }

            // Invalidate wrap cache
            self.wrap_cache.invalidate_from(start_row);

            // Move cursor to start of deleted region
            self.cursor.move_to(start_row, start_col);
            self.cursor.cancel_selection();

            // Record in history using BlockDelete for proper undo
            self.history.record(
                EditOperation::BlockDelete {
                    start_row,
                    end_row,
                    start_col,
                    end_col,
                    deleted_lines,
                },
                cursor_before,
                Position { row: start_row, col: start_col },
            );
        }
    }

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

            let mut search_start = 0;

            while search_start < line.len() {
                let remaining = &line[search_start..];
                if let Some(backtick_pos) = remaining.find('`') {
                    let wiki_pos = remaining.find("[[");

                    if wiki_pos.is_none() || backtick_pos < wiki_pos.unwrap() {
                        let abs_backtick = search_start + backtick_pos;
                        let after_backtick = &line[abs_backtick + 1..];

                        if let Some(close_backtick) = after_backtick.find('`') {
                            search_start = abs_backtick + 1 + close_backtick + 1;
                            continue;
                        } else {
                            break;
                        }
                    }
                }

                if let Some(start_pos) = remaining.find("[[") {
                    let abs_start = search_start + start_pos;
                    let after_brackets = &line[abs_start + 2..];

                    if let Some(end_pos) = after_brackets.find("]]") {
                        let raw_content = &after_brackets[..end_pos];

                        if !raw_content.is_empty() && !raw_content.contains('[') && !raw_content.contains(']') {
                            // Parse: [[target#heading|display]] - extract target for validation
                            let content = if let Some(pipe_pos) = raw_content.find('|') {
                                &raw_content[..pipe_pos]
                            } else {
                                raw_content
                            };
                            let target = if let Some(hash_pos) = content.find('#') {
                                &content[..hash_pos]
                            } else {
                                content
                            };

                            let is_valid = validator(target);

                            let start_col = line[..abs_start].chars().count();
                            let end_col = start_col + 2 + raw_content.chars().count() + 2; // [[content]]

                            self.wiki_link_ranges.push(WikiLinkRange {
                                row,
                                start_col,
                                end_col,
                                is_valid,
                            });
                        }

                        search_start = abs_start + 2 + end_pos + 2;
                        continue;
                    }
                }
                break;
            }
        }
    }

    fn wiki_link_style_at(&self, row: usize, col: usize) -> Option<Style> {
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

    // ==================== Highlight Management ====================

    pub fn add_highlight(&mut self, highlight: HighlightRange) {
        self.highlights.push(highlight);
    }

    pub fn add_highlights(&mut self, highlights: impl IntoIterator<Item = HighlightRange>) {
        self.highlights.extend(highlights);
    }

    pub fn clear_highlights(&mut self) {
        self.highlights.clear();
    }
    pub fn clear_highlights_of_type(&mut self, highlight_type: HighlightType) {
        self.highlights.retain(|h| h.highlight_type != highlight_type);
    }

    pub fn clear_highlights_for_row(&mut self, row: usize) {
        self.highlights.retain(|h| h.row != row);
    }

    pub fn clear_highlights_for_row_and_type(&mut self, row: usize, highlight_type: HighlightType) {
        self.highlights.retain(|h| h.row != row || h.highlight_type != highlight_type);
    }

    fn highlight_style_at(&self, row: usize, col: usize) -> Option<Style> {
        let mut best_match: Option<&HighlightRange> = None;

        for highlight in &self.highlights {
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

    #[allow(dead_code)]
    pub fn highlights_for_row(&self, row: usize) -> Vec<&HighlightRange> {
        self.highlights.iter().filter(|h| h.row == row).collect()
    }

    #[allow(dead_code)]
    pub fn highlights_of_type(&self, highlight_type: HighlightType) -> Vec<&HighlightRange> {
        self.highlights.iter().filter(|h| h.highlight_type == highlight_type).collect()
    }

    #[allow(dead_code)]
    pub fn has_highlights(&self) -> bool {
        !self.highlights.is_empty()
    }

    #[allow(dead_code)]
    pub fn highlight_count(&self) -> usize {
        self.highlights.len()
    }

    // ==================== Markdown Syntax Highlighting ====================

    pub fn update_markdown_highlights(&mut self) {
        self.highlights.retain(|h| h.highlight_type == HighlightType::WikiLink);

        let line_count = self.buffer.line_count();
        let mut in_code_block = false;
        let frontmatter_end = self.detect_frontmatter_end();

        for row in 0..line_count {
            let line = self.buffer.line(row).unwrap_or("").to_string();

            if let Some(fm_end) = frontmatter_end {
                if row <= fm_end {
                    self.highlights.push(HighlightRange::new(
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
                let start = line.find("```").unwrap_or(0);
                self.highlights.push(HighlightRange::new(
                    row,
                    start,
                    line.chars().count(),
                    Style::default().fg(self.code_color),
                    HighlightType::CodeBlock,
                ));
                continue;
            }

            if in_code_block {
                self.highlights.push(HighlightRange::new(
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

    /// detect the line index where frontmatter ends, returns None if no valid frontmatter is found.
    fn detect_frontmatter_end(&self) -> Option<usize> {
        let line_count = self.buffer.line_count();
        if line_count == 0 {
            return None;
        }

        let first_line = self.buffer.line(0).unwrap_or("");
        if first_line.trim() != "---" {
            return None;
        }
        for row in 1..line_count {
            let line = self.buffer.line(row).unwrap_or("");
            if line.trim() == "---" {
                return Some(row);
            }
        }

        None
    }

    fn highlight_line_markdown(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let line_len = chars.len();

        if line_len == 0 {
            return;
        }

        if let Some(header_end) = self.detect_header(line) {
            let level = line.chars().take_while(|&c| c == '#').count();
            let color = self.heading_colors[level.saturating_sub(1).min(5)];
            self.highlights.push(HighlightRange::new(
                row,
                0,
                header_end.min(line_len),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
                HighlightType::Header,
            ));
            return;
        }

        if line.trim_start().starts_with('>') {
            let start = line.find('>').unwrap_or(0);
            self.highlights.push(HighlightRange::new(
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

    fn detect_header(&self, line: &str) -> Option<usize> {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
            if hash_count <= 6 {
                let after_hashes = &trimmed[hash_count..];
                if after_hashes.is_empty() || after_hashes.starts_with(' ') {
                    return Some(line.chars().count());
                }
            }
        }
        None
    }

    fn highlight_list_marker(&mut self, row: usize, line: &str) {
        let trimmed = line.trim_start();
        let indent_chars = line.chars().take_while(|c| c.is_whitespace()).count();

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            self.highlights.push(HighlightRange::new(
                row,
                indent_chars,
                indent_chars + 1,
                Style::default().fg(self.list_marker_color),
                HighlightType::ListMarker,
            ));

            if trimmed.len() >= 5 {
                let after_marker = &trimmed[2..];
                if after_marker.starts_with("[ ] ") || after_marker.starts_with("[x] ") || after_marker.starts_with("[X] ") {
                    self.highlights.push(HighlightRange::new(
                        row,
                        indent_chars + 2,
                        indent_chars + 5,
                        Style::default().fg(self.link_color),
                        HighlightType::ListMarker,
                    ));
                }
            }
        }
        else if let Some(dot_pos) = trimmed.find(". ") {
            let num_part = &trimmed[..dot_pos];
            if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
                self.highlights.push(HighlightRange::new(
                    row,
                    indent_chars,
                    indent_chars + dot_pos + 1,
                    Style::default().fg(self.list_marker_color),
                    HighlightType::ListMarker,
                ));
            }
        }
    }

    fn highlight_inline_code(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '`' && (i + 1 >= chars.len() || chars[i + 1] != '`') {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                    let end_pos = i + 1 + end;
                    self.highlights.push(HighlightRange::new(
                        row,
                        i,
                        end_pos + 1,
                        Style::default().fg(self.code_color),
                        HighlightType::InlineCode,
                    ).with_priority(2));
                    i = end_pos + 1;
                    continue;
                }
            }
            i += 1;
        }
    }

    fn highlight_links(&mut self, row: usize, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '[' {
                if let Some(bracket_end) = chars[i + 1..].iter().position(|&c| c == ']') {
                    let bracket_end_pos = i + 1 + bracket_end;
                    if bracket_end_pos + 1 < chars.len() && chars[bracket_end_pos + 1] == '(' {
                        if let Some(paren_end) = chars[bracket_end_pos + 2..].iter().position(|&c| c == ')') {
                            let paren_end_pos = bracket_end_pos + 2 + paren_end;
                            self.highlights.push(HighlightRange::new(
                                row,
                                i,
                                paren_end_pos + 1,
                                Style::default().fg(self.link_color).add_modifier(Modifier::UNDERLINED),
                                HighlightType::Link,
                            ).with_priority(1));
                            i = paren_end_pos + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn highlight_bold(&mut self, row: usize, line: &str) {
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
                            self.highlights.push(HighlightRange::new(
                                row,
                                i,
                                j + 2,
                                style,
                                HighlightType::Bold,
                            ));
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

    fn highlight_italic(&mut self, row: usize, line: &str) {
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
                            self.highlights.push(HighlightRange::new(
                                row,
                                i,
                                j + 1,
                                style,
                                HighlightType::Italic,
                            ));
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

    fn is_position_highlighted(&self, row: usize, col: usize) -> bool {
        self.highlights.iter().any(|h| {
            h.row == row && col >= h.start_col && col < h.end_col &&
            (h.highlight_type == HighlightType::InlineCode || h.highlight_type == HighlightType::Link)
        })
    }

    pub fn clear_search_highlights(&mut self) {
        self.highlights.retain(|h| {
            h.highlight_type != HighlightType::SearchMatch &&
            h.highlight_type != HighlightType::SearchMatchCurrent
        });
    }

    pub fn set_search_highlights(
        &mut self,
        matches: &[(usize, usize, usize)],
        current_idx: usize,
        match_color: Color,
        current_color: Color,
    ) {
        self.clear_search_highlights();

        for (idx, (row, start_col, end_col)) in matches.iter().enumerate() {
            let is_current = idx == current_idx;
            let (color, highlight_type) = if is_current {
                (current_color, HighlightType::SearchMatchCurrent)
            } else {
                (match_color, HighlightType::SearchMatch)
            };

            self.highlights.push(HighlightRange {
                row: *row,
                start_col: *start_col,
                end_col: *end_col,
                style: Style::default().bg(color).fg(Color::Black),
                highlight_type,
                priority: 200, 
            });
        }
    }

    // Cursor
    pub fn cursor(&self) -> (usize, usize) {
        let pos = self.cursor.pos();
        (pos.row, pos.col)
    }

    pub fn set_cursor(&mut self, row: usize, col: usize) {
        let line_count = self.buffer.line_count();
        let safe_row = row.min(line_count.saturating_sub(1));
        let line_len = self.buffer.line_len(safe_row);
        let safe_col = col.min(line_len);
        self.cursor.move_to(safe_row, safe_col);
        self.ensure_cursor_visible();
    }

    pub fn set_cursor_no_scroll(&mut self, row: usize, col: usize) {
        let line_count = self.buffer.line_count();
        let safe_row = row.min(line_count.saturating_sub(1));
        let line_len = self.buffer.line_len(safe_row);
        let safe_col = col.min(line_len);
        self.cursor.move_to(safe_row, safe_col);
    }

    pub fn move_cursor(&mut self, movement: CursorMove) {
        let pos = self.cursor.pos();
        let line_count = self.buffer.line_count();

        match movement {
            CursorMove::Forward => {
                let line_len = self.buffer.line_len(pos.row);
                if pos.col < line_len {
                    self.cursor.move_to(pos.row, pos.col + 1);
                } else if pos.row + 1 < line_count {
                    self.cursor.move_to(pos.row + 1, 0);
                }
            }
            CursorMove::Back => {
                if pos.col > 0 {
                    self.cursor.move_to(pos.row, pos.col - 1);
                } else if pos.row > 0 {
                    let prev_len = self.buffer.line_len(pos.row - 1);
                    self.cursor.move_to(pos.row - 1, prev_len);
                }
            }
            CursorMove::Up => {
                if self.line_wrap_enabled && self.view_width > 0 {
                    let content_width = self.view_width.saturating_sub(self.right_padding as usize);
                    if content_width > 0 {
                        let visual_line_in_row = pos.col / content_width;
                        let col_in_visual_line = pos.col % content_width;
                        let preferred_visual_col = self.cursor.preferred_col
                            .map(|p| p % content_width)
                            .unwrap_or(col_in_visual_line);

                        if visual_line_in_row > 0 {
                            let new_col = (visual_line_in_row - 1) * content_width + preferred_visual_col;
                            let line_len = self.buffer.line_len(pos.row);
                            self.cursor.set_pos(Position::new(pos.row, new_col.min(line_len)), false);
                        } else if pos.row > 0 {
                            let prev_len = self.buffer.line_len(pos.row - 1);
                            let prev_visual_lines = if prev_len == 0 { 1 } else { (prev_len + content_width - 1) / content_width };
                            let last_visual_line = prev_visual_lines - 1;
                            let new_col = last_visual_line * content_width + preferred_visual_col;
                            self.cursor.set_pos(Position::new(pos.row - 1, new_col.min(prev_len)), false);
                        }
                    } else if pos.row > 0 {
                        let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                        let prev_len = self.buffer.line_len(pos.row - 1);
                        self.cursor.set_pos(Position::new(pos.row - 1, preferred.min(prev_len)), false);
                    }
                } else if pos.row > 0 {
                    let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                    let prev_len = self.buffer.line_len(pos.row - 1);
                    self.cursor.set_pos(Position::new(pos.row - 1, preferred.min(prev_len)), false);
                }
            }
            CursorMove::Down => {
                if self.line_wrap_enabled && self.view_width > 0 {
                    let content_width = self.view_width.saturating_sub(self.right_padding as usize);
                    if content_width > 0 {
                        let line_len = self.buffer.line_len(pos.row);
                        let total_visual_lines = if line_len == 0 { 1 } else { (line_len + content_width - 1) / content_width };
                        let visual_line_in_row = pos.col / content_width;
                        let col_in_visual_line = pos.col % content_width;
                        let preferred_visual_col = self.cursor.preferred_col
                            .map(|p| p % content_width)
                            .unwrap_or(col_in_visual_line);

                        if visual_line_in_row + 1 < total_visual_lines {
                            let new_col = (visual_line_in_row + 1) * content_width + preferred_visual_col;
                            self.cursor.set_pos(Position::new(pos.row, new_col.min(line_len)), false);
                        } else if pos.row + 1 < line_count {
                            let next_len = self.buffer.line_len(pos.row + 1);
                            self.cursor.set_pos(Position::new(pos.row + 1, preferred_visual_col.min(next_len)), false);
                        }
                    } else if pos.row + 1 < line_count {
                        let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                        let next_len = self.buffer.line_len(pos.row + 1);
                        self.cursor.set_pos(Position::new(pos.row + 1, preferred.min(next_len)), false);
                    }
                } else if pos.row + 1 < line_count {
                    let preferred = self.cursor.preferred_col.unwrap_or(pos.col);
                    let next_len = self.buffer.line_len(pos.row + 1);
                    self.cursor.set_pos(Position::new(pos.row + 1, preferred.min(next_len)), false);
                }
            }
            CursorMove::Head => self.cursor.move_to(pos.row, 0),
            CursorMove::End => self.cursor.move_to(pos.row, self.buffer.line_len(pos.row)),
            CursorMove::Top => self.cursor.move_to(0, 0),
            CursorMove::Bottom => {
                let last_row = line_count.saturating_sub(1);
                self.cursor.move_to(last_row, self.buffer.line_len(last_row));
            }
            CursorMove::WordForward => self.move_word_forward(),
            CursorMove::WordBack => self.move_word_back(),
            CursorMove::FirstNonBlank => {
                if let Some(line) = self.buffer.line(pos.row) {
                    let col = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                    self.cursor.move_to(pos.row, col);
                }
            }
            CursorMove::WordEndForward => self.move_word_end_forward(),
            CursorMove::BigWordForward => self.move_big_word_forward(),
            CursorMove::BigWordBack => self.move_big_word_back(),
            CursorMove::BigWordEndForward => self.move_big_word_end_forward(),
            CursorMove::WordEndBackward => self.move_word_end_backward(),
            CursorMove::BigWordEndBackward => self.move_big_word_end_backward(),
            CursorMove::ParagraphForward => {
                let mut row = pos.row;
                while row < line_count && !self.buffer.line(row).map_or(true, |l| l.trim().is_empty()) {
                    row += 1;
                }
                while row < line_count && self.buffer.line(row).map_or(false, |l| l.trim().is_empty()) {
                    row += 1;
                }
                self.cursor.move_to(row.min(line_count.saturating_sub(1)), 0);
            }
            CursorMove::ParagraphBack => {
                let mut row = pos.row;
                if row > 0 { row -= 1; }
                while row > 0 && self.buffer.line(row).map_or(false, |l| l.trim().is_empty()) {
                    row -= 1;
                }
                while row > 0 && !self.buffer.line(row - 1).map_or(true, |l| l.trim().is_empty()) {
                    row -= 1;
                }
                self.cursor.move_to(row, 0);
            }
            CursorMove::ScreenTop => {
                let row = self.scroll_offset;
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::ScreenMiddle => {
                let row = (self.scroll_offset + self.view_height / 2).min(line_count.saturating_sub(1));
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::ScreenBottom => {
                let row = (self.scroll_offset + self.view_height.saturating_sub(1)).min(line_count.saturating_sub(1));
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::HalfPageUp => {
                let half = self.view_height / 2;
                let new_row = pos.row.saturating_sub(half);
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                self.scroll_offset = self.scroll_offset.saturating_sub(half);
            }
            CursorMove::HalfPageDown => {
                let half = self.view_height / 2;
                let new_row = (pos.row + half).min(line_count.saturating_sub(1));
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                if self.scroll_offset + half < line_count.saturating_sub(self.view_height) {
                    self.scroll_offset += half;
                }
            }
            CursorMove::PageUp => {
                let page = self.view_height.saturating_sub(2);
                let new_row = pos.row.saturating_sub(page);
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                self.scroll_offset = self.scroll_offset.saturating_sub(page);
            }
            CursorMove::PageDown => {
                let page = self.view_height.saturating_sub(2);
                let new_row = (pos.row + page).min(line_count.saturating_sub(1));
                let line_len = self.buffer.line_len(new_row);
                self.cursor.move_to(new_row, pos.col.min(line_len));
                let max_scroll = line_count.saturating_sub(self.view_height);
                self.scroll_offset = (self.scroll_offset + page).min(max_scroll);
            }
            CursorMove::MatchingBracket => {
                if let Some(new_pos) = self.find_matching_bracket() {
                    self.cursor.move_to(new_pos.row, new_pos.col);
                }
            }
            CursorMove::GoToLine(line) => {
                let row = line.saturating_sub(1).min(line_count.saturating_sub(1));
                let col = self.buffer.line(row).map(|l| l.chars().position(|c| !c.is_whitespace()).unwrap_or(0)).unwrap_or(0);
                self.cursor.move_to(row, col);
            }
            CursorMove::GoToColumn(col) => {
                let line_len = self.buffer.line_len(pos.row);
                self.cursor.move_to(pos.row, col.saturating_sub(1).min(line_len));
            }
        }
        self.ensure_cursor_visible();
    }

    fn move_word_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else { return };

        let new_col = cursor::find_word_forward(line, pos.col);
        let line_len = line.chars().count();

        if new_col >= line_len && pos.row + 1 < self.buffer.line_count() {
            self.cursor.move_to(pos.row + 1, 0);
            if let Some(next_line) = self.buffer.line(pos.row + 1) {
                let skip = next_line.chars().take_while(|c| c.is_whitespace()).count();
                self.cursor.move_to(pos.row + 1, skip);
            }
        } else {
            self.cursor.move_to(pos.row, new_col.min(line_len));
        }
    }

    fn move_word_back(&mut self) {
        let pos = self.cursor.pos();

        if pos.col == 0 && pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            self.cursor.move_to(pos.row - 1, prev_len);
            return;
        }

        if let Some(line) = self.buffer.line(pos.row) {
            self.cursor.move_to(pos.row, cursor::find_word_back(line, pos.col));
        }
    }

    fn move_word_end_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else { return };
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();

        if len == 0 || pos.col >= len.saturating_sub(1) {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_word_end_forward();
            }
            return;
        }

        let mut col = pos.col + 1;
        while col < len && chars[col].is_whitespace() { col += 1; }
        if col >= len {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_word_end_forward();
            }
            return;
        }
        let is_word = cursor::is_word_char(chars[col]);
        while col < len.saturating_sub(1) {
            let next_is_word = cursor::is_word_char(chars[col + 1]);
            if chars[col + 1].is_whitespace() || next_is_word != is_word { break; }
            col += 1;
        }
        self.cursor.move_to(pos.row, col);
    }

    fn move_word_end_backward(&mut self) {
        let pos = self.cursor.pos();
        if pos.col == 0 {
            if pos.row > 0 {
                let prev_len = self.buffer.line_len(pos.row - 1);
                self.cursor.move_to(pos.row - 1, prev_len.saturating_sub(1));
            }
            return;
        }
        let Some(line) = self.buffer.line(pos.row) else { return };
        let chars: Vec<char> = line.chars().collect();
        let mut col = pos.col.saturating_sub(1);
        while col > 0 && chars[col].is_whitespace() { col -= 1; }
        let is_word = cursor::is_word_char(chars[col]);
        while col > 0 && cursor::is_word_char(chars[col - 1]) == is_word && !chars[col - 1].is_whitespace() {
            col -= 1;
        }
        self.cursor.move_to(pos.row, col);
    }

    fn move_big_word_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else { return };
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut col = pos.col;
        while col < len && !chars[col].is_whitespace() { col += 1; }
        while col < len && chars[col].is_whitespace() { col += 1; }
        if col >= len && pos.row + 1 < self.buffer.line_count() {
            self.cursor.move_to(pos.row + 1, 0);
            if let Some(next) = self.buffer.line(pos.row + 1) {
                let skip = next.chars().take_while(|c| c.is_whitespace()).count();
                self.cursor.move_to(pos.row + 1, skip);
            }
        } else {
            self.cursor.move_to(pos.row, col.min(len));
        }
    }

    fn move_big_word_back(&mut self) {
        let pos = self.cursor.pos();
        if pos.col == 0 && pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            self.cursor.move_to(pos.row - 1, prev_len);
            self.move_big_word_back();
            return;
        }
        let Some(line) = self.buffer.line(pos.row) else { return };
        let chars: Vec<char> = line.chars().collect();
        let mut col = pos.col.saturating_sub(1);
        while col > 0 && chars[col].is_whitespace() { col -= 1; }
        while col > 0 && !chars[col - 1].is_whitespace() { col -= 1; }
        self.cursor.move_to(pos.row, col);
    }

    fn move_big_word_end_forward(&mut self) {
        let pos = self.cursor.pos();
        let Some(line) = self.buffer.line(pos.row) else { return };
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        if len == 0 || pos.col >= len.saturating_sub(1) {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_big_word_end_forward();
            }
            return;
        }
        let mut col = pos.col + 1;
        while col < len && chars[col].is_whitespace() { col += 1; }
        if col >= len {
            if pos.row + 1 < self.buffer.line_count() {
                self.cursor.move_to(pos.row + 1, 0);
                self.move_big_word_end_forward();
            }
            return;
        }
        while col < len.saturating_sub(1) && !chars[col + 1].is_whitespace() { col += 1; }
        self.cursor.move_to(pos.row, col);
    }

    fn move_big_word_end_backward(&mut self) {
        let pos = self.cursor.pos();
        if pos.col == 0 {
            if pos.row > 0 {
                let prev_len = self.buffer.line_len(pos.row - 1);
                self.cursor.move_to(pos.row - 1, prev_len.saturating_sub(1));
            }
            return;
        }
        let Some(line) = self.buffer.line(pos.row) else { return };
        let chars: Vec<char> = line.chars().collect();
        let mut col = pos.col.saturating_sub(1);
        while col > 0 && chars[col].is_whitespace() { col -= 1; }
        while col > 0 && !chars[col - 1].is_whitespace() { col -= 1; }
        self.cursor.move_to(pos.row, col);
    }

    fn find_matching_bracket(&self) -> Option<Position> {
        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row)?;
        let chars: Vec<char> = line.chars().collect();
        let current = *chars.get(pos.col)?;
        let (open, close, forward) = match current {
            '(' => ('(', ')', true),
            ')' => ('(', ')', false),
            '[' => ('[', ']', true),
            ']' => ('[', ']', false),
            '{' => ('{', '}', true),
            '}' => ('{', '}', false),
            '<' => ('<', '>', true),
            '>' => ('<', '>', false),
            _ => return None,
        };
        let mut depth = 1;
        let mut row = pos.row;
        let mut col = pos.col;
        let line_count = self.buffer.line_count();
        if forward {
            col += 1;
            loop {
                let l = self.buffer.line(row)?;
                let lc: Vec<char> = l.chars().collect();
                while col < lc.len() {
                    if lc[col] == open { depth += 1; }
                    else if lc[col] == close { depth -= 1; if depth == 0 { return Some(Position::new(row, col)); } }
                    col += 1;
                }
                row += 1; col = 0;
                if row >= line_count { return None; }
            }
        } else {
            if col == 0 { if row == 0 { return None; } row -= 1; col = self.buffer.line_len(row); }
            else { col -= 1; }
            loop {
                let l = self.buffer.line(row)?;
                let lc: Vec<char> = l.chars().collect();
                loop {
                    if col < lc.len() {
                        if lc[col] == close { depth += 1; }
                        else if lc[col] == open { depth -= 1; if depth == 0 { return Some(Position::new(row, col)); } }
                    }
                    if col == 0 { break; }
                    col -= 1;
                }
                if row == 0 { return None; }
                row -= 1; col = self.buffer.line_len(row);
            }
        }
    }

    // Selection
    pub fn start_selection(&mut self) {
        self.cursor.start_selection();
    }

    pub fn cancel_selection(&mut self) {
        self.cursor.cancel_selection();
    }

    pub fn has_selection(&self) -> bool {
        self.cursor.has_selection()
    }

    pub fn selection_range(&self) -> Option<(Position, Position)> {
        self.cursor.selection_range()
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.cursor.selection_range()?;
        Some(self.buffer.get_text_range(start.row, start.col, end.row, end.col))
    }

    // Clipboard
    pub fn copy(&mut self) {
        if let Some(text) = self.selected_text() {
            self.clipboard = Some(text.clone());
            self.clipboard_linewise = false;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&text);
            }
        }
    }

    pub fn cut(&mut self) {
        if let Some((start, end)) = self.cursor.selection_range() {
            let cursor_before = self.cursor.pos();
            let deleted = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
            self.clipboard = Some(deleted.clone());
            self.clipboard_linewise = false;
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&deleted);
            }
            self.wrap_cache.invalidate_from(start.row);

            self.history.record(
                EditOperation::Delete { start, end, deleted_text: deleted },
                cursor_before,
                start,
            );

            self.cursor.move_to(start.row, start.col);
            self.cursor.cancel_selection();
        }
    }

    /// Delete the current line entirely (for dd command)
    pub fn delete_current_line(&mut self) {
        let pos = self.cursor.pos();
        let row = pos.row;
        let line_count = self.buffer.line_count();

        // Get the line content for clipboard (with newline)
        let line_text = self.buffer.line(row).unwrap_or("").to_string();
        let deleted_text = format!("{}\n", line_text);

        // Copy to clipboard
        self.clipboard = Some(deleted_text.clone());
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&deleted_text);
        }

        // Delete the line
        self.buffer.delete_line(row);
        self.wrap_cache.invalidate_from(row);

        self.history.record(
            EditOperation::Delete {
                start: Position { row, col: 0 },
                end: Position { row: row + 1, col: 0 },
                deleted_text,
            },
            pos,
            Position { row: row.min(self.buffer.line_count().saturating_sub(1)), col: 0 },
        );

        let new_row = if line_count == 1 {
            0
        } else if row >= self.buffer.line_count() {
            self.buffer.line_count().saturating_sub(1)
        } else {
            row
        };
        self.cursor.move_to(new_row, 0);
        self.cursor.cancel_selection();
    }

    pub fn paste(&mut self) {
        if let Some(text) = self.clipboard.clone() {
            self.insert_str(&text);
        }
    }

    /// Paste after cursor (vim 'p' command)
    /// For line-wise content: paste below current line
    /// For character-wise content: paste after cursor
    pub fn paste_after(&mut self) {
        if let Some(text) = self.clipboard.clone() {
            if self.clipboard_linewise {
                let (row, col) = self.cursor();
                let cursor_before = Position { row, col };

                let new_row = row + 1;

                let text_to_insert = text.trim_end_matches('\n');
                let lines: Vec<String> = text_to_insert.split('\n').map(|s| s.to_string()).collect();

                for (i, line) in lines.iter().enumerate() {
                    self.buffer.insert_line(new_row + i, line.clone());
                    self.wrap_cache.insert_line(new_row + i);
                }

                self.cursor.move_to(new_row, 0);

                self.history.record(
                    EditOperation::LineInsert {
                        row: new_row,
                        lines,
                    },
                    cursor_before,
                    Position { row: new_row, col: 0 },
                );
            } else {
                let (row, col) = self.cursor();
                let line_len = self.buffer.line(row).map(|l| l.chars().count()).unwrap_or(0);
                let new_col = (col + 1).min(line_len);
                self.cursor.move_to(row, new_col);
                self.insert_str(&text);
            }
        }
    }

    /// Paste before cursor (vim 'P' command)
    /// For line-wise content: paste above current line
    /// For character-wise content: paste before cursor
    pub fn paste_before(&mut self) {
        if let Some(text) = self.clipboard.clone() {
            if self.clipboard_linewise {
                let (row, col) = self.cursor();
                let cursor_before = Position { row, col };

                let text_to_insert = text.trim_end_matches('\n');
                let lines: Vec<String> = text_to_insert.split('\n').map(|s| s.to_string()).collect();

                for (i, line) in lines.iter().enumerate() {
                    self.buffer.insert_line(row + i, line.clone());
                    self.wrap_cache.insert_line(row + i);
                }

                self.cursor.move_to(row, 0);

                self.history.record(
                    EditOperation::LineInsert {
                        row,
                        lines,
                    },
                    cursor_before,
                    Position { row, col: 0 },
                );
            } else {
                self.insert_str(&text);
            }
        }
    }

    // Text manipulation
    pub fn insert_char(&mut self, c: char) {
        let cursor_before = self.cursor.pos();

        if self.cursor.has_selection() {
            self.delete_selection_internal();
        }

        let pos = self.cursor.pos();
        self.buffer.insert_char(pos.row, pos.col, c);
        self.wrap_cache.invalidate_line(pos.row);

        self.history.record(
            EditOperation::Insert { pos, text: c.to_string() },
            cursor_before,
            Position::new(pos.row, pos.col + 1),
        );

        self.cursor.move_to(pos.row, pos.col + 1);
        self.ensure_cursor_visible();
    }

    /// Insert string at cursor, handling multi-line text and selection replacement
    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }

        let cursor_before = self.cursor.pos();

        // Delete selection first, record for undo
        let deleted_selection = if self.cursor.has_selection() {
            if let Some((start, end)) = self.cursor.selection_range() {
                let deleted = self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
                self.wrap_cache.invalidate_from(start.row);
                self.cursor.move_to(start.row, start.col);
                self.cursor.cancel_selection();
                Some((start, end, deleted))
            } else {
                None
            }
        } else {
            None
        };

        let pos = self.cursor.pos();
        let parts: Vec<&str> = s.split('\n').collect();
        let newline_count = parts.len().saturating_sub(1);

        if newline_count == 0 {
            self.buffer.insert_str(pos.row, pos.col, s);
            self.wrap_cache.invalidate_line(pos.row);
            self.cursor.move_to(pos.row, pos.col + s.chars().count());
        } else {
            if !parts[0].is_empty() {
                self.buffer.insert_str(pos.row, pos.col, parts[0]);
            }

            let split_col = pos.col + parts[0].chars().count();
            self.buffer.split_line(pos.row, split_col);
            self.wrap_cache.insert_line(pos.row + 1);

            for (i, part) in parts[1..parts.len() - 1].iter().enumerate() {
                self.buffer.insert_line(pos.row + 1 + i, part.to_string());
                self.wrap_cache.insert_line(pos.row + 1 + i);
            }

            let last_idx = pos.row + newline_count;
            let last_part = parts[parts.len() - 1];
            if !last_part.is_empty() {
                self.buffer.insert_str(last_idx, 0, last_part);
            }

            self.wrap_cache.invalidate_from(pos.row);
            self.cursor.move_to(last_idx, last_part.chars().count());
        }

        // Record undo operations
        let had_selection = deleted_selection.is_some();
        if let Some((start, end, deleted_text)) = deleted_selection {
            self.history.record(
                EditOperation::Delete { start, end, deleted_text },
                cursor_before,
                pos,
            );
        }

        self.history.record(
            EditOperation::Insert { pos, text: s.to_string() },
            if had_selection { pos } else { cursor_before },
            self.cursor.pos(),
        );

        self.ensure_cursor_visible();
    }

    pub fn insert_newline(&mut self) {
        let cursor_before = self.cursor.pos();

        if self.cursor.has_selection() {
            self.delete_selection_internal();
        }

        let pos = self.cursor.pos();

        let list_prefix = self.buffer.line(pos.row).and_then(|line| {
            let prefix = ListPrefix::detect(line)?;
            let prefix_len = prefix.prefix_len(line);
            let line_char_count = line.chars().count();

            let is_empty_item = line_char_count <= prefix_len;

            Some((prefix, prefix_len, is_empty_item))
        });

        if let Some((_, prefix_len, true)) = &list_prefix {
            let deleted = self.buffer.delete_range(pos.row, 0, *prefix_len);
            self.wrap_cache.invalidate_line(pos.row);
            self.history.record(
                EditOperation::Delete {
                    start: Position::new(pos.row, 0),
                    end: Position::new(pos.row, *prefix_len),
                    deleted_text: deleted,
                },
                cursor_before,
                Position::new(pos.row, 0),
            );
            self.cursor.move_to(pos.row, 0);
            self.ensure_cursor_visible();
            return;
        }

        self.buffer.split_line(pos.row, pos.col);
        self.wrap_cache.insert_line(pos.row + 1);
        self.wrap_cache.invalidate_line(pos.row);

        self.history.record(
            EditOperation::SplitLine { pos },
            cursor_before,
            Position::new(pos.row + 1, 0),
        );

        if let Some((prefix, _, false)) = list_prefix {
            let next_prefix = prefix.next_prefix();
            let prefix_char_count = next_prefix.chars().count();
            self.buffer.insert_str(pos.row + 1, 0, &next_prefix);
            self.wrap_cache.invalidate_line(pos.row + 1);
            self.history.record(
                EditOperation::Insert {
                    pos: Position::new(pos.row + 1, 0),
                    text: next_prefix,
                },
                Position::new(pos.row + 1, 0),
                Position::new(pos.row + 1, prefix_char_count),
            );
            self.cursor.move_to(pos.row + 1, prefix_char_count);
        } else {
            self.cursor.move_to(pos.row + 1, 0);
        }

        self.ensure_cursor_visible();
    }

    pub fn open_line_above(&mut self) {
        let pos = self.cursor.pos();
        let cursor_before = pos;
        let indent: String = self.buffer.line(pos.row)
            .map(|line| line.chars().take_while(|c| c.is_whitespace()).collect())
            .unwrap_or_default();

        let indent_len = indent.chars().count();
        self.buffer.insert_line(pos.row, indent.clone());
        self.wrap_cache.insert_line(pos.row);
        self.history.record(
            EditOperation::LineInsert {
                row: pos.row,
                lines: vec![indent],
            },
            cursor_before,
            Position::new(pos.row, indent_len),
        );
        
        self.cursor.move_to(pos.row, indent_len);
        self.ensure_cursor_visible();
    }

    pub fn delete_char(&mut self) {
        let pos = self.cursor.pos();
        let line_len = self.buffer.line_len(pos.row);

        if pos.col < line_len {
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col) {
                self.wrap_cache.invalidate_line(pos.row);
                self.history.record(
                    EditOperation::Delete {
                        start: pos,
                        end: Position::new(pos.row, pos.col + 1),
                        deleted_text: c.to_string(),
                    },
                    pos,
                    pos,
                );
            }
        } else if pos.row + 1 < self.buffer.line_count() {
            self.buffer.join_with_previous(pos.row + 1);
            self.wrap_cache.remove_line(pos.row + 1);
            self.wrap_cache.invalidate_line(pos.row);
            self.history.record(
                EditOperation::JoinLine { row: pos.row + 1, col: line_len },
                pos,
                pos,
            );
        }
    }

    pub fn delete_newline(&mut self) {
        let pos = self.cursor.pos();

        if pos.col > 0 {
            let cursor_before = pos;
            self.cursor.move_to(pos.row, pos.col - 1);
            if let Some(c) = self.buffer.delete_char(pos.row, pos.col - 1) {
                self.wrap_cache.invalidate_line(pos.row);
                self.history.record(
                    EditOperation::Delete {
                        start: Position::new(pos.row, pos.col - 1),
                        end: pos,
                        deleted_text: c.to_string(),
                    },
                    cursor_before,
                    self.cursor.pos(),
                );
            }
        } else if pos.row > 0 {
            let prev_len = self.buffer.line_len(pos.row - 1);
            let cursor_before = pos;

            self.buffer.join_with_previous(pos.row);
            self.wrap_cache.remove_line(pos.row);
            self.wrap_cache.invalidate_line(pos.row - 1);

            self.history.record(
                EditOperation::JoinLine { row: pos.row, col: prev_len },
                cursor_before,
                Position::new(pos.row - 1, prev_len),
            );

            self.cursor.move_to(pos.row - 1, prev_len);
        }

        self.ensure_cursor_visible();
    }

    fn delete_selection_internal(&mut self) {
        if let Some((start, end)) = self.cursor.selection_range() {
            self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
            self.wrap_cache.invalidate_from(start.row);
            self.cursor.move_to(start.row, start.col);
            self.cursor.cancel_selection();
        }
    }

    // Undo/Redo
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.history.pop_undo() {
            for op in entry.operations.iter().rev() {
                self.apply_operation(&op.inverse());
            }
            self.cursor.move_to(entry.cursor_before.row, entry.cursor_before.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.history.pop_redo() {
            for op in &entry.operations {
                self.apply_operation(op);
            }
            self.cursor.move_to(entry.cursor_after.row, entry.cursor_after.col);
            self.cursor.cancel_selection();
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { pos, text } => {
                if text.contains('\n') {
                    // Use split instead of lines() to preserve trailing newlines
                    // e.g., "hello\n".lines() returns ["hello"] but split returns ["hello", ""]
                    let parts: Vec<&str> = text.split('\n').collect();
                    if parts.is_empty() {
                        return;
                    }

                    // Insert first part at position
                    self.buffer.insert_str(pos.row, pos.col, parts[0]);

                    // For each subsequent part, split line and insert
                    let mut current_row = pos.row;
                    let mut split_col = pos.col + parts[0].chars().count();

                    for part in &parts[1..] {
                        self.buffer.split_line(current_row, split_col);
                        current_row += 1;
                        if !part.is_empty() {
                            self.buffer.insert_str(current_row, 0, part);
                        }
                        split_col = part.chars().count();
                    }
                } else {
                    self.buffer.insert_str(pos.row, pos.col, text);
                }
                self.wrap_cache.invalidate_from(pos.row);
            }
            EditOperation::Delete { start, end, .. } => {
                self.buffer.delete_text_range(start.row, start.col, end.row, end.col);
                self.wrap_cache.invalidate_from(start.row);
            }
            EditOperation::SplitLine { pos } => {
                self.buffer.split_line(pos.row, pos.col);
                self.wrap_cache.insert_line(pos.row + 1);
                self.wrap_cache.invalidate_line(pos.row);
            }
            EditOperation::JoinLine { row, .. } => {
                self.buffer.join_with_previous(*row);
                self.wrap_cache.remove_line(*row);
                self.wrap_cache.invalidate_line(row - 1);
            }
            EditOperation::BlockDelete { start_row, end_row, start_col, end_col, .. } => {
                for row in (*start_row..=*end_row).rev() {
                    if let Some(line) = self.buffer.line(row) {
                        let chars: Vec<char> = line.chars().collect();
                        let line_len = chars.len();
                        let actual_start = (*start_col).min(line_len);
                        let actual_end = (*end_col + 1).min(line_len);
                        if actual_start < actual_end {
                            let new_line: String = chars[..actual_start]
                                .iter()
                                .chain(chars[actual_end..].iter())
                                .collect();
                            if let Some(line_ref) = self.buffer.line_mut(row) {
                                *line_ref = new_line;
                            }
                        }
                    }
                }
                self.wrap_cache.invalidate_from(*start_row);
            }
            EditOperation::BlockInsert { start_row, col, lines } => {
                for (i, text) in lines.iter().enumerate() {
                    let row = start_row + i;
                    if row < self.buffer.line_count() {
                        self.buffer.insert_str(row, *col, text);
                    }
                }
                self.wrap_cache.invalidate_from(*start_row);
            }
            EditOperation::LineInsert { row, lines } => {
                for (i, line) in lines.iter().enumerate() {
                    self.buffer.insert_line(row + i, line.clone());
                    self.wrap_cache.insert_line(row + i);
                }
            }
            EditOperation::LineDelete { row, lines } => {
                for _ in 0..lines.len() {
                    if *row < self.buffer.line_count() {
                        self.buffer.delete_line(*row);
                        self.wrap_cache.remove_line(*row);
                    }
                }
            }
        }
    }

    // Input processing
    pub fn input(&mut self, key: KeyEvent) {
        match process_key(key) {
            InputAction::InsertChar(c) => self.insert_char(c),
            InputAction::InsertNewline => self.insert_newline(),
            InputAction::DeleteChar => self.delete_char(),
            InputAction::DeleteCharBefore => self.delete_newline(),
            InputAction::Move(movement) => self.move_cursor(movement),
            InputAction::None => {}
        }
    }

    // Query
    pub fn lines(&self) -> Vec<&str> {
        self.buffer.lines()
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    // Scrolling
    pub fn update_scroll(&mut self, view_height: usize) {
        self.view_height = view_height;
        if view_height == 0 {
            return;
        }

        let (cursor_row, cursor_col) = self.cursor();
        let line_count = self.buffer.line_count();

        let effective_scrolloff = self.scrolloff.min(view_height / 2);

        if cursor_row < self.scroll_offset + effective_scrolloff {
            self.scroll_offset = cursor_row.saturating_sub(effective_scrolloff);
        }

        if self.line_wrap_enabled && self.view_width > 0 {
            while self.scroll_offset < cursor_row {
                let visual_lines = self.visual_lines_in_range(self.scroll_offset, cursor_row);
                if visual_lines <= view_height.saturating_sub(effective_scrolloff) {
                    break;
                }
                self.scroll_offset += 1;
            }
        } else {
            if cursor_row + effective_scrolloff >= self.scroll_offset + view_height {
                self.scroll_offset = cursor_row
                    .saturating_add(effective_scrolloff)
                    .saturating_sub(view_height.saturating_sub(1));
            }
        }

        // Clamp to valid range
        let max_scroll = line_count.saturating_sub(1);
        self.scroll_offset = self.scroll_offset.min(max_scroll);

        if self.view_width > 0 {
            let effective_width = self.view_width.saturating_sub(1);
            if cursor_col < self.h_scroll_offset {
                self.h_scroll_offset = cursor_col;
            } else if cursor_col >= self.h_scroll_offset + effective_width {
                self.h_scroll_offset = cursor_col.saturating_sub(effective_width) + 1;
            }
        }
    }

    fn visual_lines_in_range(&self, start_row: usize, end_row: usize) -> usize {
        let width = self.view_width.max(1);
        let mut visual_lines = 0;

        for row in start_row..=end_row.min(self.buffer.line_count().saturating_sub(1)) {
            let line_len = self.buffer.line_len(row);
            if line_len == 0 {
                visual_lines += 1;
            } else {
                visual_lines += (line_len + width - 1) / width;
            }
        }

        visual_lines
    }

    fn ensure_cursor_visible(&mut self) {
        if self.view_height > 0 {
            self.update_scroll(self.view_height);
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    pub fn h_scroll_offset(&self) -> usize {
        self.h_scroll_offset
    }

    /// Returns the horizontal scroll offset in display units (accounting for Unicode widths).
    /// This calculates the display width of characters from 0 to h_scroll_offset on the cursor line.
    pub fn h_scroll_display_offset(&self) -> usize {
        if self.h_scroll_offset == 0 {
            return 0;
        }

        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        let mut display_offset: usize = 0;
        for (i, ch) in chars.iter().enumerate() {
            if i >= self.h_scroll_offset {
                break;
            }
            display_offset += char_display_width(*ch, self.tab_width) as usize;
        }
        display_offset
    }

    pub fn line_number_gutter_width(&self) -> u16 {
        if self.line_number_mode != LineNumberMode::None {
            self.line_number_width
        } else {
            0
        }
    }
    pub fn content_left_offset(&self) -> u16 {
        self.left_padding + self.line_number_gutter_width()
    }

    /// Returns the cursor's display column position, accounting for Unicode character widths and tabs.
    pub fn cursor_display_col(&self) -> usize {
        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        let mut display_col: usize = 0;
        for (i, ch) in chars.iter().enumerate() {
            if i >= pos.col {
                break;
            }
            display_col += char_display_width(*ch, self.tab_width) as usize;
        }
        display_col
    }

    /// Returns the cursor screen position info for native cursor positioning.
    pub fn cursor_screen_info(&self) -> (usize, bool, usize) {
        let pos = self.cursor.pos();
        let line = self.buffer.line(pos.row).unwrap_or("");
        let chars: Vec<char> = line.chars().collect();

        let mut display_col: usize = 0;
        let mut line_display_width: usize = 0;

        for (i, ch) in chars.iter().enumerate() {
            let ch_width = char_display_width(*ch, self.tab_width) as usize;
            if i < pos.col {
                display_col += ch_width;
            }
            line_display_width += ch_width;
        }

        let is_at_line_end = pos.col >= chars.len();
        (display_col, is_at_line_end, line_display_width)
    }

    /// Returns the cursor's screen position accounting for line wrapping.
    pub fn cursor_wrapped_position(&self, content_width: usize) -> (usize, usize) {
        if content_width == 0 {
            return (0, 0);
        }

        let display_col = self.cursor_display_col();

        if !self.line_wrap_enabled {
            // No wrapping, just use display column
            return (0, display_col);
        }

        // Calculate which wrapped line the cursor is on
        let wrapped_row = display_col / content_width;
        let wrapped_col = display_col % content_width;

        (wrapped_row, wrapped_col)
    }
    pub fn line_wrapped_height(&self, row: usize, content_width: usize) -> usize {
        if content_width == 0 {
            return 1;
        }

        let line = self.buffer.line(row).unwrap_or("");
        if line.is_empty() {
            return 1;
        }

        let mut display_width: usize = 0;
        for ch in line.chars() {
            display_width += char_display_width(ch, self.tab_width) as usize;
        }
        ((display_width + content_width - 1) / content_width).max(1)
    }

    /// Center the cursor line on screen (zz command)
    pub fn center_cursor(&mut self) {
        let (cursor_row, _) = self.cursor();
        let half_height = self.view_height / 2;
        self.scroll_offset = cursor_row.saturating_sub(half_height);
    }

    /// Scroll so cursor line is at top of screen (zt command)
    pub fn scroll_cursor_to_top(&mut self) {
        let (cursor_row, _) = self.cursor();
        self.scroll_offset = cursor_row;
    }

    /// Scroll so cursor line is at bottom of screen (zb command)
    pub fn scroll_cursor_to_bottom(&mut self) {
        let (cursor_row, _) = self.cursor();
        self.scroll_offset = cursor_row.saturating_sub(self.view_height.saturating_sub(1));
    }

    pub fn set_view_size(&mut self, width: usize, height: usize) {
        self.view_width = width;
        self.view_height = height;
    }

    pub fn get_overflow_info(&self) -> (bool, bool) {
        let (cursor_row, _) = self.cursor();
        let line_len = self.buffer.line_len(cursor_row);
        (self.h_scroll_offset > 0, line_len > self.h_scroll_offset + self.view_width)
    }

    pub fn content_x_offset(&self) -> u16 {
        let gutter_width = if self.line_number_mode != LineNumberMode::None {
            self.line_number_width
        } else {
            0
        };
        self.left_padding + gutter_width
    }

    pub fn visual_to_logical_coords(&self, visual_y: usize, visual_x: usize) -> (usize, usize) {
        if !self.line_wrap_enabled || self.view_width == 0 {
            let row = visual_y + self.scroll_offset;
            let col = visual_x + self.h_scroll_offset;
            return (row, col);
        }

        let content_width = self.view_width.saturating_sub(self.right_padding as usize);
        if content_width == 0 {
            return (self.scroll_offset, 0);
        }

        let line_count = self.buffer.line_count();
        let mut visual_lines_consumed = 0;
        let mut row = self.scroll_offset;

        while row < line_count {
            let line_len = self.buffer.line_len(row);
            let visual_lines_for_row = if line_len == 0 {
                1
            } else {
                (line_len + content_width - 1) / content_width 
            };

            if visual_lines_consumed + visual_lines_for_row > visual_y {
                let visual_offset_in_row = visual_y - visual_lines_consumed;
                let col = visual_offset_in_row * content_width + visual_x;
                return (row, col.min(line_len));
            }

            visual_lines_consumed += visual_lines_for_row;
            row += 1;
        }

        if line_count > 0 {
            let last_row = line_count - 1;
            let last_col = self.buffer.line_len(last_row);
            (last_row, last_col)
        } else {
            (0, 0)
        }
    }
}

// Widget implementation
impl Widget for &Editor {
    fn render(self, area: Rect, buf: &mut RatatuiBuffer) {
        let inner_area = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        if self.line_wrap_enabled {
            self.render_wrapped(inner_area, buf);
        } else {
            self.render_no_wrap(inner_area, buf);
        }
    }
}

impl Editor {
    /// Renders a cursor at the given position in the buffer
    fn render_cursor_at(&self, buf: &mut RatatuiBuffer, x: u16, y: u16, ch: char, base_style: Style) {
        if let Some(cell) = buf.cell_mut((x, y)) {
            match self.cursor_shape {
                CursorShape::Block => {
                    // Full reversed block for Normal mode
                    cell.set_char(ch);
                    cell.set_style(base_style.add_modifier(Modifier::REVERSED));
                }
                CursorShape::Bar => {
                    // For bar cursor, don't render custom cursor - use terminal's native cursor
                    // Just render the character normally, terminal cursor will be positioned here
                    cell.set_char(ch);
                    cell.set_style(base_style);
                }
                CursorShape::Underline => {
                    // Underline + Reversed for Replace mode - more visible than underline alone
                    cell.set_char(ch);
                    cell.set_style(base_style.add_modifier(Modifier::UNDERLINED | Modifier::REVERSED));
                }
            }
        }
    }

    /// Returns true if the cursor shape uses the terminal's native cursor (not rendered by editor)
    pub fn uses_native_cursor(&self) -> bool {
        matches!(self.cursor_shape, CursorShape::Bar)
    }

    fn render_wrapped(&self, area: Rect, buf: &mut RatatuiBuffer) {
        // Account for line number gutter
        let gutter_width = if self.line_number_mode != LineNumberMode::None { self.line_number_width } else { 0 };
        let content_start_x = area.x + self.left_padding + gutter_width;
        let content_end_x = area.x + area.width.saturating_sub(self.right_padding);
        let content_width = content_end_x.saturating_sub(content_start_x) as usize;
        if content_width == 0 {
            return;
        }

        let cursor_pos = self.cursor.pos();
        let selection = if let Some((anchor_row, current_row)) = self.visual_line_selection {
            let (start_row, end_row) = if anchor_row <= current_row {
                (anchor_row, current_row)
            } else {
                (current_row, anchor_row)
            };
            let end_line_len = self.buffer.line(end_row).map(|l| l.chars().count()).unwrap_or(0);
            Some((Position { row: start_row, col: 0 }, Position { row: end_row, col: end_line_len + 1 }))
        } else {
            self.cursor.selection_range()
        };
        let block_selection = self.visual_block_selection;
        let line_count = self.buffer.line_count();

        // Use row-based scrolling (consistent with update_scroll)
        // scroll_offset is the first visible ROW, not visual line
        let start_row = self.scroll_offset.min(line_count);
        let mut screen_y = area.y;

        for row in start_row..line_count {
            if screen_y >= area.y + area.height {
                break;
            }

            let line = self.buffer.line(row).unwrap_or("");
            let is_cursor_line = row == cursor_pos.row;
            let chars: Vec<char> = line.chars().collect();

            // Render line numbers if enabled (only for first visual line of a row)
            if let Some(ln_str) = self.get_line_number_str(row, cursor_pos.row) {
                let ln_style = if is_cursor_line {
                    self.line_number_style.add_modifier(Modifier::BOLD)
                } else {
                    self.line_number_style
                };
                for (i, ch) in ln_str.chars().enumerate() {
                    if let Some(cell) = buf.cell_mut((area.x + self.left_padding + i as u16, screen_y)) {
                        cell.set_char(ch);
                        cell.set_style(ln_style);
                    }
                }
            }

            if chars.is_empty() {
                if is_cursor_line {
                    self.render_cursor_at(buf, content_start_x, screen_y, ' ', Style::default());
                }
                screen_y += 1;
                continue;
            }

            // Render line with wrapping
            let mut col = 0;
            let mut is_wrapped_continuation = false;
            while col < chars.len() {
                if screen_y >= area.y + area.height {
                    return;
                }

                let mut x = content_start_x;

                if is_wrapped_continuation && col < chars.len() && chars[col] == ' ' {
                    let is_cursor_on_space = is_cursor_line && col == cursor_pos.col;
                    if !is_cursor_on_space {
                        col += 1;
                        if col >= chars.len() {
                            if is_cursor_line && cursor_pos.col >= chars.len() {
                                self.render_cursor_at(buf, x, screen_y, ' ', Style::default());
                            }
                            screen_y += 1;
                            break;
                        }
                    }
                }

                while col < chars.len() && x < content_end_x {
                    let ch = chars[col];
                    let base_style = self.get_char_style(row, col, selection, block_selection);
                    let is_cursor = is_cursor_line && col == cursor_pos.col;

                    let ch_width = char_display_width(ch, self.tab_width);
                    if ch == '\t' {
                        for i in 0..ch_width {
                            if x >= content_end_x {
                                break;
                            }
                            if i == 0 && is_cursor {
                                self.render_cursor_at(buf, x, screen_y, ' ', base_style);
                            } else if let Some(cell) = buf.cell_mut((x, screen_y)) {
                                cell.set_char(' ');
                                cell.set_style(base_style);
                            }
                            x += 1;
                        }
                    } else {
                        if is_cursor {
                            self.render_cursor_at(buf, x, screen_y, ch, base_style);
                        } else if let Some(cell) = buf.cell_mut((x, screen_y)) {
                            cell.set_char(ch);
                            cell.set_style(base_style);
                        }
                        x += ch_width;
                    }
                    col += 1;
                }

                // Render cursor at end of line if cursor is past last char
                // Use full area width to allow cursor in right padding
                if is_cursor_line && cursor_pos.col >= chars.len() && col == chars.len() {
                    if x < area.x + area.width {
                        self.render_cursor_at(buf, x, screen_y, ' ', Style::default());
                    }
                }

                is_wrapped_continuation = true;
                screen_y += 1;
            }
        }

        if self.buffer.is_empty() {
            self.render_cursor_at(buf, content_start_x, area.y, ' ', Style::default());
        }
    }

    fn render_no_wrap(&self, area: Rect, buf: &mut RatatuiBuffer) {
        // Account for line number gutter
        let gutter_width = if self.line_number_mode != LineNumberMode::None { self.line_number_width } else { 0 };
        let content_start_x = area.x + self.left_padding + gutter_width;
        let content_end_x = area.x + area.width.saturating_sub(self.right_padding);

        let cursor_pos = self.cursor.pos();
        let selection = if let Some((anchor_row, current_row)) = self.visual_line_selection {
            let (start_row, end_row) = if anchor_row <= current_row {
                (anchor_row, current_row)
            } else {
                (current_row, anchor_row)
            };
            let end_line_len = self.buffer.line(end_row).map(|l| l.chars().count()).unwrap_or(0);
            Some((Position { row: start_row, col: 0 }, Position { row: end_row, col: end_line_len + 1 }))
        } else {
            self.cursor.selection_range()
        };
        let block_selection = self.visual_block_selection;
        let h_scroll = self.h_scroll_offset;

        let mut y = area.y;
        let end_row = (self.scroll_offset + area.height as usize).min(self.buffer.line_count());

        for row in self.scroll_offset..end_row {
            if y >= area.y + area.height {
                break;
            }

            let line = self.buffer.line(row).unwrap_or("");
            let is_cursor_line = row == cursor_pos.row;
            let chars: Vec<char> = line.chars().collect();
            let line_h_scroll = if is_cursor_line { h_scroll } else { 0 };

            // Render line numbers if enabled
            if let Some(ln_str) = self.get_line_number_str(row, cursor_pos.row) {
                let ln_style = if is_cursor_line {
                    self.line_number_style.add_modifier(Modifier::BOLD)
                } else {
                    self.line_number_style
                };
                for (i, ch) in ln_str.chars().enumerate() {
                    if let Some(cell) = buf.cell_mut((area.x + self.left_padding + i as u16, y)) {
                        cell.set_char(ch);
                        cell.set_style(ln_style);
                    }
                }
            }

            let mut x = content_start_x;
            for col in line_h_scroll..chars.len() {
                if x >= content_end_x {
                    break;
                }

                let ch = chars[col];
                let base_style = self.get_char_style(row, col, selection, block_selection);
                let is_cursor = is_cursor_line && col == cursor_pos.col;

                let ch_width = char_display_width(ch, self.tab_width);
                if ch == '\t' {
                    for i in 0..ch_width {
                        if x >= content_end_x {
                            break;
                        }
                        if i == 0 && is_cursor {
                            self.render_cursor_at(buf, x, y, ' ', base_style);
                        } else if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_char(' ');
                            cell.set_style(base_style);
                        }
                        x += 1;
                    }
                } else {
                    if is_cursor {
                        self.render_cursor_at(buf, x, y, ch, base_style);
                    } else if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(ch);
                        cell.set_style(base_style);
                    }
                    x += ch_width;
                }
            }

            if is_cursor_line && cursor_pos.col >= chars.len() {
                if x < area.x + area.width {
                    self.render_cursor_at(buf, x, y, ' ', Style::default());
                }
            }

            y += 1;
        }

        if self.buffer.line_count() <= self.scroll_offset {
            self.render_cursor_at(buf, content_start_x, area.y, ' ', Style::default());
        }
    }

    fn get_char_style(
        &self,
        row: usize,
        col: usize,
        selection: Option<(Position, Position)>,
        block_selection: Option<(Position, Position)>,
    ) -> Style {
        // Block selection takes priority (rectangular selection)
        if let Some((anchor, current)) = block_selection {
            let (start_row, end_row) = if anchor.row <= current.row {
                (anchor.row, current.row)
            } else {
                (current.row, anchor.row)
            };
            let (start_col, end_col) = if anchor.col <= current.col {
                (anchor.col, current.col)
            } else {
                (current.col, anchor.col)
            };

            let in_block = row >= start_row && row <= end_row && col >= start_col && col <= end_col;

            if in_block {
                return self.selection_style;
            }
        }

        // Character-wise selection
        if let Some((start, end)) = selection {
            let in_selection = if start.row == end.row {
                row == start.row && col >= start.col && col < end.col
            } else if row == start.row {
                col >= start.col
            } else if row == end.row {
                col < end.col
            } else {
                row > start.row && row < end.row
            };

            if in_selection {
                return self.selection_style;
            }
        }

        if let Some(highlight_style) = self.highlight_style_at(row, col) {
            return highlight_style;
        }
        if let Some(wiki_style) = self.wiki_link_style_at(row, col) {
            return wiki_style;
        }

        Style::default()
    }
}
