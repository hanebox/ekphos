use super::*;

/// Emit a terminal control command to the controlling terminal.
///
/// We avoid `std::io::stdout()` here: on Unix the process's stdout is redirected
/// to `/dev/null` at startup (see `terminal_writer` in `main.rs`) so stray
/// library output can't corrupt the alternate screen. Cursor-style escapes are
/// therefore sent straight to `/dev/tty`, the same terminal crossterm uses for
/// its own I/O. The `/dev/tty` handle is opened once and reused.
#[cfg(unix)]
pub(super) fn write_term_control(cmd: impl crossterm::Command) {
    use std::sync::{Mutex, OnceLock};
    static TTY: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
    let tty = TTY.get_or_init(|| std::fs::OpenOptions::new().write(true).open("/dev/tty").ok().map(Mutex::new));
    if let Some(tty) = tty {
        if let Ok(mut tty) = tty.lock() {
            let _ = crossterm::execute!(*tty, cmd);
            return;
        }
    }
    let _ = crossterm::execute!(std::io::stdout(), cmd);
}

#[cfg(not(unix))]
pub(super) fn write_term_control(cmd: impl crossterm::Command) {
    let _ = crossterm::execute!(std::io::stdout(), cmd);
}

pub(super) fn update_cursor_style(app: &mut App) {
    let terminal_style = match app.editor.vim.mode {
        VimMode::Insert => SetCursorStyle::SteadyBar,
        VimMode::Replace => SetCursorStyle::SteadyUnderScore,
        _ => SetCursorStyle::SteadyBlock,
    };
    write_term_control(terminal_style);
    let editor_shape = match app.editor.vim.mode {
        VimMode::Insert => CursorShape::Bar,
        VimMode::Replace => CursorShape::Underline,
        _ => CursorShape::Block,
    };
    app.editor.set_cursor_shape(editor_shape);
}

/// Activate the currently selected text link, wiki link, or inline image.
/// Returns false when the content item has no selectable target.
pub(super) fn open_selected_content_target(app: &mut App) -> bool {
    let Some(link) = app.current_selected_link() else {
        return false;
    };
    match link {
        LinkInfo::Markdown { url, .. } => app.open_link(&url),
        LinkInfo::Image { path, .. } => app.open_path_or_url(&path),
        LinkInfo::Wiki { target, heading, is_valid, .. } => {
            if is_valid {
                app.navigate_to_wiki_link_with_heading(&target, heading.as_deref());
            } else {
                app.editor.pending_wiki_target = Some(target);
                app.state.dialog = DialogState::CreateWikiNote;
            }
        }
    }
    true
}

pub fn run_app(terminal: &mut Terminal<CrosstermBackend<Box<dyn io::Write>>>, app: &mut App) -> io::Result<()> {
    let mut needs_render = true;
    loop {
        let background = app.poll_background();
        needs_render |= background.redraw;
        if needs_render {
            terminal.draw(|f| ui::render(f, app))?;
            app.reclaim_memory_if_requested();
            needs_render = false;
        }
        if background.has_work {
            if event::poll(background.poll_timeout)? {
                if process_events(terminal, app, &mut needs_render)? {
                    return Ok(());
                }
            } else if app.editor.mouse_button_held && app.editor.mode == Mode::Edit && app.editor.vim.mode == VimMode::Visual {
                handle_continuous_auto_scroll(app);
                needs_render = true;
            }
        } else if process_events(terminal, app, &mut needs_render)? {
            return Ok(());
        }
    }
}

pub(super) fn process_events(_terminal: &mut Terminal<CrosstermBackend<Box<dyn io::Write>>>, app: &mut App, needs_render: &mut bool) -> io::Result<bool> {
    const MAX_EVENTS_PER_BATCH: u8 = 8;
    let mut count = 0u8;
    loop {
        let event = event::read()?;
        count += 1;
        *needs_render = true;
        match event {
            Event::FocusGained => {
                app.reload_on_focus();
                app.state.needs_full_clear = true;
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if handle_key_event(app, key)? {
                    return Ok(true);
                }
            }
            Event::Mouse(mouse) => handle_mouse_event(app, mouse),
            Event::Paste(text) => handle_paste_event(app, text),
            Event::Resize(_, _) => {}
            _ => {}
        }
        if count >= MAX_EVENTS_PER_BATCH || !event::poll(std::time::Duration::ZERO)? {
            break;
        }
    }
    Ok(false)
}
