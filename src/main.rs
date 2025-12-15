mod theme;

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use image::DynamicImage;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};
use tui_textarea::{Input, TextArea};

use theme::{Config, Theme};

#[derive(Debug, Clone)]
struct Note {
    title: String,
    content: String,
    file_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DialogState {
    None,
    Onboarding,
    CreateNote,
    DeleteConfirm,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Focus {
    Sidebar,
    Content,
    Outline,
}

#[derive(Debug, Clone)]
struct OutlineItem {
    level: usize,
    title: String,
    line: usize,
}

struct ImageState {
    image: StatefulProtocol,
    path: String,
}

struct App<'a> {
    notes: Vec<Note>,
    selected_note: usize,
    list_state: ListState,
    focus: Focus,
    mode: Mode,
    textarea: TextArea<'a>,
    picker: Option<Picker>,
    image_cache: HashMap<String, DynamicImage>,
    current_image: Option<ImageState>,
    show_welcome: bool,
    outline: Vec<OutlineItem>,
    outline_state: ListState,
    vim_mode: VimMode,
    content_cursor: usize,
    content_items: Vec<ContentItem>,
    theme: Theme,
    config: Config,
    dialog: DialogState,
    input_buffer: String,
    search_active: bool,
    search_query: String,
    filtered_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
enum ContentItem {
    TextLine(String),
    Image(String),
    CodeLine(String),
    CodeFence(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VimMode {
    Normal,
    Insert,
    Visual,
}

impl<'a> App<'a> {
    fn new() -> Self {
        // Load config and theme first
        let config = Config::load();
        let theme = Theme::from_config(&config.theme.colors);

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.blue))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        textarea.set_cursor_line_style(Style::default().bg(theme.surface0));

        // Initialize image picker for terminal graphics
        let picker = Picker::from_query_stdio().ok();

        // Check if notes directory exists (if onboarding was complete)
        let notes_dir_exists = if config.onboarding_complete {
            config.notes_path().exists()
        } else {
            false
        };

        // Determine if we need onboarding
        // Show onboarding if not complete OR if notes directory was deleted
        let dialog = if !config.onboarding_complete || (config.onboarding_complete && !notes_dir_exists) {
            DialogState::Onboarding
        } else {
            DialogState::None
        };

        let input_buffer = config.notes_dir.clone();

        let mut app = Self {
            notes: Vec::new(),
            selected_note: 0,
            list_state,
            focus: Focus::Sidebar,
            mode: Mode::Normal,
            textarea,
            picker,
            image_cache: HashMap::new(),
            current_image: None,
            show_welcome: config.onboarding_complete && !config.welcome_shown && notes_dir_exists,
            outline: Vec::new(),
            outline_state: ListState::default(),
            vim_mode: VimMode::Normal,
            content_cursor: 0,
            content_items: Vec::new(),
            theme,
            config,
            dialog,
            input_buffer,
            search_active: false,
            search_query: String::new(),
            filtered_indices: Vec::new(),
        };

        // Load notes if onboarding is complete and directory exists
        if app.config.onboarding_complete && notes_dir_exists {
            app.load_notes_from_dir();
        }

        app
    }

    fn load_notes_from_dir(&mut self) {
        self.notes.clear();
        let notes_path = self.config.notes_path();

        // Create directory if it doesn't exist
        if !notes_path.exists() {
            let _ = fs::create_dir_all(&notes_path);
        }

        // Load all .md files
        if let Ok(entries) = fs::read_dir(&notes_path) {
            let mut notes: Vec<Note> = entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry.path().extension()
                        .map(|ext| ext == "md")
                        .unwrap_or(false)
                })
                .filter_map(|entry| {
                    let path = entry.path();
                    let content = fs::read_to_string(&path).ok()?;
                    let title = path.file_stem()?.to_string_lossy().to_string();
                    Some(Note {
                        title,
                        content,
                        file_path: Some(path),
                    })
                })
                .collect();

            // Sort by title
            notes.sort_by(|a, b| a.title.cmp(&b.title));
            self.notes = notes;
        }

        // Create welcome note if no notes exist
        if self.notes.is_empty() {
            self.create_welcome_note();
        }

        // Reset selection
        self.selected_note = 0;
        self.list_state.select(Some(0));

        // Update UI state
        self.update_outline();
        self.update_content_items();
    }

    fn create_welcome_note(&mut self) {
        let content = r#"# Welcome to Ekphos

A lightweight, fast, terminal-based markdown research tool built with Rust.

## Layout

Ekphos has three panels:

- **Sidebar** (left): List of your notes
- **Content** (center): Note content with markdown rendering
- **Outline** (right): Auto-generated headings for quick navigation

Use `Tab` to switch between panels.

## Navigation

- `j/k` or Arrow keys: Navigate up/down
- `Tab`: Switch focus between panels
- `Enter`: Jump to heading (in Outline) or open image (in Content)
- `/`: Search notes (in Sidebar)
- `?`: Show help dialog

## Notes Management

- `n`: Create new note
- `d`: Delete current note
- `e`: Enter edit mode

## Edit Mode (Vim Keybindings)

### Modes

- `i`: Insert mode
- `a`: Insert after cursor
- `A`: Insert at end of line
- `I`: Insert at start of line
- `o`: New line below
- `O`: New line above
- `v`: Visual mode (select text)
- `Esc`: Return to normal mode

### Movement

- `h/j/k/l`: Move cursor
- `w/b`: Word forward/back
- `0/$`: Line start/end
- `gg/G`: Top/bottom of file

### Editing

- `x`: Delete character
- `dd`: Delete line
- `y`: Yank (copy) line
- `p`: Paste
- `u`: Undo
- `Ctrl+r`: Redo
- `Ctrl+s`: Save and exit edit mode

## Markdown Support

### Headings

# Heading 1

## Heading 2

### Heading 3

#### Heading 4

##### Heading 5

###### Heading 6

### Lists

- Bullet item one
- Bullet item two
- Bullet item three

* Asterisk style also works
* Like this

### Blockquotes

> This is a blockquote.
> It can span multiple lines.

### Code Blocks

```rust
fn main() {
    println!("Hello, Ekphos!");
}
```

```python
def greet():
    return "Hello from Python"
```

### Horizontal Rules

---

### Images

Images can be embedded using standard markdown syntax:

```
![alt text](path/to/image.png)
```

Press `Enter` or `o` on an image line to open it in your system viewer.

Supported formats: PNG, JPEG, GIF, WebP, BMP

For inline preview, use a compatible terminal (iTerm2, Kitty, WezTerm, Sixel).

## CLI Options

Run from terminal:

- `ekphos --help`: Show help
- `ekphos --version`: Show version
- `ekphos --config`: Show config file path
- `ekphos --dir`: Show notes directory path

## Configuration

Config file: `~/.config/ekphos/config.toml`

```toml
notes_dir = "~/Documents/ekphos"

[theme]
name = "catppuccin-mocha"
```

## Themes

Built-in themes:

- catppuccin-mocha (default)
- catppuccin-latte
- catppuccin-frappe
- catppuccin-macchiato

Custom themes can be added to `~/.config/ekphos/themes/`

---

Press `q` to quit. Happy note-taking!"#.to_string();

        let notes_path = self.config.notes_path();
        let file_path = notes_path.join("Welcome.md");
        let _ = fs::write(&file_path, &content);

        self.notes.push(Note {
            title: "Welcome".to_string(),
            content,
            file_path: Some(file_path),
        });
        self.selected_note = 0;
        self.list_state.select(Some(0));
    }

    fn create_note(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }

        let notes_path = self.config.notes_path();
        let file_path = notes_path.join(format!("{}.md", name));

        // Don't overwrite existing files
        if file_path.exists() {
            return;
        }

        let content = format!("# {}\n\n", name);
        if fs::write(&file_path, &content).is_ok() {
            self.notes.push(Note {
                title: name.to_string(),
                content,
                file_path: Some(file_path),
            });
            self.notes.sort_by(|a, b| a.title.cmp(&b.title));

            // Select the new note
            if let Some(idx) = self.notes.iter().position(|n| n.title == name) {
                self.selected_note = idx;
                self.list_state.select(Some(idx));
            }

            self.update_outline();
            self.update_content_items();
        }
    }

    fn delete_current_note(&mut self) {
        if self.notes.is_empty() {
            return;
        }

        if let Some(note) = self.notes.get(self.selected_note) {
            if let Some(ref path) = note.file_path {
                let _ = fs::remove_file(path);
            }
        }

        self.notes.remove(self.selected_note);

        // Adjust selection
        if self.selected_note >= self.notes.len() && !self.notes.is_empty() {
            self.selected_note = self.notes.len() - 1;
        }
        self.list_state.select(if self.notes.is_empty() {
            None
        } else {
            Some(self.selected_note)
        });

        self.update_outline();
        self.update_content_items();
    }

    fn complete_onboarding(&mut self) {
        self.config.notes_dir = self.input_buffer.clone();
        self.config.onboarding_complete = true;
        let _ = self.config.save();

        // Create the notes directory
        let notes_path = self.config.notes_path();
        let _ = fs::create_dir_all(&notes_path);

        self.dialog = DialogState::None;
        self.load_notes_from_dir();
        self.show_welcome = true;
    }

    fn dismiss_welcome(&mut self) {
        self.show_welcome = false;
        self.config.welcome_shown = true;
        let _ = self.config.save();
    }

    fn update_outline(&mut self) {
        self.outline.clear();
        let content = self.current_note().map(|n| n.content.clone());
        if let Some(content) = content {
            for (line_num, line) in content.lines().enumerate() {
                if line.starts_with("# ") {
                    self.outline.push(OutlineItem {
                        level: 1,
                        title: line.trim_start_matches("# ").to_string(),
                        line: line_num,
                    });
                } else if line.starts_with("## ") {
                    self.outline.push(OutlineItem {
                        level: 2,
                        title: line.trim_start_matches("## ").to_string(),
                        line: line_num,
                    });
                } else if line.starts_with("### ") {
                    self.outline.push(OutlineItem {
                        level: 3,
                        title: line.trim_start_matches("### ").to_string(),
                        line: line_num,
                    });
                }
            }
        }
        if !self.outline.is_empty() {
            self.outline_state.select(Some(0));
        }
    }

    fn update_content_items(&mut self) {
        self.content_items.clear();
        let content = self.current_note().map(|n| n.content.clone());
        if let Some(content) = content {
            let mut in_code_block = false;

            for line in content.lines() {
                // Check for code fence
                if line.starts_with("```") {
                    let lang = line.trim_start_matches('`').to_string();
                    self.content_items.push(ContentItem::CodeFence(lang));
                    in_code_block = !in_code_block;
                    continue;
                }

                // If inside code block, add as CodeLine
                if in_code_block {
                    self.content_items.push(ContentItem::CodeLine(line.to_string()));
                    continue;
                }

                // Check for image
                if line.starts_with("![") && line.contains("](") && line.contains(')') {
                    if let Some(start) = line.find("](") {
                        if let Some(end) = line[start..].find(')') {
                            let path = &line[start + 2..start + end];
                            if !path.is_empty() {
                                self.content_items.push(ContentItem::Image(path.to_string()));
                                continue;
                            }
                        }
                    }
                }

                self.content_items.push(ContentItem::TextLine(line.to_string()));
            }
        }
        self.content_cursor = 0;
    }

    fn next_content_line(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        if self.content_cursor < self.content_items.len() - 1 {
            self.content_cursor += 1;
        }
    }

    fn previous_content_line(&mut self) {
        if self.content_cursor > 0 {
            self.content_cursor -= 1;
        }
    }

    fn sync_outline_to_content(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        // Find the outline item that corresponds to the current content line
        // or the closest heading before the current line
        let mut best_match: Option<usize> = None;
        for (i, item) in self.outline.iter().enumerate() {
            if item.line <= self.content_cursor {
                best_match = Some(i);
            } else {
                break;
            }
        }
        if let Some(idx) = best_match {
            self.outline_state.select(Some(idx));
        }
    }

    fn current_item_is_image(&self) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(self.content_cursor) {
            Some(path)
        } else {
            None
        }
    }

    fn open_current_image(&self) {
        if let Some(path) = self.current_item_is_image() {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() {
                #[cfg(target_os = "macos")]
                let _ = Command::new("open").arg(path).spawn();
                #[cfg(target_os = "linux")]
                let _ = Command::new("xdg-open").arg(path).spawn();
                #[cfg(target_os = "windows")]
                let _ = Command::new("cmd").args(["/c", "start", "", path]).spawn();
            }
        }
    }

    fn next_note(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        self.selected_note = (self.selected_note + 1) % self.notes.len();
        self.list_state.select(Some(self.selected_note));
        self.current_image = None;
        self.update_outline();
        self.update_content_items();
    }

    fn previous_note(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        self.selected_note = if self.selected_note == 0 {
            self.notes.len() - 1
        } else {
            self.selected_note - 1
        };
        self.list_state.select(Some(self.selected_note));
        self.current_image = None;
        self.update_outline();
        self.update_content_items();
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Content,
            Focus::Content => Focus::Outline,
            Focus::Outline => Focus::Sidebar,
        };
    }

    fn update_filtered_indices(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_indices.clear();
            return;
        }
        let query = self.search_query.to_lowercase();
        self.filtered_indices = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, note)| note.title.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();
    }

    fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.filtered_indices.clear();
    }

    fn get_visible_notes(&self) -> Vec<(usize, &Note)> {
        if self.search_active && !self.search_query.is_empty() {
            self.filtered_indices
                .iter()
                .filter_map(|&i| self.notes.get(i).map(|n| (i, n)))
                .collect()
        } else {
            self.notes.iter().enumerate().collect()
        }
    }

    fn next_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => (i + 1) % self.outline.len(),
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    fn previous_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.outline.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    fn jump_to_outline(&mut self) {
        if let Some(selected) = self.outline_state.selected() {
            if let Some(outline_item) = self.outline.get(selected) {
                let target_line = outline_item.line;
                // Set content cursor to the target line
                if target_line < self.content_items.len() {
                    self.content_cursor = target_line;
                }
                // Switch focus to content
                self.focus = Focus::Content;
            }
        }
    }

    fn current_note(&self) -> Option<&Note> {
        self.notes.get(self.selected_note)
    }

    fn enter_edit_mode(&mut self) {
        if let Some(note) = self.current_note() {
            let lines: Vec<String> = note.content.lines().map(String::from).collect();
            self.textarea = TextArea::new(lines);
            self.vim_mode = VimMode::Normal;
            self.update_editor_block();
            self.textarea.set_cursor_line_style(Style::default().bg(self.theme.surface0));
            self.mode = Mode::Edit;
            self.focus = Focus::Content;
        }
    }

    fn update_editor_block(&mut self) {
        let mode_str = match self.vim_mode {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
        };
        let color = match self.vim_mode {
            VimMode::Normal => self.theme.blue,
            VimMode::Insert => self.theme.green,
            VimMode::Visual => self.theme.mauve,
        };
        let hint = match self.vim_mode {
            VimMode::Visual => "y: Yank, d: Delete, Esc: Cancel",
            _ => "Ctrl+S: Save, Esc: Exit",
        };
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .title(format!(" {} | {} ", mode_str, hint)),
        );
        // Set selection style for visual mode
        self.textarea.set_selection_style(Style::default().bg(self.theme.surface2));
    }

    fn save_edit(&mut self) {
        if let Some(note) = self.notes.get_mut(self.selected_note) {
            note.content = self.textarea.lines().join("\n");
            // Save to file
            if let Some(ref path) = note.file_path {
                let _ = fs::write(path, &note.content);
            }
        }
        self.mode = Mode::Normal;
        self.update_outline();
        self.update_content_items();
    }

    fn cancel_edit(&mut self) {
        self.mode = Mode::Normal;
    }
}

fn print_help() {
    println!("ekphos {}", VERSION);
    println!("A lightweight, fast, terminal-based markdown research tool");
    println!();
    println!("USAGE:");
    println!("    ekphos [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print help information");
    println!("    -v, --version    Print version information");
    println!("    -c, --config     Print config file path");
    println!("    -d, --dir        Print notes directory path");
}

fn main() -> io::Result<()> {
    // Handle CLI arguments
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-v" | "--version" => {
                println!("ekphos {}", VERSION);
                return Ok(());
            }
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-c" | "--config" => {
                println!("{}", theme::Config::config_path().display());
                return Ok(());
            }
            "-d" | "--dir" => {
                let config = theme::Config::load();
                println!("{}", config.notes_path().display());
                return Ok(());
            }
            _ => {
                eprintln!("Unknown option: {}", args[1]);
                eprintln!("Run 'ekphos --help' for usage information");
                return Ok(());
            }
        }
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        match event::read()? {
            Event::Mouse(mouse) => {
                // Handle mouse scroll in normal mode only
                if app.mode == Mode::Normal && app.dialog == DialogState::None && !app.show_welcome {
                    match mouse.kind {
                        MouseEventKind::ScrollDown => {
                            match app.focus {
                                Focus::Sidebar => app.next_note(),
                                Focus::Content => {
                                    app.next_content_line();
                                    app.sync_outline_to_content();
                                }
                                Focus::Outline => app.next_outline(),
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            match app.focus {
                                Focus::Sidebar => app.previous_note(),
                                Focus::Content => {
                                    app.previous_content_line();
                                    app.sync_outline_to_content();
                                }
                                Focus::Outline => app.previous_outline(),
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Key(key) => {
            if key.kind == KeyEventKind::Press {
                // Handle dialogs first
                match app.dialog {
                    DialogState::Onboarding => {
                        match key.code {
                            KeyCode::Enter => {
                                app.complete_onboarding();
                            }
                            KeyCode::Char(c) => {
                                app.input_buffer.push(c);
                            }
                            KeyCode::Backspace => {
                                app.input_buffer.pop();
                            }
                            _ => {}
                        }
                        continue;
                    }
                    DialogState::CreateNote => {
                        match key.code {
                            KeyCode::Enter => {
                                let name = app.input_buffer.clone();
                                app.create_note(&name);
                                app.input_buffer.clear();
                                app.dialog = DialogState::None;
                            }
                            KeyCode::Esc => {
                                app.input_buffer.clear();
                                app.dialog = DialogState::None;
                            }
                            KeyCode::Char(c) => {
                                app.input_buffer.push(c);
                            }
                            KeyCode::Backspace => {
                                app.input_buffer.pop();
                            }
                            _ => {}
                        }
                        continue;
                    }
                    DialogState::DeleteConfirm => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                app.delete_current_note();
                                app.dialog = DialogState::None;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.dialog = DialogState::None;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    DialogState::Help => {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => {
                                app.dialog = DialogState::None;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    DialogState::None => {}
                }

                // Handle welcome dialog
                if app.show_welcome {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                            app.dismiss_welcome();
                        }
                        _ => {}
                    }
                    continue;
                }

                // Handle search input
                if app.search_active {
                    match key.code {
                        KeyCode::Esc => {
                            app.clear_search();
                        }
                        KeyCode::Enter => {
                            // Select first filtered note if any
                            let visible = app.get_visible_notes();
                            if !visible.is_empty() {
                                app.selected_note = visible[0].0;
                                app.current_image = None;
                                app.update_outline();
                                app.update_content_items();
                            }
                            app.search_active = false;
                        }
                        KeyCode::Backspace => {
                            app.search_query.pop();
                            app.update_filtered_indices();
                        }
                        KeyCode::Down => {
                            // Navigate within filtered results
                            let visible = app.get_visible_notes();
                            if !visible.is_empty() {
                                let current_pos = visible.iter().position(|(i, _)| *i == app.selected_note).unwrap_or(0);
                                let next_pos = (current_pos + 1) % visible.len();
                                app.selected_note = visible[next_pos].0;
                                app.current_image = None;
                                app.update_outline();
                                app.update_content_items();
                            }
                        }
                        KeyCode::Up => {
                            let visible = app.get_visible_notes();
                            if !visible.is_empty() {
                                let current_pos = visible.iter().position(|(i, _)| *i == app.selected_note).unwrap_or(0);
                                let prev_pos = if current_pos == 0 { visible.len() - 1 } else { current_pos - 1 };
                                app.selected_note = visible[prev_pos].0;
                                app.current_image = None;
                                app.update_outline();
                                app.update_content_items();
                            }
                        }
                        KeyCode::Char(c) => {
                            app.search_query.push(c);
                            app.update_filtered_indices();
                        }
                        _ => {}
                    }
                    continue;
                }

                match app.mode {
                    Mode::Normal => {
                        match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Tab => app.toggle_focus(),
                            KeyCode::Char('e') => app.enter_edit_mode(),
                            KeyCode::Char('n') => {
                                app.input_buffer.clear();
                                app.dialog = DialogState::CreateNote;
                            }
                            KeyCode::Char('d') => {
                                if !app.notes.is_empty() {
                                    app.dialog = DialogState::DeleteConfirm;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                match app.focus {
                                    Focus::Sidebar => app.next_note(),
                                    Focus::Outline => app.next_outline(),
                                    Focus::Content => {
                                        app.next_content_line();
                                        app.sync_outline_to_content();
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                match app.focus {
                                    Focus::Sidebar => app.previous_note(),
                                    Focus::Outline => app.previous_outline(),
                                    Focus::Content => {
                                        app.previous_content_line();
                                        app.sync_outline_to_content();
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                match app.focus {
                                    Focus::Content => app.open_current_image(),
                                    Focus::Outline => app.jump_to_outline(),
                                    Focus::Sidebar => {}
                                }
                            }
                            KeyCode::Char('o') => {
                                if app.focus == Focus::Content {
                                    app.open_current_image();
                                } else if app.focus == Focus::Outline {
                                    // 'o' on outline just jumps to content view without edit
                                    app.jump_to_outline();
                                }
                            }
                            KeyCode::Char('?') => {
                                app.dialog = DialogState::Help;
                            }
                            KeyCode::Char('/') => {
                                if app.focus == Focus::Sidebar {
                                    app.search_active = true;
                                    app.search_query.clear();
                                }
                            }
                            _ => {}
                        }
                    }
                    Mode::Edit => {
                        // Vim mode handling
                        match app.vim_mode {
                            VimMode::Normal => {
                                match key.code {
                                    KeyCode::Char('i') => {
                                        app.vim_mode = VimMode::Insert;
                                    }
                                    KeyCode::Char('a') => {
                                        app.vim_mode = VimMode::Insert;
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Forward);
                                    }
                                    KeyCode::Char('A') => {
                                        app.vim_mode = VimMode::Insert;
                                        app.textarea.move_cursor(tui_textarea::CursorMove::End);
                                    }
                                    KeyCode::Char('I') => {
                                        app.vim_mode = VimMode::Insert;
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Head);
                                    }
                                    KeyCode::Char('o') => {
                                        app.vim_mode = VimMode::Insert;
                                        app.textarea.move_cursor(tui_textarea::CursorMove::End);
                                        app.textarea.insert_newline();
                                    }
                                    KeyCode::Char('O') => {
                                        app.vim_mode = VimMode::Insert;
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Head);
                                        app.textarea.insert_newline();
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Up);
                                    }
                                    KeyCode::Char('v') => {
                                        app.vim_mode = VimMode::Visual;
                                        app.textarea.start_selection();
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Back);
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Down);
                                    }
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Up);
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Forward);
                                    }
                                    KeyCode::Char('w') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::WordForward);
                                    }
                                    KeyCode::Char('b') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::WordBack);
                                    }
                                    KeyCode::Char('0') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Head);
                                    }
                                    KeyCode::Char('$') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::End);
                                    }
                                    KeyCode::Char('g') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Top);
                                    }
                                    KeyCode::Char('G') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Bottom);
                                    }
                                    KeyCode::Char('x') => {
                                        app.textarea.delete_char();
                                    }
                                    KeyCode::Char('d') => {
                                        app.textarea.delete_line_by_head();
                                        app.textarea.delete_line_by_end();
                                        app.textarea.delete_newline();
                                    }
                                    KeyCode::Char('y') => {
                                        // Yank current line
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Head);
                                        app.textarea.start_selection();
                                        app.textarea.move_cursor(tui_textarea::CursorMove::End);
                                        app.textarea.copy();
                                        app.textarea.cancel_selection();
                                    }
                                    KeyCode::Char('p') => {
                                        app.textarea.paste();
                                    }
                                    KeyCode::Char('u') => {
                                        app.textarea.undo();
                                    }
                                    KeyCode::Char('r') if key.modifiers == KeyModifiers::CONTROL => {
                                        app.textarea.redo();
                                    }
                                    KeyCode::Esc => {
                                        app.cancel_edit();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                                        app.save_edit();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    _ => {}
                                }
                            }
                            VimMode::Insert => {
                                match key.code {
                                    KeyCode::Esc => {
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                                        app.save_edit();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    _ => {
                                        let input = Input::from(key);
                                        app.textarea.input(input);
                                    }
                                }
                            }
                            VimMode::Visual => {
                                match key.code {
                                    KeyCode::Esc => {
                                        app.textarea.cancel_selection();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Back);
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Down);
                                    }
                                    KeyCode::Char('k') | KeyCode::Up => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Up);
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Forward);
                                    }
                                    KeyCode::Char('w') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::WordForward);
                                    }
                                    KeyCode::Char('b') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::WordBack);
                                    }
                                    KeyCode::Char('0') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Head);
                                    }
                                    KeyCode::Char('$') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::End);
                                    }
                                    KeyCode::Char('g') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Top);
                                    }
                                    KeyCode::Char('G') => {
                                        app.textarea.move_cursor(tui_textarea::CursorMove::Bottom);
                                    }
                                    KeyCode::Char('y') => {
                                        app.textarea.copy();
                                        app.textarea.cancel_selection();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    KeyCode::Char('d') | KeyCode::Char('x') => {
                                        app.textarea.cut();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => {
                                        app.save_edit();
                                        app.textarea.cancel_selection();
                                        app.vim_mode = VimMode::Normal;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        app.update_editor_block();
                    }
                }
            }
            }
            _ => {}
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    // Create vertical layout: main area + status bar
    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Main area
            Constraint::Length(1), // Status bar
        ])
        .split(f.area());

    // Create main layout with left sidebar, content, and right outline
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Left Sidebar (Notes)
            Constraint::Percentage(60), // Content
            Constraint::Percentage(20), // Right Sidebar (Outline)
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
        DialogState::DeleteConfirm => render_delete_confirm_dialog(f, app),
        DialogState::Help => render_help_dialog(f, app),
        DialogState::None => {
            // Render welcome dialog on top if active
            if app.show_welcome {
                render_welcome_dialog(f, &app.theme);
            }
        }
    }
}

fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;

    // Split area for search input when search is active
    let (search_area, list_area) = if app.search_active {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    // Render search input if active
    if let Some(search_area) = search_area {
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.peach))
            .title(" Search ");

        let search_text = Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(theme.overlay0)),
            Span::styled(&app.search_query, Style::default().fg(theme.text)),
            Span::styled("_", Style::default().fg(theme.peach)),
        ]))
        .block(search_block);

        f.render_widget(search_text, search_area);
    }

    // Get visible notes (filtered or all)
    let visible_notes = app.get_visible_notes();

    let items: Vec<ListItem> = visible_notes
        .iter()
        .map(|(original_idx, note)| {
            let style = if *original_idx == app.selected_note {
                Style::default()
                    .fg(theme.yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            ListItem::new(Line::from(Span::styled(&note.title, style)))
        })
        .collect();

    let border_style = if app.focus == Focus::Sidebar && app.mode == Mode::Normal {
        Style::default().fg(theme.lavender)
    } else {
        Style::default().fg(theme.surface1)
    };

    let title = if app.search_active && !app.search_query.is_empty() {
        format!(" Notes ({}) ", visible_notes.len())
    } else {
        " Notes ".to_string()
    };

    let sidebar = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(theme.surface0)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    // Update list state selection based on visible notes
    let mut list_state = ListState::default();
    if let Some(pos) = visible_notes.iter().position(|(i, _)| *i == app.selected_note) {
        list_state.select(Some(pos));
    }

    f.render_stateful_widget(sidebar, list_area, &mut list_state);
}

fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focus == Focus::Content && app.mode == Mode::Normal;
    let theme = &app.theme;

    let border_style = if is_focused {
        Style::default().fg(theme.lavender)
    } else {
        Style::default().fg(theme.surface1)
    };

    let title = app
        .current_note()
        .map(|n| format!(" {} ", n.title))
        .unwrap_or_else(|| " Content ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if app.content_items.is_empty() {
        return;
    }

    // Calculate visible range based on cursor position
    let available_height = inner_area.height as usize;
    let cursor = app.content_cursor;

    // Calculate scroll offset to keep cursor visible
    let scroll_offset = if cursor >= available_height {
        cursor - available_height + 1
    } else {
        0
    };

    // Build constraints for visible items
    let mut constraints: Vec<Constraint> = Vec::new();
    let mut visible_indices: Vec<usize> = Vec::new();
    let mut total_height = 0u16;

    for (i, item) in app.content_items.iter().enumerate().skip(scroll_offset) {
        if total_height >= inner_area.height {
            break;
        }
        let item_height = match item {
            ContentItem::TextLine(_) => 1u16,
            ContentItem::Image(_) => 8u16,
            ContentItem::CodeLine(_) => 1u16,
            ContentItem::CodeFence(_) => 1u16,
        };
        constraints.push(Constraint::Length(item_height));
        visible_indices.push(i);
        total_height += item_height;
    }

    if constraints.is_empty() {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (chunk_idx, &item_idx) in visible_indices.iter().enumerate() {
        if chunk_idx >= chunks.len() {
            break;
        }
        let is_cursor_line = item_idx == cursor && is_focused;

        // Clone the item data to avoid borrow conflicts
        let item_clone = app.content_items[item_idx].clone();

        match item_clone {
            ContentItem::TextLine(line) => {
                render_content_line(f, &app.theme, &line, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::Image(path) => {
                render_inline_image_with_cursor(f, app, &path, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeLine(line) => {
                render_code_line(f, &app.theme, &line, chunks[chunk_idx], is_cursor_line);
            }
            ContentItem::CodeFence(lang) => {
                render_code_fence(f, &app.theme, &lang, chunks[chunk_idx], is_cursor_line);
            }
        }
    }
}

fn parse_inline_formatting<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut current_start = 0;

    while let Some((i, c)) = chars.next() {
        // Check for **bold**
        if c == '*' {
            if let Some(&(_, '*')) = chars.peek() {
                // Found **, look for closing **
                if i > current_start {
                    spans.push(Span::styled(&text[current_start..i], Style::default().fg(theme.text)));
                }
                chars.next(); // consume second *
                let bold_start = i + 2;
                let mut bold_end = None;

                while let Some((j, ch)) = chars.next() {
                    if ch == '*' {
                        if let Some(&(_, '*')) = chars.peek() {
                            bold_end = Some(j);
                            chars.next(); // consume second *
                            break;
                        }
                    }
                }

                if let Some(end) = bold_end {
                    spans.push(Span::styled(
                        &text[bold_start..end],
                        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                    ));
                    current_start = end + 2;
                } else {
                    // No closing **, treat as regular text
                    current_start = i;
                }
                continue;
            }
        }

        // Check for `code`
        if c == '`' {
            if i > current_start {
                spans.push(Span::styled(&text[current_start..i], Style::default().fg(theme.text)));
            }
            let code_start = i + 1;
            let mut code_end = None;

            while let Some((j, ch)) = chars.next() {
                if ch == '`' {
                    code_end = Some(j);
                    break;
                }
            }

            if let Some(end) = code_end {
                spans.push(Span::styled(
                    &text[code_start..end],
                    Style::default().fg(theme.green).bg(theme.mantle),
                ));
                current_start = end + 1;
            } else {
                // No closing `, treat as regular text
                current_start = i;
            }
            continue;
        }
    }

    // Add remaining text
    if current_start < text.len() {
        spans.push(Span::styled(&text[current_start..], Style::default().fg(theme.text)));
    }

    if spans.is_empty() {
        spans.push(Span::styled(text, Style::default().fg(theme.text)));
    }

    spans
}

fn render_content_line(f: &mut Frame, theme: &Theme, line: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    // Check headings from most specific (######) to least specific (#)
    let styled_line = if line.starts_with("###### ") {
        // H6: Smallest, italic, subtle
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled(
                line.trim_start_matches("###### "),
                Style::default()
                    .fg(theme.subtext0)
                    .add_modifier(Modifier::ITALIC),
            ),
        ])
    } else if line.starts_with("##### ") {
        // H5: Small, muted color
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled(
                line.trim_start_matches("##### "),
                Style::default()
                    .fg(theme.teal)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("#### ") {
        // H4: Small prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("› ", Style::default().fg(theme.mauve)),
            Span::styled(
                line.trim_start_matches("#### "),
                Style::default()
                    .fg(theme.mauve)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("### ") {
        // H3: Medium prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("▸ ", Style::default().fg(theme.yellow)),
            Span::styled(
                line.trim_start_matches("### "),
                Style::default()
                    .fg(theme.yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("## ") {
        // H2: Larger prefix
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("■ ", Style::default().fg(theme.green)),
            Span::styled(
                line.trim_start_matches("## "),
                Style::default()
                    .fg(theme.green)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("# ") {
        // H1: Largest, most prominent
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("◆ ", Style::default().fg(theme.blue)),
            Span::styled(
                line.trim_start_matches("# ").to_uppercase(),
                Style::default()
                    .fg(theme.blue)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else if line.starts_with("- ") {
        // Bullet list
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("• ", Style::default().fg(theme.mauve)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("- "), theme));
        Line::from(spans)
    } else if line.starts_with("> ") {
        // Blockquote
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("┃ ", Style::default().fg(theme.overlay0)),
            Span::styled(
                line.trim_start_matches("> "),
                Style::default().fg(theme.subtext0).add_modifier(Modifier::ITALIC),
            ),
        ])
    } else if line == "---" || line == "***" || line == "___" {
        // Horizontal rule
        Line::from(vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("─".repeat(40), Style::default().fg(theme.surface2)),
        ])
    } else if line.starts_with("* ") {
        // Bullet list (asterisk variant)
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
            Span::styled("• ", Style::default().fg(theme.mauve)),
        ];
        spans.extend(parse_inline_formatting(line.trim_start_matches("* "), theme));
        Line::from(spans)
    } else {
        // Regular text lines (including numbered lists)
        let mut spans = vec![
            Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
        ];
        spans.extend(parse_inline_formatting(line, theme));
        Line::from(spans)
    };

    let style = if is_cursor {
        Style::default().bg(theme.surface0)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(styled_line).style(style);
    f.render_widget(paragraph, area);
}

fn render_code_line(f: &mut Frame, theme: &Theme, line: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let styled_line = Line::from(vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
        Span::styled("│ ", Style::default().fg(theme.surface2)),
        Span::styled(line, Style::default().fg(theme.green)),
    ]);

    let style = if is_cursor {
        Style::default().bg(theme.surface0)
    } else {
        Style::default().bg(theme.mantle)
    };

    let paragraph = Paragraph::new(styled_line).style(style);
    f.render_widget(paragraph, area);
}

fn render_code_fence(f: &mut Frame, theme: &Theme, _lang: &str, area: Rect, is_cursor: bool) {
    let cursor_indicator = if is_cursor { "▶ " } else { "  " };

    let styled_line = Line::from(vec![
        Span::styled(cursor_indicator, Style::default().fg(theme.peach)),
        Span::styled("───", Style::default().fg(theme.surface2)),
    ]);

    let style = if is_cursor {
        Style::default().bg(theme.surface0)
    } else {
        Style::default().bg(theme.mantle)
    };

    let paragraph = Paragraph::new(styled_line).style(style);
    f.render_widget(paragraph, area);
}

fn render_inline_image_with_cursor(f: &mut Frame, app: &mut App, path: &str, area: Rect, is_cursor: bool) {
    // Check if we need to load a new image
    let need_load = match &app.current_image {
        Some(state) => state.path != path,
        None => true,
    };

    if need_load {
        // Load image from cache or disk
        let img = if let Some(img) = app.image_cache.get(path) {
            Some(img.clone())
        } else {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() {
                if let Ok(img) = image::open(&path_buf) {
                    app.image_cache.insert(path.to_string(), img.clone());
                    Some(img)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let (Some(img), Some(picker)) = (img, &mut app.picker) {
            let protocol = picker.new_resize_protocol(img);
            app.current_image = Some(ImageState {
                image: protocol,
                path: path.to_string(),
            });
        }
    }

    // Create a bordered area for the image with cursor indicator
    let theme = &app.theme;
    let border_color = if is_cursor {
        theme.peach
    } else {
        theme.sapphire
    };

    let title = if is_cursor {
        format!(" Image: {} [Enter/o to open] ", path)
    } else {
        format!(" Image: {} ", path)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner_area = block.inner(area);

    // Add background highlight when cursor is on image
    if is_cursor {
        let bg = Paragraph::new("").style(Style::default().bg(theme.surface0));
        f.render_widget(bg, area);
    }

    f.render_widget(block, area);

    if let Some(state) = &mut app.current_image {
        if state.path == path {
            let image_widget = StatefulImage::new(None);
            f.render_stateful_widget(image_widget, inner_area, &mut state.image);
        }
    } else {
        // Show placeholder if image couldn't be loaded
        let placeholder = Paragraph::new("  [Image not found]")
            .style(Style::default().fg(theme.red).add_modifier(Modifier::ITALIC));
        f.render_widget(placeholder, inner_area);
    }
}

fn render_editor(f: &mut Frame, app: &mut App, area: Rect) {
    f.render_widget(&app.textarea, area);
}

fn render_outline(f: &mut Frame, app: &mut App, area: Rect) {
    let theme = &app.theme;
    let items: Vec<ListItem> = app
        .outline
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.level.saturating_sub(1));
            let prefix = match item.level {
                1 => "# ",
                2 => "## ",
                3 => "### ",
                _ => "",
            };
            let style = match item.level {
                1 => Style::default().fg(theme.blue).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(theme.green),
                3 => Style::default().fg(theme.yellow),
                _ => Style::default().fg(theme.text),
            };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}{}", indent, prefix, item.title),
                style,
            )))
        })
        .collect();

    let border_style = if app.focus == Focus::Outline && app.mode == Mode::Normal {
        Style::default().fg(theme.lavender)
    } else {
        Style::default().fg(theme.surface1)
    };

    let outline = List::new(items)
        .block(
            Block::default()
                .title(" Outline ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(
            Style::default()
                .bg(theme.surface0)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(outline, area, &mut app.outline_state);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;

    // Calculate stats
    let (word_count, reading_time) = if let Some(note) = app.current_note() {
        let words: usize = note.content.split_whitespace().count();
        let minutes = (words as f64 / 200.0).ceil() as usize; // ~200 words per minute
        (words, minutes)
    } else {
        (0, 0)
    };

    // Calculate percentage complete based on cursor position
    let percentage = if app.content_items.is_empty() {
        0
    } else {
        ((app.content_cursor + 1) * 100) / app.content_items.len()
    };

    // Get current note file path
    let note_path = app
        .current_note()
        .and_then(|n| n.file_path.as_ref())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "No file".to_string());

    // Get current mode indicator
    let mode_indicator = match app.mode {
        Mode::Normal => match app.focus {
            Focus::Sidebar => "SIDEBAR",
            Focus::Content => "CONTENT",
            Focus::Outline => "OUTLINE",
        },
        Mode::Edit => match app.vim_mode {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
        },
    };

    // Build status bar content
    let logo = Span::styled(
        " ◆ Ekphos ",
        Style::default()
            .fg(theme.crust)
            .bg(theme.lavender)
            .add_modifier(Modifier::BOLD),
    );

    let mode = Span::styled(
        format!(" {} ", mode_indicator),
        Style::default()
            .fg(theme.crust)
            .bg(theme.peach),
    );

    let file_path = Span::styled(
        format!(" {} ", note_path),
        Style::default().fg(theme.text),
    );

    let separator = Span::styled(
        " │ ",
        Style::default().fg(theme.surface2),
    );

    let reading = Span::styled(
        format!("{} words ~{}min", word_count, reading_time),
        Style::default().fg(theme.green),
    );

    let progress = Span::styled(
        format!(" {}% ", percentage),
        Style::default()
            .fg(theme.crust)
            .bg(theme.mauve),
    );

    let help_key = Span::styled(
        " ? for help ",
        Style::default().fg(theme.overlay1).bg(theme.surface1),
    );

    // Calculate spacing for justify-between layout
    let left_content = vec![logo, Span::raw(" "), mode, Span::raw(" "), file_path];
    let right_content = vec![reading, separator.clone(), progress, Span::raw(" "), help_key];

    let left_width: usize = left_content.iter().map(|s| s.content.len()).sum();
    let right_width: usize = right_content.iter().map(|s| s.content.len()).sum();
    let available_width = area.width as usize;
    let padding = available_width.saturating_sub(left_width + right_width);

    let mut spans = left_content;
    spans.push(Span::styled(" ".repeat(padding), Style::default().bg(theme.surface0)));
    spans.extend(right_content);

    let status_line = Line::from(spans);
    let status_bar = Paragraph::new(status_line)
        .style(Style::default().bg(theme.surface0));

    f.render_widget(status_bar, area);
}

fn render_welcome_dialog(f: &mut Frame, theme: &Theme) {
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

fn render_onboarding_dialog(f: &mut Frame, app: &App) {
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

fn render_create_note_dialog(f: &mut Frame, app: &App) {
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

fn render_delete_confirm_dialog(f: &mut Frame, app: &App) {
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

fn render_help_dialog(f: &mut Frame, app: &App) {
    let area = f.area();
    let theme = &app.theme;

    // Calculate centered dialog area
    let dialog_width = 56.min(area.width.saturating_sub(4));
    let dialog_height = 29.min(area.height.saturating_sub(2));

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
