use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, BlockInsertMode, Focus, Mode};
use crate::vim::VimMode as VimModeNew;

pub fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    const ZEN_MAX_WIDTH: u16 = 95;

    let theme = &app.theme;

    // Calculate stats - count only actual words, not markdown syntax
    let word_count = if let Some(note) = app.current_note() {
        note.content
            .split_whitespace()
            .filter(|word| {
                word.chars().any(|c| c.is_alphanumeric())
            })
            .count()
    } else {
        0
    };

    // Calculate percentage
    let percentage = if app.content_items.is_empty() {
        0
    } else {
        ((app.content_cursor + 1) * 100) / app.content_items.len()
    };

    let note_path = if app.zen_mode {
        // In zen mode, just show the note title
        app.current_note()
            .map(|n| n.title.clone())
            .unwrap_or_else(|| "—".to_string())
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

    // Get mode indicator and command info for edit mode
    let (mode_text, pending_info, command_input, normal_status) = match app.mode {
        Mode::Normal => {
            let mode = match app.focus {
                Focus::Sidebar => "sidebar",
                Focus::Content => "content",
                Focus::Outline => "outline",
            };
            let status = app.status_message.clone();
            (mode.to_string(), String::new(), None, status)
        }
        Mode::Edit => {
            // Get detailed vim mode info
            let vim = &app.vim;
            let mode_name = match &vim.mode {
                VimModeNew::Search { .. } => "search".to_string(),
                VimModeNew::SearchLocked { .. } => "search locked".to_string(),
                VimModeNew::Command => "command".to_string(),
                VimModeNew::OperatorPending { .. } => "normal".to_string(),
                _ => {
                    if let Some(ref block_state) = app.block_insert_state {
                        match block_state.mode {
                            BlockInsertMode::Insert => "v-blk insert".to_string(),
                            BlockInsertMode::Append => "v-blk append".to_string(),
                        }
                    } else {
                        match app.vim_mode {
                            crate::app::VimMode::Normal => "normal".to_string(),
                            crate::app::VimMode::Insert => "insert".to_string(),
                            crate::app::VimMode::Replace => "replace".to_string(),
                            crate::app::VimMode::Visual => "visual".to_string(),
                            crate::app::VimMode::VisualLine => "v-line".to_string(),
                            crate::app::VimMode::VisualBlock => "v-block".to_string(),
                        }
                    }
                }
            };

            // Build pending info string
            let mut pending_parts = Vec::new();

            // Recording indicator
            if vim.macros.is_recording() {
                pending_parts.push("recording".to_string());
            }

            // Count prefix
            if let Some(count) = vim.count {
                pending_parts.push(format!("{}", count));
            }

            // Operator pending
            if let VimModeNew::OperatorPending { operator, count } = &vim.mode {
                if let Some(c) = count {
                    pending_parts.push(format!("{}", c));
                }
                pending_parts.push(format!("{}", operator.char()));
            }

            // Pending g (for gg)
            if vim.pending_g {
                pending_parts.push("g".to_string());
            }

            // Pending z (for zz, zt, zb)
            if vim.pending_z {
                pending_parts.push("z".to_string());
            }

            // Pending find (f, F, t, T)
            if vim.pending_find.is_some() {
                pending_parts.push("f/t".to_string());
            }

            // Awaiting replace char
            if vim.awaiting_replace {
                pending_parts.push("r".to_string());
            }

            // Pending text object scope (i/a)
            if let Some(scope) = &vim.pending_text_object_scope {
                let ch = match scope {
                    crate::vim::TextObjectScope::Inner => 'i',
                    crate::vim::TextObjectScope::Around => 'a',
                };
                pending_parts.push(format!("{}", ch));
            }

            // Pending mark
            if let Some(mark) = &vim.pending_mark {
                let ch = match mark {
                    crate::vim::PendingMark::Set => 'm',
                    crate::vim::PendingMark::GotoExact => '`',
                    crate::vim::PendingMark::GotoLine => '\'',
                };
                pending_parts.push(format!("{}", ch));
            }

            // Pending macro
            if let Some(mac) = &vim.pending_macro {
                let ch = match mac {
                    crate::vim::PendingMacro::Record => 'q',
                    crate::vim::PendingMacro::Play => '@',
                };
                pending_parts.push(format!("{}", ch));
            }

            // Selected register
            if let Some(reg) = vim.registers.get_selected() {
                pending_parts.push(format!("\"{}", reg));
            }

            let pending = pending_parts.join("");

            // Command mode, search mode input, or status message
            let cmd_input = if matches!(vim.mode, VimModeNew::Command) {
                Some((format!(":{}", vim.command_buffer), false))
            } else if let VimModeNew::Search { forward } = vim.mode {
                let prefix = if forward { "/" } else { "?" };
                Some((format!("{}{}", prefix, vim.search_buffer), false))
            } else if let VimModeNew::SearchLocked { forward } = vim.mode {
                let prefix = if forward { "/" } else { "?" };
                let match_info = if app.buffer_search.matches.is_empty() {
                    String::new()
                } else {
                    format!(" [{}/{}]", app.buffer_search.current_match_index + 1, app.buffer_search.matches.len())
                };
                Some((format!("{}{}{}", prefix, vim.search_buffer, match_info), false))
            } else if let Some(ref msg) = vim.status_message {
                Some((msg.clone(), true))
            } else {
                None
            };

            (mode_name, pending, cmd_input, None)
        }
    };

    let statusbar = &theme.statusbar;
    let transparent_bg = app.config.transparent_bg;

    let brand = Span::styled(
        " ekphos ",
        Style::default()
            .fg(statusbar.brand)
            .add_modifier(Modifier::BOLD),
    );

    let separator1 = Span::styled(
        "›",
        Style::default().fg(statusbar.separator),
    );

    let mode = Span::styled(
        format!(" {} ", mode_text),
        Style::default().fg(statusbar.mode),
    );

    // Pending info (operators, count, etc.)
    let pending = if !pending_info.is_empty() {
        vec![
            Span::styled(
                "›",
                Style::default().fg(statusbar.separator),
            ),
            Span::styled(
                format!(" {} ", pending_info),
                Style::default().fg(theme.warning).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };

    let separator2 = Span::styled(
        "›",
        Style::default().fg(statusbar.separator),
    );

    // Command input or file path (with optional status message for Normal mode)
    let (path_or_command, status_span) = if let Some((cmd, is_warning)) = command_input {
        let color = if is_warning { theme.warning } else { theme.primary };
        (Span::styled(
            format!(" {}", cmd),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ), None)
    } else {
        let path = Span::styled(
            format!(" {}", note_path),
            Style::default().fg(statusbar.foreground),
        );
        let status = normal_status.map(|msg| {
            vec![
                Span::styled(" › ", Style::default().fg(statusbar.separator)),
                Span::styled(msg, Style::default().fg(theme.warning).add_modifier(Modifier::BOLD)),
            ]
        });
        (path, status)
    };

    // Right side content
    // Recording indicator
    let recording_indicator = if app.mode == Mode::Edit && app.vim.macros.is_recording() {
        vec![
            Span::styled(
                "● REC  ",
                Style::default().fg(theme.error).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };

    let indexing_indicator = if app.indexing_in_progress {
        use std::sync::atomic::Ordering;
        let current = app.index_progress.load(Ordering::Relaxed);
        let total = app.index_total.load(Ordering::Relaxed);
        let progress_text = if total > 0 {
            format!("indexing ({}/{})  ", current, total)
        } else {
            "indexing  ".to_string()
        };
        vec![
            Span::styled(
                progress_text,
                Style::default().fg(theme.muted),
            ),
        ]
    } else {
        vec![]
    };

    let zen_indicator = if app.zen_mode {
        vec![
            Span::styled(
                "zen  ",
                Style::default().fg(theme.info).add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };

    let stats = Span::styled(
        format!("{} words", word_count),
        Style::default().fg(statusbar.mode),
    );

    let position = Span::styled(
        format!("  {}%", percentage),
        Style::default().fg(statusbar.mode),
    );

    let help = Span::styled(
        "  ? help ",
        Style::default().fg(statusbar.mode),
    );

    // Build layout
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

    let content_width = if app.zen_mode {
        (area.width as usize).min(ZEN_MAX_WIDTH as usize)
    } else {
        area.width as usize
    };

    let left_width: usize = left_content.iter().map(|s| s.content.chars().count()).sum();
    let right_width: usize = right_content.iter().map(|s| s.content.chars().count()).sum();
    let middle_padding = content_width.saturating_sub(left_width + right_width);
    let mut spans = Vec::new();

    let bg_style = if transparent_bg {
        Style::default()
    } else {
        Style::default().bg(statusbar.background)
    };

    if app.zen_mode {
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
    let status_bar = Paragraph::new(status_line)
        .style(bg_style);

    f.render_widget(status_bar, area);
}
