mod content;
mod dialogs;
mod editor;
mod outline;
mod sidebar;
mod status_bar;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::app::{App, DialogState, Mode};

pub use content::render_content;
pub use dialogs::{
    render_create_note_dialog, render_delete_confirm_dialog, render_help_dialog,
    render_onboarding_dialog, render_rename_note_dialog, render_welcome_dialog,
};
pub use editor::render_editor;
pub use outline::render_outline;
pub use sidebar::render_sidebar;
pub use status_bar::render_status_bar;

pub fn render(f: &mut Frame, app: &mut App) {
    // Create vertical layout: main area + status bar
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main area
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    // Create main layout with left sidebar, content, and right outline
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Left Sidebar (Notes)
            Constraint::Percentage(60), // Content
            Constraint::Percentage(20), // Right Sidebar (Outline)
        ])
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
        DialogState::DeleteConfirm => render_delete_confirm_dialog(f, app),
        DialogState::RenameNote => render_rename_note_dialog(f, app),
        DialogState::Help => render_help_dialog(f, app),
        DialogState::None => {
            // Render welcome dialog on top if active
            if app.show_welcome {
                render_welcome_dialog(f, &app.theme);
            }
        }
    }
}
