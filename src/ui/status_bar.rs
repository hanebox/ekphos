use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, BlockInsertMode, Focus, Mode};
use ekphos_vim::VimMode;

pub fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    const ZEN_MAX_WIDTH: u16 = 95;
    let theme = &app.state.theme;
    let word_count = match app.editor.mode {
        Mode::Normal => app.current_body().map_or(0, |body| body.split_whitespace().filter(|word| word.chars().any(|c| c.is_alphanumeric())).count()),
        Mode::Edit => (0..app.editor.line_count()).filter_map(|row| app.editor.line(row)).flat_map(str::split_whitespace).filter(|word| word.chars().any(|character| character.is_alphanumeric())).count(),
    };
    let (position, item_count) = app.editor.edit_preview_position.unwrap_or((app.document.content_cursor, app.document.content_items.len()));
    let percentage = if item_count == 0 { 0 } else { ((position + 1) * 100) / item_count };
    let note_path = if app.state.zen_mode {
        app.current_note().map(|n| n.title.clone()).unwrap_or_else(|| "—".to_string())
    } else {
        app.current_note()
            .and_then(|n| n.file_path.as_ref())
            .map(|p| {
                let path_str = p.to_string_lossy().to_string();
                if let Some(home) = dirs::home_dir() {
                    let home_str = home.to_string_lossy().to_string();
                    if path_str.starts_with(&home_str) {
                        return path_str.replacen(&home_str, "~", 1);
                    }
                }
                path_str
            })
            .unwrap_or_else(|| "—".to_string())
    };
    let (mode_text, pending_info, command_input, normal_status) = match app.editor.mode {
        Mode::Normal => {
            let mode = match app.state.focus {
                Focus::Sidebar => "sidebar",
                Focus::Content => "content",
                Focus::Outline => "outline",
            };
            let status = app.state.status_message.clone();
            (mode.to_string(), String::new(), None, status)
        }
        Mode::Edit => {
            let vim = &app.editor.vim;
            let mode_name = match &vim.mode {
                VimMode::Search { .. } => "search".to_string(),
                VimMode::SearchLocked { .. } => "search locked".to_string(),
                VimMode::Command => "command".to_string(),
                VimMode::OperatorPending { .. } => "normal".to_string(),
                _ => {
                    if let Some(ref block_state) = app.editor.block_insert_state {
                        match block_state.mode {
                            BlockInsertMode::Insert => "v-blk insert".to_string(),
                            BlockInsertMode::Append => "v-blk append".to_string(),
                        }
                    } else {
                        vim.mode.display_name().to_ascii_lowercase()
                    }
                }
            };
            let mut pending_parts = Vec::new();
            if vim.macros.is_recording() {
                pending_parts.push("recording".to_string());
            }
            if let Some(count) = vim.count {
                pending_parts.push(format!("{}", count));
            }
            if let VimMode::OperatorPending { operator, count } = &vim.mode {
                if let Some(c) = count {
                    pending_parts.push(format!("{}", c));
                }
                pending_parts.push(format!("{}", operator.char()));
            }
            if vim.pending_g {
                pending_parts.push("g".to_string());
            }
            if vim.pending_z {
                pending_parts.push("z".to_string());
            }
            if vim.pending_find.is_some() {
                pending_parts.push("f/t".to_string());
            }
            if vim.awaiting_replace {
                pending_parts.push("r".to_string());
            }
            if let Some(scope) = &vim.pending_text_object_scope {
                let ch = match scope {
                    ekphos_vim::TextObjectScope::Inner => 'i',
                    ekphos_vim::TextObjectScope::Around => 'a',
                };
                pending_parts.push(format!("{}", ch));
            }
            if let Some(mark) = &vim.pending_mark {
                let ch = match mark {
                    ekphos_vim::PendingMark::Set => 'm',
                    ekphos_vim::PendingMark::GotoExact => '`',
                    ekphos_vim::PendingMark::GotoLine => '\'',
                };
                pending_parts.push(format!("{}", ch));
            }
            if let Some(mac) = &vim.pending_macro {
                let ch = match mac {
                    ekphos_vim::PendingMacro::Record => 'q',
                    ekphos_vim::PendingMacro::Play => '@',
                };
                pending_parts.push(format!("{}", ch));
            }
            if let Some(reg) = vim.registers.get_selected() {
                pending_parts.push(format!("\"{}", reg));
            }
            let pending = pending_parts.join("");
            let cmd_input = if matches!(vim.mode, VimMode::Command) {
                Some((format!(":{}", vim.command_buffer), false))
            } else if let VimMode::Search { forward } = vim.mode {
                let prefix = if forward { "/" } else { "?" };
                Some((format!("{}{}", prefix, vim.search_buffer), false))
            } else if let VimMode::SearchLocked { forward } = vim.mode {
                let prefix = if forward { "/" } else { "?" };
                let match_info = if app.search.buffer_search.matches.is_empty() { String::new() } else { format!(" [{}/{}]", app.search.buffer_search.current_match_index + 1, app.search.buffer_search.matches.len()) };
                Some((format!("{}{}{}", prefix, vim.search_buffer, match_info), false))
            } else {
                vim.status_message.as_ref().map(|msg| (msg.clone(), true))
            };
            (mode_name, pending, cmd_input, None)
        }
    };
    let statusbar = &theme.statusbar;
    let transparent_bg = app.state.config.transparent_bg;
    let brand = Span::styled(" ekphos ", Style::default().fg(statusbar.brand).add_modifier(Modifier::BOLD));
    let separator1 = Span::styled("›", Style::default().fg(statusbar.separator));
    let mode = Span::styled(format!(" {} ", mode_text), Style::default().fg(statusbar.mode));
    let pending = if !pending_info.is_empty() { vec![Span::styled("›", Style::default().fg(statusbar.separator)), Span::styled(format!(" {} ", pending_info), Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))] } else { vec![] };
    let separator2 = Span::styled("›", Style::default().fg(statusbar.separator));
    let (path_or_command, status_span) = if let Some((cmd, is_warning)) = command_input {
        let color = if is_warning { theme.warning } else { theme.primary };
        (Span::styled(format!(" {}", cmd), Style::default().fg(color).add_modifier(Modifier::BOLD)), None)
    } else {
        let path = Span::styled(format!(" {}", note_path), Style::default().fg(statusbar.foreground));
        let status = normal_status.map(|msg| vec![Span::styled(" › ", Style::default().fg(statusbar.separator)), Span::styled(msg, Style::default().fg(theme.warning).add_modifier(Modifier::BOLD))]);
        (path, status)
    };
    let recording_indicator = if app.editor.mode == Mode::Edit && app.editor.vim.macros.is_recording() { vec![Span::styled("● REC  ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))] } else { vec![] };
    let indexing_indicator = if app.search.indexing_in_progress {
        use std::sync::atomic::Ordering;
        let current = app.search.index_progress.load(Ordering::Relaxed);
        let total = app.search.index_total.load(Ordering::Relaxed);
        let progress_text = if total > 0 { format!("indexing ({}/{})  ", current, total) } else { "indexing  ".to_string() };
        vec![Span::styled(progress_text, Style::default().fg(theme.muted))]
    } else {
        vec![]
    };
    let zen_indicator = if app.state.zen_mode { vec![Span::styled("zen  ", Style::default().fg(theme.info).add_modifier(Modifier::BOLD))] } else { vec![] };
    let stats = Span::styled(format!("{} words", word_count), Style::default().fg(statusbar.mode));
    let position = Span::styled(format!("  {}%", percentage), Style::default().fg(statusbar.mode));
    let help = Span::styled("  ? help ", Style::default().fg(statusbar.mode));
    let mut left_content = vec![brand, separator1, mode];
    left_content.extend(pending);
    left_content.push(separator2);
    left_content.push(path_or_command);
    if let Some(status_spans) = status_span {
        left_content.extend(status_spans);
    }
    let mut right_content = recording_indicator;
    right_content.extend(indexing_indicator);
    right_content.extend(zen_indicator);
    right_content.extend(vec![stats, position, help]);
    let content_width = if app.state.zen_mode { (area.width as usize).min(ZEN_MAX_WIDTH as usize) } else { area.width as usize };
    let left_width: usize = left_content.iter().map(|s| s.content.chars().count()).sum();
    let right_width: usize = right_content.iter().map(|s| s.content.chars().count()).sum();
    let middle_padding = content_width.saturating_sub(left_width + right_width);
    let mut spans = Vec::new();
    let bg_style = if transparent_bg { Style::default() } else { Style::default().bg(statusbar.background) };
    if app.state.zen_mode {
        let left_margin = (area.width as usize).saturating_sub(content_width) / 2;
        if left_margin > 0 {
            spans.push(Span::styled(" ".repeat(left_margin), bg_style));
        }
    }
    spans.extend(left_content);
    spans.push(Span::styled(" ".repeat(middle_padding), bg_style));
    spans.extend(right_content);
    let current_width = spans.iter().map(|s| s.content.chars().count()).sum::<usize>();
    let right_margin = (area.width as usize).saturating_sub(current_width);
    if right_margin > 0 {
        spans.push(Span::styled(" ".repeat(right_margin), bg_style));
    }
    let status_line = Line::from(spans);
    let status_bar = Paragraph::new(status_line).style(bg_style);
    f.render_widget(status_bar, area);
}
