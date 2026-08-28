mod content;
mod context_menu;
mod dialogs;
mod editor;
mod file_picker;
mod graph_view;
mod outline;
mod search_dialog;
mod sidebar;
mod status_bar;
mod theme_picker;
mod toast;
mod wiki_autocomplete;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::{Block, Widget},
    Frame,
};

use crate::app::{App, ContextMenuState, DialogState, Mode, SearchPickerState, WikiAutocompleteState};
use crate::config::Config;
fn main_layout_constraints(zen_mode: bool, sidebar_collapsed: bool, outline_collapsed: bool, sidebar_width_percent: u16, outline_width_percent: u16) -> [Constraint; 3] {
    let sidebar_constraint = if zen_mode {
        Constraint::Length(0)
    } else if sidebar_collapsed || sidebar_width_percent < Config::MINIMIZED_PANEL_WIDTH_PERCENT {
        Constraint::Length(5)
    } else {
        Constraint::Percentage(sidebar_width_percent)
    };
    let outline_constraint = if zen_mode {
        Constraint::Length(0)
    } else if outline_collapsed || outline_width_percent < Config::MINIMIZED_PANEL_WIDTH_PERCENT {
        Constraint::Length(5)
    } else {
        Constraint::Percentage(outline_width_percent)
    };
    [sidebar_constraint, Constraint::Min(20), outline_constraint]
}

pub(crate) use content::content_item_click_col;
pub use content::render_content;
pub use dialogs::{
    render_create_folder_dialog, render_create_note_dialog, render_create_note_in_folder_dialog, render_create_wiki_note_dialog, render_delete_confirm_dialog, render_delete_folder_confirm_dialog, render_directory_not_found_dialog, render_empty_directory_dialog, render_help_dialog,
    render_keybinding_warning, render_onboarding_dialog, render_rename_folder_dialog, render_rename_note_dialog, render_unsaved_changes_dialog, render_welcome_dialog,
};
pub use outline::{render_outline, OutlineView};
pub use sidebar::{render_sidebar, SidebarView};
pub use status_bar::render_status_bar;

pub fn render(f: &mut Frame, app: &mut App) {
    if !app.state.config.transparent_bg {
        let bg = Block::default().style(Style::default().bg(app.state.theme.background));
        bg.render(f.area(), f.buffer_mut());
    }
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main area
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());
    let main_constraints = main_layout_constraints(app.state.zen_mode, app.state.sidebar_collapsed, app.state.outline_collapsed, app.state.config.effective_sidebar_width_percent(), app.state.config.effective_outline_width_percent());
    let chunks = Layout::default().direction(Direction::Horizontal).constraints(main_constraints).split(vertical_chunks[0]);
    let sidebar_area = render_sidebar(f, SidebarView { theme: &app.state.theme, vault: &app.vault, search: &app.search, focus: app.state.focus, mode: app.editor.mode, minimized: app.is_sidebar_minimized() }, chunks[0]);
    match app.editor.mode {
        Mode::Normal => render_content(f, app, chunks[1]),
        Mode::Edit => {
            let layout = editor::editor_layout(app.state.zen_mode, chunks[1]);
            app.editor.editor_area = layout.area;
            app.editor.set_view_size(layout.inner_width, layout.inner_height);
            app.update_editor_scroll(layout.inner_height);
            editor::render_editor(f, editor::EditorView { theme: &app.state.theme, editor: &app.editor, editing_mode: app.state.config.editor.mode, keymap: &app.state.keymap, zen_mode: app.state.zen_mode }, layout);
        }
    }
    let outline = render_outline(f, OutlineView { theme: &app.state.theme, document: &app.document, snapshot: app.document(), editor: &app.editor, focus: app.state.focus, minimized: app.is_outline_minimized() }, chunks[2]);
    app.state.sidebar_area = sidebar_area;
    app.state.outline_area = outline.area;
    app.document.outline_state = outline.state;
    render_status_bar(f, app, vertical_chunks[1]);
    match app.state.dialog {
        DialogState::Onboarding => render_onboarding_dialog(f, app),
        DialogState::CreateNote => render_create_note_dialog(f, app),
        DialogState::CreateFolder => render_create_folder_dialog(f, app),
        DialogState::CreateNoteInFolder => render_create_note_in_folder_dialog(f, app),
        DialogState::DeleteConfirm => render_delete_confirm_dialog(f, app),
        DialogState::DeleteFolderConfirm => render_delete_folder_confirm_dialog(f, app),
        DialogState::RenameNote => render_rename_note_dialog(f, app),
        DialogState::RenameFolder => render_rename_folder_dialog(f, app),
        DialogState::Help => app.state.help_scroll = render_help_dialog(f, app),
        DialogState::EmptyDirectory => render_empty_directory_dialog(f, app),
        DialogState::DirectoryNotFound => render_directory_not_found_dialog(f, app),
        DialogState::UnsavedChanges => render_unsaved_changes_dialog(f, app),
        DialogState::CreateWikiNote => render_create_wiki_note_dialog(f, app),
        DialogState::GraphView => {
            let action = graph_view::prepare_graph_view(app, f.area());
            let view = &mut app.graph.graph_view;
            view.graph_area = action.area;
            view.view_width = action.view_width;
            view.view_height = action.view_height;
            if let Some((x, y, zoom)) = action.camera {
                view.viewport_x = x;
                view.viewport_y = y;
                view.zoom = zoom;
            }
            if action.clear_dirty {
                view.dirty = false;
            }
            if action.clear_needs_center {
                view.needs_center = false;
            }
            graph_view::render_graph_view(f, app);
        }
        DialogState::ThemeSelector => {
            if let Some(picker) = app.state.theme_picker.as_ref() {
                let scroll = theme_picker::render_theme_picker(f, theme_picker::ThemePickerView { theme: &app.state.theme, picker });
                if let (Some(scroll), Some(picker)) = (scroll, app.state.theme_picker.as_mut()) {
                    picker.scroll_offset = scroll;
                }
            }
        }
        DialogState::None => {
            if app.state.show_welcome {
                render_welcome_dialog(f, &app.state.theme);
            }
        }
    }
    if app.editor.mode == Mode::Edit && app.editor.context_menu_state != ContextMenuState::None {
        context_menu::render_context_menu(f, app);
    }
    if app.editor.mode == Mode::Edit && !matches!(app.editor.wiki_autocomplete, WikiAutocompleteState::None) {
        wiki_autocomplete::render_wiki_autocomplete(f, app);
    }
    if app.search.buffer_search.active {
        search_dialog::render_search_dialog(f, app, app.editor.editor_area);
    }
    if !matches!(app.search.search_picker, SearchPickerState::Closed) {
        app.ensure_search_hydrated();
        if let Some(action) = file_picker::render_search_picker(f, file_picker::SearchPickerView { theme: &app.state.theme, keymap: &app.state.keymap, picker: &app.search.search_picker }) {
            app.search.search_picker_area = action.area;
            app.search.search_picker_results_area = action.results_area;
        }
    }
    if app.state.keybinding_warning.is_some() {
        render_keybinding_warning(f, app);
    }
    toast::render_toast(f, app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppDependencies, DialogState};
    use crate::syntax_service::SyntaxServiceStatus;
    use image::{Rgba, RgbaImage};
    use ratatui::layout::Rect;
    use ratatui::{backend::TestBackend, Terminal};
    use ratatui_image::picker::Picker;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    static NEXT_GOLDEN_ROOT: AtomicU64 = AtomicU64::new(0);
    struct GoldenApp {
        app: App,
        root: PathBuf,
    }
    impl GoldenApp {
        fn new() -> Self {
            Self::with_content("---\ntags: [golden]\n---\n# Golden fixture\n\nA [[fixture]] link.\n\n- [ ] stable task\n")
        }
        fn with_content(content: &str) -> Self {
            let id = NEXT_GOLDEN_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-golden-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            fs::create_dir_all(&vault).unwrap();
            fs::write(vault.join("fixture.md"), content).unwrap();
            let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, ..Default::default() };
            let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            let mut app = App::new_injected(config, vault, None, dependencies);
            app.state.show_welcome = false;
            app.state.dialog = DialogState::None;
            let started = Instant::now();
            while (app.search.indexing_in_progress || app.graph.is_indexing()) && started.elapsed() < Duration::from_secs(5) {
                app.poll_index_build();
                app.poll_graph_workers();
                std::thread::yield_now();
            }
            app.state.config.notes_dir = "/fixture/vault".to_string();
            app.state.input_buffer = "/fixture/vault".to_string();
            if let Some(note) = app.vault.notes.first_mut() {
                note.file_path = Some(PathBuf::from("/fixture/vault/fixture.md"));
            }
            Self { app, root }
        }
        fn hash(&mut self, width: u16, height: u16) -> u64 {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &mut self.app)).unwrap();
            let buffer = terminal.backend().buffer();
            let mut hash = 0xcbf29ce484222325u64;
            for y in 0..height {
                for x in 0..width {
                    for byte in buffer[(x, y)].symbol().as_bytes() {
                        hash ^= u64::from(*byte);
                        hash = hash.wrapping_mul(0x100000001b3);
                    }
                }
                hash ^= u64::from(b'\n');
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash
        }
    }
    impl Drop for GoldenApp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn golden_main_view_100x30() {
        let mut fixture = GoldenApp::new();
        assert_eq!(fixture.hash(100, 30), 2_737_915_595_076_798_973);
    }

    #[test]
    fn phase11_terminal_goldens_cover_tiny_narrow_normal_and_wide_sizes() {
        let sizes = [(20, 8), (60, 18), (100, 30), (160, 50)];
        let actual: Vec<_> = sizes
            .into_iter()
            .map(|(width, height)| {
                let mut fixture = GoldenApp::new();
                fixture.hash(width, height)
            })
            .collect();
        assert_eq!(actual, [11_602_399_305_202_422_691, 4_212_271_247_269_996_847, 2_737_915_595_076_798_973, 8_070_470_284_126_182_397]);
    }

    #[test]
    fn golden_edit_view_80x24() {
        let mut fixture = GoldenApp::new();
        fixture.app.enter_edit_mode();
        assert_eq!(fixture.hash(80, 24), 15_822_003_958_405_314_542);
    }

    #[test]
    fn tiny_edit_geometry_and_mouse_auto_scroll_do_not_panic() {
        let mut fixture = GoldenApp::new();
        fixture.app.enter_edit_mode();
        fixture.app.editor.set_line_wrap(false);
        let backend = TestBackend::new(2, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                let layout = editor::EditorLayout { area, inner_width: 0, inner_height: 0 };
                fixture.app.editor.editor_area = area;
                fixture.app.editor.set_view_size(0, 0);
                editor::render_editor(frame, editor::EditorView { theme: &fixture.app.state.theme, editor: &fixture.app.editor, editing_mode: fixture.app.state.config.editor.mode, keymap: &fixture.app.state.keymap, zen_mode: false }, layout);
            })
            .unwrap();

        fixture.app.editor.editor_area = Rect::new(0, 0, 1, 1);
        assert_eq!(fixture.app.get_auto_scroll_direction(0), 0);
    }

    #[test]
    fn golden_document_snapshot_unicode_tables_links_and_inline_images() {
        let mut fixture = GoldenApp::with_content(
            "---\ntags: [golden, phase6]\ndate: 2026-08-21\n---\n# ASCII and\ttabs\n\nCombining e\u{301}, CJK 日本語, emoji 😀, and a [wide link 開く](https://example.test).\n\nA deliberately long wrapping line keeps ASCII, e\u{301}, 日本語, and 😀 coordinates stable across terminal rows.\n\n- [ ] task with [[fixture|wiki alias]]\n\n| left | centered 日本 | right 😀 |\n|:-----|:-------------:|---------:|\n| e\u{301} | [開く](https://example.test/table) | tabs\there |\n\nText before ![inline](missing.png) and after.\n",
        );
        assert_eq!(fixture.hash(100, 36), 1_420_924_652_973_427_897);
    }

    #[test]
    fn syntect_loads_only_for_a_visible_language_block_and_document_eviction_clears_results() {
        let mut content = String::from("# Lazy syntax\n\n");
        for line in 0..80 {
            content.push_str(&format!("plain line {line}\n"));
        }
        content.push_str("```rust\nfn main() { println!(\"visible\"); }\n```\n");
        let mut fixture = GoldenApp::with_content(&content);
        fixture.hash(70, 12);
        assert_eq!(fixture.app.syntax_service_status(), SyntaxServiceStatus::Unloaded);
        assert_eq!(fixture.app.memory_snapshot().syntax_definition_bytes, 0);
        fixture.app.document.content_cursor = fixture.app.document.content_items.iter().position(|item| matches!(item, crate::app::ContentItem::CodeLine { .. })).unwrap();
        fixture.app.state.focus = crate::app::Focus::Content;
        fixture.hash(70, 12);
        assert_eq!(fixture.app.syntax_service_status(), SyntaxServiceStatus::Loading);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !fixture.app.poll_highlighter() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(fixture.app.syntax_service_status(), SyntaxServiceStatus::Ready);
        fixture.hash(70, 12);
        let loaded = fixture.app.memory_snapshot();
        assert!(loaded.syntax_definition_bytes > 0);
        assert!(loaded.syntax_result_cache_bytes > 0);
        fixture.app.enter_edit_mode();
        let evicted = fixture.app.memory_snapshot();
        assert_eq!(evicted.syntax_definition_bytes, loaded.syntax_definition_bytes);
        assert_eq!(evicted.syntax_result_cache_bytes, 0);
    }

    #[test]
    fn image_protocols_are_viewport_scoped_and_missing_protocol_falls_back_safely() {
        let id = NEXT_GOLDEN_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("ekphos-image-lifecycle-{}-{id}", std::process::id()));
        let vault = root.join("vault");
        fs::create_dir_all(&vault).unwrap();
        let image_path = root.join("fixture.png");
        RgbaImage::from_pixel(32, 24, Rgba([20, 80, 160, 255])).save(&image_path).unwrap();
        let mut note = format!("# Images\n\n![fixture]({})\n", image_path.display());
        for line in 0..80 {
            note.push_str(&format!("plain line {line}\n"));
        }
        fs::write(vault.join("fixture.md"), note).unwrap();
        let config = Config { general: crate::config::GeneralConfig { welcome_shown: false, check_updates: false, ..Default::default() }, ..Default::default() };
        let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
        let mut app = App::new_injected(config, vault, None, dependencies);
        app.images.picker = Some(Picker::halfblocks());
        app.state.focus = crate::app::Focus::Content;
        app.document.content_cursor = app.document.content_items.iter().position(|item| matches!(item, crate::app::ContentItem::Image { .. })).unwrap();
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.image_has_background_work() && Instant::now() < deadline {
            app.poll_pending_images();
            std::thread::yield_now();
        }
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert_eq!(app.images.image_states.len(), 1);
        assert!(app.memory_snapshot().image_protocol_bytes > 0);
        app.document.content_cursor = app.document.content_items.len() - 1;
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert!(app.images.image_states.is_empty());
        assert_eq!(app.memory_snapshot().image_protocol_bytes, 0);
        app.images.picker = None;
        app.document.content_cursor = app.document.content_items.iter().position(|item| matches!(item, crate::app::ContentItem::Image { .. })).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert!(app.images.image_states.is_empty());
        assert!(app.memory_snapshot().image_decoded_bytes > 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unicode_and_tab_link_click_columns_use_terminal_cells() {
        let fixture = GoldenApp::with_content("# Clicks\n\nASCII e\u{301} 日本 😀\t[開く](https://example.test) tail\n");
        let item = fixture.app.document.content_items.iter().position(|item| item.source_line() == 2).unwrap();
        let links = fixture.app.item_links_at(item);
        assert_eq!(links.len(), 1);
        assert_eq!((links[0].2, links[0].3), (19, 23));
        assert_eq!(fixture.app.find_clicked_link_at_col(item, 21).as_deref(), Some("https://example.test"));
        assert_eq!(fixture.app.find_clicked_link_at_col(item, 24).as_deref(), Some("https://example.test"));
        assert_eq!(fixture.app.find_clicked_link_at_col(item, 25), None);
    }

    #[test]
    fn golden_onboarding_dialog_100x30() {
        let mut fixture = GoldenApp::new();
        fixture.app.state.dialog = DialogState::Onboarding;
        assert_eq!(fixture.hash(100, 30), 15_714_349_546_688_206_610);
    }

    #[test]
    fn golden_create_note_dialog_72x22() {
        let mut fixture = GoldenApp::new();
        fixture.app.state.dialog = DialogState::CreateNote;
        fixture.app.state.input_buffer = "deterministic-note".to_string();
        assert_eq!(fixture.hash(72, 22), 16_799_502_509_508_382_863);
    }

    #[test]
    fn default_panel_layout_keeps_twenty_percent_sides_and_center_minimum() {
        let config = Config::default();
        assert_eq!(main_layout_constraints(false, false, false, config.effective_sidebar_width_percent(), config.effective_outline_width_percent(),), [Constraint::Percentage(20), Constraint::Min(20), Constraint::Percentage(20),]);
    }

    #[test]
    fn custom_panel_layout_uses_independent_effective_widths() {
        let config = Config { general: crate::config::GeneralConfig { sidebar_width_percent: 30, outline_width_percent: 140, ..Default::default() }, ..Default::default() };
        assert_eq!(main_layout_constraints(false, false, false, config.effective_sidebar_width_percent(), config.effective_outline_width_percent(),), [Constraint::Percentage(30), Constraint::Min(20), Constraint::Percentage(95),]);
    }

    #[test]
    fn collapsed_panels_override_configured_widths() {
        assert_eq!(main_layout_constraints(false, true, true, 35, 45), [Constraint::Length(5), Constraint::Min(20), Constraint::Length(5),]);
    }

    #[test]
    fn widths_below_ten_percent_use_minimized_constraints() {
        assert_eq!(main_layout_constraints(false, false, false, 9, 5), [Constraint::Length(5), Constraint::Min(20), Constraint::Length(5),]);
        assert_eq!(main_layout_constraints(false, false, false, 10, 10), [Constraint::Percentage(10), Constraint::Min(20), Constraint::Percentage(10),]);
    }

    #[test]
    fn zen_mode_overrides_configured_and_collapsed_widths() {
        assert_eq!(main_layout_constraints(true, true, false, 35, 45), [Constraint::Length(0), Constraint::Min(20), Constraint::Length(0),]);
    }

    #[test]
    fn wide_layout_applies_independent_panel_percentages() {
        let chunks = Layout::default().direction(Direction::Horizontal).constraints(main_layout_constraints(false, false, false, 25, 15)).split(Rect::new(0, 0, 200, 20));
        assert_eq!(chunks[0].width, 50);
        assert_eq!(chunks[1].width, 120);
        assert_eq!(chunks[2].width, 30);
    }

    #[test]
    fn narrow_layout_retains_center_panel_minimum() {
        let chunks = Layout::default().direction(Direction::Horizontal).constraints(main_layout_constraints(false, false, false, 95, 95)).split(Rect::new(0, 0, 40, 20));
        assert!(chunks[1].width >= 20);
        assert_eq!(chunks.iter().map(|chunk| chunk.width).sum::<u16>(), 40);
    }
}
