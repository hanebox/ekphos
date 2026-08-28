use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{BlockInsertMode, EditorSession};
use crate::config::{EditingMode, Theme};
use crate::keybindings::{AppCommand, Keymap};
use ekphos_vim::VimMode;

#[derive(Clone, Copy)]
pub struct EditorLayout {
    pub area: Rect,
    pub inner_width: usize,
    pub inner_height: usize,
}

pub struct EditorView<'a> {
    pub theme: &'a Theme,
    pub editor: &'a EditorSession,
    pub editing_mode: EditingMode,
    pub keymap: &'a Keymap,
    pub zen_mode: bool,
}

pub fn editor_layout(zen_mode: bool, area: Rect) -> EditorLayout {
    const ZEN_MAX_WIDTH: u16 = 95;
    let (editor_area, inner_width, inner_height) = if zen_mode {
        let content_width = area.width.min(ZEN_MAX_WIDTH);
        let x_offset = (area.width.saturating_sub(content_width)) / 2;
        let editor_area = Rect {
            x: area.x + x_offset,
            y: area.y + 2, // 1 for status line + 1 for padding
            width: content_width,
            height: area.height.saturating_sub(2),
        };
        let inner_width = editor_area.width as usize;
        let inner_height = editor_area.height as usize;
        (editor_area, inner_width, inner_height)
    } else {
        let inner_width = area.width.saturating_sub(2) as usize;
        let inner_height = area.height.saturating_sub(2) as usize;
        (area, inner_width, inner_height)
    };
    EditorLayout { area: editor_area, inner_width, inner_height }
}

pub fn render_editor(f: &mut Frame, view: EditorView<'_>, layout: EditorLayout) {
    let EditorLayout { area: editor_area, inner_width: _, inner_height } = layout;
    if view.zen_mode {
        render_zen_status_line(f, &view, Rect { x: editor_area.x, y: editor_area.y.saturating_sub(2), width: editor_area.width, height: 1 });
    }
    f.render_widget(&**view.editor, editor_area);
    if view.editor.uses_native_cursor() {
        let (cursor_row, _cursor_col) = view.editor.cursor();
        let scroll_top = view.editor.editor_scroll_top;
        let y_offset: u16 = if view.zen_mode { 0 } else { 1 }; // border offset
        let x_offset: u16 = if view.zen_mode { 0 } else { 1 }; // border offset
        let content_left_offset = view.editor.content_left_offset();
        if cursor_row >= scroll_top {
            if view.editor.line_wrap_enabled() {
                let (wrap_row_offset, wrap_col) = view.editor.cursor_wrapped_position();
                let mut visual_row: usize = 0;
                for row in scroll_top..cursor_row {
                    visual_row += view.editor.line_wrapped_height(row);
                    if visual_row >= inner_height {
                        break;
                    }
                }
                visual_row += wrap_row_offset;
                if visual_row < inner_height {
                    let screen_y = editor_area.y + y_offset + visual_row as u16;
                    let screen_x = editor_area.x + x_offset + content_left_offset + wrap_col as u16;
                    let max_x = editor_area.x + editor_area.width.saturating_sub(if view.zen_mode { 0 } else { 1 });
                    if screen_x < max_x {
                        f.set_cursor_position((screen_x, screen_y));
                    }
                }
            } else if cursor_row < scroll_top + inner_height {
                let screen_y = editor_area.y + y_offset + (cursor_row - scroll_top) as u16;
                let display_col = view.editor.cursor_display_col();
                let h_scroll_display = view.editor.h_scroll_display_offset();
                let adjusted_col = display_col.saturating_sub(h_scroll_display);
                let screen_x = editor_area.x + x_offset + content_left_offset + adjusted_col as u16;
                let max_x = editor_area.x + editor_area.width.saturating_sub(if view.zen_mode { 0 } else { 1 });
                if screen_x < max_x {
                    f.set_cursor_position((screen_x, screen_y));
                }
            }
        }
    }
    if !view.editor.line_wrap_enabled() {
        let theme = view.theme;
        let (cursor_row, _cursor_col) = view.editor.cursor();
        let scroll_top = view.editor.editor_scroll_top;
        let (has_left_overflow, has_right_overflow) = view.editor.get_overflow_info();
        let y_offset = if view.zen_mode { 0 } else { 1 };
        if cursor_row >= scroll_top && cursor_row < scroll_top + inner_height {
            let y = editor_area.y + y_offset + (cursor_row - scroll_top) as u16;
            if has_left_overflow {
                let indicator = Paragraph::new("«│").style(Style::default().fg(theme.warning));
                let x = if view.zen_mode { editor_area.x } else { editor_area.x + 1 };
                f.render_widget(indicator, Rect::new(x, y, 2, 1));
            }
            let minimum_width = if view.zen_mode { 2 } else { 3 };
            if has_right_overflow && editor_area.width >= minimum_width {
                let indicator = Paragraph::new("│»").style(Style::default().fg(theme.warning));
                let trailing = if view.zen_mode { 2 } else { 3 };
                let x = editor_area.x.saturating_add(editor_area.width.saturating_sub(trailing));
                f.render_widget(indicator, Rect::new(x, y, 2, 1));
            }
        }
    }
}
fn render_zen_status_line(f: &mut Frame, view: &EditorView<'_>, area: Rect) {
    let theme = view.theme;
    if view.editing_mode == EditingMode::Standard {
        let toggle_key = view.keymap.binding_label(AppCommand::ToggleEditorMode);
        let status_line = Line::from(vec![
            Span::styled(" STANDARD ", Style::default().fg(theme.background).bg(theme.success).add_modifier(Modifier::BOLD)),
            Span::styled(" │ ", Style::default().fg(theme.border)),
            Span::styled(format!("Ctrl+S Save · Esc Preview · Ctrl+F Find · {toggle_key} Vim · F1 Help"), Style::default().fg(theme.muted)),
        ]);
        f.render_widget(Paragraph::new(status_line), area);
        return;
    }
    let is_command_mode = view.editor.vim.mode.is_command();
    let mode_str = if is_command_mode {
        "COMMAND"
    } else if let Some(ref block_state) = view.editor.block_insert_state {
        match block_state.mode {
            BlockInsertMode::Insert => "V-BLK INSERT",
            BlockInsertMode::Append => "V-BLK APPEND",
        }
    } else {
        view.editor.vim.mode.display_name()
    };
    let pending_str = match (&view.editor.pending_delete, view.editor.pending_operator) {
        (Some(_), _) => " [DEL]",
        (None, Some('d')) => " d-",
        _ => "",
    };
    let color = if is_command_mode {
        theme.info
    } else {
        match (&view.editor.pending_delete, view.editor.vim.mode) {
            (Some(_), _) => theme.error,
            (None, VimMode::Normal) if view.editor.pending_operator.is_some() => theme.warning,
            (None, VimMode::Normal) => theme.primary,
            (None, VimMode::Insert) => theme.success,
            (None, VimMode::Replace) => theme.warning,
            (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => theme.secondary,
            (None, _) => theme.info,
        }
    };
    let hint = if is_command_mode {
        "Enter: Execute, Esc: Cancel"
    } else if view.editor.block_insert_state.is_some() {
        "Type text, Esc: Apply to all lines"
    } else {
        match (&view.editor.pending_delete, view.editor.vim.mode) {
            (Some(_), _) => "d: Confirm, Esc: Cancel",
            (None, VimMode::Visual | VimMode::VisualLine | VimMode::VisualBlock) => "y: Yank, d: Delete, Esc: Cancel",
            (None, _) if view.editor.pending_operator == Some('d') => "d: Line, w: Word→, b: Word←",
            _ => "Ctrl+S: Save, Esc: Exit",
        }
    };
    let status_line = Line::from(vec![
        Span::styled(format!(" {} ", mode_str), Style::default().fg(theme.background).bg(color).add_modifier(Modifier::BOLD)),
        Span::styled(pending_str, Style::default().fg(color)),
        Span::styled(" │ ", Style::default().fg(theme.border)),
        Span::styled(hint, Style::default().fg(theme.muted)),
    ]);
    let paragraph = Paragraph::new(status_line);
    f.render_widget(paragraph, area);
}
