use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::config::Theme;

const TITLE_MAIN: &[&str] = &[
    "████████ ██   ██ ██████  ██   ██  ██████  ███████",
    "██       ██  ██  ██   ██ ██   ██ ██    ██ ██     ",
    "█████    █████   ██████  ███████ ██    ██ ███████",
    "██       ██  ██  ██      ██   ██ ██    ██      ██",
    "████████ ██   ██ ██      ██   ██  ██████  ███████",
];

fn render_flat_title(theme: &Theme, dialog_width: u16) -> Vec<Line<'static>> {
    let main_color = theme.dialog.title;

    let title_width = TITLE_MAIN[0].chars().count();
    let inner_width = dialog_width.saturating_sub(2) as usize; // minus borders
    let left_pad = inner_width.saturating_sub(title_width) / 2;
    let padding = " ".repeat(left_pad);

    TITLE_MAIN.iter().map(|line| {
        let styled_line: String = line.chars().map(|ch| {
            if ch != ' ' { '█' } else { ' ' }
        }).collect();
        Line::from(vec![
            Span::raw(padding.clone()),
            Span::styled(styled_line, Style::default().fg(main_color)),
        ]).alignment(Alignment::Left)
    }).collect()
}

pub fn render_welcome_dialog(f: &mut Frame, theme: &Theme) {
    let area = f.area();
    let dialog_theme = &theme.dialog;

    // Calculate centered dialog area
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 24.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    // Build welcome content with bitmap title
    let mut welcome_text = vec![Line::from("")];

    // Add flat bitmap title with manual centering
    welcome_text.extend(render_flat_title(theme, dialog_width));

    welcome_text.extend(vec![
        Line::from(""),
        Line::from(Span::styled(
            "A lightweight markdown research tool",
            Style::default().fg(dialog_theme.text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("j/k ", Style::default().fg(theme.warning)),
            Span::styled("Navigate notes", Style::default().fg(dialog_theme.text)),
        ]),
        Line::from(vec![
            Span::styled("Tab ", Style::default().fg(theme.warning)),
            Span::styled("Switch focus  ", Style::default().fg(dialog_theme.text)),
        ]),
        Line::from(vec![
            Span::styled("e   ", Style::default().fg(theme.warning)),
            Span::styled("Edit note     ", Style::default().fg(dialog_theme.text)),
        ]),
        Line::from(vec![
            Span::styled("?   ", Style::default().fg(theme.warning)),
            Span::styled("Help          ", Style::default().fg(dialog_theme.text)),
        ]),
        Line::from(vec![
            Span::styled("q   ", Style::default().fg(theme.warning)),
            Span::styled("Quit          ", Style::default().fg(dialog_theme.text)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Read the docs at docs.ekphos.xyz",
            Style::default().fg(dialog_theme.text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter or Space to continue",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ]);

    let welcome = Paragraph::new(welcome_text)
        .block(
            Block::default()
                .title(" Welcome ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(dialog_theme.border))
                .style(Style::default().bg(dialog_theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(welcome, dialog_area);
}

pub fn render_onboarding_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    // Calculate centered dialog area
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 12.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Welcome to Ekphos!",
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Where would you like to store your notes?",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.warning)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.foreground)),
            Span::styled("█", Style::default().fg(theme.cursor)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to confirm",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Setup ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.primary))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_create_note_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let has_context = app.target_folder.is_some();
    let has_error = app.dialog_error.is_some();
    let base_height = if has_context { 10 } else { 9 };
    let dialog_height = if has_error { base_height + 2 } else { base_height };

    // Calculate centered dialog area
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter note name:",
            Style::default().fg(theme.foreground),
        )),
    ];

    // Show context if creating in a subfolder
    if let Some(ref folder_path) = app.target_folder {
        if let Some(folder_name) = folder_path.file_name() {
            content.push(Line::from(Span::styled(
                format!("in {}/", folder_name.to_string_lossy()),
                Style::default().fg(theme.info),
            )));
        }
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.warning)),
        Span::styled(&app.input_buffer, Style::default().fg(theme.foreground)),
        Span::styled("█", Style::default().fg(theme.cursor)),
    ]));

    // Show error message if present
    if let Some(ref error) = app.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            error.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        "Enter: Create  |  Esc: Cancel",
        Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
    )));

    let border_color = if has_error { theme.error } else { theme.success };

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" New Note ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_delete_confirm_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    // Calculate centered dialog area
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 9.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    let note_name = app.current_note()
        .map(|n| n.title.as_str())
        .unwrap_or("this note");

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Delete note?",
            Style::default().fg(theme.error).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            note_name,
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y: Yes  |  n: No",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Confirm Delete ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.error))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_unsaved_changes_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 10.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "You have unsaved changes!",
            Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Do you want to save before exiting?",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y: Save  |  n: Discard  |  Esc: Cancel",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Unsaved Changes ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_create_wiki_note_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let dialog_width = 55.min(area.width.saturating_sub(4));
    let dialog_height = 10.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let target = app.pending_wiki_target.as_deref().unwrap_or("note");

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Note '[[{}]]' doesn't exist.", target),
            Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Would you like to create it?",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y: Create  |  n: Cancel",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Create Note ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.info))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_delete_folder_confirm_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 11.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let folder_name = app.get_selected_folder_name()
        .unwrap_or_else(|| "this folder".to_string());

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Delete folder and all contents?",
            Style::default().fg(theme.error).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            folder_name,
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "This will delete all notes inside!",
            Style::default().fg(theme.warning),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y: Yes  |  n: No",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Confirm Delete Folder ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.error))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_rename_note_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 9.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter new name:",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.warning)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.foreground)),
            Span::styled("█", Style::default().fg(theme.cursor)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter: Rename  |  Esc: Cancel",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Rename Note ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_rename_folder_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let has_error = app.dialog_error.is_some();
    let dialog_height = if has_error { 11 } else { 9 };

    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter new folder name:",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.info)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.foreground)),
            Span::styled("█", Style::default().fg(theme.cursor)),
        ]),
    ];

    if let Some(ref error) = app.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            error.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        "Enter: Rename  |  Esc: Cancel",
        Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
    )));

    let border_color = if has_error { theme.error } else { theme.info };

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Rename Folder ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_create_folder_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let has_error = app.dialog_error.is_some();
    let has_context = app.target_folder.is_some();
    let dialog_height = if has_error { 11 } else if has_context { 10 } else { 9 };

    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter folder name:",
            Style::default().fg(theme.foreground),
        )),
    ];

    if let Some(ref folder_path) = app.target_folder {
        if let Some(folder_name) = folder_path.file_name() {
            content.push(Line::from(Span::styled(
                format!("in {}/", folder_name.to_string_lossy()),
                Style::default().fg(theme.info),
            )));
        }
    }

    content.push(Line::from(""));
    content.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.info)),
        Span::styled(&app.input_buffer, Style::default().fg(theme.foreground)),
        Span::styled("█", Style::default().fg(theme.cursor)),
    ]));

    if let Some(ref error) = app.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            error.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        "Enter: Create  |  Esc: Cancel",
        Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
    )));

    let border_color = if has_error { theme.error } else { theme.info };

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" New Folder ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_create_note_in_folder_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let has_error = app.dialog_error.is_some();
    let dialog_height = if has_error { 13 } else { 11 };

    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = dialog_height.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let folder_name = app.target_folder
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "folder".to_string());

    let mut content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Folder created! Now create your first note:",
            Style::default().fg(theme.success),
        )),
        Line::from(Span::styled(
            format!("in {}/", folder_name),
            Style::default().fg(theme.info),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.warning)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.foreground)),
            Span::styled("█", Style::default().fg(theme.cursor)),
        ]),
    ];

    if let Some(ref error) = app.dialog_error {
        content.push(Line::from(""));
        content.push(Line::from(Span::styled(
            error.as_str(),
            Style::default().fg(theme.error),
        )));
    }

    content.push(Line::from(""));
    content.push(Line::from(Span::styled(
        "Enter: Create  |  Esc: Cancel (removes folder)",
        Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
    )));

    let border_color = if has_error { theme.error } else { theme.success };

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" New Note in Folder ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_empty_directory_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let dialog_width = 55.min(area.width.saturating_sub(4));
    let dialog_height = 12.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Oops! This directory seems empty",
            Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "No markdown notes found in:",
            Style::default().fg(theme.foreground),
        )),
        Line::from(Span::styled(
            &app.config.notes_dir,
            Style::default().fg(theme.muted),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press 'n' to create your first note!",
            Style::default().fg(theme.success),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter or Esc to continue",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Getting Started ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_help_dialog(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let theme = &app.theme;
    let dialog_theme = &theme.dialog;

    // Calculate centered dialog area - wider for two columns
    let dialog_width = 90.min(area.width.saturating_sub(4));
    let dialog_height = area.height.saturating_sub(4).min(50);

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(" Help - Keybindings (j/k to scroll) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(dialog_theme.border))
        .style(Style::default().bg(dialog_theme.background));

    let inner_area = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner_area);

    let key_style = Style::default().fg(theme.warning);
    let desc_style = Style::default().fg(dialog_theme.text);
    let header_style = Style::default().fg(dialog_theme.title).add_modifier(Modifier::BOLD);
    let subheader_style = Style::default().fg(theme.info).add_modifier(Modifier::BOLD);

    let left_content = vec![
        Line::from(""),
        Line::from(Span::styled(" Global", header_style)),
        Line::from(vec![
            Span::styled(" j/k       ", key_style),
            Span::styled("Navigate up/down", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Tab       ", key_style),
            Span::styled("Switch focus", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Shift+Tab ", key_style),
            Span::styled("Switch focus (reverse)", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Enter/o   ", key_style),
            Span::styled("Open / Jump to heading", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" ?         ", key_style),
            Span::styled("Show this help", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" q         ", key_style),
            Span::styled("Quit", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+b    ", key_style),
            Span::styled("Toggle sidebar", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+o    ", key_style),
            Span::styled("Toggle outline", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+f    ", key_style),
            Span::styled("Find in buffer", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+z    ", key_style),
            Span::styled("Toggle zen mode", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" R         ", key_style),
            Span::styled("Reload files from disk", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+Sh+R ", key_style),
            Span::styled("Reload config/theme", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(" Sidebar", header_style)),
        Line::from(vec![
            Span::styled(" n         ", key_style),
            Span::styled("Create new note", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" N         ", key_style),
            Span::styled("Create new folder", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Enter     ", key_style),
            Span::styled("Toggle folder / Open", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" r         ", key_style),
            Span::styled("Rename", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" d         ", key_style),
            Span::styled("Delete", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" e         ", key_style),
            Span::styled("Edit note", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" /         ", key_style),
            Span::styled("Search notes", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(" Content View", header_style)),
        Line::from(vec![
            Span::styled(" j/k       ", key_style),
            Span::styled("Navigate lines", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Shift+J/K ", key_style),
            Span::styled("Toggle floating cursor", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" gg        ", key_style),
            Span::styled("Go to beginning", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" G         ", key_style),
            Span::styled("Go to end", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Space     ", key_style),
            Span::styled("Toggle task/Open link", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" ]/[       ", key_style),
            Span::styled("Next/Previous link", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" za        ", key_style),
            Span::styled("Toggle heading fold", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" zM        ", key_style),
            Span::styled("Fold all headings", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" zR        ", key_style),
            Span::styled("Unfold all headings", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " Press Esc or ? to close",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let right_content = vec![
        Line::from(""),
        Line::from(Span::styled(" Edit Mode - Normal", header_style)),
        Line::from(""),
        Line::from(Span::styled("  Mode Changes", subheader_style)),
        Line::from(vec![
            Span::styled(" i/a       ", key_style),
            Span::styled("Insert before/after cursor", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" I/A       ", key_style),
            Span::styled("Insert at line start/end", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" o/O       ", key_style),
            Span::styled("New line below/above", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" v/V       ", key_style),
            Span::styled("Visual / Visual Line mode", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" :         ", key_style),
            Span::styled("Command mode", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Movement", subheader_style)),
        Line::from(vec![
            Span::styled(" h/j/k/l   ", key_style),
            Span::styled("Move left/down/up/right", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" w/W       ", key_style),
            Span::styled("Word/WORD forward", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" b/B       ", key_style),
            Span::styled("Word/WORD backward", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" e/E       ", key_style),
            Span::styled("Word/WORD end forward", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" 0/^/$     ", key_style),
            Span::styled("Line start/first char/end", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" gg/G      ", key_style),
            Span::styled("File top/bottom", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" {/}       ", key_style),
            Span::styled("Paragraph backward/forward", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" %         ", key_style),
            Span::styled("Matching bracket", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Find Character", subheader_style)),
        Line::from(vec![
            Span::styled(" f/F{c}    ", key_style),
            Span::styled("Find char forward/backward", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" t/T{c}    ", key_style),
            Span::styled("Till char forward/backward", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" ;/,       ", key_style),
            Span::styled("Repeat find / reverse", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Operators (+ motion/text obj)", subheader_style)),
        Line::from(vec![
            Span::styled(" d{motion} ", key_style),
            Span::styled("Delete", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" c{motion} ", key_style),
            Span::styled("Change (delete + insert)", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" y{motion} ", key_style),
            Span::styled("Yank (copy)", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" >/< {m}   ", key_style),
            Span::styled("Indent / Outdent", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" dd/cc/yy  ", key_style),
            Span::styled("Operate on whole line", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" D/C/Y     ", key_style),
            Span::styled("Operate to line end", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Text Objects (inner/around)", subheader_style)),
        Line::from(vec![
            Span::styled(" iw/aw     ", key_style),
            Span::styled("Inner/around word", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" iW/aW     ", key_style),
            Span::styled("Inner/around WORD", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" i\"/a\"     ", key_style),
            Span::styled("Inner/around double quotes", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" i'/a'     ", key_style),
            Span::styled("Inner/around single quotes", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" i(/a(     ", key_style),
            Span::styled("Inner/around parentheses", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" i[/a[     ", key_style),
            Span::styled("Inner/around brackets", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" i{/a{     ", key_style),
            Span::styled("Inner/around braces", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" ip/ap     ", key_style),
            Span::styled("Inner/around paragraph", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Actions", subheader_style)),
        Line::from(vec![
            Span::styled(" x/X       ", key_style),
            Span::styled("Delete char fwd/back", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" s/S       ", key_style),
            Span::styled("Substitute char/line", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" r{c}      ", key_style),
            Span::styled("Replace char", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" J         ", key_style),
            Span::styled("Join lines", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" p/P       ", key_style),
            Span::styled("Paste after/before", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" u/Ctrl+r  ", key_style),
            Span::styled("Undo / Redo", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" .         ", key_style),
            Span::styled("Repeat last command", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" ~         ", key_style),
            Span::styled("Toggle case", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Registers", subheader_style)),
        Line::from(vec![
            Span::styled(" \"{reg}    ", key_style),
            Span::styled("Use register (a-z, 0-9)", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" \"+/\"*     ", key_style),
            Span::styled("System clipboard", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Macros", subheader_style)),
        Line::from(vec![
            Span::styled(" q{reg}    ", key_style),
            Span::styled("Record macro to register", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" q         ", key_style),
            Span::styled("Stop recording", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" @{reg}    ", key_style),
            Span::styled("Play macro from register", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" @@        ", key_style),
            Span::styled("Repeat last macro", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Marks", subheader_style)),
        Line::from(vec![
            Span::styled(" m{a-z}    ", key_style),
            Span::styled("Set mark", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" '{a-z}    ", key_style),
            Span::styled("Jump to mark (line)", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" `{a-z}    ", key_style),
            Span::styled("Jump to mark (exact)", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Scrolling", subheader_style)),
        Line::from(vec![
            Span::styled(" Ctrl+u/d  ", key_style),
            Span::styled("Half page up/down", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Ctrl+b/f  ", key_style),
            Span::styled("Full page up/down", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Count Prefix", subheader_style)),
        Line::from(vec![
            Span::styled(" {n}{cmd}  ", key_style),
            Span::styled("Repeat cmd n times", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" 5j, 3w    ", key_style),
            Span::styled("Move 5 down, 3 words", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" 2dd       ", key_style),
            Span::styled("Delete 2 lines", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Save/Exit", subheader_style)),
        Line::from(vec![
            Span::styled(" Ctrl+s    ", key_style),
            Span::styled("Save and exit", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" Esc       ", key_style),
            Span::styled("Exit to normal / cancel", desc_style),
        ]),
        Line::from(vec![
            Span::styled(" :w/:q/:wq ", key_style),
            Span::styled("Write/Quit/Both", desc_style),
        ]),
        Line::from(""),
    ];

    // Calculate total lines and visible height
    let left_total = left_content.len();
    let right_total = right_content.len();
    let max_total = left_total.max(right_total);
    let visible_height = inner_area.height as usize;

    // Apply scroll offset and clamp to prevent overflow
    let max_scroll = max_total.saturating_sub(visible_height);
    app.help_scroll = app.help_scroll.min(max_scroll);
    let scroll = app.help_scroll;

    let left_visible: Vec<Line> = left_content
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect();

    let right_visible: Vec<Line> = right_content
        .into_iter()
        .skip(scroll)
        .take(visible_height)
        .collect();

    let left_paragraph = Paragraph::new(left_visible).alignment(Alignment::Left);
    let right_paragraph = Paragraph::new(right_visible).alignment(Alignment::Left);

    f.render_widget(left_paragraph, columns[0]);
    f.render_widget(right_paragraph, columns[1]);
}

pub fn render_directory_not_found_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    let dialog_width = 58.min(area.width.saturating_sub(4));
    let dialog_height = 14.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    f.render_widget(Clear, dialog_area);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Directory Not Found",
            Style::default().fg(theme.error).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "The configured notes directory does not exist:",
            Style::default().fg(theme.foreground),
        )),
        Line::from(Span::styled(
            &app.config.notes_dir,
            Style::default().fg(theme.warning),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Would you like to create it?",
            Style::default().fg(theme.foreground),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("c", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::styled(" Create directory  ", Style::default().fg(theme.foreground)),
            Span::styled("q", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
            Span::styled(" Quit and fix config", Style::default().fg(theme.foreground)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Config: ~/.config/ekphos/config.toml",
            Style::default().fg(theme.muted).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Error ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.error))
                .style(Style::default().bg(theme.background)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}
