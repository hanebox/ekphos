use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};

use image::DynamicImage;
use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, ListState},
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use crate::editor::{CursorMove, Editor};
use crate::highlight::Highlighter;
use crate::theme::{Config, Theme};

const WELCOME_NOTE_CONTENT: &str = r#"# Welcome to Ekphos

A lightweight, fast, terminal-based markdown research tool built with Rust.

## Layout

Ekphos has three panels:

- **Sidebar** (left): List of your notes
- **Content** (center): Note content with markdown rendering
- **Outline** (right): Auto-generated headings for quick navigation

Use `Tab` to switch between panels.

## Navigation

- `j/k` or Arrow keys: Navigate up/down
- `J/K` (Shift): Toggle floating cursor mode (view stays fixed)
- `Tab`: Switch focus between panels
- `Enter`: Jump to heading (in Outline) or open image (in Content)
- `Space`: Toggle task checkbox or open link in browser
- `]/[`: Next/previous link (when multiple links on same line)
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

### Task Lists

Track your tasks with checkboxes! Press `Space` to toggle:

- [ ] Unchecked task
- [x] Completed task
- [ ] Another pending task
- [x] This one is done too

### Tables

| Feature | Status | Notes |
|---------|--------|-------|
| Headings | Done | H1-H6 |
| Lists | Done | Bullets |
| Tables | Done | New! |

### Collapsible Details

Use `<details>` for collapsible sections. Press `Space` to toggle:

<details>
<summary>Click to expand this section</summary>

This content is hidden by default.

You can include any text here, and it will be revealed when the details section is expanded.

Use this for FAQs, spoilers, or optional information.
</details>

<details>
<summary>Another collapsible section</summary>

More hidden content here!
</details>

**Syntax example:**

```html
<details>
<summary>Summary text here</summary>

Your hidden content goes here.
Multiple lines are supported.
</details>
```

### Blockquotes

> This is a blockquote.
> It can span multiple lines.

### Code Blocks

Code blocks support **syntax highlighting** for many languages:

```rust
// Rust example with syntax highlighting
fn main() {
    let message = "Hello, Ekphos!";
    println!("{}", message);

    for i in 0..5 {
        println!("Count: {}", i);
    }
}
```

```python
# Python also works
def greet(name: str) -> str:
    return f"Hello, {name}!"

if __name__ == "__main__":
    print(greet("World"))
```

```javascript
// JavaScript too
const greet = (name) => {
    return `Hello, ${name}!`;
};
console.log(greet("Ekphos"));
```

### Horizontal Rules

---

### Images

Images can be embedded using standard markdown syntax:

```
![alt text](path/to/image.png)
![remote](https://example.com/image.png)
```

Both local files and remote URLs (http/https) are supported.

Click on an image or press `Enter`/`o` to open it in your system viewer.

Supported formats: PNG, JPEG, GIF, WebP, BMP

For inline preview, use a compatible terminal (iTerm2, Kitty, WezTerm, Sixel).

### Links

Links are rendered with special styling. Click or press `Space` to open in your browser:

- Visit the [Ekphos Website](https://ekphos.xyz) for more information
- Check out [GitHub](https://github.com) for the source code
- Multiple links on one line: [Google](https://google.com) and [DuckDuckGo](https://duckduckgo.com)

**Keyboard navigation:**
- `Space`: Open the selected link
- `]`: Next link (when multiple links on same line)
- `[`: Previous link

The selected link is highlighted with a yellow background.

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

Press `q` to quit. Happy note-taking!"#;

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
    CreateFolder,
    CreateNoteInFolder,
    DeleteConfirm,
    DeleteFolderConfirm,
    RenameNote,
    RenameFolder,
    Help,
    EmptyDirectory,
    DirectoryNotFound,
    UnsavedChanges,
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
    TaskItem { text: String, checked: bool, line_index: usize },
    TableRow { cells: Vec<String>, is_separator: bool, is_header: bool, column_widths: Vec<usize> },
    Details { summary: String, content_lines: Vec<String>, id: usize },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
}

/// Context menu state for right-click actions
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ContextMenuState {
    #[default]
    None,
    Open { x: u16, y: u16, selected_index: usize },
}

/// Context menu items
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContextMenuItem {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

impl ContextMenuItem {
    pub fn all() -> &'static [ContextMenuItem] {
        &[
            ContextMenuItem::Copy,
            ContextMenuItem::Cut,
            ContextMenuItem::Paste,
            ContextMenuItem::SelectAll,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            ContextMenuItem::Copy => "Copy",
            ContextMenuItem::Cut => "Cut",
            ContextMenuItem::Paste => "Paste",
            ContextMenuItem::SelectAll => "Select All",
        }
    }
}

#[derive(Debug, Clone)]
pub enum FileTreeItem {
    Folder {
        name: String,
        path: PathBuf,
        expanded: bool,
        children: Vec<FileTreeItem>,
        depth: usize,
    },
    Note {
        note_index: usize,
        depth: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SidebarItem {
    pub kind: SidebarItemKind,
    pub depth: usize,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub enum SidebarItemKind {
    Folder { path: PathBuf, expanded: bool },
    Note { note_index: usize },
}

pub struct App {
    pub notes: Vec<Note>,
    pub selected_note: usize,
    pub list_state: ListState,
    pub focus: Focus,
    pub mode: Mode,
    pub editor: Editor,
    pub picker: Option<Picker>,
    pub image_cache: HashMap<String, DynamicImage>,
    pub current_image: Option<ImageState>,
    pub pending_images: HashSet<String>,
    pub image_sender: Sender<(String, DynamicImage)>,
    pub image_receiver: Receiver<(String, DynamicImage)>,
    pub show_welcome: bool,
    pub outline: Vec<OutlineItem>,
    pub outline_state: ListState,
    pub vim_mode: VimMode,
    pub content_cursor: usize,
    pub content_scroll_offset: usize,
    pub floating_cursor_mode: bool,
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
    pub file_tree: Vec<FileTreeItem>,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_sidebar_index: usize,
    pub folder_states: HashMap<PathBuf, bool>,
    pub target_folder: Option<PathBuf>,
    pub dialog_error: Option<String>,
    pub search_matched_notes: Vec<usize>,
    pub content_area: Rect,
    pub mouse_hover_item: Option<usize>,
    pub content_item_rects: Vec<(usize, Rect)>,
    pub selected_link_index: usize,
    pub details_open_states: HashMap<usize, bool>,
    pub highlighter: Highlighter,
    pub sidebar_collapsed: bool,
    pub outline_collapsed: bool,
    // Mouse selection state
    pub mouse_button_held: bool,
    pub mouse_drag_start: Option<(u16, u16)>,
    pub last_mouse_y: u16,
    pub editor_area: Rect,
    pub context_menu_state: ContextMenuState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeleteType {
    Word,
    Line,
}

impl App {
    pub fn new() -> Self {
        // Check if config exists before loading (determines if onboarding is needed)
        // This must be checked before load_or_create() which creates the config
        let config_exists = Config::exists();

        let config = Config::load_or_create();

        // For first launch: config was just created, so notes_dir won't exist yet
        let is_first_launch = !config_exists;

        let theme = Theme::from_name(&config.theme);
        let syntax_theme = config.syntax_theme.clone();

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut editor = Editor::default();
        editor.set_line_wrap(config.editor.line_wrap);
        editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.blue))
                .title(" NORMAL | Ctrl+S: Save, Esc: Exit "),
        );
        // No line highlighting in normal mode - only word highlighting via selection
        editor.set_cursor_line_style(Style::default());
        editor.set_selection_style(
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

        let (image_sender, image_receiver) = mpsc::channel();

        let mut app = Self {
            notes: Vec::new(),
            selected_note: 0,
            list_state,
            focus: Focus::Sidebar,
            mode: Mode::Normal,
            editor,
            picker,
            image_cache: HashMap::new(),
            current_image: None,
            pending_images: HashSet::new(),
            image_sender,
            image_receiver,
            show_welcome: !is_first_launch && config.welcome_shown && notes_dir_exists && !notes_dir_empty,
            outline: Vec::new(),
            outline_state: ListState::default(),
            vim_mode: VimMode::Normal,
            content_cursor: 0,
            content_scroll_offset: 0,
            floating_cursor_mode: false,
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
            file_tree: Vec::new(),
            sidebar_items: Vec::new(),
            selected_sidebar_index: 0,
            folder_states: HashMap::new(),
            target_folder: None,
            dialog_error: None,
            search_matched_notes: Vec::new(),
            content_area: Rect::default(),
            mouse_hover_item: None,
            content_item_rects: Vec::new(),
            selected_link_index: 0,
            details_open_states: HashMap::new(),
            highlighter: Highlighter::new(&syntax_theme),
            sidebar_collapsed: false,
            outline_collapsed: false,
            // Mouse selection state
            mouse_button_held: false,
            mouse_drag_start: None,
            last_mouse_y: 0,
            editor_area: Rect::default(),
            context_menu_state: ContextMenuState::None,
        };

        if !is_first_launch && notes_dir_exists {
            app.load_notes_from_dir();
        }

        app
    }

    fn directory_has_notes(path: &PathBuf) -> bool {
        Self::directory_has_notes_recursive(path)
    }

    fn directory_has_notes_recursive(path: &PathBuf) -> bool {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    if entry_path.file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    if Self::directory_has_notes_recursive(&entry_path) {
                        return true;
                    }
                } else if let Some(ext) = entry_path.extension() {
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
        self.file_tree.clear();
        let notes_path = self.config.notes_path();

        if !notes_path.exists() {
            let _ = fs::create_dir_all(&notes_path);
        }

        self.file_tree = self.build_tree(&notes_path, 0);

        self.sort_tree();

        self.rebuild_sidebar_items();

        self.selected_sidebar_index = 0;
        self.sync_selected_note_from_sidebar();

        self.update_outline();
        self.update_content_items();
    }

    fn build_tree(&mut self, dir: &PathBuf, depth: usize) -> Vec<FileTreeItem> {
        let mut items = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                if path.is_dir() {
                    if path.file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    let children = self.build_tree(&path, depth + 1);

                    if self.config.show_empty_dir || Self::tree_has_notes(&children) {
                        let name = path.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        let expanded = self.folder_states
                            .get(&path)
                            .copied()
                            .unwrap_or(false);

                        items.push(FileTreeItem::Folder {
                            name,
                            path,
                            expanded,
                            children,
                            depth,
                        });
                    }
                } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let title = path.file_stem()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        let note_index = self.notes.len();
                        self.notes.push(Note {
                            title,
                            content,
                            file_path: Some(path),
                        });

                        items.push(FileTreeItem::Note {
                            note_index,
                            depth,
                        });
                    }
                }
            }
        }

        items
    }

    fn tree_has_notes(items: &[FileTreeItem]) -> bool {
        items.iter().any(|item| match item {
            FileTreeItem::Note { .. } => true,
            FileTreeItem::Folder { children, .. } => Self::tree_has_notes(children),
        })
    }

    fn sort_tree(&mut self) {
        Self::sort_tree_items(&mut self.file_tree, &self.notes);
    }

    fn sort_tree_items(items: &mut [FileTreeItem], notes: &[Note]) {
        items.sort_by(|a, b| {
            let name_a = Self::get_tree_item_name(a, notes);
            let name_b = Self::get_tree_item_name(b, notes);
            name_a.to_lowercase().cmp(&name_b.to_lowercase())
        });

        for item in items.iter_mut() {
            if let FileTreeItem::Folder { children, .. } = item {
                Self::sort_tree_items(children, notes);
            }
        }
    }

    fn get_tree_item_name<'b>(item: &'b FileTreeItem, notes: &'b [Note]) -> &'b str {
        match item {
            FileTreeItem::Folder { name, .. } => name,
            FileTreeItem::Note { note_index, .. } => &notes[*note_index].title,
        }
    }

    pub fn rebuild_sidebar_items(&mut self) {
        self.sidebar_items.clear();

        // Add root folder first
        let notes_path = self.config.notes_path();
        let root_name = notes_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Notes".to_string());

        let root_expanded = self.folder_states
            .get(&notes_path)
            .copied()
            .unwrap_or(true); // Root expanded by default

        self.sidebar_items.push(SidebarItem {
            kind: SidebarItemKind::Folder {
                path: notes_path,
                expanded: root_expanded,
            },
            depth: 0,
            display_name: root_name,
        });

        // Only add children if root is expanded
        if root_expanded {
            let tree_clone = self.file_tree.clone();
            self.flatten_tree_into_sidebar(&tree_clone, 1); // Start at depth 1
        }
    }

    fn flatten_tree_into_sidebar(&mut self, items: &[FileTreeItem], depth_offset: usize) {
        for item in items {
            match item {
                FileTreeItem::Folder { name, path, expanded, children, depth } => {
                    self.sidebar_items.push(SidebarItem {
                        kind: SidebarItemKind::Folder {
                            path: path.clone(),
                            expanded: *expanded,
                        },
                        depth: *depth + depth_offset,
                        display_name: name.clone(),
                    });

                    if *expanded {
                        self.flatten_tree_into_sidebar(children, depth_offset);
                    }
                }
                FileTreeItem::Note { note_index, depth } => {
                    self.sidebar_items.push(SidebarItem {
                        kind: SidebarItemKind::Note {
                            note_index: *note_index,
                        },
                        depth: *depth + depth_offset,
                        display_name: self.notes[*note_index].title.clone(),
                    });
                }
            }
        }
    }

    pub fn sync_selected_note_from_sidebar(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                self.selected_note = *note_index;
                self.current_image = None;
            }
        }
    }

    fn create_welcome_note(&mut self) {
        let content = WELCOME_NOTE_CONTENT.to_string();

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

        let parent_path = self.target_folder.clone()
            .unwrap_or_else(|| self.config.notes_path());
        let file_path = parent_path.join(format!("{}.md", name));

        // Don't overwrite existing files
        if file_path.exists() {
            return;
        }

        let content = format!("# {}\n\n", name);
        if fs::write(&file_path, &content).is_ok() {
            if let Some(ref folder_path) = self.target_folder {
                self.folder_states.insert(folder_path.clone(), true);
            }

            self.load_notes_from_dir();

            let name_owned = name.to_string();
            for (idx, item) in self.sidebar_items.iter().enumerate() {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    if self.notes[*note_index].title == name_owned {
                        self.selected_sidebar_index = idx;
                        self.selected_note = *note_index;
                        break;
                    }
                }
            }

            self.update_outline();
            self.update_content_items();
            self.focus = Focus::Content;
        }

        self.target_folder = None;
    }

    pub fn create_folder(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }

        let parent_path = self.target_folder.clone()
            .unwrap_or_else(|| self.config.notes_path());
        let folder_path = parent_path.join(name);

        if folder_path.exists() {
            self.dialog_error = Some(format!("Folder '{}' already exists", name));
            return false;
        }

        if fs::create_dir(&folder_path).is_ok() {
            self.target_folder = Some(folder_path);
            self.dialog_error = None;
            true
        } else {
            self.dialog_error = Some("Failed to create folder".to_string());
            false
        }
    }

    pub fn get_current_context_folder(&self) -> Option<PathBuf> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => Some(path.clone()),
                SidebarItemKind::Note { note_index } => {
                    if let Some(note) = self.notes.get(*note_index) {
                        if let Some(ref file_path) = note.file_path {
                            return file_path.parent().map(|p| p.to_path_buf());
                        }
                    }
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn get_selected_folder_path(&self) -> Option<PathBuf> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Folder { path, .. } = &item.kind {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn get_selected_folder_name(&self) -> Option<String> {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Folder { .. } = &item.kind {
                return Some(item.display_name.clone());
            }
        }
        None
    }

    pub fn delete_current_note(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                if let Some(ref path) = self.notes[*note_index].file_path {
                    let _ = fs::remove_file(path);
                }

                self.load_notes_from_dir();

                if self.selected_sidebar_index >= self.sidebar_items.len() {
                    self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
                }
                self.sync_selected_note_from_sidebar();

                self.update_outline();
                self.update_content_items();
            }
        }
    }

    pub fn delete_current_folder(&mut self) {
        if let Some(path) = self.get_selected_folder_path() {
            if fs::remove_dir_all(&path).is_ok() {
                self.folder_states.remove(&path);

                self.load_notes_from_dir();

                if self.selected_sidebar_index >= self.sidebar_items.len() {
                    self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
                }
                self.sync_selected_note_from_sidebar();

                self.update_outline();
                self.update_content_items();
            }
        }
    }

    pub fn rename_note(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            if let SidebarItemKind::Note { note_index } = &item.kind {
                let note_index = *note_index;

                if self.notes[note_index].title == new_name {
                    return;
                }

                let new_file_path = if let Some(ref old_path) = self.notes[note_index].file_path {
                    if let Some(parent) = old_path.parent() {
                        parent.join(format!("{}.md", new_name))
                    } else {
                        return;
                    }
                } else {
                    return;
                };

                if new_file_path.exists() {
                    return;
                }

                if let Some(ref old_path) = self.notes[note_index].file_path {
                    if fs::rename(old_path, &new_file_path).is_ok() {
                        self.load_notes_from_dir();

                        let new_name_owned = new_name.to_string();
                        for (idx, item) in self.sidebar_items.iter().enumerate() {
                            if let SidebarItemKind::Note { note_index } = &item.kind {
                                if self.notes[*note_index].title == new_name_owned {
                                    self.selected_sidebar_index = idx;
                                    self.selected_note = *note_index;
                                    break;
                                }
                            }
                        }

                        self.update_outline();
                        self.update_content_items();
                    }
                }
            }
        }
    }

    pub fn rename_folder(&mut self, new_name: &str) {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return;
        }

        if let Some(old_path) = self.get_selected_folder_path() {
            let old_name = old_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if old_name == new_name {
                return;
            }

            if let Some(parent) = old_path.parent() {
                let new_path = parent.join(new_name);

                if new_path.exists() {
                    self.dialog_error = Some(format!("Folder '{}' already exists", new_name));
                    return;
                }

                if fs::rename(&old_path, &new_path).is_ok() {
                    if let Some(expanded) = self.folder_states.remove(&old_path) {
                        self.folder_states.insert(new_path.clone(), expanded);
                    }

                    self.load_notes_from_dir();

                    let new_name_owned = new_name.to_string();
                    for (idx, item) in self.sidebar_items.iter().enumerate() {
                        if let SidebarItemKind::Folder { path, .. } = &item.kind {
                            if path == &new_path {
                                self.selected_sidebar_index = idx;
                                break;
                            }
                        }
                        if item.display_name == new_name_owned {
                            if let SidebarItemKind::Folder { .. } = &item.kind {
                                self.selected_sidebar_index = idx;
                                break;
                            }
                        }
                    }

                    self.update_outline();
                    self.update_content_items();
                }
            }
        }
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
            let lines: Vec<&str> = content.lines().collect();
            let mut i = 0;

            while i < lines.len() {
                let line = lines[i];
                let line_index = i;

                // Check for code fence
                if line.starts_with("```") {
                    let lang = line.trim_start_matches('`').to_string();
                    self.content_items.push(ContentItem::CodeFence(lang));
                    in_code_block = !in_code_block;
                    i += 1;
                    continue;
                }

                // If inside code block, add as CodeLine
                if in_code_block {
                    self.content_items.push(ContentItem::CodeLine(line.to_string()));
                    i += 1;
                    continue;
                }

                // Check for image
                if line.starts_with("![") && line.contains("](") && line.contains(')') {
                    if let Some(start) = line.find("](") {
                        if let Some(end) = line[start..].find(')') {
                            let path = &line[start + 2..start + end];
                            if !path.is_empty() {
                                self.content_items.push(ContentItem::Image(path.to_string()));
                                i += 1;
                                continue;
                            }
                        }
                    }
                }

                let trimmed = line.trim_start();
                if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
                    let checked = trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ");
                    let text = trimmed[6..].to_string();
                    self.content_items.push(ContentItem::TaskItem { text, checked, line_index });
                    i += 1;
                    continue;
                }

                let trimmed_line = line.trim();
                if trimmed_line.starts_with("<details") && (trimmed_line.ends_with(">") || trimmed_line.contains("><")) {
                    let details_start_line = line_index;
                    let mut summary = String::new();
                    let mut content_lines: Vec<String> = Vec::new();
                    let mut found_end = false;
                    i += 1;

                    while i < lines.len() {
                        let dline = lines[i].trim();

                        if dline.contains("</details>") {
                            found_end = true;
                            i += 1;
                            break;
                        }

                        if dline.starts_with("<summary>") || dline.contains("<summary>") {
                            if dline.contains("</summary>") {
                                if let Some(start) = dline.find("<summary>") {
                                    if let Some(end) = dline.find("</summary>") {
                                        summary = dline[start + 9..end].trim().to_string();
                                    }
                                }
                            } else {
                                summary = dline.trim_start_matches("<summary>").trim().to_string();
                            }
                            i += 1;
                            continue;
                        }

                        if dline == "</summary>" {
                            i += 1;
                            continue;
                        }

                        content_lines.push(lines[i].to_string());
                        i += 1;
                    }

                    if found_end {
                        if summary.is_empty() {
                            summary = "Details".to_string();
                        }
                        self.content_items.push(ContentItem::Details {
                            summary,
                            content_lines,
                            id: details_start_line,
                        });
                        continue;
                    } else {
                        self.content_items.push(ContentItem::TextLine(line.to_string()));
                        continue;
                    }
                }

                if trimmed_line.starts_with('|') && trimmed_line.ends_with('|') {
                    let mut table_rows: Vec<(Vec<String>, bool)> = Vec::new();

                    while i < lines.len() {
                        let tline = lines[i].trim();
                        if tline.starts_with('|') && tline.ends_with('|') {
                            let inner = &tline[1..tline.len()-1];
                            let cells: Vec<String> = inner.split('|').map(|s| s.trim().to_string()).collect();
                            let is_separator = cells.iter().all(|cell| {
                                let c = cell.trim();
                                !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')
                            });
                            table_rows.push((cells, is_separator));
                            i += 1;
                        } else {
                            break;
                        }
                    }

                    let num_cols = table_rows.iter().map(|(cells, _)| cells.len()).max().unwrap_or(0);
                    let mut column_widths: Vec<usize> = vec![0; num_cols];

                    for (cells, is_sep) in &table_rows {
                        if !is_sep {
                            for (col_idx, cell) in cells.iter().enumerate() {
                                if col_idx < column_widths.len() {
                                    column_widths[col_idx] = column_widths[col_idx].max(cell.chars().count());
                                }
                            }
                        }
                    }

                    for w in &mut column_widths {
                        *w = (*w).max(3);
                    }

                    let separator_idx = table_rows.iter().position(|(_, is_sep)| *is_sep);

                    for (row_idx, (cells, is_separator)) in table_rows.into_iter().enumerate() {
                        let is_header = separator_idx.map(|sep_idx| row_idx < sep_idx).unwrap_or(false);
                        self.content_items.push(ContentItem::TableRow {
                            cells,
                            is_separator,
                            is_header,
                            column_widths: column_widths.clone(),
                        });
                    }
                    continue;
                }

                self.content_items.push(ContentItem::TextLine(line.to_string()));
                i += 1;
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
            self.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn previous_content_line(&mut self) {
        if self.content_cursor > 0 {
            self.content_cursor -= 1;
            self.selected_link_index = 0; // Reset link selection when moving lines
        }
    }

    pub fn toggle_floating_cursor(&mut self) {
        self.floating_cursor_mode = !self.floating_cursor_mode;
    }

    pub fn floating_move_down(&mut self) {
        if self.content_items.is_empty() || !self.floating_cursor_mode {
            return;
        }

        if self.content_cursor < self.content_items.len() - 1 {
            self.content_cursor += 1;
            self.selected_link_index = 0;
        }
    }

    pub fn floating_move_up(&mut self) {
        if !self.floating_cursor_mode {
            return;
        }

        if self.content_cursor > 0 {
            self.content_cursor -= 1;
            self.selected_link_index = 0;
        }
    }

    pub fn toggle_current_task(&mut self) {
        let saved_cursor = self.content_cursor;

        if let Some(item) = self.content_items.get(self.content_cursor) {
            if let ContentItem::TaskItem { line_index, checked, .. } = item {
                let line_index = *line_index;
                let new_checked = !*checked;

                if let Some(note) = self.notes.get_mut(self.selected_note) {
                    let lines: Vec<&str> = note.content.lines().collect();
                    if line_index < lines.len() {
                        let line = lines[line_index];
                        let new_line = if new_checked {
                            line.replacen("- [ ]", "- [x]", 1)
                        } else {
                            line.replacen("- [x]", "- [ ]", 1)
                                .replacen("- [X]", "- [ ]", 1)
                        };

                        let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
                        new_lines[line_index] = new_line;
                        note.content = new_lines.join("\n");

                        if let Some(ref path) = note.file_path {
                            let _ = fs::write(path, &note.content);
                        }
                    }
                }

                self.update_content_items();
                self.content_cursor = saved_cursor.min(self.content_items.len().saturating_sub(1));
            }
        }
    }

    pub fn toggle_current_details(&mut self) {
        if let Some(item) = self.content_items.get(self.content_cursor) {
            if let ContentItem::Details { id, .. } = item {
                let id = *id;
                let current = self.details_open_states.get(&id).copied().unwrap_or(false);
                self.details_open_states.insert(id, !current);
            }
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

    pub fn current_item_link(&self) -> Option<String> {
        let links = self.item_links_at(self.content_cursor);
        if links.is_empty() {
            return None;
        }
        let idx = self.selected_link_index.min(links.len().saturating_sub(1));
        links.get(idx).map(|(_, url, _, _)| url.clone())
    }

    pub fn current_line_link_count(&self) -> usize {
        self.item_links_at(self.content_cursor).len()
    }

    pub fn next_link(&mut self) {
        let link_count = self.current_line_link_count();
        if link_count > 1 {
            self.selected_link_index = (self.selected_link_index + 1) % link_count;
        }
    }

    pub fn previous_link(&mut self) {
        let link_count = self.current_line_link_count();
        if link_count > 1 {
            if self.selected_link_index == 0 {
                self.selected_link_index = link_count - 1;
            } else {
                self.selected_link_index -= 1;
            }
        }
    }

    pub fn item_link_at(&self, index: usize) -> Option<String> {
        self.item_links_at(index).first().map(|(_, url, _, _)| url.clone())
    }

    /// Extract all links from a specific content item as (text, url, start_col, end_col) tuples
    /// The columns are character positions in the rendered line (after prefix like "▶ " or "• ")
    pub fn item_links_at(&self, index: usize) -> Vec<(String, String, usize, usize)> {
        let text = match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => line.as_str(),
            Some(ContentItem::TaskItem { text, .. }) => text.as_str(),
            _ => return Vec::new(),
        };

        let mut links = Vec::new();
        let mut search_start = 0;

        while search_start < text.len() {
            let remaining = &text[search_start..];
            if let Some(bracket_pos) = remaining.find('[') {
                let abs_bracket_pos = search_start + bracket_pos;
                let from_bracket = &text[abs_bracket_pos..];

                if let Some(bracket_end) = from_bracket.find("](") {
                    let after_bracket = &from_bracket[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &from_bracket[1..bracket_end];
                        let url = &after_bracket[..paren_end];

                        if !url.is_empty() {
                            let rendered_start = Self::calc_rendered_pos(text, abs_bracket_pos);
                            let rendered_end = rendered_start + link_text.chars().count();

                            links.push((
                                link_text.to_string(),
                                url.to_string(),
                                rendered_start,
                                rendered_end,
                            ));
                        }

                        search_start = abs_bracket_pos + bracket_end + 2 + paren_end + 1;
                        continue;
                    }
                }
            }
            break;
        }

        links
    }

    fn calc_rendered_pos(text: &str, target_pos: usize) -> usize {
        let mut rendered_pos = 0;
        let mut i = 0;

        while i < target_pos && i < text.len() {
            let remaining = &text[i..];
            if remaining.starts_with('[') {
                if let Some(bracket_end) = remaining.find("](") {
                    let after_bracket = &remaining[bracket_end + 2..];
                    if let Some(paren_end) = after_bracket.find(')') {
                        let link_text = &remaining[1..bracket_end];
                        let full_link_len = bracket_end + 2 + paren_end + 1;

                        if i + full_link_len <= target_pos {
                            rendered_pos += link_text.chars().count();
                            i += full_link_len;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            rendered_pos += 1;
            i += remaining.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }

        rendered_pos
    }

    /// find which link was clicked based on column position within the content area
    /// Returns the URL if a link was clicked, None otherwise
    /// `col` is the column relative to the content area start
    pub fn find_clicked_link(&self, index: usize, col: u16, content_x: u16) -> Option<String> {
        let links = self.item_links_at(index);
        if links.is_empty() {
            return None;
        }

        let prefix_len = self.get_line_prefix_len(index);
        let click_col = (col.saturating_sub(content_x)) as usize;

        for (_, url, start, end) in &links {
            let adjusted_start = prefix_len + *start;
            let adjusted_end = prefix_len + *end;
            if click_col >= adjusted_start && click_col < adjusted_end {
                return Some(url.clone());
            }
        }

        links.first().map(|(_, url, _, _)| url.clone())
    }

    fn get_line_prefix_len(&self, index: usize) -> usize {
        match self.content_items.get(index) {
            Some(ContentItem::TextLine(line)) => {
                let mut len = 2; 
                if line.starts_with("- ") || line.starts_with("* ") {
                    len += 2; 
                }
                len
            }
            Some(ContentItem::TaskItem { .. }) => 6, 
            _ => 2,
        }
    }

    pub fn item_is_image_at(&self, index: usize) -> Option<&str> {
        if let Some(ContentItem::Image(path)) = self.content_items.get(index) {
            Some(path)
        } else {
            None
        }
    }

    pub fn item_is_details_at(&self, index: usize) -> bool {
        matches!(self.content_items.get(index), Some(ContentItem::Details { .. }))
    }

    pub fn toggle_details_at(&mut self, index: usize) {
        if let Some(ContentItem::Details { id, .. }) = self.content_items.get(index) {
            let id = *id;
            let current = self.details_open_states.get(&id).copied().unwrap_or(false);
            self.details_open_states.insert(id, !current);
        }
    }

    pub fn open_current_link(&self) {
        if let Some(url) = self.current_item_link() {
            #[cfg(target_os = "macos")]
            let _ = Command::new("open").arg(&url).spawn();
            #[cfg(target_os = "linux")]
            let _ = Command::new("xdg-open").arg(&url).spawn();
            #[cfg(target_os = "windows")]
            let _ = Command::new("cmd").args(["/c", "start", "", &url]).spawn();
        }
    }

    pub fn open_current_image(&self) {
        if let Some(path) = self.current_item_is_image() {
            let is_url = path.starts_with("http://") || path.starts_with("https://");
            let should_open = is_url || PathBuf::from(path).exists();

            if should_open {
                #[cfg(target_os = "macos")]
                let _ = Command::new("open").arg(path).spawn();
                #[cfg(target_os = "linux")]
                let _ = Command::new("xdg-open").arg(path).spawn();
                #[cfg(target_os = "windows")]
                let _ = Command::new("cmd").args(["/c", "start", "", path]).spawn();
            }
        }
    }

    pub fn next_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = (self.selected_sidebar_index + 1) % self.sidebar_items.len();
        self.sync_selected_note_from_sidebar();
        self.update_outline();
        self.update_content_items();
    }

    pub fn previous_sidebar_item(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }
        self.selected_sidebar_index = if self.selected_sidebar_index == 0 {
            self.sidebar_items.len() - 1
        } else {
            self.selected_sidebar_index - 1
        };
        self.sync_selected_note_from_sidebar();
        self.update_outline();
        self.update_content_items();
    }

    pub fn handle_sidebar_enter(&mut self) {
        if let Some(item) = self.sidebar_items.get(self.selected_sidebar_index) {
            match &item.kind {
                SidebarItemKind::Folder { path, .. } => {
                    self.toggle_folder(path.clone());
                }
                SidebarItemKind::Note { .. } => {
                }
            }
        }
    }

    pub fn toggle_folder(&mut self, path: PathBuf) {
        let new_state = !self.folder_states.get(&path).copied().unwrap_or(false);
        self.folder_states.insert(path.clone(), new_state);

        Self::update_folder_in_tree(&mut self.file_tree, &path, new_state);

        self.rebuild_sidebar_items();

        if self.selected_sidebar_index >= self.sidebar_items.len() {
            self.selected_sidebar_index = self.sidebar_items.len().saturating_sub(1);
        }

        self.sync_selected_note_from_sidebar();
    }

    fn update_folder_in_tree(items: &mut [FileTreeItem], target_path: &PathBuf, new_state: bool) {
        for item in items {
            if let FileTreeItem::Folder { path, expanded, children, .. } = item {
                if path == target_path {
                    *expanded = new_state;
                    return;
                }
                Self::update_folder_in_tree(children, target_path, new_state);
            }
        }
    }

    pub fn toggle_focus(&mut self, backwards: bool) {
        self.focus = match self.focus {
            Focus::Sidebar => if backwards { Focus::Outline } else { Focus::Content },
            Focus::Content => if backwards { Focus::Sidebar } else { Focus::Outline },
            Focus::Outline => if backwards {Focus::Content} else {Focus::Sidebar},
        };
    }

    pub fn toggle_sidebar_collapsed(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    pub fn toggle_outline_collapsed(&mut self) {
        self.outline_collapsed = !self.outline_collapsed;
    }

    pub fn update_filtered_indices(&mut self) {
        if self.search_query.is_empty() {
            self.search_matched_notes.clear();
            self.filtered_indices.clear();
            return;
        }

        let query = self.search_query.to_lowercase();

        self.search_matched_notes = self.notes
            .iter()
            .enumerate()
            .filter(|(_, note)| note.title.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect();

        for &note_index in &self.search_matched_notes {
            if let Some(note) = self.notes.get(note_index) {
                if let Some(ref file_path) = note.file_path {
                    let notes_root = self.config.notes_path();
                    let mut current = file_path.parent();
                    while let Some(parent) = current {
                        if parent == notes_root {
                            break;
                        }
                        self.folder_states.insert(parent.to_path_buf(), true);
                        current = parent.parent();
                    }
                }
            }
        }

        Self::update_tree_expanded_states(&mut self.file_tree, &self.folder_states);

        self.rebuild_sidebar_items();

        self.filtered_indices = self.sidebar_items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if let SidebarItemKind::Note { note_index } = &item.kind {
                    self.search_matched_notes.contains(note_index)
                } else {
                    false
                }
            })
            .map(|(i, _)| i)
            .collect();

        if !self.filtered_indices.is_empty() {
            self.selected_sidebar_index = self.filtered_indices[0];
            self.sync_selected_note_from_sidebar();
            self.update_outline();
            self.update_content_items();
        }
    }

    fn update_tree_expanded_states(items: &mut [FileTreeItem], folder_states: &HashMap<PathBuf, bool>) {
        for item in items {
            if let FileTreeItem::Folder { path, expanded, children, .. } = item {
                if let Some(&state) = folder_states.get(path) {
                    *expanded = state;
                }
                Self::update_tree_expanded_states(children, folder_states);
            }
        }
    }

    pub fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.filtered_indices.clear();
        self.search_matched_notes.clear();
    }

    pub fn get_visible_sidebar_indices(&self) -> Vec<usize> {
        if self.search_active && !self.search_query.is_empty() {
            self.filtered_indices.clone()
        } else {
            (0..self.sidebar_items.len()).collect()
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
            self.editor = Editor::new(lines);
            self.vim_mode = VimMode::Normal;
            self.editor_scroll_top = 0;
            for _ in 0..target_row {
                self.editor.move_cursor(CursorMove::Down);
            }
            self.update_editor_block();
            self.mode = Mode::Edit;
            self.focus = Focus::Content;
        }
    }

    pub fn update_editor_scroll(&mut self, view_height: usize) {
        self.editor_view_height = view_height;
        self.editor.update_scroll(view_height);
        self.editor_scroll_top = self.editor.scroll_offset();
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
        self.editor.set_block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color))
                .title(format!(" {}{} | {} ", mode_str, pending_str, hint)),
        );
        self.editor.set_selection_style(
            Style::default()
                .fg(self.theme.selection_text)
                .bg(self.theme.selection_bg)
        );
        self.editor.set_cursor_line_style(Style::default());
    }

    pub fn save_edit(&mut self) {
        let (cursor_row, _) = self.editor.cursor();
        if let Some(note) = self.notes.get_mut(self.selected_note) {
            note.content = self.editor.lines().join("\n");
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
        let (cursor_row, _) = self.editor.cursor();
        self.mode = Mode::Normal;
        self.content_cursor = cursor_row.min(self.content_items.len().saturating_sub(1));
    }

    pub fn has_unsaved_changes(&self) -> bool {
        if let Some(note) = self.notes.get(self.selected_note) {
            let current_content = self.editor.lines().join("\n");
            current_content != note.content
        } else {
            false
        }
    }

    pub fn poll_pending_images(&mut self) {
        while let Ok((url, img)) = self.image_receiver.try_recv() {
            self.pending_images.remove(&url);
            self.image_cache.insert(url, img);
        }
    }

    pub fn is_image_pending(&self, url: &str) -> bool {
        self.pending_images.contains(url)
    }

    pub fn start_remote_image_fetch(&mut self, url: &str) {
        if self.pending_images.contains(url) || self.image_cache.contains_key(url) {
            return;
        }

        self.pending_images.insert(url.to_string());
        let url_owned = url.to_string();
        let sender = self.image_sender.clone();

        std::thread::spawn(move || {
            if let Some(img) = fetch_remote_image_blocking(&url_owned) {
                let _ = sender.send((url_owned, img));
            }
        });
    }

    // ==================== Mouse Selection Helpers ====================

    /// Convert mouse screen coordinates to editor row/col.
    /// Returns None if mouse is outside the editor area.
    pub fn screen_to_editor_coords(&self, mouse_x: u16, mouse_y: u16) -> Option<(usize, usize)> {
        // Check if mouse is within editor bounds (account for border)
        let inner_x = self.editor_area.x + 1;
        let inner_y = self.editor_area.y + 1;
        let inner_width = self.editor_area.width.saturating_sub(2);
        let inner_height = self.editor_area.height.saturating_sub(2);

        if mouse_x < inner_x || mouse_x >= inner_x + inner_width ||
           mouse_y < inner_y || mouse_y >= inner_y + inner_height {
            return None;
        }

        // Calculate relative position within editor
        let rel_x = (mouse_x - inner_x) as usize;
        let rel_y = (mouse_y - inner_y) as usize;

        // Add scroll offset to get actual row
        let row = rel_y + self.editor_scroll_top;
        let col = rel_x;

        Some((row, col))
    }

    /// Check if mouse is in the auto-scroll zone (top or bottom edge).
    /// Returns scroll direction: -1 for up, 1 for down, 0 for no scroll.
    pub fn get_auto_scroll_direction(&self, mouse_y: u16) -> i8 {
        const SCROLL_THRESHOLD: u16 = 2;

        let inner_y = self.editor_area.y + 1;
        let inner_height = self.editor_area.height.saturating_sub(2);

        if mouse_y < inner_y + SCROLL_THRESHOLD && self.editor_scroll_top > 0 {
            -1 // Scroll up
        } else if mouse_y >= inner_y + inner_height - SCROLL_THRESHOLD {
            1 // Scroll down
        } else {
            0
        }
    }
}

fn fetch_remote_image_blocking(url: &str) -> Option<DynamicImage> {
    use std::io::Read;

    let response = ureq::get(url)
        .set("User-Agent", "ekphos/0.4")
        .call()
        .ok()?;

    let content_type = response
        .header("Content-Type")
        .unwrap_or("")
        .to_lowercase();

    if !content_type.starts_with("image/") {
        return None;
    }

    let mut bytes = Vec::new();
    response.into_reader().take(10 * 1024 * 1024).read_to_end(&mut bytes).ok()?;

    image::load_from_memory(&bytes).ok()
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
