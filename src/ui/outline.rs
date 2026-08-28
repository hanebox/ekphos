use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::app::{DocumentSnapshot, DocumentState, EditorSession, Focus, Mode};
use crate::config::Theme;

pub struct OutlineView<'a> {
    pub theme: &'a Theme,
    pub document: &'a DocumentState,
    pub snapshot: Option<&'a DocumentSnapshot>,
    pub editor: &'a EditorSession,
    pub focus: Focus,
    pub minimized: bool,
}

pub struct OutlineRender {
    pub area: Rect,
    pub state: ListState,
}
fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

pub fn render_outline(f: &mut Frame, view: OutlineView<'_>, area: Rect) -> OutlineRender {
    let theme = view.theme;
    let outline_theme = &theme.outline;
    if view.minimized {
        return render_collapsed_outline(f, &view, area);
    }
    let items: Vec<ListItem> = view
        .document
        .outline
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.level.saturating_sub(1) as usize);
            let prefix = match item.level {
                1 => "# ",
                2 => "## ",
                3 => "### ",
                _ => "",
            };
            let style = match item.level {
                1 => Style::default().fg(outline_theme.heading1).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(outline_theme.heading2),
                3 => Style::default().fg(outline_theme.heading3),
                _ => Style::default().fg(outline_theme.heading4),
            };
            let source_line = item.source_line as usize;
            let raw_title = if view.editor.mode == Mode::Edit { view.editor.line(source_line).unwrap_or("") } else { view.snapshot.and_then(|document| document.line(source_line)).unwrap_or("") };
            let title = ekphos_core::markdown::heading(raw_title).map_or(raw_title, |heading| heading.text);
            ListItem::new(Line::from(Span::styled(format!("{}{}{}", indent, prefix, expand_tabs(title)), style)))
        })
        .collect();
    let border_style = if view.focus == Focus::Outline && view.editor.mode == Mode::Normal { Style::default().fg(theme.primary) } else { Style::default().fg(theme.border) };
    let mut outline = List::new(items).block(Block::default().title(" Outline ").borders(Borders::ALL).border_style(border_style));
    if view.editor.mode != Mode::Edit {
        outline = outline.highlight_style(Style::default().bg(theme.selection).add_modifier(Modifier::BOLD)).highlight_symbol("▶ ");
    }
    let mut state = view.document.outline_state;
    f.render_stateful_widget(outline, area, &mut state);
    OutlineRender { area, state }
}
fn render_collapsed_outline(f: &mut Frame, view: &OutlineView<'_>, area: Rect) -> OutlineRender {
    let theme = view.theme;
    let outline_theme = &theme.outline;
    let in_edit_mode = view.editor.mode == Mode::Edit;
    let items: Vec<ListItem> = view
        .document
        .outline
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = !in_edit_mode && view.document.outline_state.selected() == Some(idx);
            let symbol = match item.level {
                1 => "◆", // H1
                2 => "■", // H2
                3 => "▸", // H3
                _ => "›", // H4+
            };
            let style = match item.level {
                1 => Style::default().fg(outline_theme.heading1).add_modifier(Modifier::BOLD),
                2 => Style::default().fg(outline_theme.heading2),
                3 => Style::default().fg(outline_theme.heading3),
                _ => Style::default().fg(outline_theme.heading4),
            };
            let display = if is_selected { format!("▶{}", symbol) } else { format!(" {}", symbol) };
            ListItem::new(Line::from(Span::styled(display, style)))
        })
        .collect();
    let border_style = if view.focus == Focus::Outline && view.editor.mode == Mode::Normal { Style::default().fg(theme.primary) } else { Style::default().fg(theme.border) };
    let mut outline = List::new(items).block(Block::default().borders(Borders::ALL).border_style(border_style));
    if !in_edit_mode {
        outline = outline.highlight_style(Style::default().bg(theme.selection).add_modifier(Modifier::BOLD));
    }
    let mut state = view.document.outline_state;
    f.render_stateful_widget(outline, area, &mut state);
    OutlineRender { area, state }
}
