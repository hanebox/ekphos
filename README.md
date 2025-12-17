# Ekphos

[![Crates.io](https://img.shields.io/crates/v/ekphos)](https://crates.io/crates/ekphos)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/ekphos)](https://github.com/hanebox/ekphos/blob/main/LICENSE)

An open source, lightweight, fast, terminal-based markdown research tool built with Rust.

![Ekphos Preview](examples/ekphos-screenshot.png)

## Requirements

- Rust 1.70+ (run `rustup update` to update)
- A terminal emulator (for inline image preview: iTerm2, Kitty, WezTerm, Ghostty, or Sixel-compatible terminal)

## Installation

### Using Cargo

```bash
cargo install ekphos
```

### Using Docker

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
docker build -t ekphos-ssh .
docker compose up -d
```

and after the container is up, you can SSH into the machine with the following command
`ssh ekphos@your-docker-container-ip`

### Using Make

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
make
sudo make install
```

## Update

```bash
cargo install ekphos
```

## CLI Options

| Flag              | Description            |
| ----------------- | ---------------------- |
| `-h`, `--help`    | Print help information |
| `-v`, `--version` | Print version          |
| `-c`, `--config`  | Print config file path |
| `-d`, `--dir`     | Print notes directory  |

## Uninstall

### If installed with Make

```bash
sudo make uninstall
```

### If installed with Cargo

```bash
cargo uninstall ekphos
```

## Configuration

Configuration is stored in `~/.config/ekphos/config.toml`.

```toml
# ~/.config/ekphos/config.toml
notes_dir = "~/Documents/ekphos"
welcome_shown = false
theme = "catppuccin-mocha"
show_empty_dir = true
```

| Setting          | Description                            | Default              |
| ---------------- | -------------------------------------- | -------------------- |
| `notes_dir`      | Directory where notes are stored       | `~/Documents/ekphos` |
| `welcome_shown`  | Show welcome dialog on startup         | `true`               |
| `theme`          | Theme name (without .toml extension)   | `catppuccin-mocha`   |
| `show_empty_dir` | Show folders that contain no .md files | `true`               |

> **Note:** This configuration format requires v0.3.0 or later. Earlier versions have a broken config system, please upgrade to latest version to enjoy proper configuration and theming support.

## Themes

**EXPERIMENTAL**: Themes are still highly experimental and not well standardized. We expect to finalize the theme standard around v0.6.0 or v0.7.0

Themes are stored in `~/.config/ekphos/themes/` and use the **Alacritty color scheme format**.

### Bundled Theme

- `catppuccin-mocha` (default)

### Custom Themes

Create a `.toml` file in the themes directory using the Alacritty color format:

```toml
# ~/.config/ekphos/themes/mytheme.toml

[colors.primary]
background = "#1e1e2e"
foreground = "#cdd6f4"

[colors.cursor]
text = "#1e1e2e"
cursor = "#f5e0dc"

[colors.selection]
text = "#1e1e2e"
background = "#f5e0dc"

[colors.normal]
black = "#45475a"
red = "#f38ba8"
green = "#a6e3a1"
yellow = "#f9e2af"
blue = "#89b4fa"
magenta = "#f5c2e7"
cyan = "#94e2d5"
white = "#bac2de"

[colors.bright]
black = "#585b70"
red = "#f38ba8"
green = "#a6e3a1"
yellow = "#f9e2af"
blue = "#89b4fa"
magenta = "#f5c2e7"
cyan = "#94e2d5"
white = "#a6adc8"
```

Then set in config:

```toml
theme = "mytheme"
```

### Using Alacritty Themes

Ekphos is fully compatible with [Alacritty Themes](https://github.com/alacritty/alacritty-theme). To use any Alacritty theme:

1. **Browse themes** at https://github.com/alacritty/alacritty-theme/tree/master/themes
2. **Copy the theme file** (e.g., `dracula.toml`) to your themes directory:
   ```bash
   # Example: Download Dracula theme
   curl -o ~/.config/ekphos/themes/dracula.toml \
     https://raw.githubusercontent.com/alacritty/alacritty-theme/master/themes/dracula.toml
   ```
3. **Set the theme** in your config using the filename (without `.toml`):
   ```toml
   # ~/.config/ekphos/config.toml
   theme = "dracula"
   ```

**Theme naming convention:**
| Theme File | Config Setting |
| ---------- | -------------- |
| `~/.config/ekphos/themes/dracula.toml` | `theme = "dracula"` |
| `~/.config/ekphos/themes/gruvbox_dark.toml` | `theme = "gruvbox_dark"` |
| `~/.config/ekphos/themes/tokyo-night.toml` | `theme = "tokyo-night"` |

> **Note:** Alacritty theme compatibility requires v0.3.0 or later. Earlier versions have a broken theming system.

## Usage

### Layout

Ekphos has three panels:

- **Sidebar** (left): Collapsible folder tree with notes
- **Content** (center): Note content with markdown rendering
- **Outline** (right): Auto-generated headings for quick navigation

Use `Tab` or `Shift+Tab` to switch between panels.

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

**Context-aware**: When your cursor is on a folder or a note inside a folder, pressing `n` will create the new note in that folder.

### Creating Folders

1. Press `N` (Shift+N) to create a new folder
2. Enter the folder name
3. Press `Enter` to confirm
4. A dialog will appear to create the first note in the folder
5. Enter the note name and press `Enter` (or `Esc` to cancel and remove the empty folder)

**Context-aware**: When your cursor is on a folder, pressing `N` will create the new folder as a subfolder.

### Renaming

- Select a note or folder in the sidebar
- Press `r` to rename
- Edit the name and press `Enter` to confirm (or `Esc` to cancel)

### Deleting

- Select a note or folder in the sidebar
- Press `d` to delete
- Confirm with `y` or cancel with `n`
- **Warning**: Deleting a folder will remove all notes inside it!

### Searching Notes

1. Press `/` in the sidebar to start searching
2. Type your search query
3. Results are highlighted in green, title shows match count
4. Use `Arrow keys` or `Ctrl+j/k/n/p` to navigate between matches
5. Press `Enter` to select and close search
6. Press `Esc` to cancel search

**Features**:
- Searches all notes recursively, including those in collapsed folders
- Auto-expands folders containing matched notes
- Border color indicates status: yellow (typing), green (matches found), red (no matches)

### Editing Notes

1. Select a note in the sidebar
2. Press `e` to enter edit mode
3. Edit using vim keybindings
4. Press `Ctrl+s` to save
5. Press `Esc` to exit edit mode

### Vim Keybindings (Edit Mode - Normal)

| Key       | Action                  |
| --------- | ----------------------- |
| `i`       | Insert before cursor    |
| `a`       | Insert after cursor     |
| `A`       | Insert at end of line   |
| `I`       | Insert at start of line |
| `o`       | New line below          |
| `O`       | New line above          |
| `v`       | Visual mode             |
| `h/l`     | Move cursor left/right  |
| `j/k`     | Move cursor up/down     |
| `w/b`     | Move by word            |
| `0/$`     | Line start/end          |
| `g/G`     | Top/bottom of file      |
| `x`       | Delete character        |
| `dd`      | Delete line             |
| `dw`      | Delete word forward     |
| `db`      | Delete word backward    |
| `y`       | Yank (copy) selection   |
| `p`       | Paste                   |
| `u`       | Undo                    |
| `Ctrl+r`  | Redo                    |
| `Ctrl+s`  | Save and exit           |
| `Esc`     | Exit (discard changes)  |

### Delete Commands Flow

Delete commands (`dd`, `dw`, `db`) use a confirmation flow with visual feedback:

1. Press `d` - Title shows `NORMAL d-` with yellow border
   - Available: `d` (line), `w` (word forward), `b` (word backward)
2. Press target key - Text is highlighted, title shows `NORMAL [DEL]` with red border
   - Press `d` to confirm deletion
   - Press `Esc` to cancel
   - Any other key cancels and performs its action

### Visual Mode

Press `v` in normal mode to enter visual mode for text selection.

| Key     | Action           |
| ------- | ---------------- |
| `h/j/k/l` | Extend selection |
| `w/b`   | Extend by word   |
| `y`     | Yank selection   |
| `d/x`   | Delete selection |
| `Esc`   | Cancel           |

### Supported Markdown

| Syntax            | Rendered As           |
| ----------------- | --------------------- |
| `# Heading`       | ◆ HEADING (blue)      |
| `## Heading`      | ■ Heading (green)     |
| `### Heading`     | ▸ Heading (yellow)    |
| `#### Heading`    | › Heading (mauve)     |
| `##### Heading`   | Heading (teal)        |
| `###### Heading`  | _Heading_ (subtle)    |
| `- item`          | • item                |
| `- [ ] task`      | [ ] task (unchecked)  |
| `- [x] task`      | [x] task (checked)    |
| `` `code` ``      | Inline code (green)   |
| ` ``` `           | Code block            |
| `![alt](path)`    | Inline image          |

### Adding Images

Use standard markdown image syntax:

```markdown
![alt text](path/to/image.png)
![screenshot](~/pictures/screenshot.png)
![diagram](./diagrams/flow.png)
```

Supported formats: PNG, JPEG, GIF, WebP, BMP

### Viewing Images

1. Navigate to the image line in content view
2. Press `Enter` or `o` to open in system viewer

### Terminal Image Support

For inline image preview, use a compatible terminal:

- iTerm2 (macOS)
- Kitty
- WezTerm
- Sixel-enabled terminals

### Using the Outline

The outline panel shows all headings in your note:

1. Press `Tab` to focus the outline
2. Use `j/k` to navigate headings
3. Press `Enter` to jump to that heading

## Keyboard Shortcuts

### Global

| Key         | Action                                    |
| ----------- | ----------------------------------------- |
| `j/k`       | Navigate up/down                          |
| `Tab`       | Switch focus (Sidebar → Content → Outline)|
| `Shift+Tab` | Switch focus (reverse)                    |
| `Enter/o`   | Open image / Jump to heading              |
| `?`         | Show help dialog                          |
| `q`         | Quit                                      |

### Sidebar

| Key     | Action                       |
| ------- | ---------------------------- |
| `n`     | Create new note              |
| `N`     | Create new folder            |
| `Enter` | Toggle folder / Open note    |
| `r`     | Rename note/folder           |
| `d`     | Delete note/folder           |
| `e`     | Edit note                    |
| `/`     | Search notes                 |

### Content View

| Key         | Action                                           |
| ----------- | ------------------------------------------------ |
| `j/k`       | Navigate lines                                   |
| `Shift+J/K` | Toggle floating cursor mode                      |
| `Space`     | Toggle task checkbox                             |
| `Enter/o`   | Open image in system viewer                      |

**Floating Cursor Mode**: When enabled (yellow border, `[FLOAT]` indicator), the cursor moves freely within the visible area. The view only scrolls when the cursor reaches the top or bottom edge. Toggle with `Shift+J` or `Shift+K`.

### Edit Mode

| Key      | Action                    |
| -------- | ------------------------- |
| `Ctrl+s` | Save and exit edit mode   |
| `Esc`    | Exit (discard changes)    |

See **Vim Keybindings** section above for full edit mode controls.

## Contributing

Ekphos is open source and contributions are welcome.

### Development Setup

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
make run
```

### Branch Strategy

- `main` - Development branch
- `release` - Stable release branch

### Workflow

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Submit a PR to the `main` branch

## Disclaimer

This project is in an early development stage, so there will be frequent unexpected breaking changes throughout the pre-release, but things should remain usable throughout this stage.

## Socials

We don't have socials yet, but things are open for discussion, go create a discussion in this repo for how socials should be done

## License

MIT
