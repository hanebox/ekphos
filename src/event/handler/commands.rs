use super::*;

/// Returns true if the app should quit.
pub(super) fn handle_normal_mode(app: &mut App, key: crossterm::event::KeyEvent) -> bool {
    app.state.status_message = None; // Clear old status message on new keystroke
    let available: Vec<_> = AppCommand::ALL.into_iter().filter(|command| app_command_available(app, *command)).collect();
    let resolution = app.state.keymap.resolve(key, |command| available.contains(&command));
    match resolution {
        KeyResolution::Command(command) => execute_app_command(app, command),
        KeyResolution::NoMatch | KeyResolution::Pending => false,
    }
}

pub(super) fn app_command_available(app: &App, command: AppCommand) -> bool {
    match command {
        AppCommand::FocusNext | AppCommand::FocusPrevious => !app.state.zen_mode,
        AppCommand::OpenJournal | AppCommand::CreateNote | AppCommand::CreateFolder | AppCommand::DeleteItem | AppCommand::RenameItem => !app.state.zen_mode,
        AppCommand::CutItem => !app.state.zen_mode && app.state.focus == Focus::Sidebar,
        AppCommand::PasteItem => !app.state.zen_mode && app.state.focus == Focus::Sidebar && app.vault.cut_buffer.is_some(),
        AppCommand::HistoryBack | AppCommand::HistoryForward => app.state.focus != Focus::Sidebar,
        AppCommand::OpenSelected => matches!(app.state.focus, Focus::Content | Focus::Outline),
        AppCommand::ContentAction | AppCommand::NextTarget | AppCommand::PreviousTarget | AppCommand::ToggleFloatingCursor | AppCommand::HalfPageDown | AppCommand::HalfPageUp | AppCommand::ToggleFrontmatter | AppCommand::ToggleFold | AppCommand::FoldAll | AppCommand::UnfoldAll => {
            app.state.focus == Focus::Content
        }
        AppCommand::CancelCut => app.state.focus == Focus::Sidebar && app.vault.cut_buffer.is_some(),
        AppCommand::SidebarSearch | AppCommand::CycleSort => app.state.focus == Focus::Sidebar,
        _ => true,
    }
}

/// Executes a resolved main-view command. Returns true when the app should quit.
pub(super) fn execute_app_command(app: &mut App, command: AppCommand) -> bool {
    match command {
        AppCommand::Quit => return true,
        AppCommand::FocusNext => app.toggle_focus(false),
        AppCommand::FocusPrevious => app.toggle_focus(true),
        AppCommand::EditNote => {
            app.push_navigation_history(app.vault.selected_note);
            app.enter_edit_mode();
        }
        AppCommand::CreateNote => {
            app.state.input_buffer.clear();
            app.state.dialog_error = None;
            let context_folder = app.get_current_context_folder();
            if context_folder.as_ref() != Some(&app.state.config.notes_path()) {
                app.vault.target_folder = context_folder;
            } else {
                app.vault.target_folder = None;
            }
            app.state.dialog = DialogState::CreateNote;
        }
        AppCommand::CreateFolder => {
            app.state.input_buffer.clear();
            app.state.dialog_error = None;
            let context_folder = app.get_current_context_folder();
            if context_folder.as_ref() != Some(&app.state.config.notes_path()) {
                app.vault.target_folder = context_folder;
            } else {
                app.vault.target_folder = None;
            }
            app.state.dialog = DialogState::CreateFolder;
        }
        AppCommand::DeleteItem => {
            if let Some(item) = app.vault.sidebar_items.get(app.vault.selected_sidebar_index) {
                match &item.kind {
                    SidebarItemKind::Note { .. } => {
                        app.state.dialog = DialogState::DeleteConfirm;
                    }
                    SidebarItemKind::Folder(folder) if folder.path != app.state.config.notes_path() => {
                        app.state.dialog = DialogState::DeleteFolderConfirm;
                    }
                    SidebarItemKind::Folder(_) => app.state.status_message = Some("The vault root cannot be deleted".to_string()),
                }
            }
        }
        AppCommand::CutItem => {
            app.cut_selected_item();
        }
        AppCommand::PasteItem => {
            if let Err(e) = app.paste_cut_item() {
                app.state.status_message = Some(format!("Move failed: {}", e));
            }
        }
        AppCommand::RenameItem => {
            if let Some(item) = app.vault.sidebar_items.get(app.vault.selected_sidebar_index) {
                match &item.kind {
                    SidebarItemKind::Note { note_id } => {
                        if let Some(note) = app.vault.notes.iter().find(|note| note.id == *note_id) {
                            app.state.input_buffer = note.title.clone();
                            app.state.dialog_error = None;
                            app.state.dialog = DialogState::RenameNote;
                        }
                    }
                    SidebarItemKind::Folder(folder) if folder.path != app.state.config.notes_path() => {
                        app.state.input_buffer = item.display_name.clone();
                        app.state.dialog_error = None;
                        app.state.dialog = DialogState::RenameFolder;
                    }
                    SidebarItemKind::Folder(_) => app.state.status_message = Some("The vault root cannot be renamed".to_string()),
                }
            }
        }
        AppCommand::ReloadConfig => {
            app.reload_config();
            app.state.needs_full_clear = true;
        }
        AppCommand::ReloadFiles => {
            app.reload_on_focus();
            app.state.needs_full_clear = true;
        }
        AppCommand::OpenQuickSearch => app.open_search_picker(),
        AppCommand::OpenThemeSelector => app.open_theme_selector(),
        AppCommand::OpenJournal => app.open_or_create_journal(),
        AppCommand::MoveDown => match app.state.focus {
            Focus::Sidebar => app.next_sidebar_item(),
            Focus::Outline => app.next_outline(),
            Focus::Content => {
                if app.editor.floating_cursor_mode {
                    app.floating_move_down();
                } else {
                    app.next_content_line();
                }
                app.sync_outline_to_content();
            }
        },
        AppCommand::MoveUp => match app.state.focus {
            Focus::Sidebar => app.previous_sidebar_item(),
            Focus::Outline => app.previous_outline(),
            Focus::Content => {
                if app.editor.floating_cursor_mode {
                    app.floating_move_up();
                } else {
                    app.previous_content_line();
                }
                app.sync_outline_to_content();
            }
        },
        AppCommand::Activate => match app.state.focus {
            Focus::Content => {
                if !open_selected_content_target(app) {
                    app.open_current_image();
                }
            }
            Focus::Outline => app.jump_to_outline(),
            Focus::Sidebar => app.handle_sidebar_enter(),
        },
        AppCommand::ToggleOutline => app.toggle_outline_collapsed(),
        AppCommand::HistoryBack => {
            app.navigate_back();
        }
        AppCommand::HistoryForward => {
            app.navigate_forward();
        }
        AppCommand::OpenSelected => {
            if app.state.focus == Focus::Content {
                if !open_selected_content_target(app) {
                    app.open_current_image();
                }
            } else if app.state.focus == Focus::Outline {
                app.jump_to_outline();
            }
        }
        AppCommand::ShowHelp => app.state.dialog = DialogState::Help,
        AppCommand::SidebarSearch => app.activate_sidebar_search(),
        AppCommand::CycleSort => app.cycle_sort_mode(),
        AppCommand::ContentAction => {
            if let Some(crate::app::ContentItem::TaskItem { .. }) = app.document.content_items.get(app.document.content_cursor) {
                if app.is_task_checkbox_selected() || !open_selected_content_target(app) {
                    app.toggle_current_task();
                }
            } else if let Some(crate::app::ContentItem::Details { .. }) = app.document.content_items.get(app.document.content_cursor) {
                app.toggle_current_details();
            } else if app.is_heading_at(app.document.content_cursor) {
                app.toggle_current_heading_fold();
            } else {
                open_selected_content_target(app);
            }
        }
        AppCommand::NextTarget => app.next_link(),
        AppCommand::PreviousTarget => app.previous_link(),
        AppCommand::ToggleFloatingCursor => app.toggle_floating_cursor(),
        AppCommand::ToggleSidebar => app.toggle_sidebar_collapsed(),
        AppCommand::HalfPageDown => {
            app.half_page_down_content();
            app.sync_outline_to_content();
        }
        AppCommand::HalfPageUp => {
            app.half_page_up_content();
            app.sync_outline_to_content();
        }
        AppCommand::FindInBuffer => app.start_buffer_search(),
        AppCommand::OpenGraph => {
            app.build_graph();
            app.state.dialog = DialogState::GraphView;
        }
        AppCommand::ToggleZen => app.toggle_zen_mode(),
        AppCommand::ToggleFrontmatter => app.toggle_frontmatter_hidden(),
        AppCommand::ToggleFold => app.toggle_current_heading_fold(),
        AppCommand::FoldAll => app.fold_all_headings(),
        AppCommand::UnfoldAll => app.unfold_all_headings(),
        AppCommand::GoFirst => match app.state.focus {
            Focus::Sidebar => app.goto_first_sidebar_item(),
            Focus::Outline => app.goto_first_outline(),
            Focus::Content => {
                app.goto_first_content_line();
                app.sync_outline_to_content();
            }
        },
        AppCommand::GoLast => match app.state.focus {
            Focus::Sidebar => app.goto_last_sidebar_item(),
            Focus::Outline => app.goto_last_outline(),
            Focus::Content => {
                app.goto_last_content_line();
                app.sync_outline_to_content();
            }
        },
        AppCommand::CancelCut => app.clear_cut_buffer(),
        AppCommand::ShrinkPanel => app.resize_focused_panel(-Config::PANEL_RESIZE_STEP_PERCENT),
        AppCommand::GrowPanel => app.resize_focused_panel(Config::PANEL_RESIZE_STEP_PERCENT),
    }
    false
}
