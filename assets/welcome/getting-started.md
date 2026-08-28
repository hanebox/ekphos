---
title: Getting Started
tags: [welcome, tutorial, ekphos]
date: 2024-01-01
---

# Getting Started

A lightweight, fast, terminal-based markdown research tool built with Rust.

## Frontmatter

This note has YAML frontmatter! Look at the tag badges above. Press `Ctrl+m` to toggle viewing the raw frontmatter.

## Layout

Ekphos has three panels:

- **Sidebar** (left): Collapsible folder tree with notes
- **Content** (center): Note content with markdown rendering
- **Outline** (right): Auto-generated headings for quick navigation

Use `Tab` or `Shift+Tab` to switch between panels.

**Collapsible Panels:**

- `Ctrl+b` to collapse/expand the sidebar
- `Ctrl+o` to collapse/expand the outline

## Quick Start

These are the default app shortcuts. You can remap them in the `[keybindings]`
section of `~/.config/ekphos/config.toml`.

- `j/k`: Navigate up/down
- `e`: Enter edit mode
- `n`: Create new note
- `t`: Open today's journal
- `/`: Search notes
- `?`: Show help dialog
- `Ctrl+g`: Open the active note's Local graph
- `Ctrl+z`: Toggle zen mode
- `Ctrl+m`: Toggle frontmatter

Press `?` for the full keybind reference, or visit [docs.ekphos.xyz](https://docs.ekphos.xyz) for comprehensive vim keybindings and documentation.

## Interactive Demo

Try these interactive elements! Press `Space` or click to interact:

### Task Lists

- [ ] Try pressing Space on this checkbox
- [ ] Or click on a task to toggle it
- [x] This one is already completed

### Wikilinks

Navigate between notes using wikilinks:

- [[02-Demo Note]] - Press `Space` or click to visit
- Use `]` and `[` to jump between links on a line
- In edit mode, type `[[` for autocomplete suggestions
- [[Non-existent Note]] - Opens a dialog to create it!

### Collapsible Sections

<details>
<summary>Click or press Space to expand this section</summary>

This content is hidden by default! Great for:
- FAQs and documentation
- Optional information
- Keeping notes organized
</details>

<details>
<summary>Another collapsible section</summary>

You can have multiple collapsible sections in one note.
Each maintains its own open/closed state.
</details>

## Graph View

Press `Ctrl+g` to open a fast Local graph centered on the active note.

- `[` / `]` changes connection depth, and `d` filters incoming/outgoing links
- Press `Enter` to open the focused node
- Press `Space` to explore the selected note without leaving the graph
- Press `v` for the complete vault graph and `/` to filter by title, path, or `#tag`
- Click nodes to select, double-click to open, drag to pan, and scroll to zoom

## Markdown Features

Ekphos renders a rich subset of Markdown right inside your terminal.

### Headings

Use `#` through `######` for six levels of headings. H1–H3 are foldable — press `Tab` or `Space` on a heading to collapse the section beneath it — and every heading shows up in the Outline panel for quick navigation.

### Text Formatting

- **Bold text** with `**double asterisks**` (or `__underscores__`)
- *Italic text* with `*single asterisks*` (or `_underscores_`)
- `Inline code` with backticks
- ~~Strikethrough~~ with `~~double tildes~~`

### Lists

Unordered, ordered, and nested lists are all supported:

- First bullet (`-` or `*`)
- Second bullet
    - Nested item with indentation
    - Another nested item
- Third bullet

1. Ordered lists use numbers
2. They render in sequence
3. Great for step-by-step instructions

### Tables

Pipe tables support per-column alignment (set with `:` in the separator row) and `<br>` for line breaks inside a cell:

| Alignment | Marker  | Example                   |
| :-------- | :-----: | ------------------------: |
| Left      | `:---`  | text hugs the left        |
| Center    | `:---:` | centered                  |
| Right     | `---:`  | numbers line up           |
| Wrapping  | `<br>`  | first line<br>second line |

### Code Blocks

Fenced code blocks get syntax highlighting based on the language tag:

```rust
fn main() {
    println!("Hello, Ekphos!");
}
```

### Blockquotes

> Blockquotes are rendered with a colored border.
> Great for highlighting important information.

### Horizontal Rules

Use `---`, `***`, or `___` on their own line to draw a divider:

---

### Links

- [Inline links](https://docs.ekphos.xyz) with `[text](url)`
- Bare URLs like https://ekphos.xyz are auto-detected
- Press `Enter`, `o`, or click to open a link in your browser

### Images

Embed images with `![alt](path/to/image.png)`. Press `Enter`, `o`, or click to open in system viewer.

![Ekphos Screenshot](https://raw.githubusercontent.com/hanebox/ekphos/release/examples/ekphos-screenshot.png)

Inline preview works in terminals with image support (iTerm2, Kitty, WezTerm, Ghostty, Sixel).

---

Read the docs at [docs.ekphos.xyz](https://docs.ekphos.xyz) for full documentation, vim keybindings, themes, and configuration.

Press `q` to quit. Happy note-taking!
