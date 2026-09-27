//! A vertical splitter: several panes sharing one fixed height, with a drag
//! handle between each pair.
//!
//! Not a stack of `egui::Panel`s. A chain of panels each clamps itself to
//! whatever the one before it left, so the panes do not add up to anything in
//! particular: with four of them in a short panel the last ones get slivers,
//! a handle whose pane is already at its floor stops responding, and content
//! that does not fit is drawn over its neighbour. A splitter fixes the total
//! instead: the panes always sum to exactly the height there is, a drag moves
//! space between two neighbours and nobody else, and every pane is clipped to
//! its own rect, so nothing can paint outside the slot it was given.

use eframe::egui;

/// Room reserved for one drag handle, between two panes.
const HANDLE: f32 = 7.0;

/// Height a pane keeps when its neighbour is dragged into it. Below this a
/// pane shows nothing useful, and egui's own `ScrollArea` floor
/// (`min_scrolled_size`, 64px) starts drawing taller than the slot.
pub(super) const MIN_PANE: f32 = 64.0;

/// Fit `weights` into `total`, with every pane at least `min` tall.
///
/// Returns heights that sum to `total`, always: the panes are the space,
/// rather than requests against it. When `total` is too small for `min` all
/// round, `min` gives way (an equal share each) rather than the last pane.
pub(super) fn fit(weights: &[f32], total: f32, min: f32) -> Vec<f32> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    let even = (total / n as f32).max(0.0);
    if total <= 0.0 {
        return vec![0.0; n];
    }
    let min = min.min(even);
    let sum: f32 = weights.iter().filter(|w| w.is_finite()).sum();
    let mut h: Vec<f32> = if sum > 0.0 {
        weights.iter().map(|w| w.max(0.0) * total / sum).collect()
    } else {
        vec![even; n]
    };
    // Lift everyone to `min`, taking it from whoever has slack in proportion
    // to that slack. Repeated, because taking from one can push it under.
    for _ in 0..n {
        let deficit: f32 = h.iter().map(|x| (min - x).max(0.0)).sum();
        if deficit <= 0.01 {
            break;
        }
        let slack: f32 = h.iter().map(|x| (x - min).max(0.0)).sum();
        if slack <= 0.0 {
            return vec![even; n];
        }
        let take = deficit.min(slack);
        for x in h.iter_mut() {
            if *x > min {
                *x -= (*x - min) / slack * take;
            }
        }
        for x in h.iter_mut() {
            *x = x.max(min);
        }
    }
    // Rounding leaves a pixel or two over; give it to the roomiest pane so the
    // sum is exact and the last pane never ends up short of the edge.
    let err = total - h.iter().sum::<f32>();
    if let Some(i) = h
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
    {
        h[i] = (h[i] + err).max(0.0);
    }
    h
}

/// Lay `n` panes out over the rest of `ui`, draw the handles between them and
/// hand back one rect per pane.
///
/// Heights persist in egui memory under `id` and are re-fitted to the space
/// every frame, so a resized window, a re-docked panel or a section that just
/// collapsed never leaves a pane hanging outside.
pub(super) fn vertical_splitter(ui: &mut egui::Ui, id: egui::Id, n: usize) -> Vec<egui::Rect> {
    let area = ui.available_rect_before_wrap();
    if n == 0 || area.height() <= 0.0 {
        return vec![area; n];
    }
    let total = (area.height() - (n - 1) as f32 * HANDLE).max(0.0);

    let mut weights: Vec<f32> = ui
        .data(|d| d.get_temp::<Vec<f32>>(id))
        .filter(|w| w.len() == n)
        .unwrap_or_else(|| vec![total / n as f32; n]);
    let mut h = fit(&weights, total, MIN_PANE);

    // Resolve the drags against last frame's handle positions, before laying
    // anything out, so the panes follow the pointer in the same frame rather
    // than one behind it. The boundary is set to where the pointer is, not
    // nudged by a delta: a delta drifts away from the cursor as soon as a
    // clamp eats part of it.
    for i in 0..n.saturating_sub(1) {
        let Some(resp) = ui.ctx().read_response(id.with(i)) else {
            continue;
        };
        if !(resp.dragged() || resp.drag_stopped()) {
            continue;
        }
        let Some(pointer) = resp.interact_pointer_pos() else {
            continue;
        };
        let boundary: f32 = area.top() + h[..=i].iter().sum::<f32>() + i as f32 * HANDLE;
        let min = MIN_PANE.min(total / n as f32);
        let delta = (pointer.y - HANDLE * 0.5 - boundary)
            .max(min - h[i])
            .min(h[i + 1] - min);
        h[i] += delta;
        h[i + 1] -= delta;
    }

    weights.clear();
    weights.extend_from_slice(&h);
    ui.data_mut(|d| d.insert_temp(id, weights));

    let mut rects = Vec::with_capacity(n);
    let mut y = area.top();
    for (i, height) in h.iter().enumerate() {
        rects.push(egui::Rect::from_min_max(
            egui::pos2(area.left(), y),
            egui::pos2(area.right(), y + height),
        ));
        y += height;
        if i + 1 < n {
            draw_handle(ui, id.with(i), area, y);
            y += HANDLE;
        }
    }
    // The splitter owns this space now: tell the parent, or whatever comes
    // after would be laid out over the top of it.
    ui.advance_cursor_after_rect(area);
    rects
}

/// One handle, drawn where a `Panel`'s separator would be so the two read the
/// same. Widened past its visible line, because a 1px target is unusable.
fn draw_handle(ui: &mut egui::Ui, id: egui::Id, area: egui::Rect, y: f32) {
    let rect = egui::Rect::from_min_max(
        egui::pos2(area.left(), y),
        egui::pos2(area.right(), y + HANDLE),
    );
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    let stroke = if resp.dragged() {
        ui.style().visuals.widgets.active.fg_stroke
    } else if resp.hovered() {
        ui.style().visuals.widgets.hovered.fg_stroke
    } else {
        ui.style().visuals.widgets.noninteractive.bg_stroke
    };
    if stroke.width > 0.0 {
        ui.painter().hline(area.x_range(), rect.center().y, stroke);
    }
}

/// Run `add_contents` inside `rect`, clipped to it.
///
/// The clip is the half of "never overlap" that a budget cannot give you: a
/// `TextEdit` asked for eight rows, or a table with more rows than fit, is
/// bigger than its slot whatever the slot is, and without a clip it paints
/// over the pane below.
pub(super) fn pane<R>(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add_contents(&mut child)
}

#[cfg(test)]
#[path = "splitter_tests.rs"]
mod tests;
