use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Size},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use ratatui_image::{
    sliced::{SignedPosition, SlicedImage, SlicedProtocol},
    Resize,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{normalize_image_destination, App, ContentItem, DialogState, DocumentRange, DocumentSnapshot, Focus, InlineImageRect, LinkInfo, Mode};
use crate::config::Theme;

mod blocks;
mod images;
mod inline;
mod layout;
mod links;
mod search_highlights;
mod tables;
mod wrapping;

use blocks::*;
use images::*;
use inline::*;
pub use layout::render_content;
pub(crate) use links::detect_bare_url_len;
use search_highlights::*;
pub(crate) use tables::cell_visible_width;
use tables::*;
pub(crate) use wrapping::content_item_click_col;
use wrapping::*;

#[derive(Clone, Copy)]
struct RenderContext<'a> {
    theme: &'a Theme,
    area: Rect,
    is_cursor: bool,
    selected_link: usize,
    has_link: bool,
}

impl<'a> RenderContext<'a> {
    fn new(theme: &'a Theme, area: Rect, is_cursor: bool, selected_link: usize, has_link: bool) -> Self {
        Self { theme, area, is_cursor, selected_link, has_link }
    }
}

const INLINE_THUMBNAIL_HORIZONTAL_PADDING: u16 = 2;
const INLINE_THUMBNAIL_GAP: u16 = 1;
const INLINE_THUMBNAIL_MIN_WIDTH: u16 = 12;
const INLINE_THUMBNAIL_MAX_WIDTH: u16 = 40;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_protocol_keys_are_stable_per_placement() {
        assert_eq!(standalone_image_state_key(7, "image.png"), standalone_image_state_key(7, "image.png"));
        assert_eq!(inline_image_state_key(7, 2, "image.png"), inline_image_state_key(7, 2, "image.png"));
    }

    #[test]
    fn image_protocol_keys_distinguish_duplicate_placements() {
        assert_ne!(inline_image_state_key(7, 1, "image.png"), inline_image_state_key(7, 2, "image.png"));
        assert_ne!(standalone_image_state_key(7, "image.png"), inline_image_state_key(7, 1, "image.png"));
    }

    #[test]
    fn inline_thumbnail_rows_use_configured_height() {
        assert_eq!(inline_thumbnails_height(0, 80, 8), 0);
        assert_eq!(inline_thumbnails_height(1, 80, 8), 8);
        assert_eq!(inline_thumbnails_height(3, 80, 8), 8);
    }

    #[test]
    fn inline_thumbnail_rows_wrap_at_content_boundary() {
        // At 30 columns, 12-column thumbnails plus a one-column gap fit two per row.
        assert_eq!(inline_thumbnails_per_row(30, 4), 2);
        assert_eq!(inline_thumbnails_height(2, 30, 4), 4);
        assert_eq!(inline_thumbnails_height(3, 30, 4), 8);
    }

    #[test]
    fn inline_thumbnail_rows_saturate_on_extreme_values() {
        assert_eq!(inline_thumbnails_height(usize::MAX, u16::MAX, u16::MAX), u16::MAX);
    }

    #[test]
    fn inline_thumbnail_uses_the_visible_part_at_the_viewport_bottom() {
        let image = Rect::new(2, 3, 12, 4);
        assert_eq!(image.intersection(Rect::new(0, 0, 80, 8)).height, 4);
        assert_eq!(image.intersection(Rect::new(0, 0, 80, 6)).height, 3);
        assert_eq!(image.intersection(Rect::new(0, 0, 80, 5)).height, 2);
        assert_eq!(image.intersection(Rect::new(0, 0, 80, 4)).height, 1);
        assert_eq!(image.intersection(Rect::new(0, 0, 80, 3)).height, 0);
    }

    #[test]
    fn clipped_image_frame_leaves_room_for_a_visible_image_row() {
        let one_row = Block::default().borders(image_frame_borders(1, 4)).inner(Rect::new(0, 0, 12, 1));
        let partial = Block::default().borders(image_frame_borders(2, 4)).inner(Rect::new(0, 0, 12, 2));
        let complete = Block::default().borders(image_frame_borders(4, 4)).inner(Rect::new(0, 0, 12, 4));
        assert_eq!(one_row, Rect::new(1, 0, 10, 1));
        assert_eq!(partial, Rect::new(1, 1, 10, 1));
        assert_eq!(complete, Rect::new(1, 1, 10, 2));
    }

    #[test]
    fn bottom_item_is_clipped_to_the_remaining_viewport_height() {
        let viewport = Rect::new(0, 0, 80, 22);
        let last_item_height = visible_item_height(20, viewport.height, 5);
        let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(20), Constraint::Length(last_item_height)]).split(viewport);
        let visible_item = chunks[1].intersection(viewport);
        let visible_image = Rect::new(chunks[1].x + 2, chunks[1].y + 1, 12, 4).intersection(viewport);
        let image_area = Block::default().borders(image_frame_borders(visible_image.height, 4)).inner(visible_image);
        assert_eq!(last_item_height, 2);
        assert_eq!(chunks[0].height, 20);
        assert_eq!(chunks[1].height, 2);
        assert_eq!(visible_item.height, 2);
        assert_eq!(visible_image.height, 1);
        assert_eq!(image_area.height, 1);
    }

    #[test]
    fn cell_visible_width_plain_text() {
        assert_eq!(cell_visible_width("Plain URL"), 9);
    }

    #[test]
    fn cell_visible_width_strips_markdown_link() {
        // `[label](url)` -> `label`. A prior off-by-one counted the closing `)` toward visible.
        assert_eq!(cell_visible_width("[Top 5](https://x.test)"), 5);
    }

    #[test]
    fn cell_visible_width_strips_bold_italic_code() {
        assert_eq!(cell_visible_width("**bold text**"), 9);
        assert_eq!(cell_visible_width("*em*"), 2);
        assert_eq!(cell_visible_width("`code`"), 4);
    }

    #[test]
    fn cell_visible_width_mixed_text_and_link() {
        // "one [a](u) two" renders as "one a two" = 9 visible chars.
        assert_eq!(cell_visible_width("one [a](https://u.test) two"), 9);
    }

    #[test]
    fn cell_visible_width_multiple_links_same_cell() {
        // Pins the off-by-one fix: before the fix, each link inflated visible by 1,
        // so a 2-link cell miscounted by 2 and tables with uneven link counts
        // misaligned their borders.
        // "[a](u1) [b](u2)" -> "a b" = 3 visible chars.
        assert_eq!(cell_visible_width("[a](https://u1.test) [b](https://u2.test)"), 3);
    }

    #[test]
    fn cell_visible_width_removes_preview_images_from_prose() {
        assert_eq!(cell_visible_width("![one](1.png)"), 0);
        assert_eq!(cell_visible_width("before ![one](1.png) after"), "before  after".width());
        // Double-bang images intentionally remain text-only links.
        assert_eq!(cell_visible_width("!![one](1.png)"), "one".width());
    }

    #[test]
    fn image_only_source_line_has_no_blank_prose_row() {
        let theme = Theme::default();
        assert!(inline_prose_text("![one](1.png) ![two](2.png)", &theme).is_empty());
        assert!(inline_prose_text("![one](1.png)![two](2.png)", &theme).is_empty());
        assert_eq!(inline_prose_text("caption ![one](1.png) ![two](2.png)", &theme), "caption");
    }

    #[test]
    fn detect_bare_url_basic() {
        assert_eq!(detect_bare_url_len("see https://example.com now", 4), Some(19));
        assert_eq!(detect_bare_url_len("http://a.test", 0), Some(13));
    }

    #[test]
    fn detect_bare_url_strips_trailing_punctuation() {
        // GFM: the trailing `.` should not be part of the URL.
        assert_eq!(detect_bare_url_len("visit https://example.com.", 6), Some(19));
    }

    #[test]
    fn detect_bare_url_stops_at_delimiters() {
        // "https://x.test" = 14 chars; the `)` / `>` terminator is not included.
        assert_eq!(detect_bare_url_len("(https://x.test)", 1), Some(14));
        assert_eq!(detect_bare_url_len("<https://x.test>", 1), Some(14));
    }

    #[test]
    fn detect_bare_url_no_match_returns_none() {
        assert_eq!(detect_bare_url_len("nothing here", 0), None);
        assert_eq!(detect_bare_url_len("http:/broken", 0), None); // missing second slash
    }

    #[test]
    fn cell_visible_width_counts_bare_url_one_to_one() {
        // Bare URL is not shrunk — visible width equals its character count.
        assert_eq!(cell_visible_width("visit https://x.test"), 20);
    }

    #[test]
    fn cell_visible_width_counts_emoji_as_two_columns() {
        // 🟡 is one char but displays as 2 columns in a terminal. The char-count
        // version under-counted: 1 (emoji) + 11 ("In-Progress") = 12, so column
        // widths were reserved at 12 cols while the cell actually renders in 13.
        // That off-by-one forced an unnecessary wrap.
        assert_eq!(cell_visible_width("🟡In-Progress"), 13);
        assert_eq!(cell_visible_width("🟡"), 2);
        // ASCII control: still matches char count.
        assert_eq!(cell_visible_width("In-Progress"), 11);
    }

    #[test]
    fn cap_column_widths_leaves_narrow_columns_alone() {
        // Natural sum = 7 + 12 + 500 = 519; budget = 107 (like a ~120-col terminal).
        // Expect narrow columns untouched, Description shrunk to fill what's left.
        let natural = vec![7, 12, 500];
        let capped = cap_column_widths(&natural, 107);
        assert_eq!(capped[0], 7);
        assert_eq!(capped[1], 12);
        assert_eq!(capped[0] + capped[1] + capped[2], 107);
    }

    #[test]
    fn cap_column_widths_no_shrink_when_it_fits() {
        let natural = vec![4, 6, 10];
        assert_eq!(cap_column_widths(&natural, 50), vec![4, 6, 10]);
    }

    #[test]
    fn cap_column_widths_respects_min_floor() {
        // If available is absurdly small, columns bottom out at TABLE_COLUMN_MIN_WIDTH
        // (unless their natural width is already below that — those stay at natural).
        let natural = vec![3, 50, 50]; // 3 is below the floor; leave it alone
        let capped = cap_column_widths(&natural, 5);
        assert_eq!(capped[0], 3);
        assert_eq!(capped[1], TABLE_COLUMN_MIN_WIDTH);
        assert_eq!(capped[2], TABLE_COLUMN_MIN_WIDTH);
    }
    // --- distribute_spans_across_lines ---
    // Tests use Style::default() for "plain" spans and a non-default modifier
    // (BOLD) as a proxy for any styled span (links/bold/code/etc.), matching
    // how `parse_inline_formatting` emits them.
    fn plain_color() -> ratatui::style::Color {
        ratatui::style::Color::Reset
    }
    fn plain(content: &'static str) -> Span<'static> {
        Span::styled(content.to_string(), Style::default())
    }
    fn atomic(content: &'static str) -> Span<'static> {
        // Any non-default style qualifies the span as "atomic" to our logic.
        Span::styled(content.to_string(), Style::default().add_modifier(Modifier::BOLD))
    }
    fn line_text(line: &[Span<'static>]) -> String {
        line.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn distribute_plain_text_wraps_at_word_boundary() {
        // "alpha beta gamma delta" at width 10: "alpha beta" = 10 fits, "gamma delta" = 11
        // does not, so "gamma" and "delta" each get their own line.
        let lines = distribute_spans_across_lines(vec![plain("alpha beta gamma delta")], 10, plain_color());
        let texts: Vec<String> = lines.iter().map(|l| line_text(l)).collect();
        assert_eq!(texts, vec!["alpha beta".to_string(), "gamma".to_string(), "delta".to_string()]);
    }

    #[test]
    fn distribute_hard_breaks_over_wide_plain_word() {
        let lines = distribute_spans_across_lines(vec![plain("supercalifragilisticexpialidocious")], 10, plain_color());
        assert!(lines.len() >= 4);
        for l in &lines {
            let width: usize = l.iter().map(|s| UnicodeWidthStr::width(s.content.as_ref())).sum();
            assert!(width <= 10, "line {:?} exceeds width", line_text(l));
        }
    }

    #[test]
    fn distribute_keeps_atomic_span_on_one_line_even_when_wider_than_column() {
        // A styled span wider than the column is accepted as overflow — splitting its
        // content would corrupt the rendered markdown construct.
        let lines = distribute_spans_across_lines(vec![atomic("VeryLongStyledContent")], 10, plain_color());
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "VeryLongStyledContent");
    }

    #[test]
    fn distribute_packs_plain_then_atomic_on_same_line_when_it_fits() {
        // "see" (plain, 3) + "blog" (atomic, 4) -> "see blog" on one line, width 20.
        let lines = distribute_spans_across_lines(vec![plain("see "), atomic("blog")], 20, plain_color());
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "see blog");
    }

    #[test]
    fn distribute_breaks_to_new_line_when_atomic_would_overflow() {
        // "a short prefix " (plain, 15 incl. trailing space) + "XXXXXXX" atomic (7):
        // 15+7=22 > 18 budget, so atomic starts on a new line. The plain span's
        // trailing space is preserved on line 1 (invisible when rendered).
        let lines = distribute_spans_across_lines(vec![plain("a short prefix "), atomic("XXXXXXX")], 18, plain_color());
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]).trim_end(), "a short prefix");
        assert_eq!(line_text(&lines[1]), "XXXXXXX");
    }

    #[test]
    fn distribute_empty_input_returns_one_empty_line() {
        let lines = distribute_spans_across_lines(Vec::new(), 10, plain_color());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].is_empty());
    }

    #[test]
    fn distribute_width_zero_returns_one_line_owned() {
        // When width is 0 we don't wrap — caller decides how to handle.
        let lines = distribute_spans_across_lines(vec![plain("hello world")], 0, plain_color());
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn split_cell_by_br_basic_variants() {
        assert_eq!(split_cell_by_br("no break"), vec!["no break"]);
        assert_eq!(split_cell_by_br("a<br>b"), vec!["a", "b"]);
        assert_eq!(split_cell_by_br("a<br/>b"), vec!["a", "b"]);
        assert_eq!(split_cell_by_br("a<br />b"), vec!["a", "b"]);
    }

    #[test]
    fn split_cell_by_br_case_insensitive() {
        assert_eq!(split_cell_by_br("A<BR>B"), vec!["A", "B"]);
        assert_eq!(split_cell_by_br("A<Br/>B"), vec!["A", "B"]);
    }

    #[test]
    fn split_cell_by_br_multiple_and_empty_segments() {
        assert_eq!(split_cell_by_br("<br>head<br>mid<br>"), vec!["", "head", "mid", ""]);
    }

    #[test]
    fn split_cell_by_br_malformed_tag_passes_through() {
        // No closing `>` — treat literally.
        assert_eq!(split_cell_by_br("a<br b"), vec!["a<br b"]);
        // Different tag — not a break.
        assert_eq!(split_cell_by_br("a<brief>b"), vec!["a<brief>b"]);
    }

    #[test]
    fn distribute_does_not_inject_space_between_atomic_and_adjacent_punctuation() {
        // Reproduces the `two kernels: \`gate+up\`, then \`down\`` case. Previously
        // the flatten-to-words step lost the fact that "," had no leading space,
        // and we injected one, bumping the visible width and forcing an extra wrap.
        // With span-preserving distribution, no space is injected.
        let spans = vec![plain("two kernels: "), atomic("gate+up"), plain(", then "), atomic("down")];
        let lines = distribute_spans_across_lines(spans, 31, plain_color());
        assert_eq!(lines.len(), 1, "got {:?}", lines.iter().map(|l| line_text(l)).collect::<Vec<_>>());
        assert_eq!(line_text(&lines[0]), "two kernels: gate+up, then down");
    }
    // Issue #59: a code line whose aligned trailing comment is wide CJK text wraps to
    // multiple rows. The layout row budget must equal what render_code_line actually
    // draws, or the wrapped tail is clipped and the comment text vanishes in view mode.
    fn cjk_aligned_comment_line() -> String {
        // `return view;` + alignment padding + a wide CJK trailing comment.
        format!("    return view;{}// 3. 函数结束，str 被销毁，view 变成了悬垂指针", " ".repeat(30))
    }

    #[test]
    fn code_line_height_matches_rendered_rows() {
        let theme = Theme::default();
        let line = cjk_aligned_comment_line();
        // Width chosen so the alignment padding still fits on row 1 (the renderer keeps
        // it, pushing the comment to wrap). The old calc_wrapped_height collapsed that
        // padding and budgeted a single row here — the regression this guards.
        let inner_width: u16 = 70;
        // Spans built exactly as render_code_line does for an un-highlighted line.
        let spans = vec![plain("  "), Span::styled("│ ", Style::default()), Span::styled(expand_tabs(&line), Style::default())];
        let rendered_rows = wrap_line_for_cursor(spans, inner_width as usize - 1, &theme).len();
        assert!(rendered_rows >= 2, "scenario must wrap to multiple rows, got {rendered_rows}");
        assert_eq!(code_line_height(&line, None, inner_width, &theme) as usize, rendered_rows, "row budget must equal the rendered row count, else the wrapped tail is clipped",);
    }

    #[test]
    fn code_line_wrap_preserves_cjk_tail() {
        let theme = Theme::default();
        let line = cjk_aligned_comment_line();
        let spans = vec![plain("  "), Span::styled("│ ", Style::default()), Span::styled(expand_tabs(&line), Style::default())];
        let joined: String = wrap_line_for_cursor(spans, 59, &theme).iter().flat_map(|l| l.spans.iter()).map(|s| s.content.as_ref()).collect();
        assert!(joined.contains("函数结束"), "wrap lost the CJK head: {joined:?}");
        assert!(joined.contains("变成了悬垂指针"), "wrap lost the CJK tail: {joined:?}");
    }

    #[test]
    fn wrapped_click_maps_to_second_row_logical_column() {
        let theme = Theme::default();
        let spans = vec![plain("  "), atomic("a"), plain(" padding padding "), atomic("b")];
        // With 10 content columns, the renderer produces:
        //   row 0: "  a padding"
        //   row 1: "  padding b"
        // The boundary space after row 0 is trimmed visually but remains part
        // of the unwrapped coordinate space before `b`.
        let b_col = rendered_col_for_wrapped_click(spans.clone(), 12, 1, 10, &theme);
        assert_eq!(b_col, Some(20));
        // The same x coordinate on row 0 maps to the first row, not to `b`.
        let first_row_col = rendered_col_for_wrapped_click(spans, 12, 0, 10, &theme);
        assert_eq!(first_row_col, Some(10));
    }

    #[test]
    fn wrapped_click_maps_each_half_of_wide_character() {
        let theme = Theme::default();
        let spans = vec![plain("  "), plain("padding padding "), atomic("界")];
        // `界` is two terminal cells and wraps onto the continuation row.
        assert_eq!(rendered_col_for_wrapped_click(spans.clone(), 12, 1, 10, &theme), Some(18));
        assert_eq!(rendered_col_for_wrapped_click(spans, 12, 1, 11, &theme), Some(19));
    }
}
