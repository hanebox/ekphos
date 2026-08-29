use super::*;

#[derive(Clone)]
pub(super) enum MathBlockRenderState {
    Ready { image_key: String, size: Size },
    Pending { height: u16 },
    Failed { height: u16 },
    Unsupported { height: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InlineMathRenderState {
    Ready { image_key: String, width: u16 },
    Pending,
    Failed,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InlineMathPlacement {
    pub(super) expression_index: usize,
    pub(super) rect: Rect,
}

impl MathBlockRenderState {
    pub(super) fn height(&self) -> u16 {
        match self {
            Self::Ready { size, .. } => size.height.saturating_add(2),
            Self::Pending { height } | Self::Failed { height } | Self::Unsupported { height } => *height,
        }
    }
}

fn indexed_color_rgb(index: u8) -> [u8; 3] {
    const ANSI: [[u8; 3]; 16] = [[0, 0, 0], [205, 49, 49], [13, 188, 121], [229, 229, 16], [36, 114, 200], [188, 63, 188], [17, 168, 205], [229, 229, 229], [102, 102, 102], [241, 76, 76], [35, 209, 139], [245, 245, 67], [59, 142, 234], [214, 112, 214], [41, 184, 219], [255, 255, 255]];
    match index {
        0..=15 => ANSI[index as usize],
        16..=231 => {
            let cube = index - 16;
            let component = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
            [component(cube / 36), component((cube % 36) / 6), component(cube % 6)]
        }
        _ => {
            let value = 8 + (index - 232) * 10;
            [value, value, value]
        }
    }
}

fn terminal_color_rgb(color: ratatui::style::Color, fallback: ratatui::style::Color) -> [u8; 3] {
    use ratatui::style::Color;
    match color {
        Color::Reset => terminal_color_rgb(fallback, Color::Gray),
        Color::Black => indexed_color_rgb(0),
        Color::Red => indexed_color_rgb(1),
        Color::Green => indexed_color_rgb(2),
        Color::Yellow => indexed_color_rgb(3),
        Color::Blue => indexed_color_rgb(4),
        Color::Magenta => indexed_color_rgb(5),
        Color::Cyan => indexed_color_rgb(6),
        Color::Gray => indexed_color_rgb(7),
        Color::DarkGray => indexed_color_rgb(8),
        Color::LightRed => indexed_color_rgb(9),
        Color::LightGreen => indexed_color_rgb(10),
        Color::LightYellow => indexed_color_rgb(11),
        Color::LightBlue => indexed_color_rgb(12),
        Color::LightMagenta => indexed_color_rgb(13),
        Color::LightCyan => indexed_color_rgb(14),
        Color::White => indexed_color_rgb(15),
        Color::Rgb(red, green, blue) => [red, green, blue],
        Color::Indexed(index) => indexed_color_rgb(index),
    }
}

fn math_image_key(latex: &str, color: [u8; 3]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    "ratex-0.1.14".hash(&mut hasher);
    latex.hash(&mut hasher);
    color.hash(&mut hasher);
    format!("math:{:016x}", hasher.finish())
}

fn display_math_state_key(item_index: usize, image_key: &str) -> String {
    format!("math:block:{item_index}:{image_key}")
}

fn inline_math_state_key(item_index: usize, expression_index: usize, image_key: &str) -> String {
    format!("math:inline:{item_index}:{expression_index}:{image_key}")
}

pub(super) fn fit_math_size(natural: Size, available: Size, preferred_height: u16) -> Size {
    let max_width = available.width.max(1);
    let max_height = available.height.max(1);
    let width_scale = max_width as f32 / natural.width.max(1) as f32;
    let height_scale = preferred_height.min(max_height).max(1) as f32 / natural.height.max(1) as f32;
    let scale = width_scale.min(height_scale);
    Size::new(((natural.width.max(1) as f32 * scale).round() as u16).clamp(1, max_width), ((natural.height.max(1) as f32 * scale).round() as u16).clamp(1, max_height))
}

pub(super) fn prepare_math_blocks(app: &mut App, viewport: Size, render_images: bool) -> Vec<Option<MathBlockRenderState>> {
    let expressions: Vec<(usize, String)> = match app.document() {
        Some(document) => app
            .document
            .content_items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                ContentItem::MathBlock { range, .. } => Some((index, document.slice(*range).trim().to_string())),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    };
    let mut states = vec![None; app.document.content_items.len()];
    let fallback_color = app.state.theme.foreground;
    let color = terminal_color_rgb(app.state.theme.content.text, fallback_color);
    let block_height = app.state.config.effective_latex_height();
    let image_height = block_height.saturating_sub(2).max(1);
    let font_size = render_images.then(|| app.images.picker.as_ref().map(|picker| picker.font_size())).flatten();
    for (item_index, latex) in expressions {
        let Some(font_size) = font_size else {
            states[item_index] = Some(MathBlockRenderState::Unsupported { height: block_height });
            continue;
        };
        let image_key = math_image_key(&latex, color);
        if let Some(image) = app.decoded_image(&image_key) {
            let natural = Resize::natural_size(image.as_ref(), font_size);
            let available = Size::new(viewport.width.saturating_sub(6).max(1), image_height);
            states[item_index] = Some(MathBlockRenderState::Ready { image_key, size: fit_math_size(natural, available, image_height) });
        } else if app.image_load_failed(&image_key) {
            states[item_index] = Some(MathBlockRenderState::Failed { height: block_height });
        } else {
            if !app.is_image_pending(&image_key) {
                app.request_math_image(&image_key, latex, color);
            }
            states[item_index] = Some(MathBlockRenderState::Pending { height: block_height });
        }
    }
    states
}

pub(super) fn prepare_inline_math(app: &mut App, viewport_width: u16, render_images: bool) -> Vec<Vec<InlineMathRenderState>> {
    let expressions: Vec<(usize, Vec<String>)> = match app.document() {
        Some(document) => app
            .document
            .content_items
            .iter()
            .enumerate()
            .filter_map(|(item_index, item)| {
                let source = match item {
                    ContentItem::TextLine { range, heading_level: 0, .. } => document.slice(*range),
                    ContentItem::TaskItem { text, .. } => document.slice(*text),
                    _ => return None,
                };
                let math = ekphos_core::markdown::inline_math(source).into_iter().map(|expression| expression.source.to_string()).collect::<Vec<_>>();
                (!math.is_empty()).then_some((item_index, math))
            })
            .collect(),
        None => Vec::new(),
    };
    let mut states = vec![Vec::new(); app.document.content_items.len()];
    let fallback_color = app.state.theme.foreground;
    let color = terminal_color_rgb(app.state.theme.content.text, fallback_color);
    let font_size = render_images.then(|| app.images.picker.as_ref().map(|picker| picker.font_size())).flatten();
    let max_width = viewport_width.saturating_sub(6).max(1);
    for (item_index, item_expressions) in expressions {
        for latex in item_expressions {
            let Some(font_size) = font_size else {
                states[item_index].push(InlineMathRenderState::Unsupported);
                continue;
            };
            let image_key = math_image_key(&latex, color);
            if let Some(image) = app.decoded_image(&image_key) {
                let natural = Resize::natural_size(image.as_ref(), font_size);
                let width = ((natural.width.max(1) as f32 / natural.height.max(1) as f32).round() as u16).clamp(1, max_width);
                states[item_index].push(InlineMathRenderState::Ready { image_key, width });
            } else if app.image_load_failed(&image_key) {
                states[item_index].push(InlineMathRenderState::Failed);
            } else {
                if !app.is_image_pending(&image_key) {
                    app.request_math_image(&image_key, latex, color);
                }
                states[item_index].push(InlineMathRenderState::Pending);
            }
        }
    }
    states
}

fn ensure_math_image_state(app: &mut App, state_key: String, image_key: &str, size: Size) -> String {
    if app.touch_image_state(&state_key, size) || size.width == 0 || size.height == 0 {
        return state_key;
    }
    app.remove_image_state(&state_key);
    let Some(image) = app.decoded_image(image_key) else {
        return state_key;
    };
    let Some(picker) = app.images.picker.as_ref() else {
        return state_key;
    };
    let source_bytes = crate::image_service::decoded_image_bytes(image.as_ref());
    if let Ok(protocol) = SlicedProtocol::new_with_resize(picker, image.as_ref().clone(), size, Resize::Fit(None)) {
        app.insert_image_state(state_key.clone(), protocol, size, source_bytes);
    }
    state_key
}

pub(super) struct MathBlockView<'a> {
    pub(super) item_index: usize,
    pub(super) latex: &'a str,
    pub(super) state: &'a MathBlockRenderState,
    pub(super) viewport: Rect,
    pub(super) is_cursor: bool,
}

pub(super) fn render_math_block(f: &mut Frame, app: &mut App, view: MathBlockView<'_>, area: Rect) {
    if view.is_cursor {
        f.render_widget(Paragraph::new("").style(Style::default().bg(app.state.theme.selection)), area);
    }
    let content_area = Rect { x: area.x.saturating_add(2), width: area.width.saturating_sub(2), ..area };
    match view.state {
        MathBlockRenderState::Ready { image_key, size } => {
            let image_area =
                Rect { x: area.x.saturating_add(2).saturating_add(area.width.saturating_sub(2).saturating_sub(size.width) / 2), y: area.y.saturating_add(1), width: size.width.min(area.width.saturating_sub(2)), height: size.height.min(area.height.saturating_sub(1)) }.intersection(view.viewport);
            if image_area.width > 0 && image_area.height > 0 {
                let state_key = ensure_math_image_state(app, display_math_state_key(view.item_index, image_key), image_key, *size);
                if let Some(image_state) = app.images.image_states.get(&state_key) {
                    f.render_widget(SlicedImage::new(&image_state.image, SignedPosition::from((0, 0))), image_area);
                }
            }
        }
        MathBlockRenderState::Pending { .. } => {
            let source = view.latex.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = vec![Line::from(Span::styled("∑ Rendering equation…", Style::default().fg(app.state.theme.secondary).add_modifier(Modifier::ITALIC))), Line::from(Span::styled(source, Style::default().fg(app.state.theme.muted)))];
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), content_area);
        }
        MathBlockRenderState::Failed { .. } => {
            let source = view.latex.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = vec![Line::from(Span::styled("⚠ Equation could not be rendered", Style::default().fg(app.state.theme.error).add_modifier(Modifier::BOLD))), Line::from(Span::styled(source, Style::default().fg(app.state.theme.muted)))];
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), content_area);
        }
        MathBlockRenderState::Unsupported { .. } => {
            let source = view.latex.split_whitespace().collect::<Vec<_>>().join(" ");
            f.render_widget(Paragraph::new(Line::from(vec![Span::styled("∑ ", Style::default().fg(app.state.theme.secondary)), Span::styled(source, Style::default().fg(app.state.theme.content.code))])).wrap(Wrap { trim: true }), content_area);
        }
    }
    let indicator = if view.is_cursor { "▶" } else { " " };
    f.render_widget(Paragraph::new(Span::styled(indicator, Style::default().fg(app.state.theme.warning))), Rect { width: 1, ..area });
}

pub(super) fn render_inline_math(f: &mut Frame, app: &mut App, item_index: usize, states: &[InlineMathRenderState], placements: &[InlineMathPlacement], viewport: Rect) {
    for placement in placements {
        let Some(InlineMathRenderState::Ready { image_key, width }) = states.get(placement.expression_index) else {
            continue;
        };
        let size = Size::new(*width, 1);
        let area = placement.rect.intersection(viewport);
        if area.width == 0 || area.height == 0 {
            continue;
        }
        let state_key = ensure_math_image_state(app, inline_math_state_key(item_index, placement.expression_index, image_key), image_key, size);
        if let Some(image_state) = app.images.image_states.get(&state_key) {
            f.render_widget(SlicedImage::new(&image_state.image, SignedPosition::from((0, 0))), area);
        }
    }
}

pub(super) fn standalone_image_state_key(item_index: usize, resolved_path: &str) -> String {
    format!("standalone:{item_index}:{resolved_path}")
}

pub(super) fn inline_image_state_key(item_index: usize, selection_index: usize, resolved_path: &str) -> String {
    format!("inline:{item_index}:{selection_index}:{resolved_path}")
}

pub(super) fn ensure_image_state(app: &mut App, state_key: &str, resolved_path: Option<&std::path::Path>, resolved_path_str: &str, normalized_path: &str, is_remote: bool, size: Size) {
    if app.touch_image_state(state_key, size) || size.width == 0 || size.height == 0 {
        return;
    }
    app.remove_image_state(state_key);
    let image = app.decoded_image(resolved_path_str);
    let Some(image) = image else {
        app.request_image_load(resolved_path_str, resolved_path, is_remote.then_some(normalized_path));
        return;
    };
    let Some(picker) = app.images.picker.as_ref() else {
        return;
    };
    let source_bytes = crate::image_service::decoded_image_bytes(image.as_ref());
    let Ok(protocol) = SlicedProtocol::new_with_resize(picker, image.as_ref().clone(), size, Resize::Fit(None)) else {
        return;
    };
    app.insert_image_state(state_key.to_string(), protocol, size, source_bytes);
}

pub(super) fn inline_thumbnail_width(area_width: u16, image_height: u16) -> u16 {
    let available_width = area_width.saturating_sub(INLINE_THUMBNAIL_HORIZONTAL_PADDING * 2);
    image_height.saturating_mul(2).clamp(INLINE_THUMBNAIL_MIN_WIDTH, INLINE_THUMBNAIL_MAX_WIDTH).min(available_width.max(1))
}

pub(super) fn inline_thumbnails_per_row(area_width: u16, image_height: u16) -> usize {
    let available_width = area_width.saturating_sub(INLINE_THUMBNAIL_HORIZONTAL_PADDING * 2);
    let thumbnail_width = inline_thumbnail_width(area_width, image_height);
    usize::from(available_width.saturating_add(INLINE_THUMBNAIL_GAP) / thumbnail_width.saturating_add(INLINE_THUMBNAIL_GAP).max(1)).max(1)
}

pub(super) fn inline_thumbnails_height(image_count: usize, area_width: u16, image_height: u16) -> u16 {
    if image_count == 0 {
        return 0;
    }
    let per_row = inline_thumbnails_per_row(area_width, image_height);
    let rows = image_count.saturating_add(per_row - 1) / per_row;
    u16::try_from(rows).unwrap_or(u16::MAX).saturating_mul(image_height)
}

pub(super) fn image_frame_borders(visible_height: u16, configured_height: u16) -> Borders {
    let mut borders = Borders::LEFT | Borders::RIGHT;
    if visible_height > 1 {
        borders |= Borders::TOP;
    }
    if visible_height >= configured_height {
        borders |= Borders::BOTTOM;
    }
    borders
}

pub(super) fn visible_item_height(total_height: u16, viewport_height: u16, item_height: u16) -> u16 {
    viewport_height.saturating_sub(total_height).min(item_height)
}

#[cfg(test)]
pub(super) fn inline_prose_text(text: &str, theme: &Theme) -> String {
    inline_prose_text_with_math(text, theme, &[])
}

pub(super) fn inline_prose_text_with_math(text: &str, theme: &Theme, math_states: &[InlineMathRenderState]) -> String {
    parse_inline_formatting_with_math::<fn(&str) -> bool>(text, theme, None, None, math_states).iter().map(|span| span.content.as_ref()).collect::<String>().trim().to_string()
}
