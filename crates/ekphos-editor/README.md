# Experimental editor

**EXPERIMENTAL** This is experimental but usable, any contribution is sooo welcome to extend this editor capability :)

This implementation is meant for replacing `tui-textarea` with simpler logic and better interaction by default (support line wrap btw)

## Architecture

```
src/editor/
  mod.rs      - Main Editor struct and Widget impl
  buffer.rs   - Gap buffer for text storage
  cursor.rs   - Cursor, Position, Selection
  history.rs  - Undo/redo with operation merging
  input.rs    - Keyboard input processing
  wrap.rs     - Line wrap cache stub
```

## Design Decisions

### Gap Buffer

Uses a line-based gap buffer instead of rope:

- O(1) for localized edits (single-cursor editing)
- Lower memory overhead (no tree structure)
- Ideal for markdown notes (<100KB files)

```rust
struct TextBuffer {
    before: Vec<String>,  // Lines before gap
    after: Vec<String>,   // Lines after gap (reversed)
}
```

### History

- Capped at 1000 entries to limit memory
- Groups rapid character insertions (500ms timeout)
- Supports inverse operations for undo/redo

### Rendering

- Soft line wrapping (visual only, no text modification)
- Horizontal scrolling when wrap disabled
- Overflow indicators (`<<` / `>>`)

## Public API

```rust
// Construction
Editor::default()
Editor::new(lines: Vec<String>)

// Cursor
move_cursor(movement: CursorMove)
cursor() -> (row, col)

// Selection
start_selection()
cancel_selection()

// Clipboard
copy()
cut()
paste()

// Editing
insert_char(c: char)
insert_newline()
delete_char()
delete_newline()
input(key: KeyEvent)

// Undo/Redo
undo() -> bool
redo() -> bool

// Configuration
set_line_wrap(enabled: bool)
set_block(block: Block)
set_selection_style(style: Style)
set_cursor_line_style(style: Style)

// Query
lines() -> Vec<&str>
is_empty() -> bool
```

## Mouse Selection

Full mouse support for text selection:

- **Click** - Position cursor
- **Drag** - Select text range
- **Double-click** - Select word (planned)
- **Right-click** - Context menu (Copy/Cut/Paste)
- **Auto-scroll** - Continuous scrolling when dragging near edges

The editor tracks mouse state via the App:

```rust
mouse_button_held: bool      // Left button currently pressed
mouse_drag_start: (u16, u16) // Initial drag position
last_mouse_y: u16            // For auto-scroll direction
editor_area: Rect            // Editor bounds for hit testing
```

Auto-scroll triggers every 50ms while mouse is held near top/bottom edges.

## Config

Enable/disable line wrap in `config.toml`:

```toml
[editor]
line_wrap = true
```
