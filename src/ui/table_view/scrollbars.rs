//! The table's own scrollbars. Painted by hand because the grid is a
//! virtual renderer, not an egui `ScrollArea`.

use egui::{Painter, Sense, Ui, Vec2};

use super::{TableInteraction, TableViewState, VirtualThumb, virtual_thumb};
use crate::ui::theme::ThemeColors;

/// The frame's layout numbers both scrollbars are drawn from.
pub(super) struct ScrollGeometry {
    pub panel_rect: egui::Rect,
    pub view_width: f32,
    pub view_height: f32,
    pub row_height: f32,
    pub row_count: usize,
    pub total_content_height: f32,
    pub total_col_width: f32,
    pub vscroll_width: f32,
}

/// In large-file mode the bar stands for the whole file, not for the loaded
/// page: a 2,000-row page out of 100,000,000 gives a thumb that says nothing
/// and a drag that reaches nowhere. `virtual_rows` swaps the arithmetic over
/// to rows and reports the landing row instead of moving `scroll_y`.
pub(super) fn draw_vertical_scrollbar(
    ui: &Ui,
    painter: &Painter,
    state: &mut TableViewState,
    colors: &ThemeColors,
    g: &ScrollGeometry,
    interaction: &mut TableInteraction,
) {
    let ScrollGeometry {
        panel_rect,
        view_height,
        row_height,
        row_count,
        total_content_height,
        ..
    } = *g;
    if let Some((page_offset, file_rows)) = state.virtual_rows.filter(|&(_, n)| n > row_count) {
        let scrollbar_width = 10.0;
        let scrollbar_x = panel_rect.right() - scrollbar_width - 1.0;
        let track_top = panel_rect.top();
        let track_height = view_height;
        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, track_top),
            Vec2::new(scrollbar_width, track_height),
        );
        painter.rect_filled(track_rect, scrollbar_width / 2.0, colors.scrollbar_track);

        // Where in the file the top of the viewport sits - or, mid-drag, where
        // the user has dragged it to, so the thumb tracks the cursor even
        // though the page itself only moves on release.
        let top_row = page_offset as f32 + state.scroll_y / row_height;
        let shown_row = state.virtual_drag_row.unwrap_or(top_row);
        let VirtualThumb {
            height,
            offset,
            travel,
            max_row,
        } = virtual_thumb(shown_row, file_rows, view_height / row_height, track_height);
        let thumb_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, track_top + offset),
            Vec2::new(scrollbar_width, height),
        );

        let sb = ui.interact(thumb_rect, ui.id().with("vscroll_thumb"), Sense::drag());
        painter.rect_filled(
            thumb_rect,
            scrollbar_width / 2.0,
            if sb.dragged() || sb.hovered() {
                colors.scrollbar_thumb_hover
            } else {
                colors.scrollbar_thumb
            },
        );
        if sb.dragged() && travel > 0.0 {
            let rows_per_pixel = max_row / travel;
            let from = state.virtual_drag_row.unwrap_or(top_row);
            state.virtual_drag_row =
                Some((from + sb.drag_delta().y * rows_per_pixel).clamp(0.0, max_row));
        }
        if sb.drag_stopped() {
            interaction.jump_to_row = state.virtual_drag_row.take().map(|r| r as usize);
        }
        let track_resp = ui.interact(track_rect, ui.id().with("vscroll_track"), Sense::click());
        if track_resp.clicked()
            && let Some(pos) = track_resp.interact_pointer_pos()
        {
            let fraction = ((pos.y - track_top) / track_height).clamp(0.0, 1.0);
            interaction.jump_to_row = Some((fraction * max_row) as usize);
        }
    } else if total_content_height > view_height {
        let scrollbar_width = 10.0;
        let scrollbar_x = panel_rect.right() - scrollbar_width - 1.0;
        let scrollbar_track_top = panel_rect.top();
        let scrollbar_track_height = view_height;

        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, scrollbar_track_top),
            Vec2::new(scrollbar_width, scrollbar_track_height),
        );
        painter.rect_filled(track_rect, scrollbar_width / 2.0, colors.scrollbar_track);

        let thumb_fraction = view_height / total_content_height;
        let thumb_height = (thumb_fraction * scrollbar_track_height).max(24.0);
        let max_scroll = total_content_height - view_height;
        let thumb_offset = if max_scroll > 0.0 {
            (state.scroll_y / max_scroll) * (scrollbar_track_height - thumb_height)
        } else {
            0.0
        };

        let thumb_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, scrollbar_track_top + thumb_offset),
            Vec2::new(scrollbar_width, thumb_height),
        );

        let sb_response = ui.interact(thumb_rect, ui.id().with("vscroll_thumb"), Sense::drag());
        let thumb_color = if sb_response.dragged() || sb_response.hovered() {
            colors.scrollbar_thumb_hover
        } else {
            colors.scrollbar_thumb
        };
        painter.rect_filled(thumb_rect, scrollbar_width / 2.0, thumb_color);

        if sb_response.dragged() {
            let delta_y = sb_response.drag_delta().y;
            let scroll_per_pixel = max_scroll / (scrollbar_track_height - thumb_height);
            state.scroll_y = (state.scroll_y + delta_y * scroll_per_pixel).clamp(0.0, max_scroll);
        }

        let track_response = ui.interact(track_rect, ui.id().with("vscroll_track"), Sense::click());
        if track_response.clicked()
            && let Some(pos) = track_response.interact_pointer_pos()
        {
            let click_fraction = (pos.y - scrollbar_track_top) / scrollbar_track_height;
            state.scroll_y =
                (click_fraction * total_content_height - view_height / 2.0).clamp(0.0, max_scroll);
        }
    }
}

pub(super) fn draw_horizontal_scrollbar(
    ui: &Ui,
    painter: &Painter,
    state: &mut TableViewState,
    colors: &ThemeColors,
    g: &ScrollGeometry,
) {
    let ScrollGeometry {
        panel_rect,
        view_width,
        total_col_width,
        vscroll_width,
        ..
    } = *g;
    let scrollbar_height = 10.0;
    let scrollbar_y = panel_rect.bottom() - scrollbar_height - 1.0;
    let scrollbar_track_left = panel_rect.left();
    let scrollbar_track_width = view_width;

    let track_rect = egui::Rect::from_min_size(
        egui::pos2(scrollbar_track_left, scrollbar_y),
        Vec2::new(scrollbar_track_width, scrollbar_height),
    );
    painter.rect_filled(track_rect, scrollbar_height / 2.0, colors.scrollbar_track);

    let effective_width = view_width - vscroll_width;
    let thumb_fraction = effective_width / total_col_width;
    let thumb_width = (thumb_fraction * scrollbar_track_width).max(24.0);
    let max_scroll = (total_col_width + vscroll_width - view_width).max(0.0);
    let thumb_offset = if max_scroll > 0.0 {
        (state.scroll_x / max_scroll) * (scrollbar_track_width - thumb_width)
    } else {
        0.0
    };

    let thumb_rect = egui::Rect::from_min_size(
        egui::pos2(scrollbar_track_left + thumb_offset, scrollbar_y),
        Vec2::new(thumb_width, scrollbar_height),
    );

    let sb_response = ui.interact(thumb_rect, ui.id().with("hscroll_thumb"), Sense::drag());
    let thumb_color = if sb_response.dragged() || sb_response.hovered() {
        colors.scrollbar_thumb_hover
    } else {
        colors.scrollbar_thumb
    };
    painter.rect_filled(thumb_rect, scrollbar_height / 2.0, thumb_color);

    if sb_response.dragged() {
        let delta_x = sb_response.drag_delta().x;
        let scroll_per_pixel = max_scroll / (scrollbar_track_width - thumb_width);
        state.scroll_x = (state.scroll_x + delta_x * scroll_per_pixel).clamp(0.0, max_scroll);
    }

    let track_response = ui.interact(track_rect, ui.id().with("hscroll_track"), Sense::click());
    if track_response.clicked()
        && let Some(pos) = track_response.interact_pointer_pos()
    {
        let click_fraction = (pos.x - scrollbar_track_left) / scrollbar_track_width;
        state.scroll_x =
            (click_fraction * total_col_width - view_width / 2.0).clamp(0.0, max_scroll);
    }
}
