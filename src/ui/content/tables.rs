use super::*;

/// Visible width of a table cell after inline markdown shrinks
/// (e.g. `[label](url)` -> `label`). Measured in *display columns*, so wide
/// characters (CJK, emoji) contribute their full terminal width — not just 1
/// char each. Markdown markers stripped by `calc_formatting_shrinkage` are all
/// ASCII (1 col each), so subtracting their char-count from the display width
/// gives the visible-content's display width.
pub(crate) fn cell_visible_width(cell: &str) -> usize {
    let display_width: usize = cell
        .chars()
        .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
        .sum();
    let total_chars = cell.chars().count();
    let marker_chars = calc_formatting_shrinkage(cell, total_chars);
    display_width.saturating_sub(marker_chars)
}

/// Per-column minimum width when shrinking a wide table to fit the terminal.
pub(super) const TABLE_COLUMN_MIN_WIDTH: usize = 8;

/// Given the "natural" width of each column (max content width) and the available
/// budget for content (= terminal area minus borders/padding), return capped widths
/// that sum to at most `available`. Shrinks the widest column(s) first so narrow
/// columns keep their full width whenever possible. Each column stays at or above
/// `TABLE_COLUMN_MIN_WIDTH` unless its natural width is already below that.
pub(crate) fn cap_column_widths(natural: &[usize], available: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = natural.to_vec();
    if widths.is_empty() {
        return widths;
    }
    loop {
        let total: usize = widths.iter().sum();
        if total <= available {
            return widths;
        }
        // Pick the widest column that can still shrink.
        let mut target: Option<usize> = None;
        let mut max_w: usize = 0;
        for (i, &w) in widths.iter().enumerate() {
            let floor = TABLE_COLUMN_MIN_WIDTH.min(natural[i]);
            if w > floor && w > max_w {
                max_w = w;
                target = Some(i);
            }
        }
        match target {
            Some(i) => widths[i] -= 1,
            None => return widths, // every column already at its floor; can't shrink further
        }
    }
}

/// Split a table cell on GFM-style line-break tags (`<br>`, `<br/>`, `<br />`,
/// case-insensitive). Returns one slice per logical line — at least one slice,
/// even for an empty cell.
///
/// Tag recognition is deliberately narrow: only the three common forms with
/// optional single-space and trailing slash. Anything else (attributes, unusual
/// whitespace, non-ASCII case folding) is passed through as literal text.
pub(crate) fn split_cell_by_br(cell: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let bytes = cell.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < cell.len() {
        if bytes[i] == b'<' {
            if let Some(end) = try_match_br(bytes, i) {
                parts.push(&cell[start..i]);
                start = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    parts.push(&cell[start..]);
    parts
}

/// Try to match a `<br>` / `<br/>` / `<br />` tag starting at byte offset `at`.
/// Returns the byte offset just past the closing `>` if matched, else `None`.
pub(super) fn try_match_br(bytes: &[u8], at: usize) -> Option<usize> {
    let b = bytes;
    if b.get(at) != Some(&b'<') {
        return None;
    }
    if !matches!(b.get(at + 1), Some(b'b' | b'B')) {
        return None;
    }
    if !matches!(b.get(at + 2), Some(b'r' | b'R')) {
        return None;
    }
    let mut i = at + 3;
    // Optional single space ("<br />" form).
    if b.get(i) == Some(&b' ') {
        i += 1;
    }
    // Optional self-closing slash.
    if b.get(i) == Some(&b'/') {
        i += 1;
    }
    // Must end in `>`.
    if b.get(i) == Some(&b'>') {
        Some(i + 1)
    } else {
        None
    }
}

/// Calculate the adjusted column for a table cell
/// Raw format: "| cell1 | cell2 |"
/// Rendered:   "▶ │ cell1 │ cell2 │" with cells padded to column widths
pub(super) fn calc_table_adjusted_col(
    raw_col: usize,
    document: &DocumentSnapshot,
    cells: &[DocumentRange],
    column_widths: &[u16],
    alignments: &[crate::app::Alignment],
) -> usize {
    use crate::app::Alignment;
    let mut rendered_pos = 3;
    let mut raw_pos = 0;

    for (cell_idx, range) in cells.iter().enumerate() {
        let cell = document.slice(*range);
        let col_width = column_widths.get(cell_idx).copied().unwrap_or(3) as usize;
        if raw_pos == 0 {
            raw_pos = 1;
        }

        let raw_cell_start = raw_pos;

        let cell_char_len = cell.chars().count();
        let cell_display_width: usize = cell
            .chars()
            .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
            .sum();
        let raw_cell_end = raw_cell_start + cell_char_len + 3; // " content |"

        if raw_col >= raw_cell_start && raw_col < raw_cell_end {
            let char_offset_in_raw_cell = raw_col.saturating_sub(raw_cell_start + 1); // +1 for leading space
                                                                                      // Convert character offset to display width
            let display_offset: usize = cell
                .chars()
                .take(char_offset_in_raw_cell.min(cell_char_len))
                .map(|character| if character == '\t' { 4 } else { character.width().unwrap_or(0) })
                .sum();
            let pad = col_width.saturating_sub(cell_display_width);
            let alignment = alignments.get(cell_idx).copied().unwrap_or(Alignment::Left);
            let content_padding = match alignment {
                Alignment::Left => 0,
                Alignment::Right => pad,
                Alignment::Center => pad / 2,
            };
            let rendered_content_start = rendered_pos + 1 + content_padding; // +1 for leading space

            return rendered_content_start + display_offset;
        }

        raw_pos = raw_cell_end;
        rendered_pos += col_width + 2 + 1;
    }

    3 + raw_col
}
