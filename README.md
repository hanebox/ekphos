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

### Using Make

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
make
sudo make install
```

### Using Cargo

```bash
cargo install ekphos
```

### Using Docker

```bash
docker build -t ekphos-ssh .
docker compose up -d
```

and after the container is up, you can SSH into the machine with the following command
`ssh ekphos@your-docker-container-ip`

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

### Remove config (optional)

```bash
rm -rf ~/.config/ekphos
```

## Configuration

Configuration is stored in `~/.config/ekphos/config.toml`.

```toml
# General settings
theme = "catppuccin-mocha"
notes_dir = "~/notes"

# Keybinds
[keybinds]
quit = "q"
edit = "e"
save = "ctrl+s"
navigate_up = "k"
navigate_down = "j"
switch_focus = "tab"
```

## Themes

Themes are stored in `~/.config/ekphos/themes/`.

### Default Themes

- `catppuccin-mocha` (default)
- `catppuccin-latte`
- `catppuccin-frappe`
- `catppuccin-macchiato`

### Custom Themes

Create a `.toml` file in the themes directory:

```toml
# ~/.config/ekphos/themes/mytheme.toml
name = "mytheme"

[colors]
base = "#1e1e2e"
surface0 = "#313244"
text = "#cdd6f4"
subtext0 = "#a6adc8"
overlay0 = "#6c7086"
lavender = "#b4befe"
peach = "#fab387"
green = "#a6e3a1"
red = "#f38ba8"
yellow = "#f9e2af"
```

Then set in config:

```toml
theme = "mytheme"
```

## Usage

### Layout

Ekphos has three panels:

- **Sidebar** (left): List of notes
- **Content** (center): Note content with markdown rendering
- **Outline** (right): Auto-generated headings for quick navigation

Use `Tab` to switch between panels.

### Creating Notes

1. Press `n` to create a new note
2. Enter the note name
3. Press `Enter` to confirm

Notes are stored as `.md` files in your configured notes directory.

### Deleting Notes

1. Select the note in the sidebar
2. Press `d` to delete
3. Confirm with `y` or cancel with `n`

### Editing Notes

1. Select a note in the sidebar
2. Press `e` to enter edit mode
3. Edit using vim keybindings
4. Press `Ctrl+s` to save
5. Press `Esc` to exit edit mode

### Vim Keybindings (Edit Mode)

| Key       | Action                  |
| --------- | ----------------------- |
| `i`       | Insert mode             |
| `a`       | Insert after cursor     |
| `A`       | Insert at end of line   |
| `I`       | Insert at start of line |
| `o`       | New line below          |
| `O`       | New line above          |
| `v`       | Visual mode             |
| `h/j/k/l` | Move cursor             |
| `w/b`     | Word forward/back       |
| `0/$`     | Line start/end          |
| `gg/G`    | Top/bottom of file      |
| `x`       | Delete character        |
| `dd`      | Delete line             |
| `y`       | Yank (copy) line        |
| `p`       | Paste                   |
| `u`       | Undo                    |
| `Ctrl+r`  | Redo                    |

### Visual Mode

Press `v` in normal mode to enter visual mode for text selection.

| Key   | Action           |
| ----- | ---------------- |
| `y`   | Yank selection   |
| `d`   | Delete selection |
| `Esc` | Cancel           |

### Supported Markdown

| Syntax           | Rendered As        |
| ---------------- | ------------------ |
| `# Heading`      | ◆ HEADING (blue)   |
| `## Heading`     | ■ Heading (green)  |
| `### Heading`    | ▸ Heading (yellow) |
| `#### Heading`   | › Heading (mauve)  |
| `##### Heading`  | Heading (teal)     |
| `###### Heading` | _Heading_ (subtle) |
| `- item`         | • item             |
| `![alt](path)`   | Inline image       |

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

| Key      | Action                       |
| -------- | ---------------------------- |
| `j/k`    | Navigate up/down             |
| `Tab`    | Switch focus                 |
| `/`      | Search notes                 |
| `n`      | New note                     |
| `d`      | Delete note                  |
| `e`      | Edit mode                    |
| `Esc`    | Exit edit mode / Cancel      |
| `Ctrl+s` | Save                         |
| `Enter`  | Open image / Jump to heading |
| `?`      | Show help                    |
| `q`      | Quit                         |

## Contributing

Ekphos is open source and contributions are welcome.

```bash
git clone https://github.com/hanebox/ekphos.git
cd ekphos
make run
```

## License

MIT

## Disclaimer

This project is in an early development stage, so there will be frequent unexpected breaking changes throughout the pre-release, but things should remain usable throughout this stage.

## Socials

We don't have socials yet, but things are open for discussion, you can DM hanebox via telegram here: https://t.me/havernut
