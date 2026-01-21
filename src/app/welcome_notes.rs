pub const GETTING_STARTED_CONTENT: &str = r#"---
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

- `j/k`: Navigate up/down
- `e`: Enter edit mode
- `n`: Create new note
- `/`: Search notes
- `?`: Show help dialog
- `Ctrl+g`: Open graph view
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

Press `Ctrl+g` to open the interactive graph view and visualize connections between your notes.

- See how your notes link together
- Click on nodes to navigate
- Drag to pan, scroll to zoom

## Markdown Features

### Text Formatting

- **Bold text** with double asterisks
- *Italic text* with single asterisks
- `Inline code` with backticks
- ~~Strikethrough~~ in task items

### Code Blocks

```rust
fn main() {
    println!("Hello, Ekphos!");
}
```

### Blockquotes

> Blockquotes are rendered with a colored border.
> Great for highlighting important information.

### Images

Embed images with `![alt](path/to/image.png)`. Press `Enter`, `o`, or click to open in system viewer.

![Ekphos Screenshot](https://raw.githubusercontent.com/hanebox/ekphos/release/examples/ekphos-screenshot.png)

Inline preview works in terminals with image support (iTerm2, Kitty, WezTerm, Ghostty, Sixel).

---

Read the docs at [docs.ekphos.xyz](https://docs.ekphos.xyz) for full documentation, vim keybindings, themes, and configuration.

Press `q` to quit. Happy note-taking!"#;

pub const DEMO_NOTE_CONTENT: &str = r#"---
title: Demo Note
tags: [demo, wikilinks, features]
author: Ekphos
---

# Demo Note

This is a demo note to showcase wikilinks and interactive markdown features!

## Wikilinks

Wikilinks let you connect your notes together, creating a personal knowledge base.

- [[Getting Started]] - Link back to the main documentation
- [[Getting Started#Graph View]] - Link to a specific heading
- [[Getting Started|Main Guide]] - Custom display text with `|`

### Creating Wikilinks

1. Press `e` to enter edit mode
2. Type `[[` to see autocomplete suggestions
3. Add `#` to link to specific headings
4. Add `|` to customize the display text
5. Press `Ctrl+s` or `:w` to save

### Navigation

- Press `Space` or click on any wikilink to navigate
- Use `]` to jump to next link, `[` for previous
- Links to non-existent notes will prompt to create them

## Interactive Elements

### Tasks with Links

- [ ] Check out the [[Getting Started]] guide
- [ ] Try pressing `Space` on this checkbox
- [x] Complete the tutorial

### Collapsible Content

<details>
<summary>Wikilink Ideas</summary>

Here are some ways to use wikilinks:
- Create a **daily notes** system with links between days
- Build a **zettelkasten** for research and learning
- Organize **project notes** with interconnected topics
- Make a **personal wiki** for anything you want to remember
</details>

## Graph View

Press `Ctrl+g` to see how this note connects to [[Getting Started]] in the graph visualization!

Happy linking!"#;
