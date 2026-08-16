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

fn main_layout_constraints(
    zen_mode: bool,
    sidebar_collapsed: bool,
    outline_collapsed: bool,
    sidebar_width_percent: u16,
    outline_width_percent: u16,
) -> [Constraint; 3] {
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
pub(crate) use content::{cell_visible_width, detect_bare_url_len};
pub use dialogs::{
    render_create_folder_dialog, render_create_note_dialog, render_create_note_in_folder_dialog, render_create_wiki_note_dialog, render_delete_confirm_dialog,
    render_delete_folder_confirm_dialog, render_directory_not_found_dialog, render_empty_directory_dialog, render_help_dialog, render_keybinding_warning,
    render_onboarding_dialog, render_rename_folder_dialog, render_rename_note_dialog, render_unsaved_changes_dialog, render_welcome_dialog,
};
pub use editor::render_editor;
pub use outline::render_outline;
pub use sidebar::render_sidebar;
pub use status_bar::render_status_bar;

pub fn render(f: &mut Frame, app: &mut App) {
    if !app.config.transparent_bg {
        let bg = Block::default().style(Style::default().bg(app.theme.background));
        bg.render(f.area(), f.buffer_mut());
    }

    // Create vertical layout: main area + status bar
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main area
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    let main_constraints = main_layout_constraints(
        app.zen_mode,
        app.sidebar_collapsed,
        app.outline_collapsed,
        app.config.effective_sidebar_width_percent(),
        app.config.effective_outline_width_percent(),
    );

    // Create main layout with left sidebar, content, and right outline
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(main_constraints)
        .split(vertical_chunks[0]);

    // Render left sidebar (notes list)
    render_sidebar(f, app, chunks[0]);

    // Render content (either view or edit mode)
    match app.mode {
        Mode::Normal => render_content(f, app, chunks[1]),
        Mode::Edit => render_editor(f, app, chunks[1]),
    }

    // Render right sidebar (outline)
    render_outline(f, app, chunks[2]);

    // Render status bar
    render_status_bar(f, app, vertical_chunks[1]);

    // Render dialogs on top
    match app.dialog {
        DialogState::Onboarding => render_onboarding_dialog(f, app),
        DialogState::CreateNote => render_create_note_dialog(f, app),
        DialogState::CreateFolder => render_create_folder_dialog(f, app),
        DialogState::CreateNoteInFolder => render_create_note_in_folder_dialog(f, app),
        DialogState::DeleteConfirm => render_delete_confirm_dialog(f, app),
        DialogState::DeleteFolderConfirm => render_delete_folder_confirm_dialog(f, app),
        DialogState::RenameNote => render_rename_note_dialog(f, app),
        DialogState::RenameFolder => render_rename_folder_dialog(f, app),
        DialogState::Help => render_help_dialog(f, app),
        DialogState::EmptyDirectory => render_empty_directory_dialog(f, app),
        DialogState::DirectoryNotFound => render_directory_not_found_dialog(f, app),
        DialogState::UnsavedChanges => render_unsaved_changes_dialog(f, app),
        DialogState::CreateWikiNote => render_create_wiki_note_dialog(f, app),
        DialogState::GraphView => graph_view::render_graph_view(f, app),
        DialogState::ThemeSelector => theme_picker::render_theme_picker(f, app),
        DialogState::None => {
            // Render welcome dialog on top if active
            if app.show_welcome {
                render_welcome_dialog(f, &app.theme);
            }
        }
    }

    // Render context menu on top of everything (Edit mode only)
    if app.mode == Mode::Edit && app.context_menu_state != ContextMenuState::None {
        context_menu::render_context_menu(f, app);
    }

    if app.mode == Mode::Edit && !matches!(app.wiki_autocomplete, WikiAutocompleteState::None) {
        wiki_autocomplete::render_wiki_autocomplete(f, app);
    }

    if app.buffer_search.active {
        search_dialog::render_search_dialog(f, app, app.editor_area);
    }

    if !matches!(app.search_picker, SearchPickerState::Closed) {
        file_picker::render_search_picker(f, app);
    }

    if app.keybinding_warning.is_some() {
        render_keybinding_warning(f, app);
    }

    // Toast notifications float above everything else.
    toast::render_toast(f, app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppDependencies, DialogState};
    use ratatui::layout::Rect;
    use ratatui::{backend::TestBackend, Terminal};
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
            let id = NEXT_GOLDEN_ROOT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("ekphos-golden-{}-{id}", std::process::id()));
            let vault = root.join("vault");
            fs::create_dir_all(&vault).unwrap();
            fs::write(
                vault.join("fixture.md"),
                "---\ntags: [golden]\n---\n# Golden fixture\n\nA [[fixture]] link.\n\n- [ ] stable task\n",
            )
            .unwrap();
            let config = Config {
                welcome_shown: false,
                check_updates: false,
                ..Config::default()
            };
            let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
            let mut app = App::new_injected(config, vault, None, dependencies);
            app.show_welcome = false;
            app.dialog = DialogState::None;
            let started = Instant::now();
            while (app.indexing_in_progress || app.graph_indexing) && started.elapsed() < Duration::from_secs(5) {
                app.poll_index_build();
                app.poll_graph_workers();
                std::thread::yield_now();
            }
            app.config.notes_dir = "/fixture/vault".to_string();
            app.input_buffer = "/fixture/vault".to_string();
            if let Some(note) = app.notes.first_mut() {
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
    fn golden_edit_view_80x24() {
        let mut fixture = GoldenApp::new();
        fixture.app.enter_edit_mode();
        assert_eq!(fixture.hash(80, 24), 16_481_934_834_706_604_804);
    }

    #[test]
    fn golden_onboarding_dialog_100x30() {
        let mut fixture = GoldenApp::new();
        fixture.app.dialog = DialogState::Onboarding;
        assert_eq!(fixture.hash(100, 30), 15_714_349_546_688_206_610);
    }

    #[test]
    fn golden_create_note_dialog_72x22() {
        let mut fixture = GoldenApp::new();
        fixture.app.dialog = DialogState::CreateNote;
        fixture.app.input_buffer = "deterministic-note".to_string();
        assert_eq!(fixture.hash(72, 22), 16_799_502_509_508_382_863);
    }

    #[test]
    fn default_panel_layout_keeps_twenty_percent_sides_and_center_minimum() {
        let config = Config::default();

        assert_eq!(
            main_layout_constraints(
                false,
                false,
                false,
                config.effective_sidebar_width_percent(),
                config.effective_outline_width_percent(),
            ),
            [Constraint::Percentage(20), Constraint::Min(20), Constraint::Percentage(20),]
        );
    }

    #[test]
    fn custom_panel_layout_uses_independent_effective_widths() {
        let mut config = Config::default();
        config.sidebar_width_percent = 30;
        config.outline_width_percent = 140;

        assert_eq!(
            main_layout_constraints(
                false,
                false,
                false,
                config.effective_sidebar_width_percent(),
                config.effective_outline_width_percent(),
            ),
            [Constraint::Percentage(30), Constraint::Min(20), Constraint::Percentage(95),]
        );
    }

    #[test]
    fn collapsed_panels_override_configured_widths() {
        assert_eq!(
            main_layout_constraints(false, true, true, 35, 45),
            [Constraint::Length(5), Constraint::Min(20), Constraint::Length(5),]
        );
    }

    #[test]
    fn widths_below_ten_percent_use_minimized_constraints() {
        assert_eq!(
            main_layout_constraints(false, false, false, 9, 5),
            [Constraint::Length(5), Constraint::Min(20), Constraint::Length(5),]
        );
        assert_eq!(
            main_layout_constraints(false, false, false, 10, 10),
            [Constraint::Percentage(10), Constraint::Min(20), Constraint::Percentage(10),]
        );
    }

    #[test]
    fn zen_mode_overrides_configured_and_collapsed_widths() {
        assert_eq!(
            main_layout_constraints(true, true, false, 35, 45),
            [Constraint::Length(0), Constraint::Min(20), Constraint::Length(0),]
        );
    }

    #[test]
    fn wide_layout_applies_independent_panel_percentages() {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(main_layout_constraints(false, false, false, 25, 15))
            .split(Rect::new(0, 0, 200, 20));

        assert_eq!(chunks[0].width, 50);
        assert_eq!(chunks[1].width, 120);
        assert_eq!(chunks[2].width, 30);
    }

    #[test]
    fn narrow_layout_retains_center_panel_minimum() {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(main_layout_constraints(false, false, false, 95, 95))
            .split(Rect::new(0, 0, 40, 20));

        assert!(chunks[1].width >= 20);
        assert_eq!(chunks.iter().map(|chunk| chunk.width).sum::<u16>(), 40);
    }
}
