use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, VimMode};

pub fn render_editor(f: &mut Frame, app: &mut App, area: Rect) {
    // Store editor area for mouse coordinate translation
    app.editor_area = area;

    const ZEN_MAX_WIDTH: u16 = 95;

    let (editor_area, inner_width, inner_height) = if app.zen_mode {
        // In zen mode: centered with max width, status line at top, then editor below
        let content_width = area.width.min(ZEN_MAX_WIDTH);
        let x_offset = (area.width.saturating_sub(content_width)) / 2;

        let status_area = Rect {
            x: area.x + x_offset,
            y: area.y,
            width: content_width,
            height: 1,
        };
        render_zen_status_line(f, app, status_area);

        let editor_area = Rect {
            x: area.x + x_offset,
            y: area.y + 2, // 1 for status line + 1 for padding
            width: content_width,
            height: area.height.saturating_sub(2),
        };
        // No border in zen mode, so inner dimensions = full area
        let inner_width = editor_area.width as usize;
        let inner_height = editor_area.height as usize;
        (editor_area, inner_width, inner_height)
    } else {
        // Normal mode: account for borders
        let inner_width = area.width.saturating_sub(2) as usize;
        let inner_height = area.height.saturating_sub(2) as usize;
        (area, inner_width, inner_height)
    };

    // Update editor view dimensions and scroll
    app.editor.set_view_size(inner_width, inner_height);
    app.update_editor_scroll(inner_height);

    f.render_widget(&app.editor, editor_area);

    // Only show overflow indicators when line wrap is disabled
    if !app.editor.line_wrap_enabled() {
        let theme = &app.theme;
        let (cursor_row, _cursor_col) = app.editor.cursor();
        let scroll_top = app.editor_scroll_top;

        // Get overflow info from editor's horizontal scroll tracking
        let (has_left_overflow, has_right_overflow) = app.editor.get_overflow_info();

        // Render overflow indicators on the cursor line
        let y_offset = if app.zen_mode { 0 } else { 1 };
        if cursor_row >= scroll_top && cursor_row < scroll_top + inner_height {
            let y = editor_area.y + y_offset + (cursor_row - scroll_top) as u16;

            if has_left_overflow {
                let indicator = Paragraph::new("«│")
                    .style(Style::default().fg(theme.warning));
                let x = if app.zen_mode { editor_area.x } else { editor_area.x + 1 };
                f.render_widget(indicator, Rect::new(x, y, 2, 1));
            }

            if has_right_overflow {
                let indicator = Paragraph::new("│»")
                    .style(Style::default().fg(theme.warning));
                let x = if app.zen_mode {
                    editor_area.x + editor_area.width - 2
                } else {
                    editor_area.x + editor_area.width - 3
                };
                f.render_widget(indicator, Rect::new(x, y, 2, 1));
            }
        }
    }
}

fn render_zen_status_line(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let is_command_mode = app.vim.mode.is_command();

    let mode_str = if is_command_mode {
        "COMMAND"
    } else {
        match app.vim_mode {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Replace => "REPLACE",
            VimMode::Visual => "VISUAL",
            VimMode::VisualLine => "V-LINE",
            VimMode::VisualBlock => "V-BLOCK",
        }
    };

    let pending_str = match (&app.pending_delete, app.pending_operator) {
        (Some(_), _) => " [DEL]",
        (None, Some('d')) => " d-",
        _ => "",
    };

    let color = if is_command_mode {
        theme.info
    } else {
        match (&app.pending_delete, app.vim_mode) {
            (Some(_), _) => theme.error,
            (None, VimMode::Normal) if app.pending_operator.is_some() => theme.warning,
            (None, VimMode::Normal) => theme.primary,
            (None, VimMode::Insert) => theme.success,
            (None, VimMode::Replace) => theme.warning,
            (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => {
                theme.secondary
            }
        }
    };

    let hint = if is_command_mode {
        "Enter: Execute, Esc: Cancel"
    } else {
        match (&app.pending_delete, app.vim_mode) {
            (Some(_), _) => "d: Confirm, Esc: Cancel",
            (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => {
                "y: Yank, d: Delete, Esc: Cancel"
            }
            (None, _) if app.pending_operator == Some('d') => "d: Line, w: Word→, b: Word←",
            _ => "Ctrl+S: Save, Esc: Exit",
        }
    };

    let status_line = Line::from(vec![
        Span::styled(
            format!(" {} ", mode_str),
            Style::default()
                .fg(theme.background)
                .bg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(pending_str, Style::default().fg(color)),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled(hint, Style::default().fg(theme.muted)),
    ]);

    let paragraph = Paragraph::new(status_line);
    f.render_widget(paragraph, area);
}
