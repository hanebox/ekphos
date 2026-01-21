use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, BlockInsertMode, VimMode};

pub fn render_editor(f: &mut Frame, app: &mut App, area: Rect) {
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

    app.editor_area = editor_area;

    // Update editor view dimensions and scroll
    app.editor.set_view_size(inner_width, inner_height);
    app.update_editor_scroll(inner_height);

    f.render_widget(&app.editor, editor_area);

    // Set terminal cursor position for Insert mode (bar cursor)
    if app.editor.uses_native_cursor() {
        let (cursor_row, _cursor_col) = app.editor.cursor();
        let scroll_top = app.editor_scroll_top;
        let y_offset: u16 = if app.zen_mode { 0 } else { 1 }; // border offset
        let x_offset: u16 = if app.zen_mode { 0 } else { 1 }; // border offset

        let content_left_offset = app.editor.content_left_offset();
        let content_width = inner_width.saturating_sub(content_left_offset as usize);

        if cursor_row >= scroll_top {
            if app.editor.line_wrap_enabled() {
                let (wrap_row_offset, wrap_col) = app.editor.cursor_wrapped_position(content_width);
                let mut visual_row: usize = 0;
                for row in scroll_top..cursor_row {
                    visual_row += app.editor.line_wrapped_height(row, content_width);
                    if visual_row >= inner_height {
                        break; 
                    }
                }
                visual_row += wrap_row_offset;

                if visual_row < inner_height {
                    let screen_y = editor_area.y + y_offset + visual_row as u16;
                    let screen_x = editor_area.x + x_offset + content_left_offset + wrap_col as u16;
                    let max_x = editor_area.x + editor_area.width.saturating_sub(if app.zen_mode { 0 } else { 1 });
                    if screen_x < max_x {
                        f.set_cursor_position((screen_x, screen_y));
                    }
                }
            } else {
                if cursor_row < scroll_top + inner_height {
                    let screen_y = editor_area.y + y_offset + (cursor_row - scroll_top) as u16;

                    let display_col = app.editor.cursor_display_col();
                    let h_scroll_display = app.editor.h_scroll_display_offset();
                    let adjusted_col = display_col.saturating_sub(h_scroll_display);
                    let screen_x = editor_area.x + x_offset + content_left_offset + adjusted_col as u16;

                    let max_x = editor_area.x + editor_area.width.saturating_sub(if app.zen_mode { 0 } else { 1 });
                    if screen_x < max_x {
                        f.set_cursor_position((screen_x, screen_y));
                    }
                }
            }
        }
    }

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
    } else if let Some(ref block_state) = app.block_insert_state {
        match block_state.mode {
            BlockInsertMode::Insert => "V-BLK INSERT",
            BlockInsertMode::Append => "V-BLK APPEND",
        }
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
    } else if app.block_insert_state.is_some() {
        "Type text, Esc: Apply to all lines"
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
