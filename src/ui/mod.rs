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
    render_create_folder_dialog, render_create_note_dialog, render_create_note_in_folder_dialog,
    render_delete_confirm_dialog, render_delete_folder_confirm_dialog, render_directory_not_found_dialog,
    render_empty_directory_dialog, render_help_dialog, render_onboarding_dialog, render_rename_folder_dialog,
    render_rename_note_dialog, render_unsaved_changes_dialog, render_welcome_dialog,
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

    // Determine panel widths based on collapsed state
    let sidebar_constraint = if app.sidebar_collapsed {
        Constraint::Length(5) 
    } else {
        Constraint::Percentage(20)
    };

    let outline_constraint = if app.outline_collapsed {
        Constraint::Length(5) 
    } else {
        Constraint::Percentage(20)
    };

    // Create main layout with left sidebar, content, and right outline
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            sidebar_constraint,     
            Constraint::Min(20),    
            outline_constraint,    
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
        DialogState::None => {
            // Render welcome dialog on top if active
            if app.show_welcome {
                render_welcome_dialog(f, &app.theme);
            }
        }
    }
}
