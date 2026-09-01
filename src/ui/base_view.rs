use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::app::{App, Focus};

pub fn render_base_view(frame: &mut Frame, app: &mut App, area: Rect) {
    app.state.content_area = area;
    app.structured.base.row_rects.clear();
    app.structured.base.column_left_rect = None;
    app.structured.base.column_right_rect = None;
    let focused = app.state.focus == Focus::Content;
    let border = if focused { app.state.theme.primary } else { app.state.theme.border };
    let note_title = app.current_note().map_or("Base", |note| note.title.as_str());
    let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(border)).title(format!(" {note_title}.base "));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 12 || inner.height < 4 {
        frame.render_widget(Paragraph::new("Terminal too small for this Base").style(Style::default().fg(app.state.theme.muted)), inner);
        return;
    }
    if let Some(error) = app.structured.base.error.as_deref() {
        render_error(frame, inner, "Could not parse Base", error, app.state.theme.error, app.state.theme.muted);
        return;
    }
    if app.structured.base.loading {
        frame.render_widget(Paragraph::new("Evaluating Base…").alignment(ratatui::layout::Alignment::Center).style(Style::default().fg(app.state.theme.muted)), Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1));
        return;
    }
    let Some(result) = app.structured.base.result.clone() else {
        frame.render_widget(Paragraph::new("No Base view available").style(Style::default().fg(app.state.theme.muted)), inner);
        return;
    };

    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let view_counter = if app.structured.base.view_count > 1 { format!("  [ / ] switch view  {}/{}", result.view_index + 1, app.structured.base.view_count) } else { String::new() };
    let top = Line::from(vec![Span::styled(format!(" {} ", result.view_name), Style::default().fg(app.state.theme.primary).add_modifier(Modifier::BOLD)), Span::styled(format!("{} · {} matches{view_counter}", result.view_kind, result.matched_rows), Style::default().fg(app.state.theme.muted))]);
    frame.render_widget(Paragraph::new(top), chunks[0]);

    let first_column = app.structured.base.column_offset.min(result.columns.len().saturating_sub(1));
    let hidden_left = first_column > 0;
    let width_without_left = chunks[1].width.saturating_sub(u16::from(hidden_left));
    let hidden_right = first_column + columns_that_fit(width_without_left) < result.columns.len();
    let table_area = Rect::new(chunks[1].x + u16::from(hidden_left), chunks[1].y, chunks[1].width.saturating_sub(u16::from(hidden_left) + u16::from(hidden_right)), chunks[1].height);
    if hidden_left {
        let rect = Rect::new(chunks[1].x, chunks[1].y, 1, chunks[1].height);
        render_column_overflow(frame, rect, "◀", app.state.theme.primary, app.state.theme.background_secondary);
        app.structured.base.column_left_rect = Some(rect);
    }
    if hidden_right {
        let rect = Rect::new(chunks[1].right().saturating_sub(1), chunks[1].y, 1, chunks[1].height);
        render_column_overflow(frame, rect, "▶", app.state.theme.primary, app.state.theme.background_secondary);
        app.structured.base.column_right_rect = Some(rect);
    }
    let visible_count = columns_that_fit(table_area.width);
    let last_column = (first_column + visible_count).min(result.columns.len());
    let visible_columns = &result.columns[first_column..last_column];
    let column_count = visible_columns.len().max(1) as u16;
    let spacing = column_count.saturating_sub(1);
    let width = (table_area.width.saturating_sub(spacing) / column_count).max(8);
    let widths = vec![Constraint::Length(width); visible_columns.len()];
    let header = Row::new(visible_columns.iter().map(|column| Cell::from(column.label.clone()))).style(Style::default().fg(app.state.theme.foreground).bg(app.state.theme.background_secondary).add_modifier(Modifier::BOLD));

    let mut rows = Vec::new();
    let mut table_to_logical = Vec::new();
    let mut logical_index = 0usize;
    for group in &result.groups {
        if let Some(label) = &group.label {
            let mut cells = vec![Cell::from(format!("▾ {label}"))];
            cells.extend((1..visible_columns.len()).map(|_| Cell::from("")));
            rows.push(Row::new(cells).style(Style::default().fg(app.state.theme.secondary).add_modifier(Modifier::BOLD)));
            table_to_logical.push(None);
        }
        for row in &group.rows {
            rows.push(Row::new(row.cells[first_column..last_column].iter().map(|value| Cell::from(value.plain_text()))));
            table_to_logical.push(Some(logical_index));
            logical_index += 1;
        }
    }
    if result.groups.iter().all(|group| group.rows.is_empty()) {
        rows.push(Row::new(vec![Cell::from("No matching files")]).style(Style::default().fg(app.state.theme.muted)));
        table_to_logical.push(None);
    }
    if result.summaries.iter().any(Option::is_some) {
        rows.push(Row::new(result.summaries[first_column..last_column].iter().map(|value| Cell::from(value.as_ref().map_or_else(String::new, |value| value.plain_text())))).style(Style::default().fg(app.state.theme.info).add_modifier(Modifier::BOLD)));
        table_to_logical.push(None);
    }

    let selected_table_row = table_to_logical.iter().position(|row| *row == Some(app.structured.base.selected_row));
    let table = Table::new(rows, widths).header(header).column_spacing(1).row_highlight_style(Style::default().fg(app.state.theme.foreground).bg(app.state.theme.selection).add_modifier(Modifier::BOLD));
    let mut state = TableState::default().with_selected(selected_table_row);
    *state.offset_mut() = app.structured.base.row_offset;
    frame.render_stateful_widget(table, table_area, &mut state);
    app.structured.base.row_offset = state.offset();
    let visible_rows = table_area.height.saturating_sub(1) as usize;
    for (display_index, logical) in table_to_logical.iter().enumerate().skip(state.offset()).take(visible_rows) {
        if let Some(logical) = logical {
            let y = table_area.y + 1 + (display_index - state.offset()) as u16;
            app.structured.base.row_rects.push((*logical, Rect::new(table_area.x, y, table_area.width, 1)));
        }
    }

    let columns_hint = match (hidden_left, hidden_right) {
        (true, true) => "  ◀/▶ more columns",
        (true, false) => "  ◀ more columns",
        (false, true) => "  ▶ more columns",
        (false, false) => "",
    };
    let warning = if result.diagnostics.is_empty() { String::new() } else { format!("  ⚠ {} issue(s)", result.diagnostics.len()) };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(" ↑/↓ select · Enter open · e edit source", Style::default().fg(app.state.theme.muted)), Span::styled(columns_hint, Style::default().fg(app.state.theme.muted)), Span::styled(warning, Style::default().fg(app.state.theme.warning))])),
        chunks[2],
    );
}

fn columns_that_fit(width: u16) -> usize {
    (width as usize / 14).max(1)
}

fn render_column_overflow(frame: &mut Frame, area: Rect, arrow: &'static str, foreground: ratatui::style::Color, background: ratatui::style::Color) {
    frame.render_widget(Block::default().style(Style::default().bg(background)), area);
    let arrow_area = Rect::new(area.x, area.y + area.height.saturating_sub(1) / 2, area.width, 1);
    frame.render_widget(Paragraph::new(arrow).alignment(Alignment::Center).style(Style::default().fg(foreground).bg(background).add_modifier(Modifier::BOLD)), arrow_area);
}

fn render_error(frame: &mut Frame, area: Rect, title: &str, error: &str, error_color: ratatui::style::Color, muted: ratatui::style::Color) {
    frame.render_widget(
        Paragraph::new(vec![Line::from(Span::styled(title, Style::default().fg(error_color).add_modifier(Modifier::BOLD))), Line::from(Span::styled(error, Style::default().fg(muted))), Line::from(""), Line::from(Span::styled("Press e to edit the raw source", Style::default().fg(muted)))])
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppDependencies;
    use crate::config::Config;
    use ratatui::{backend::TestBackend, style::Color, Terminal};
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    static NEXT_BASE_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn project_columns_use_the_established_compact_width() {
        assert_eq!(columns_that_fit(13), 1);
        assert_eq!(columns_that_fit(14), 1);
        assert_eq!(columns_that_fit(28), 2);
    }

    #[test]
    fn overflow_gutters_put_arrows_at_the_clipped_table_edges() {
        let backend = TestBackend::new(7, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_column_overflow(frame, Rect::new(0, 0, 1, 5), "◀", Color::Yellow, Color::DarkGray);
                render_column_overflow(frame, Rect::new(6, 0, 1, 5), "▶", Color::Yellow, Color::DarkGray);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(buffer.cell((0, 2)).unwrap().symbol(), "◀");
        assert_eq!(buffer.cell((6, 2)).unwrap().symbol(), "▶");
        assert_eq!(buffer.cell((0, 2)).unwrap().bg, Color::DarkGray);
        assert_eq!(buffer.cell((6, 2)).unwrap().bg, Color::DarkGray);
    }

    #[test]
    fn project_view_only_shows_edge_arrows_when_more_columns_exist() {
        let id = NEXT_BASE_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("ekphos-base-ui-{}-{id}", std::process::id()));
        let vault = root.join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("Projects.base"), "views:\n  - type: table\n    name: Projects\n    order: [file.name, one, two, three, four, five]\n").unwrap();
        fs::write(vault.join("Project.md"), "---\none: 1\ntwo: 2\nthree: 3\nfour: 4\nfive: 5\n---\n# Project").unwrap();
        let dependencies = AppDependencies::headless(root.join("config"), root.join("cache"));
        let mut app = App::new_injected(Config::default(), vault, None, dependencies);
        app.state.focus = Focus::Content;
        assert!(app.select_note_by_path(&root.join("vault/Projects.base")));
        let started = Instant::now();
        while app.structured.base.result.is_none() && started.elapsed() < Duration::from_secs(2) {
            app.poll_background();
            std::thread::yield_now();
        }
        assert!(app.structured.base.result.is_some());
        let backend = TestBackend::new(44, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| render_base_view(frame, &mut app, frame.area())).unwrap();
        assert!(app.structured.base.column_left_rect.is_none());
        let right = app.structured.base.column_right_rect.expect("right overflow indicator");
        assert_eq!(terminal.backend().buffer().cell((right.x, right.y + right.height.saturating_sub(1) / 2)).unwrap().symbol(), "▶");

        app.structured.base.column_offset = 2;
        terminal.draw(|frame| render_base_view(frame, &mut app, frame.area())).unwrap();
        let left = app.structured.base.column_left_rect.expect("left overflow indicator");
        assert_eq!(terminal.backend().buffer().cell((left.x, left.y + left.height.saturating_sub(1) / 2)).unwrap().symbol(), "◀");
        assert!(app.structured.base.column_right_rect.is_some());

        let _ = fs::remove_dir_all(root);
    }
}
