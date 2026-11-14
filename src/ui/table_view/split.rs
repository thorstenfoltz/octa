//! Split view: up to [`MAX_SPLIT_PANES`] independently scrolling panes over
//! the same table, either stacked one above the other or side by side.
//!
//! Everything except the scroll offsets is shared, because every pane is the
//! same [`TableViewState`]: columns, widths, frozen band, filters, sort, marks,
//! edits and the selection, so a range from row 12 to row 900,000 is one
//! selection. Each pane owns **both** scroll axes: sharing the cross axis
//! lines the bands up prettily, but it also means two bands showing the same
//! cells, which is the one thing a split exists to avoid. Frozen columns are
//! the tool for keeping the leading columns in sight while the rest moves.
//!
//! [`TableViewState::pane_scroll`] holds the offsets of every pane but the
//! first, and they are swapped into `scroll_x` / `scroll_y` around that pane's
//! `draw_table` call, because the renderer knows only those two fields.
//! Calling the existing renderer once per pane is also what gives each band
//! its own header and scrollbars.

use egui::{CursorIcon, Sense, Ui, Vec2};

use crate::data::DataTable;

use super::{MAX_SPLIT_PANES, TableCtx, TableInteraction, TableViewState, ThemeColors, draw_table};

/// Thickness of a draggable divider between two panes.
const DIVIDER_HEIGHT: f32 = 6.0;
/// Smallest pane that is still worth looking at (header plus a row or two,
/// or the row-number gutter plus a column).
const MIN_PANE_HEIGHT: f32 = 80.0;

/// Size of each pane along the split axis, for the given divider fractions.
///
/// Pure so the clamping is testable: no pane may collapse, the dividers stay
/// in order, and a panel too short to hold them all splits evenly rather than
/// reporting negative sizes.
///
/// `fractions` are cumulative positions along the axis, ascending, one per
/// divider. An empty (or wrong-length) slice means evenly spaced.
pub(crate) fn split_sizes(total: f32, panes: usize, fractions: &[f32]) -> Vec<f32> {
    crate::ui::tiles::sizes_along(
        total,
        panes.clamp(1, MAX_SPLIT_PANES),
        fractions,
        MIN_PANE_HEIGHT,
        DIVIDER_HEIGHT,
    )
}

/// Hand one pane its own scroll offsets, or take them back. Called twice
/// around that pane's `draw_table`, so the state comes out as it went in.
/// Pane 0 is not in `pane_scroll`: it uses `scroll_x` / `scroll_y` directly.
pub(super) fn swap_pane_scroll(state: &mut TableViewState, pane: usize) {
    let Some(slot) = state.pane_scroll.get_mut(pane.wrapping_sub(1)) else {
        return;
    };
    let saved = *slot;
    *slot = (state.scroll_x, state.scroll_y);
    state.scroll_x = saved.0;
    state.scroll_y = saved.1;
}

/// Draw the table once per pane, with a draggable divider between each pair.
///
/// Returns one [`TableInteraction`] per pane; the caller handles them in
/// order, exactly as it handles the single one from [`draw_table`].
pub fn draw_table_split(
    ui: &mut Ui,
    table: &mut DataTable,
    state: &mut TableViewState,
    cx: TableCtx<'_>,
) -> Vec<TableInteraction> {
    let side_by_side = state.split_side_by_side;
    let panes = state.split_panes();
    let full = ui.available_rect_before_wrap();
    let along = if side_by_side {
        full.width()
    } else {
        full.height()
    };
    let sizes = split_sizes(along, panes, &state.split_fractions);
    let usable = (along - DIVIDER_HEIGHT * (panes - 1) as f32).max(1.0);

    let pane_vec = |size: f32| {
        if side_by_side {
            Vec2::new(size, full.height())
        } else {
            Vec2::new(full.width(), size)
        }
    };
    let divider_vec = if side_by_side {
        Vec2::new(DIVIDER_HEIGHT, full.height())
    } else {
        Vec2::new(full.width(), DIVIDER_HEIGHT)
    };

    // Keyboard and wheel follow the pointer, and stick when it leaves every
    // pane: a menu click must not hand the arrow keys back to the first one.
    let mut offset = 0.0;
    for (i, size) in sizes.iter().enumerate() {
        let origin = if side_by_side {
            egui::pos2(full.left() + offset, full.top())
        } else {
            egui::pos2(full.left(), full.top() + offset)
        };
        if ui.rect_contains_pointer(egui::Rect::from_min_size(origin, pane_vec(*size))) {
            state.active_pane = i;
        }
        offset += size + DIVIDER_HEIGHT;
    }
    let active = state.active_pane.min(panes - 1);

    // Hold Alt and every band takes the same wheel notch. Alt is the one
    // modifier egui leaves alone: it reads Ctrl/Cmd as zoom and Shift as
    // "scroll sideways", and swallows the scroll delta for the first. Shift
    // still works alongside, so Alt+Shift+wheel walks every band across the
    // columns together.
    let scroll_all = ui.input(|i| i.modifiers.alt);

    // A drag is applied after the loop: the dividers are drawn between the
    // panes, and moving one must not resize the pane already painted.
    let mut dragged: Option<(usize, f32)> = None;

    let mut draw = |ui: &mut Ui| {
        let mut out = Vec::with_capacity(panes);
        for (i, size) in sizes.iter().enumerate() {
            if i > 0 {
                let (rect, divider) = ui.allocate_exact_size(divider_vec, Sense::drag());
                let colors = ThemeColors::for_mode(cx.theme_mode);
                ui.painter().rect_filled(rect, 0.0, colors.border);
                if divider.hovered() || divider.dragged() {
                    ui.ctx().set_cursor_icon(if side_by_side {
                        CursorIcon::ResizeHorizontal
                    } else {
                        CursorIcon::ResizeVertical
                    });
                }
                if divider.dragged() {
                    let delta = divider.drag_delta();
                    dragged = Some((i - 1, if side_by_side { delta.x } else { delta.y }));
                }
            }
            let mut pane_cx = cx;
            // A tile that is not focused hands no pane the keyboard, and one
            // the pointer is not over hands no pane the wheel.
            pane_cx.handles_input = cx.handles_input && i == active;
            pane_cx.takes_wheel = cx.takes_wheel && (i == active || scroll_all);
            // Every pane but the first reads and writes its own offsets.
            swap_pane_scroll(state, i);
            let interaction = ui
                .allocate_ui(pane_vec(*size), |ui| {
                    // Each pane needs its own widget ids. `allocate_ui` alone
                    // does not give it one: egui derives a child id from the
                    // parent's plus a counter it deliberately rewinds after
                    // each scope, so every pane came out as `parent/child` and
                    // every `ui.id().with(...)` inside collided. One shared id
                    // meant one shared scrollbar thumb: grabbing any of them
                    // dragged all the panes at once, and the last one drawn
                    // owned the interaction. Salting by index separates the
                    // thumbs, the tracks, the column-resize handles and the
                    // in-cell editors.
                    ui.push_id(i, |ui| draw_table(ui, table, state, pane_cx))
                        .inner
                })
                .inner;
            swap_pane_scroll(state, i);
            out.push(interaction);
        }
        out
    };

    let interactions = if side_by_side {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            draw(ui)
        })
        .inner
    } else {
        draw(ui)
    };

    // Store every divider, not just the dragged one: the sizes above are the
    // clamped truth, so writing them all back keeps a drag from silently
    // re-spacing its neighbours on the next frame.
    if let Some((divider, delta)) = dragged {
        let mut bounds = Vec::with_capacity(panes - 1);
        let mut acc = 0.0;
        for size in sizes.iter().take(panes - 1) {
            acc += size;
            bounds.push(acc);
        }
        bounds[divider] += delta;
        state.split_fractions = bounds
            .iter()
            .map(|b| (b / usable).clamp(0.0, 1.0))
            .collect();
    }

    interactions
}
