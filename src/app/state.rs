use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use ekphos_core::NoteId;
use image::DynamicImage;
use ratatui::{
    layout::{Rect, Size},
    style::Style,
    widgets::{Block, Borders, ListState},
};
use ratatui_image::{picker::Picker, sliced::SlicedProtocol};

use crate::config::{Config, Theme, ThemeEntry, ThemeFile};
use crate::highlight::Highlighter;
use crate::highlight_worker::{HighlightColors, HighlightResult, HighlightWorker};
use crate::keybindings::{KeybindingFallback, KeybindingWarning, Keymap};
use ekphos_editor::{Editor, Position};
use ekphos_graph as graph;
use ekphos_graph::{GraphEdge, GraphFilter, GraphIndex, GraphLinkScope, GraphMode, GraphNode, GraphSourceMetadata};
use ekphos_search as search;
use ekphos_search::{SearchHit, SearchIndex, SearchWorker};
use ekphos_vim::VimState;

mod session_types;
pub use session_types::*;

use super::welcome_notes::{DEMO_NOTE_CONTENT, GETTING_STARTED_CONTENT};

/// Convert a heading into a link-fragment slug: lowercased, whitespace
/// collapsed to dashes, punctuation stripped (GitHub-style). Matches the
/// `[text](./file.md#sub-section1)` form used for jumping to headings.
fn slugify_heading(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for ch in s.trim().chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            last_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Decode `%XX` escapes in a URL fragment, leaving other bytes intact.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Normalize a Markdown image destination before using it for I/O.
///
/// CommonMark permits destinations containing spaces when they are enclosed in
/// angle brackets, and paths are commonly URL-encoded. Remote URLs keep their
/// percent escapes because the HTTP client expects a URL rather than a local
/// filesystem path.
pub(crate) fn normalize_image_destination(destination: &str) -> String {
    let destination = destination.strip_prefix('<').and_then(|inner| inner.strip_suffix('>')).unwrap_or(destination);

    if destination.starts_with("http://") || destination.starts_with("https://") {
        destination.to_string()
    } else {
        percent_decode(destination)
    }
}

/// If the line is a markdown ATX heading (`#` through `######`), return the
/// heading text with any trailing `#`s stripped.
fn heading_text(line: &str) -> Option<&str> {
    ekphos_core::markdown::heading(line.trim_start()).map(|heading| heading.text)
}

fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(".")).join(".cache"))
        .join("ekphos")
}

fn last_note_path(cache_dir: &std::path::Path) -> PathBuf {
    cache_dir.join("last_note")
}

fn read_last_opened_note(cache_dir: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_to_string(last_note_path(cache_dir))
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.exists())
}

fn save_last_opened_note(cache_dir: &std::path::Path, path: &std::path::Path) {
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(last_note_path(cache_dir), path.to_string_lossy().as_bytes());
}

/// Return the destination when a source line contains exactly one Markdown image.
/// Images mixed with text or followed by another image stay on the prose line so
/// the content renderer can lay them out as inline, wrapping thumbnails.
fn standalone_image_path(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("![") || trimmed.starts_with("!![") {
        return None;
    }

    let bracket_end = trimmed[1..].find("](")?;
    let destination_start = 1 + bracket_end + 2;
    let paren_end = trimmed[destination_start..].find(')')?;
    let destination_end = destination_start + paren_end;

    if destination_end + 1 != trimmed.len() {
        return None;
    }

    let path = &trimmed[destination_start..destination_end];
    (!path.is_empty()).then_some(path)
}

fn is_inside_inline_code(text: &str, position: usize) -> bool {
    text[..position].chars().filter(|&ch| ch == '`').count() % 2 == 1
}

mod interaction_types;
pub use interaction_types::*;

mod app_data;
pub use app_data::*;

mod dependencies;
pub use dependencies::*;

enum AppLaunch {
    Configured { first_launch: bool },
    ExplicitPath { target_file: Option<PathBuf> },
}

struct AppBuilder {
    config: Config,
    launch: AppLaunch,
    dependencies: AppDependencies,
}

impl AppBuilder {
    fn configured(config: Config, first_launch: bool) -> Self {
        Self {
            config,
            launch: AppLaunch::Configured { first_launch },
            dependencies: AppDependencies::production(),
        }
    }

    fn explicit(config: Config, target_file: Option<PathBuf>) -> Self {
        Self {
            config,
            launch: AppLaunch::ExplicitPath { target_file },
            dependencies: AppDependencies::production(),
        }
    }

    fn injected(config: Config, target_file: Option<PathBuf>, dependencies: AppDependencies) -> Self {
        Self {
            config,
            launch: AppLaunch::ExplicitPath { target_file },
            dependencies,
        }
    }

    fn build(self) -> App {
        let config = self.config;
        let dependencies = self.dependencies;
        let (keymap, keybinding_warning) = match Keymap::from_config(&config.keybindings) {
            Ok(keymap) => (keymap, None),
            Err(error) => (Keymap::default(), Some(KeybindingWarning::new(error, KeybindingFallback::Defaults))),
        };

        let theme = Theme::from_name_in(&config.theme, &Config::themes_dir_in(&dependencies.config_dir));
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut editor = Editor::new_with_clipboard(vec![String::new()], Arc::clone(&dependencies.clipboard));
        editor.set_line_wrap(config.editor.line_wrap);
        editor.set_tab_width(config.editor.tab_width);
        editor.set_padding(config.editor.left_padding, config.editor.right_padding);
        editor.set_line_number_mode(config.editor.line_numbers);
        editor.set_scrolloff(config.editor.scrolloff as usize);
        editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.primary))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        editor.set_cursor_line_style(Style::default());
        editor.set_selection_style(Style::default().fg(theme.foreground).bg(theme.selection));

        let notes_dir_exists = config.notes_path().exists();
        let notes_dir_empty = notes_dir_exists && !App::directory_has_notes(&config.notes_path());
        let (dialog, show_welcome, should_load, target_file) = match self.launch {
            AppLaunch::Configured { first_launch } => {
                let dialog = if first_launch {
                    DialogState::Onboarding
                } else if !notes_dir_exists {
                    DialogState::DirectoryNotFound
                } else if notes_dir_empty {
                    DialogState::EmptyDirectory
                } else {
                    DialogState::None
                };
                (
                    dialog,
                    !first_launch && config.welcome_shown && notes_dir_exists && !notes_dir_empty,
                    !first_launch && notes_dir_exists,
                    None,
                )
            }
            AppLaunch::ExplicitPath { target_file } => {
                let dialog = if !notes_dir_exists {
                    DialogState::DirectoryNotFound
                } else if notes_dir_empty {
                    DialogState::EmptyDirectory
                } else {
                    DialogState::None
                };
                (dialog, false, notes_dir_exists, target_file)
            }
        };

        let input_buffer = config.notes_dir.clone();
        let sidebar_collapsed = config.sidebar_collapsed;
        let outline_collapsed = config.outline_collapsed;
        let frontmatter_hidden = config.frontmatter_hidden;
        let (image_sender, image_receiver) = mpsc::channel();
        let (highlighter_sender, highlighter_receiver) = mpsc::channel();
        let (_, index_receiver) = mpsc::channel();
        let (_, graph_index_receiver) = mpsc::channel();
        let (_, graph_layout_receiver) = mpsc::channel();

        let mut app = App {
            vault: ekphos_vault::Vault::default(),
            body_cache: ekphos_vault::BodyCache::default(),
            active_note_id: None,
            active_fingerprint: None,
            active_body: None,
            document_generation: 0,
            notes: Vec::new(),
            selected_note: 0,
            list_state,
            focus: Focus::Sidebar,
            mode: Mode::Normal,
            editor,
            picker: Picker::from_query_stdio().ok(),
            image_cache_dir: get_image_cache_dir(&dependencies.cache_dir),
            image_states: HashMap::new(),
            pending_images: HashSet::new(),
            image_sender,
            image_receiver,
            show_welcome,
            outline: Vec::new(),
            outline_state: ListState::default(),
            vim_mode: VimMode::Normal,
            vim: VimState::new(),
            visual_line_anchor: None,
            visual_line_current: None,
            visual_block_anchor: None,
            block_insert_state: None,
            content_cursor: 0,
            content_scroll_offset: 0,
            floating_cursor_mode: config.floating_cursor,
            content_items: Vec::new(),
            content_item_source_lines: Vec::new(),
            theme,
            config,
            dialog,
            input_buffer,
            search_active: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
            editor_scroll_top: 0,
            editor_view_height: 0,
            pending_operator: None,
            pending_delete: None,
            file_tree: Vec::new(),
            sidebar_items: Vec::new(),
            selected_sidebar_index: 0,
            folder_states: HashMap::new(),
            target_folder: None,
            dialog_error: None,
            search_matched_notes: Vec::new(),
            pre_search_folder_states: None,
            pre_search_sidebar_index: None,
            content_area: Rect::default(),
            sidebar_area: Rect::default(),
            outline_area: Rect::default(),
            mouse_hover_item: None,
            content_item_rects: Vec::new(),
            inline_image_rects: Vec::new(),
            mouse_hover_inline_image: None,
            selected_link_index: 0,
            details_open_states: HashMap::new(),
            heading_fold_states: HashMap::new(),
            highlighter: None,
            highlighter_loading: false,
            highlighter_sender,
            highlighter_receiver,
            sidebar_collapsed,
            outline_collapsed,
            zen_mode: false,
            mouse_button_held: false,
            mouse_drag_start: None,
            last_mouse_y: 0,
            editor_area: Rect::default(),
            context_menu_state: ContextMenuState::None,
            wiki_autocomplete: WikiAutocompleteState::None,
            pending_wiki_target: None,
            needs_full_clear: false,
            keymap,
            keybinding_warning,
            status_message: None,
            toast: None,
            buffer_search: BufferSearchState::new(),
            help_scroll: 0,
            graph_view: GraphViewState::default(),
            graph_index: None,
            graph_index_receiver,
            graph_index_generation: 0,
            graph_indexing: false,
            graph_layout_receiver,
            graph_layout_generation: 0,
            sort_mode: SortMode::default(),
            navigation_history: Vec::new(),
            navigation_index: 0,
            frontmatter_hidden,
            theme_picker: ThemePicker::default(),
            search_picker: SearchPickerState::Closed,
            search_picker_area: Rect::default(),
            search_picker_results_area: Rect::default(),
            search_picker_last_click: None,
            next_search_id: 0,
            search_generation: 0,
            search_generation_signal: Arc::new(AtomicU64::new(0)),
            search_worker: None,
            search_index: None,
            index_receiver,
            indexing_in_progress: false,
            index_progress: Arc::new(AtomicUsize::new(0)),
            index_total: Arc::new(AtomicUsize::new(0)),
            index_started_at: None,
            cut_buffer: None,
            highlight_worker: Some(HighlightWorker::new()),
            highlight_version: 0,
            highlight_pending: false,
            dependencies,
        };

        if should_load {
            app.load_notes_from_dir();
            if let Some(target_path) = target_file {
                app.select_note_by_path(&target_path);
            } else if let Some(last_path) = read_last_opened_note(&app.dependencies.cache_dir) {
                app.select_note_by_path(&last_path);
            }
        }

        app
    }
}

mod document;
mod editing;
mod graph_state;
mod lifecycle;
mod links;
mod memory;
pub use memory::*;
mod search_state;
mod services;
mod ui_state;
mod vault;
fn fetch_remote_image_blocking(url: &str) -> Option<DynamicImage> {
    use std::io::Read;

    let response = ureq::get(url).set("User-Agent", "ekphos/0.4").call().ok()?;

    let content_type = response.header("Content-Type").unwrap_or("").to_lowercase();

    if !content_type.starts_with("image/") {
        return None;
    }

    let mut bytes = Vec::new();
    response.into_reader().take(10 * 1024 * 1024).read_to_end(&mut bytes).ok()?;

    image::load_from_memory(&bytes).ok()
}
fn get_image_cache_dir(cache_dir: &std::path::Path) -> PathBuf {
    let dir = cache_dir.join("images");
    let _ = fs::create_dir_all(&dir);
    dir
}
fn cache_key_to_filename(key: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:x}.png", hasher.finish())
}
fn resize_for_cache(img: DynamicImage) -> DynamicImage {
    const MAX_DIMENSION: u32 = 300;
    let (width, height) = (img.width(), img.height());
    if width <= MAX_DIMENSION && height <= MAX_DIMENSION {
        return img;
    }

    let scale = if width > height {
        MAX_DIMENSION as f32 / width as f32
    } else {
        MAX_DIMENSION as f32 / height as f32
    };

    let new_width = (width as f32 * scale) as u32;
    let new_height = (height as f32 * scale) as u32;

    img.resize(new_width, new_height, image::imageops::FilterType::Triangle)
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// fuzzy matching algorithm that scores matches based on:
///
/// - Empty query matches everything with base score.
/// - Exact match: highest score.
/// - Prefix match: high score.
/// - Consecutive character matches: bonus points.
/// - Earlier matches in the string: bonus points.
/// returns None if no match, Some(score) if matched
fn fuzzy_match(text: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    let text_chars: Vec<char> = text_lower.chars().collect();
    let query_chars: Vec<char> = query_lower.chars().collect();

    if text_lower == query_lower {
        return Some(1000);
    }

    if text_lower.starts_with(&query_lower) {
        return Some(900 + (100 - text.len() as i32).max(0));
    }

    if text_lower.contains(&query_lower) {
        let pos = text_lower.find(&query_lower).unwrap_or(0);
        return Some(500 + (50 - pos as i32).max(0));
    }

    let mut text_idx = 0;
    let mut query_idx = 0;
    let mut score: i32 = 0;
    let mut prev_matched = false;
    let mut consecutive_bonus = 0;

    while text_idx < text_chars.len() && query_idx < query_chars.len() {
        if text_chars[text_idx] == query_chars[query_idx] {
            score += (100 - text_idx as i32).max(1);
            if prev_matched {
                consecutive_bonus += 20;
            }

            if text_idx == 0 || matches!(text_chars.get(text_idx.saturating_sub(1)), Some(' ' | '_' | '-')) {
                score += 30;
            }

            prev_matched = true;
            query_idx += 1;
        } else {
            prev_matched = false;
        }
        text_idx += 1;
    }

    if query_idx == query_chars.len() {
        Some(score + consecutive_bonus)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase3_note_layout_is_compact() {
        assert!(std::mem::size_of::<Note>() <= 136);
        assert!(std::mem::size_of::<FileTreeItem>() <= 16);
        assert!(std::mem::size_of::<SidebarItem>() <= 48);
    }

    #[test]
    fn standalone_image_requires_exactly_one_image_on_the_source_line() {
        assert_eq!(standalone_image_path("![hero](hero.png)"), Some("hero.png"));
        assert_eq!(standalone_image_path("  ![hero](hero.png)  "), Some("hero.png"));
        assert_eq!(standalone_image_path("![one](1.png) ![two](2.png)"), None);
        assert_eq!(standalone_image_path("caption ![hero](hero.png)"), None);
        assert_eq!(standalone_image_path("![hero](hero.png) caption"), None);
        assert_eq!(standalone_image_path("!![hero](hero.png)"), None);
    }

    #[test]
    fn inline_images_keep_source_order_and_zero_width_text_positions() {
        let images = App::inline_image_links_in_text("before ![one](1.png) and ![two](2.png)");

        assert_eq!(images, vec![("1.png".to_string(), 7), ("2.png".to_string(), 12)]);
    }

    #[test]
    fn inline_images_ignore_text_only_and_code_syntax() {
        let images = App::inline_image_links_in_text("!![text only](one.png) `![code](two.png)` ![preview](three.png)");

        assert_eq!(images.len(), 1);
        assert_eq!(images[0].0, "three.png");
    }

    #[test]
    fn every_inline_image_has_its_own_selection_index() {
        let links = vec![
            LinkInfo::Markdown {
                text: "docs".to_string(),
                url: "docs.md".to_string(),
                start_col: 0,
                end_col: 4,
            },
            LinkInfo::Image {
                path: "one.png".to_string(),
                start_col: 5,
                end_col: 5,
            },
            LinkInfo::Image {
                path: "two.png".to_string(),
                start_col: 6,
                end_col: 6,
            },
        ];

        assert_eq!(
            App::inline_image_selections_for_links(links.clone(), false),
            vec![("one.png".to_string(), 1), ("two.png".to_string(), 2)]
        );
        assert_eq!(
            App::inline_image_selections_for_links(links, true),
            vec![("one.png".to_string(), 2), ("two.png".to_string(), 3)]
        );
    }

    #[test]
    fn normalize_image_destination_supports_commonmark_local_paths() {
        assert_eq!(
            normalize_image_destination("<attachments/Pasted image 20250916224004.png>"),
            "attachments/Pasted image 20250916224004.png"
        );
        assert_eq!(
            normalize_image_destination("attachments/Pasted%20image%2020250916224004.png"),
            "attachments/Pasted image 20250916224004.png"
        );
        assert_eq!(normalize_image_destination("<attachments/Pasted%20image.png>"), "attachments/Pasted image.png");
    }

    #[test]
    fn normalize_image_destination_preserves_remote_url_escapes() {
        assert_eq!(
            normalize_image_destination("<https://example.com/Pasted%20image.png>"),
            "https://example.com/Pasted%20image.png"
        );
    }

    #[test]
    fn normalize_image_destination_strips_only_one_angle_pair() {
        assert_eq!(normalize_image_destination("<<image.png>>"), "<image.png>");
        assert_eq!(normalize_image_destination("<image.png"), "<image.png");
    }

    #[test]
    fn alignment_from_separator_cell_classifies_each_form() {
        assert_eq!(Alignment::from_separator_cell("---"), Alignment::Left);
        assert_eq!(Alignment::from_separator_cell(":---"), Alignment::Left);
        assert_eq!(Alignment::from_separator_cell("---:"), Alignment::Right);
        assert_eq!(Alignment::from_separator_cell(":---:"), Alignment::Center);
        // Surrounding whitespace should not change classification.
        assert_eq!(Alignment::from_separator_cell("  :---:  "), Alignment::Center);
    }

    #[test]
    fn extract_simple_table_links_single_link_in_second_cell() {
        // Row: "| Name | [Top 5](https://x.test) |"
        // Cells (already trimmed during parse): ["Name", "[Top 5](https://x.test)"]
        // Column widths follow visible width: cell 0 = 4, cell 1 = 6 ("Top 5").
        let cells = vec!["Name".to_string(), "[Top 5](https://x.test)".to_string()];
        let widths = vec![4, 6];
        let alignments = vec![Alignment::Left, Alignment::Left];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);

        assert_eq!(links.len(), 1);
        let (label, url, start, end) = &links[0];
        assert_eq!(label, "Top 5");
        assert_eq!(url, "https://x.test");
        // Layout within content area (prefix `  │` not counted):
        //   cell 0 occupies " Name " (cols 0..=5), "│" at 6, cell 1 opens at 7 with " " leading.
        //   Left-aligned, so label starts at 7 + 1 = 8.
        assert_eq!(*start, 8);
        assert_eq!(*end, 8 + "Top 5".chars().count());
    }

    #[test]
    fn extract_simple_table_links_respects_right_alignment() {
        // Right-aligned cell: label sits flush against the right edge.
        // Cells: ["X", "[a](u)"]; widths: [3, 5]; alignment: [Left, Right].
        // Cell 0 occupies " X   " (1 + width 3 + 1 = 5 chars) + "│" -> col_cursor = 6.
        // Cell 1 visible = 1 ("a"), pad = 4, Right -> left_pad = 4.
        // Link starts at col_cursor(6) + 1 (leading space) + 4 (left_pad) = 11.
        let cells = vec!["X".to_string(), "[a](u)".to_string()];
        let widths = vec![3, 5];
        let alignments = vec![Alignment::Left, Alignment::Right];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "a");
        assert_eq!(links[0].2, 11);
        assert_eq!(links[0].3, 12);
    }

    #[test]
    fn extract_simple_table_links_ignores_wiki_link() {
        // `[[wiki]]` is not a markdown link; should not be emitted here.
        let cells = vec!["X".to_string(), "[[wiki]]".to_string()];
        let widths = vec![3, 4];
        let alignments = vec![Alignment::Left, Alignment::Left];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_simple_table_links_skips_link_with_empty_url() {
        let cells = vec!["[label]()".to_string()];
        let widths = vec![5];
        let alignments = vec![Alignment::Left];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_simple_table_links_bare_url_in_cell() {
        // Cell 0 occupies " X   " + "│" -> col_cursor=6.
        // Cell 1 (Left): URL starts at 6 + 1 + 0 = 7, ends at 7 + 19 = 26.
        let cells = vec!["X".to_string(), "https://example.com".to_string()];
        let widths = vec![3, 19];
        let alignments = vec![Alignment::Left, Alignment::Left];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "https://example.com");
        assert_eq!(links[0].1, "https://example.com");
        assert_eq!(links[0].2, 7);
        assert_eq!(links[0].3, 26);
    }

    #[test]
    fn extract_simple_table_links_emits_both_bracket_and_bare_in_same_cell() {
        // A cell with both a bracket link and a trailing bare URL emits both,
        // in source order. Bracket link's URL is not re-emitted as a bare URL.
        let cells = vec!["[label](https://a) https://b.test".to_string()];
        let widths = vec![33];
        let alignments = vec![Alignment::Left];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "label");
        assert_eq!(links[0].1, "https://a");
        assert_eq!(links[1].0, "https://b.test");
        assert_eq!(links[1].1, "https://b.test");
    }

    #[test]
    fn extract_simple_table_links_multiple_bracket_links_in_same_cell() {
        // Multiple `[text](url)` in one cell should all be emitted.
        let cells = vec!["[alpha](u1) and [beta](u2)".to_string()];
        let widths = vec![16];
        let alignments = vec![Alignment::Left];
        let links = App::extract_simple_table_links(&cells, &widths, &alignments);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "alpha");
        assert_eq!(links[1].0, "beta");
    }
}
