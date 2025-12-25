# Ekphos

[![Crates.io](https://img.shields.io/crates/v/ekphos)](https://crates.io/crates/ekphos)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/ekphos)](https://github.com/hanebox/ekphos/blob/main/LICENSE)

An open source, lightweight, fast, terminal-based markdown research tool built with Rust.

![Ekphos Preview](examples/ekphos-screenshot.png)

> **Important:** Ekphos is moving fast. Always update to the latest version with `cargo install ekphos`. If you encounter config issues after updating, run `ekphos --reset` (available since v0.7.0) to reset your configuration and apply breaking changes correctly.

## Table of Contents

- [Getting Started](#getting-started)
  - [Requirements](#requirements)
  - [Installation](#installation)
    - [Using Cargo (Recommended)](#using-cargo-recommended)
    - [Using Make](#using-make)
    - [Using Docker](#using-docker)
  - [Update](#update)
  - [Uninstall](#uninstall)
  - [CLI Options](#cli-options)
    - [Opening Files and Folders](#opening-files-and-folders)
    - [Resetting Configuration](#resetting-configuration)
- [Configuration](#configuration)
  - [Config File](#config-file)
  - [Themes](#themes)
    - [Bundled Theme](#bundled-theme)
    - [Theme Format](#theme-format)
    - [Creating Custom Themes](#creating-custom-themes)
    - [Contributing Themes](#contributing-themes)
- [Usage](#usage)
  - [Layout](#layout)
  - [Folder Tree](#folder-tree)
  - [Creating Notes](#creating-notes)
  - [Creating Folders](#creating-folders)
  - [Renaming](#renaming)
  - [Deleting](#deleting)
  - [Searching Notes](#searching-notes)
  - [Editing Notes](#editing-notes)
  - [Editor Syntax Highlighting](#editor-syntax-highlighting)
  - [Markdown Support](#markdown-support)
  - [Syntax Highlighting](#syntax-highlighting)
  - [Images](#images)
    - [Adding Images](#adding-images)
    - [Viewing Images](#viewing-images)
    - [Terminal Image Support](#terminal-image-support)
  - [Links](#links)
  - [Wiki Links](#wiki-links)
  - [Collapsible Details](#collapsible-details)
  - [Using the Outline](#using-the-outline)
- [Keyboard Shortcuts](#keyboard-shortcuts)
  - [Global](#global)
  - [Sidebar](#sidebar)
  - [Content View](#content-view)
  - [Edit Mode](#edit-mode)
    - [Normal Mode](#normal-mode)
    - [Delete Commands Flow](#delete-commands-flow)
  - [Visual Mode](#visual-mode)
  - [Mouse Selection](#mouse-selection)
- [Contributing](#contributing)
  - [Development Setup](#development-setup)
  - [Branch Strategy](#branch-strategy)
  - [Workflow](#workflow)
- [Disclaimer](#disclaimer)
- [Socials](#socials)
- [License](#license)

## Getting Started

### Requirements

- Rust 1.70+ (run `rustup update` to update)
- A terminal emulator (for inline image preview: iTerm2, Kitty, WezTerm, Ghostty, or Sixel-compatible terminal)

### Installation

#### Using Cargo (Recommended)

```bash
cargo install ekphos
```

#### Using Make

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
make
sudo make install
```

#### Using Docker

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
docker build -t ekphos-ssh .
docker compose up -d
```

After the container is up, SSH into the machine:

```bash
ssh ekphos@your-docker-container-ip
```

### Update

```bash
cargo install ekphos
```

### Uninstall

**If installed with Cargo:**

```bash
cargo uninstall ekphos
```

**If installed with Make:**

```bash
cd ekphos  # navigate to the cloned repo
sudo make uninstall
```

### CLI Options

| Flag              | Description                         |
| ----------------- | ----------------------------------- |
| `-h`, `--help`    | Print help information              |
| `-v`, `--version` | Print version                       |
| `-c`, `--config`  | Print config file path              |
| `-d`, `--dir`     | Print notes directory               |
| `--reset`         | Reset config and themes to defaults |

#### Opening Files and Folders

You can open files or folders directly from the command line:

```bash
ekphos [PATH]
```

| Command                | Behavior                                                            |
| ---------------------- | ------------------------------------------------------------------- |
| `ekphos ~/notes`       | Opens `~/notes` folder as the notes directory                       |
| `ekphos ./my-note.md`  | Opens `my-note.md` and sets its parent folder as the notes directory |
| `ekphos .`             | Opens the current directory as the notes folder                     |

**Features:**

- Supports tilde (`~`) expansion for home directory paths
- Accepts both relative and absolute paths
- When opening a `.md` file, automatically selects it in the sidebar
- Skips the welcome dialog when opening via CLI path

#### Resetting Configuration

If you encounter issues after a breaking update (e.g., theme format changes), you can reset your configuration:

```bash
ekphos --reset
```

This will:

- Delete your config file (`~/.config/ekphos/config.toml`)
- Delete your themes directory (`~/.config/ekphos/themes/`)
- Regenerate fresh defaults from the latest version

Your notes are **not** affected.

---

## Configuration

### Config File

Configuration is stored in `~/.config/ekphos/config.toml`.

```toml
# ~/.config/ekphos/config.toml
notes_dir = "~/Documents/ekphos"
welcome_shown = false
theme = "ekphos-dawn"
show_empty_dir = true
syntax_theme = "base16-ocean.dark"

[editor]
line_wrap = true
tab_width = 4
left_padding = 0
right_padding = 1
```

| Setting          | Description                            | Default              |
| ---------------- | -------------------------------------- | -------------------- |
| `notes_dir`      | Directory where notes are stored       | `~/Documents/ekphos` |
| `welcome_shown`  | Show welcome dialog on startup         | `true`               |
| `theme`          | Theme name (without .toml extension)   | `ekphos-dawn`        |
| `show_empty_dir` | Show folders that contain no .md files | `true`               |
| `syntax_theme`   | Syntax highlighting theme for code     | `base16-ocean.dark`  |

**Editor settings:**

| Setting                | Description                          | Default |
| ---------------------- | ------------------------------------ | ------- |
| `editor.line_wrap`     | Enable soft line wrapping in editor  | `true`  |
| `editor.tab_width`     | Number of spaces to display for tabs | `4`     |
| `editor.left_padding`  | Left padding in editor (columns)     | `0`     |
| `editor.right_padding` | Right padding in editor (columns)    | `1`     |

> **Note:** This configuration format requires v0.3.0 or later.

### Themes

Ekphos uses its own theme format designed for simplicity and semantic color naming. Themes are stored in `~/.config/ekphos/themes/` as `.toml` files.

#### Bundled Theme

- `ekphos-dawn` (default) - A smooth dark theme with blue accents

#### Theme Format

Ekphos themes use a clean, semantic structure with both base sections and component-specific overrides:

**Base Sections:**

| Section      | Purpose                                           |
| ------------ | ------------------------------------------------- |
| `[base]`     | Core background and text colors                   |
| `[accent]`   | Primary and secondary accent colors               |
| `[semantic]` | Functional colors (error, warning, success, info) |
| `[ui]`       | UI element colors (borders, selection, cursor)    |

**Component Sections (Optional):**

| Section          | Purpose                       |
| ---------------- | ----------------------------- |
| `[ui.statusbar]` | Status bar specific colors    |
| `[ui.dialog]`    | Dialog/popup specific colors  |
| `[ui.sidebar]`   | Sidebar specific colors       |
| `[ui.content]`   | Content view specific colors  |
| `[ui.outline]`   | Outline panel specific colors |

**Base Color Reference:**

| Color                  | Usage                                   |
| ---------------------- | --------------------------------------- |
| `background`           | Main background                         |
| `background_secondary` | Popups, code blocks                     |
| `foreground`           | Primary text                            |
| `muted`                | Secondary text, hints                   |
| `primary`              | Focused borders, brand accent, headings |
| `secondary`            | Secondary accent, visual mode           |
| `error`                | Error messages, invalid links           |
| `warning`              | Warnings, selected items                |
| `success`              | Success messages, valid states          |
| `info`                 | Info messages, links                    |
| `border`               | Unfocused borders                       |
| `border_focused`       | Focused panel borders                   |
| `selection`            | Text selection background               |
| `cursor`               | Cursor color                            |

#### Creating Custom Themes

Create a `.toml` file in the themes directory:

```toml
# ~/.config/ekphos/themes/mytheme.toml

[base]
background = "#1a1b26"
background_secondary = "#24283b"
foreground = "#c0caf5"
muted = "#565f89"

[accent]
primary = "#7aa2f7"
secondary = "#bb9af7"

[semantic]
error = "#f7768e"
warning = "#e0af68"
success = "#9ece6a"
info = "#7dcfff"

[ui]
border = "#3b4261"
border_focused = "#7aa2f7"
selection = "#283457"
cursor = "#c0caf5"

# Optional: Component-specific overrides
[ui.statusbar]
background = "#1a1b26"
foreground = "#c0caf5"
brand = "#7aa2f7"
mode = "#565f89"
separator = "#3b4261"

[ui.dialog]
background = "#1a1b26"
border = "#7aa2f7"
title = "#7aa2f7"
text = "#c0caf5"

[ui.sidebar]
background = "#1a1b26"
item = "#c0caf5"
item_selected = "#e0af68"
folder = "#7dcfff"
folder_expanded = "#7dcfff"

[ui.content]
background = "#1a1b26"
text = "#c0caf5"
heading1 = "#7aa2f7"
heading2 = "#9ece6a"
heading3 = "#e0af68"
heading4 = "#bb9af7"
link = "#7dcfff"
link_invalid = "#f7768e"
code = "#9ece6a"
code_background = "#24283b"
blockquote = "#565f89"
list_marker = "#bb9af7"

[ui.outline]
background = "#1a1b26"
heading1 = "#7aa2f7"
heading2 = "#9ece6a"
heading3 = "#e0af68"
heading4 = "#bb9af7"
```

Then set in config:

```toml
theme = "mytheme"
```

> **Tip:** All color values use hex format (`#RRGGBB`). Component sections are optional - missing values fall back to base section colors, then to default ekphos-dawn colors.

#### Contributing Themes

Want to share your theme with the community? Here's how:

1. **Create your theme file** in the `/themes` folder of the repository:

   ```
   themes/your-theme-name.toml
   ```

2. **Follow the naming convention:**

   - Use lowercase with hyphens: `tokyo-night.toml`, `dracula.toml`
   - Be descriptive: `nord-light.toml`, `gruvbox-dark.toml`

3. **Include a header comment** with theme info:

   ```toml
   # Theme Name - Brief Description
   # Author: Your Name (optional)
   # Inspired by: Original theme (if applicable)

   [base]
   ...
   ```

4. **Test your theme** by copying it to `~/.config/ekphos/themes/` and setting it in your config

5. **Submit a PR** to the `main` branch with:
   - Your theme file in `/themes`
   - A brief description in the PR

**Theme Guidelines:**

- Ensure sufficient contrast between background and foreground
- Test all UI elements (dialogs, status bar, editor, content, outline)
- Consider both focused and unfocused states for borders
- Component sections are optional but allow fine-grained customization

---

## Usage

### Layout

Ekphos has three panels:

| Panel       | Position | Description                            |
| ----------- | -------- | -------------------------------------- |
| **Sidebar** | Left     | Collapsible folder tree with notes     |
| **Content** | Center   | Note content with markdown rendering   |
| **Outline** | Right    | Auto-generated headings for navigation |

Use `Tab` or `Shift+Tab` to switch between panels.

**Collapsible Panels:**

- Press `Ctrl+b` to collapse/expand the sidebar (shows ≡ icon with note count when collapsed)
- Press `Ctrl+o` to collapse/expand the outline (shows heading symbols ◆■▸ when collapsed)

### Folder Tree

The sidebar displays a hierarchical folder tree that automatically detects subdirectories containing `.md` files:

- Folders are shown with `▶` (collapsed) or `▼` (expanded) icons
- Press `Enter` on a folder to toggle expand/collapse
- Folders and notes are sorted alphabetically together
- Folders start collapsed by default

### Creating Notes

1. Press `n` to create a new note
2. Enter the note name
3. Press `Enter` to confirm

Notes are stored as `.md` files in your configured notes directory.

**Context-aware:** When your cursor is on a folder or a note inside a folder, pressing `n` will create the new note in that folder.

### Creating Folders

1. Press `N` (Shift+N) to create a new folder
2. Enter the folder name
3. Press `Enter` to confirm
4. A dialog will appear to create the first note in the folder
5. Enter the note name and press `Enter` (or `Esc` to cancel and remove the empty folder)

**Context-aware:** When your cursor is on a folder, pressing `N` will create the new folder as a subfolder.

### Renaming

1. Select a note or folder in the sidebar
2. Press `r` to rename
3. Edit the name and press `Enter` to confirm (or `Esc` to cancel)

### Deleting

1. Select a note or folder in the sidebar
2. Press `d` to delete
3. Confirm with `y` or cancel with `n`

> **Warning:** Deleting a folder will remove all notes inside it!

### Searching Notes

1. Press `/` in the sidebar to start searching
2. Type your search query
3. Results are highlighted in green, title shows match count
4. Use `Arrow keys` or `Ctrl+j/k/n/p` to navigate between matches
5. Press `Enter` to select and close search
6. Press `Esc` to cancel search

**Features:**

- Searches all notes recursively, including those in collapsed folders
- Auto-expands folders containing matched notes
- Border color indicates status: yellow (typing), green (matches found), red (no matches)

### Editing Notes

1. Select a note in the sidebar
2. Press `e` to enter edit mode
3. Edit using vim keybindings
4. Press `Ctrl+s` to save
5. Press `Esc` to exit edit mode (discards unsaved changes)

### Editor Syntax Highlighting

The editor provides real-time markdown syntax highlighting while you type:

| Syntax               | Style                        |
| -------------------- | ---------------------------- |
| `# H1`               | Blue + Bold                  |
| `## H2`              | Green + Bold                 |
| `### H3`             | Yellow + Bold                |
| `#### H4`            | Magenta + Bold               |
| `##### H5`           | Cyan + Bold                  |
| `###### H6`          | Gray + Bold                  |
| `**bold**`           | Bold                         |
| `*italic*`           | Italic                       |
| `` `code` ``         | Green                        |
| ` ``` ` blocks       | Green                        |
| `[link](url)`        | Cyan + Underline             |
| `[[wiki link]]`      | Cyan (valid) / Red (invalid) |
| `>` blockquote       | Cyan                         |
| `- * +` list markers | Yellow                       |
| `1. 2.` ordered list | Yellow                       |
| `[ ] [x]` task boxes | Cyan                         |

Highlighting updates automatically as you edit, helping you visualize markdown structure without leaving edit mode.

### Markdown Support

| Syntax           | Rendered As                 |
| ---------------- | --------------------------- |
| `# Heading`      | ◆ HEADING (blue)            |
| `## Heading`     | ■ Heading (green)           |
| `### Heading`    | ▸ Heading (yellow)          |
| `#### Heading`   | › Heading (mauve)           |
| `##### Heading`  | Heading (teal)              |
| `###### Heading` | _Heading_ (subtle)          |
| `- item`         | • item                      |
| `- [ ] task`     | [ ] task (unchecked)        |
| `- [x] task`     | [x] task (checked)          |
| `` `code` ``     | Inline code (green)         |
| ` ```lang `      | Syntax-highlighted code     |
| `![alt](path)`   | Inline image                |
| `[text](url)`    | Clickable link (cyan)       |
| `[[note]]`       | Wiki link (cyan/red)        |
| `\| table \|`    | Formatted table             |
| `<details>`      | Collapsible dropdown (cyan) |

### Syntax Highlighting

Code blocks with a language specifier are syntax-highlighted using [syntect](https://github.com/trishume/syntect):

````markdown
```rust
fn main() {
    let message = "Hello, Ekphos!";
    println!("{}", message);
}
```
````

**Supported languages:** Rust, Python, JavaScript, TypeScript, Go, C, C++, Java, Ruby, PHP, Shell, SQL, HTML, CSS, JSON, YAML, Markdown, and [many more](https://github.com/sublimehq/Packages).

Code blocks without a language specifier render in a uniform green color.

**Available syntax themes:**

| Theme                  | Description                 |
| ---------------------- | --------------------------- |
| `base16-ocean.dark`    | Dark ocean theme (default)  |
| `base16-ocean.light`   | Light ocean theme           |
| `base16-eighties.dark` | Dark 80s retro theme        |
| `base16-mocha.dark`    | Dark mocha theme            |
| `InspiredGitHub`       | GitHub-inspired light theme |
| `Solarized (dark)`     | Solarized dark theme        |
| `Solarized (light)`    | Solarized light theme       |

Set in config:

```toml
# ~/.config/ekphos/config.toml
syntax_theme = "base16-mocha.dark"
```

### Images

#### Adding Images

Use standard markdown image syntax:

```markdown
![alt text](path/to/image.png)
![screenshot](~/pictures/screenshot.png)
![diagram](./diagrams/flow.png)
![remote](https://example.com/image.png)
```

Both local files and remote URLs (http/https) are supported.

**Supported formats:** PNG, JPEG, GIF, WebP, BMP

#### Viewing Images

1. Navigate to the image line in content view
2. Click on the image or press `Enter`/`o` to open in system viewer

#### Terminal Image Support

For inline image preview, use a compatible terminal:

- iTerm2 (macOS)
- Kitty
- WezTerm
- Ghostty
- Sixel-enabled terminals

### Links

Markdown links are rendered with underlined cyan text:

```markdown
[Ekphos Website](https://ekphos.xyz)
[GitHub](https://github.com)
```

**Opening links:**

- Click on a link to open in your default browser
- Or navigate to the line and press `Space`
- Hover over a link to see the "Open ↗" hint

**Multiple links on same line:**

- Use `]` to select next link, `[` to select previous
- Selected link is highlighted with yellow background
- `Space` opens the currently selected link

### Wiki Links

Ekphos supports Obsidian-style wiki links for inter-document linking:

```markdown
[[note name]]
[[folder/nested note]]
```

**Syntax:**

| Format            | Description                          |
| ----------------- | ------------------------------------ |
| `[[note]]`        | Link to a note in the root directory |
| `[[folder/note]]` | Link to a note in a subfolder        |

**Preview Mode:**

- Valid links appear in cyan with underline
- Invalid links (non-existent notes) appear in red
- Navigate to link and press `Space` to open the linked note
- If the target doesn't exist, you'll be prompted to create it

**Edit Mode:**

- Wiki links are syntax-highlighted (cyan for valid, red for invalid)
- Type `[[` to trigger the autocomplete popup
- Use `↑/↓` to navigate suggestions
- Folders are prefixed with `dir:`
- Press `Enter` to insert the selected note/folder
- Press `Esc` to close the popup
- Selecting a folder inserts `[[folder/` and continues autocomplete

**Creating Notes from Links:**

When you navigate to a wiki link that doesn't exist:

1. A dialog appears asking if you want to create the note
2. Press `y` to create the note at the specified path
3. Press `n` or `Esc` to cancel

This allows you to write links to notes you plan to create later, then create them when needed.

### Collapsible Details

Use HTML `<details>` tags for collapsible/expandable sections:

```markdown
<details>
<summary>Click to expand</summary>

Hidden content goes here.
This can include multiple lines.

</details>
```

**Usage:**

- Click on the details line to toggle open/close
- Or navigate with keyboard and press `Space`
- When collapsed, shows `▶` indicator
- When expanded, shows `▼` indicator with content below

Use cases: FAQs, spoilers, optional information, long code examples.

### Using the Outline

The outline panel shows all headings in your note:

1. Press `Tab` to focus the outline
2. Use `j/k` to navigate headings
3. Press `Enter` to jump to that heading

---

## Keyboard Shortcuts

### Global

| Key            | Action                                     |
| -------------- | ------------------------------------------ |
| `j/k`          | Navigate up/down                           |
| `gg`           | Go to first item                           |
| `G`            | Go to last item                            |
| `Tab`          | Switch focus (Sidebar → Content → Outline) |
| `Shift+Tab`    | Switch focus (reverse)                     |
| `Enter/o`      | Open image / Jump to heading               |
| `?`            | Show help dialog                           |
| `q`            | Quit                                       |
| `Ctrl+b`       | Toggle sidebar collapse                    |
| `Ctrl+o`       | Toggle outline collapse                    |
| `R`            | Reload files from disk                     |
| `Ctrl+Shift+R` | Reload config and theme                    |

### Sidebar

| Key     | Action                    |
| ------- | ------------------------- |
| `n`     | Create new note           |
| `N`     | Create new folder         |
| `Enter` | Toggle folder / Open note |
| `r`     | Rename note/folder        |
| `d`     | Delete note/folder        |
| `e`     | Edit note                 |
| `/`     | Search notes              |

### Content View

| Key         | Action                                     |
| ----------- | ------------------------------------------ |
| `j/k`       | Navigate lines                             |
| `Shift+J/K` | Toggle floating cursor mode                |
| `gg`        | Go to beginning of file                    |
| `G`         | Go to end of file                          |
| `Space`     | Toggle task / details dropdown / Open link |
| `]/[`       | Next/previous link (multi-link lines)      |
| `Enter/o`   | Open image in system viewer                |
| `Click`     | Open link or image                         |

**Floating Cursor Mode:** When enabled (yellow border, `[FLOAT]` indicator), the cursor moves freely within the visible area. The view only scrolls when the cursor reaches the top or bottom edge. Toggle with `Shift+J` or `Shift+K`.

### Edit Mode

#### Normal Mode

| Key      | Action                  |
| -------- | ----------------------- |
| `i`      | Insert before cursor    |
| `a`      | Insert after cursor     |
| `A`      | Insert at end of line   |
| `I`      | Insert at start of line |
| `o`      | New line below          |
| `O`      | New line above          |
| `v`      | Visual mode             |
| `h/l`    | Move cursor left/right  |
| `j/k`    | Move cursor up/down     |
| `w/b`    | Move by word            |
| `0/$`    | Line start/end          |
| `g/G`    | Top/bottom of file      |
| `x`      | Delete character        |
| `dd`     | Delete line             |
| `dw`     | Delete word forward     |
| `db`     | Delete word backward    |
| `y`      | Yank (copy) selection   |
| `p`      | Paste                   |
| `u`      | Undo                    |
| `Ctrl+r` | Redo                    |
| `Ctrl+s` | Save and exit           |
| `Esc`    | Exit (discard changes)  |

#### Delete Commands Flow

Delete commands (`dd`, `dw`, `db`) use a confirmation flow with visual feedback:

1. Press `d` - Title shows `NORMAL d-` with yellow border
   - Available: `d` (line), `w` (word forward), `b` (word backward)
2. Press target key - Text is highlighted, title shows `NORMAL [DEL]` with red border
   - Press `d` to confirm deletion
   - Press `Esc` to cancel
   - Any other key cancels and performs its action

### Visual Mode

Press `v` in normal mode to enter visual mode for text selection.

| Key       | Action           |
| --------- | ---------------- |
| `h/j/k/l` | Extend selection |
| `w/b`     | Extend by word   |
| `y`       | Yank selection   |
| `d/x`     | Delete selection |
| `Esc`     | Cancel           |

### Mouse Selection

In edit mode, use the mouse for quick text selection:

| Action           | Result                            |
| ---------------- | --------------------------------- |
| **Click**        | Position cursor                   |
| **Drag**         | Select text (enters visual mode)  |
| **Right-click**  | Context menu (Copy / Cut / Paste) |
| **Drag to edge** | Auto-scroll while selecting       |

**Auto-scroll:** When dragging near the top or bottom edge of the editor, the view automatically scrolls to allow selecting text beyond the visible area.

---

## Contributing

Ekphos is open source and contributions are welcome.

### Development Setup

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
make run
```

### Branch Strategy

| Branch    | Purpose               |
| --------- | --------------------- |
| `main`    | Development branch    |
| `release` | Stable release branch |

### Workflow

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Submit a PR to the `main` branch

---

## Disclaimer

This project is in an early development stage. There may be frequent unexpected breaking changes throughout the pre-release, but things should remain usable throughout this stage.

## Socials

We don't have socials yet, but things are open for discussion. Feel free to create a discussion in this repo.

## License

MIT
