use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use image::DynamicImage;
use ratatui::{
    style::Style,
    widgets::{Block, Borders, ListState},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use tui_textarea::TextArea;

use crate::theme::{Config, Theme};

#[derive(Debug, Clone)]
pub struct Note {
    pub title: String,
    pub content: String,
    pub file_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DialogState {
    None,
    Onboarding,
    CreateNote,
    DeleteConfirm,
    RenameNote,
    Help,
    EmptyDirectory,
    DirectoryNotFound,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    Content,
    Outline,
}

#[derive(Debug, Clone)]
pub struct OutlineItem {
    pub level: usize,
    pub title: String,
    pub line: usize,
}

pub struct ImageState {
    pub image: StatefulProtocol,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum ContentItem {
    TextLine(String),
    Image(String),
    CodeLine(String),
    CodeFence(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

pub struct App<'a> {
    pub notes: Vec<Note>,
    pub selected_note: usize,
    pub list_state: ListState,
    pub focus: Focus,
    pub mode: Mode,
    pub textarea: TextArea<'a>,
    pub picker: Option<Picker>,
    pub image_cache: HashMap<String, DynamicImage>,
    pub current_image: Option<ImageState>,
    pub show_welcome: bool,
    pub outline: Vec<OutlineItem>,
    pub outline_state: ListState,
    pub vim_mode: VimMode,
    pub content_cursor: usize,
    pub content_items: Vec<ContentItem>,
    pub theme: Theme,
    pub config: Config,
    pub dialog: DialogState,
    pub input_buffer: String,
    pub search_active: bool,
    pub search_query: String,
    pub filtered_indices: Vec<usize>,
    pub editor_scroll_top: usize,
    pub editor_view_height: usize,
    pub pending_operator: Option<char>,
    pub pending_delete: Option<DeleteType>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeleteType {
    Word,
    Line,
}

impl<'a> App<'a> {
    pub fn new() -> Self {
        // Check if config exists before loading (determines if onboarding is needed)
        // This must be checked before load_or_create() which creates the config
        let config_exists = Config::exists();

        let config = Config::load_or_create();

        // For first launch: config was just created, so notes_dir won't exist yet
        let is_first_launch = !config_exists;

        let theme = Theme::from_name(&config.theme);

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut textarea = TextArea::default();
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.blue))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        // No line highlighting in normal mode - only word highlighting via selection
        textarea.set_cursor_line_style(Style::default());
        textarea.set_selection_style(
            Style::default()
                .fg(theme.selection_text)
                .bg(theme.selection_bg)
        );

        // Initialize image picker for terminal graphics
        let picker = Picker::from_query_stdio().ok();

        // Check if notes directory exists
        let notes_dir_exists = config.notes_path().exists();

        // Check if notes directory has any .md files
        let notes_dir_empty = if notes_dir_exists {
            !Self::directory_has_notes(&config.notes_path())
        } else {
            true
        };

        let dialog = if is_first_launch {
            DialogState::Onboarding
        } else if !notes_dir_exists {
            DialogState::DirectoryNotFound
        } else if notes_dir_empty {
            DialogState::EmptyDirectory
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
            show_welcome: !is_first_launch && config.welcome_shown && notes_dir_exists && !notes_dir_empty,
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
            editor_scroll_top: 0,
            editor_view_height: 0,
            pending_operator: None,
            pending_delete: None,
        };

        if !is_first_launch && notes_dir_exists {
            app.load_notes_from_dir();
        }

        app
    }

    fn directory_has_notes(path: &PathBuf) -> bool {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "md" {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn load_notes_from_dir(&mut self) {
        self.notes.clear();
        let notes_path = self.config.notes_path();

        // Create directory if it doesn't exist
        if !notes_path.exists() {
            let _ = fs::create_dir_all(&notes_path);
        }

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

        self.selected_note = 0;
        self.list_state.select(Some(0));

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
        self.update_outline();
        self.update_content_items();
    }

    pub fn create_note(&mut self, name: &str) {
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

    pub fn delete_current_note(&mut self) {
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

    pub fn rename_note(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if self.notes.is_empty() {
            return;
        }

        let notes_path = self.config.notes_path();
        let new_file_path = notes_path.join(format!("{}.md", new_name));

        if let Some(note) = self.notes.get(self.selected_note) {
            if note.title == new_name {
                return;
            }
            if new_file_path.exists() {
                return;
            }
        }

        if let Some(note) = self.notes.get_mut(self.selected_note) {
            if let Some(ref old_path) = note.file_path {
                if fs::rename(old_path, &new_file_path).is_ok() {
                    note.title = new_name.to_string();
                    note.file_path = Some(new_file_path);
                }
            }
        }

        let new_name_owned = new_name.to_string();
        self.notes.sort_by(|a, b| a.title.cmp(&b.title));

        if let Some(idx) = self.notes.iter().position(|n| n.title == new_name_owned) {
            self.selected_note = idx;
            self.list_state.select(Some(idx));
        }

        self.update_outline();
        self.update_content_items();
    }

    pub fn complete_onboarding(&mut self) {
        self.config.notes_dir = self.input_buffer.clone();
        // Save config (this creates the config file, marking onboarding as complete)
        let _ = self.config.save();

        // Create the notes directory
        let notes_path = self.config.notes_path();
        let _ = fs::create_dir_all(&notes_path);

        self.dialog = DialogState::None;
        self.load_notes_from_dir();

        // Create welcome note only on first launch
        if self.notes.is_empty() {
            self.create_welcome_note();
        }

        self.show_welcome = true;
    }

    /// Create the notes directory when it doesn't exist
    pub fn create_notes_directory(&mut self) {
        let notes_path = self.config.notes_path();
        if fs::create_dir_all(&notes_path).is_ok() {
            self.load_notes_from_dir();
            // Show empty directory dialog since we just created an empty directory
            if self.notes.is_empty() {
                self.dialog = DialogState::EmptyDirectory;
            } else {
                self.dialog = DialogState::None;
            }
        }
    }

    pub fn dismiss_welcome(&mut self) {
        self.show_welcome = false;
        self.config.welcome_shown = false; // Set to false so welcome won't show again
        let _ = self.config.save();
    }

    pub fn update_outline(&mut self) {
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

    pub fn update_content_items(&mut self) {
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

    pub fn next_content_line(&mut self) {
        if self.content_items.is_empty() {
            return;
        }
        if self.content_cursor < self.content_items.len() - 1 {
            self.content_cursor += 1;
        }
    }

    pub fn previous_content_line(&mut self) {
        if self.content_cursor > 0 {
            self.content_cursor -= 1;
        }
    }

    pub fn sync_outline_to_content(&mut self) {
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

    pub fn current_item_is_image(&self) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(self.content_cursor) {
            Some(path)
        } else {
            None
        }
    }

    pub fn open_current_image(&self) {
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

    pub fn next_note(&mut self) {
        if self.notes.is_empty() {
            return;
        }
        self.selected_note = (self.selected_note + 1) % self.notes.len();
        self.list_state.select(Some(self.selected_note));
        self.current_image = None;
        self.update_outline();
        self.update_content_items();
    }

    pub fn previous_note(&mut self) {
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

    pub fn toggle_focus(&mut self, backwards: bool) {
        self.focus = match self.focus {
            Focus::Sidebar => if backwards { Focus::Outline } else { Focus::Content },
            Focus::Content => if backwards { Focus::Sidebar } else { Focus::Outline },
            Focus::Outline => if backwards {Focus::Content} else {Focus::Sidebar},
        };
    }

    pub fn update_filtered_indices(&mut self) {
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

    pub fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.filtered_indices.clear();
    }

    pub fn get_visible_notes(&self) -> Vec<(usize, &Note)> {
        if self.search_active && !self.search_query.is_empty() {
            self.filtered_indices
                .iter()
                .filter_map(|&i| self.notes.get(i).map(|n| (i, n)))
                .collect()
        } else {
            self.notes.iter().enumerate().collect()
        }
    }

    pub fn next_outline(&mut self) {
        if self.outline.is_empty() {
            return;
        }
        let i = match self.outline_state.selected() {
            Some(i) => (i + 1) % self.outline.len(),
            None => 0,
        };
        self.outline_state.select(Some(i));
    }

    pub fn previous_outline(&mut self) {
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

    pub fn jump_to_outline(&mut self) {
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

    pub fn current_note(&self) -> Option<&Note> {
        self.notes.get(self.selected_note)
    }

    pub fn enter_edit_mode(&mut self) {
        if let Some(note) = self.current_note() {
            let lines: Vec<String> = note.content.lines().map(String::from).collect();
            let target_row = self.content_cursor.min(lines.len().saturating_sub(1));
            self.textarea = TextArea::new(lines);
            self.vim_mode = VimMode::Normal;
            self.editor_scroll_top = 0;
            for _ in 0..target_row {
                self.textarea.move_cursor(tui_textarea::CursorMove::Down);
            }
            self.update_editor_block();
            self.mode = Mode::Edit;
            self.focus = Focus::Content;
        }
    }

    pub fn update_editor_scroll(&mut self, view_height: usize) {
        self.editor_view_height = view_height;
        let (cursor_row, _) = self.textarea.cursor();

        if cursor_row < self.editor_scroll_top {
            self.editor_scroll_top = cursor_row;
        }
        else if cursor_row >= self.editor_scroll_top + view_height {
            self.editor_scroll_top = cursor_row - view_height + 1;
        }
    }

    pub fn update_editor_block(&mut self) {
        let mode_str = match self.vim_mode {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
        };
        let pending_str = match (&self.pending_delete, self.pending_operator) {
            (Some(_), _) => " [DEL]",
            (None, Some('d')) => " d-",
            _ => "",
        };
        let color = match (&self.pending_delete, self.vim_mode) {
            (Some(_), _) => self.theme.red,
            (None, VimMode::Normal) if self.pending_operator.is_some() => self.theme.yellow,
            (None, VimMode::Normal) => self.theme.blue,
            (None, VimMode::Insert) => self.theme.green,
            (None, VimMode::Visual) => self.theme.magenta,
        };
        let hint = match (&self.pending_delete, self.vim_mode) {
            (Some(_), _) => "d: Confirm, Esc: Cancel",
            (None, VimMode::Visual) => "y: Yank, d: Delete, Esc: Cancel",
            (None, _) if self.pending_operator == Some('d') => "d: Line, w: Word→, b: Word←",
            _ => "Ctrl+S: Save, Esc: Exit",
        };
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .title(format!(" {}{} | {} ", mode_str, pending_str, hint)),
        );
        self.textarea.set_selection_style(
            Style::default()
                .fg(self.theme.selection_text)
                .bg(self.theme.selection_bg)
        );
        if self.vim_mode == VimMode::Visual {
            self.textarea.set_cursor_line_style(Style::default().bg(self.theme.bright_black));
        } else {
            self.textarea.set_cursor_line_style(Style::default());
        }
    }

    pub fn save_edit(&mut self) {
        let (cursor_row, _) = self.textarea.cursor();
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
        self.content_cursor = cursor_row.min(self.content_items.len().saturating_sub(1));
    }

    pub fn cancel_edit(&mut self) {
        let (cursor_row, _) = self.textarea.cursor();
        self.mode = Mode::Normal;
        self.content_cursor = cursor_row.min(self.content_items.len().saturating_sub(1));
    }
}

impl Default for App<'_> {
    fn default() -> Self {
        Self::new()
    }
}
