use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme::Theme;

pub fn render_welcome_dialog(f: &mut Frame, theme: &Theme) {
    let area = f.area();

    // Calculate centered dialog area
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 20.min(area.height.saturating_sub(4));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    // Create welcome content
    let welcome_text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "   _____ _          _               ",
                Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  | ____| | ___ __ | |__   ___  ___ ",
                Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  |  _| | |/ / '_ \\| '_ \\ / _ \\/ __|",
                Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  | |___|   <| |_) | | | | (_) \\__ \\",
                Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  |_____|_|\\_\\ .__/|_| |_|\\___/|___/",
                Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "             |_|                    ",
                Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "A lightweight markdown research tool",
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("j/k ", Style::default().fg(theme.peach)),
            Span::styled("Navigate notes", Style::default().fg(theme.subtext0)),
        ]),
        Line::from(vec![
            Span::styled("Tab ", Style::default().fg(theme.peach)),
            Span::styled("Switch focus  ", Style::default().fg(theme.subtext0)),
        ]),
        Line::from(vec![
            Span::styled("e   ", Style::default().fg(theme.peach)),
            Span::styled("Edit note     ", Style::default().fg(theme.subtext0)),
        ]),
        Line::from(vec![
            Span::styled("?   ", Style::default().fg(theme.peach)),
            Span::styled("Help          ", Style::default().fg(theme.subtext0)),
        ]),
        Line::from(vec![
            Span::styled("q   ", Style::default().fg(theme.peach)),
            Span::styled("Quit          ", Style::default().fg(theme.subtext0)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter or Space to continue",
            Style::default().fg(theme.overlay0).add_modifier(Modifier::ITALIC),
        )),
    ];

    let welcome = Paragraph::new(welcome_text)
        .block(
            Block::default()
                .title(" Welcome ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.lavender))
                .style(Style::default().bg(theme.base)),
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
            Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Where would you like to store your notes?",
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.peach)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.peach)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to confirm",
            Style::default().fg(theme.overlay0).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Setup ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.lavender))
                .style(Style::default().bg(theme.base)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_create_note_dialog(f: &mut Frame, app: &App) {
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

    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Enter note name:",
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.peach)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.peach)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter: Create  |  Esc: Cancel",
            Style::default().fg(theme.overlay0).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" New Note ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.green))
                .style(Style::default().bg(theme.base)),
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
            Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            note_name,
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y: Yes  |  n: No",
            Style::default().fg(theme.overlay0).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Confirm Delete ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.red))
                .style(Style::default().bg(theme.base)),
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
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(theme.peach)),
            Span::styled(&app.input_buffer, Style::default().fg(theme.text)),
            Span::styled("█", Style::default().fg(theme.peach)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter: Rename  |  Esc: Cancel",
            Style::default().fg(theme.overlay0).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Rename Note ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.yellow))
                .style(Style::default().bg(theme.base)),
        )
        .alignment(Alignment::Center);

    f.render_widget(dialog, dialog_area);
}

pub fn render_help_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    // Calculate centered dialog area
    let dialog_width = 56.min(area.width.saturating_sub(4));
    let dialog_height = 30.min(area.height.saturating_sub(2));

    let dialog_area = Rect {
        x: (area.width.saturating_sub(dialog_width)) / 2,
        y: (area.height.saturating_sub(dialog_height)) / 2,
        width: dialog_width,
        height: dialog_height,
    };

    // Clear the area behind the dialog
    f.render_widget(Clear, dialog_area);

    let key_style = Style::default().fg(theme.peach);
    let desc_style = Style::default().fg(theme.subtext0);
    let header_style = Style::default().fg(theme.lavender).add_modifier(Modifier::BOLD);

    let content = vec![
        Line::from(""),
        Line::from(Span::styled("  Navigation", header_style)),
        Line::from(vec![
            Span::styled("  j/k      ", key_style),
            Span::styled("Navigate up/down", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Tab      ", key_style),
            Span::styled("Switch focus (Sidebar/Content/Outline)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Enter    ", key_style),
            Span::styled("Open image / Jump to heading", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Notes", header_style)),
        Line::from(vec![
            Span::styled("  n        ", key_style),
            Span::styled("New note", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  r        ", key_style),
            Span::styled("Rename note", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  d        ", key_style),
            Span::styled("Delete note", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  e        ", key_style),
            Span::styled("Edit note", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  /        ", key_style),
            Span::styled("Search notes", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Edit Mode (Vim)", header_style)),
        Line::from(vec![
            Span::styled("  i/a/A/I  ", key_style),
            Span::styled("Insert mode", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  v        ", key_style),
            Span::styled("Visual mode (select text)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  y/p      ", key_style),
            Span::styled("Yank (copy) / Paste", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  d/x      ", key_style),
            Span::styled("Delete line / character", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  u/Ctrl+r ", key_style),
            Span::styled("Undo / Redo", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+s   ", key_style),
            Span::styled("Save", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  Esc      ", key_style),
            Span::styled("Exit edit mode", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Other", header_style)),
        Line::from(vec![
            Span::styled("  ?        ", key_style),
            Span::styled("Show this help", desc_style),
        ]),
        Line::from(vec![
            Span::styled("  q        ", key_style),
            Span::styled("Quit", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press Esc or ? to close",
            Style::default().fg(theme.overlay0).add_modifier(Modifier::ITALIC),
        )),
    ];

    let dialog = Paragraph::new(content)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.lavender))
                .style(Style::default().bg(theme.base)),
        )
        .alignment(Alignment::Left);

    f.render_widget(dialog, dialog_area);
}
