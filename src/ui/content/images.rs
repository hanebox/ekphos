use super::*;

pub(super) fn standalone_image_state_key(item_index: usize, resolved_path: &str) -> String {
    format!("standalone:{item_index}:{resolved_path}")
}

pub(super) fn inline_image_state_key(item_index: usize, selection_index: usize, resolved_path: &str) -> String {
    format!("inline:{item_index}:{selection_index}:{resolved_path}")
}

pub(super) fn ensure_image_state(
    app: &mut App,
    state_key: &str,
    resolved_path: Option<&std::path::Path>,
    resolved_path_str: &str,
    normalized_path: &str,
    is_remote: bool,
    is_pending: bool,
    size: Size,
) {
    let needs_rebuild = app.image_states.get(state_key).is_none_or(|state| state.size != size);
    if !needs_rebuild || size.width == 0 || size.height == 0 {
        return;
    }

    app.image_states.remove(state_key);

    let img = if let Some(img) = app.get_cached_image(resolved_path_str) {
        Some(img)
    } else if is_remote {
        if !is_pending {
            app.start_remote_image_fetch(normalized_path);
        }
        None
    } else if let Some(resolved) = resolved_path {
        if let Ok(img) = image::open(resolved) {
            app.cache_image(resolved_path_str, img);
            app.get_cached_image(resolved_path_str)
        } else {
            None
        }
    } else {
        None
    };

    let (Some(img), Some(picker)) = (img, app.picker.as_ref()) else {
        return;
    };
    let Ok(image) = SlicedProtocol::new_with_resize(picker, img, size, Resize::Fit(None)) else {
        return;
    };

    app.image_states.insert(state_key.to_string(), ImageState { image, size });
}

pub(super) fn inline_thumbnail_width(area_width: u16, image_height: u16) -> u16 {
    let available_width = area_width.saturating_sub(INLINE_THUMBNAIL_HORIZONTAL_PADDING * 2);
    image_height
        .saturating_mul(2)
        .max(INLINE_THUMBNAIL_MIN_WIDTH)
        .min(INLINE_THUMBNAIL_MAX_WIDTH)
        .min(available_width.max(1))
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

pub(super) fn inline_prose_text(text: &str, theme: &Theme) -> String {
    parse_inline_formatting::<fn(&str) -> bool>(text, theme, None, None)
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim()
        .to_string()
}
