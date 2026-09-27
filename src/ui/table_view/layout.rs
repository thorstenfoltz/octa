//! Pure layout arithmetic for the grid: scrolling a cell into view, the
//! frozen band, drag targets, and the variable row-height prefix sums.

use egui::Ui;

use super::*;
use crate::data::{BinaryDisplayMode, DataTable};

/// Scroll vertically so the given display-row index stays visible.
pub(super) fn scroll_row_into_view(
    state: &mut TableViewState,
    display_idx: usize,
    row_height: f32,
    data_area_height: f32,
    max_scroll_y: f32,
) {
    let (row_top, row_bottom) = if state.row_y_offsets.len() > display_idx {
        let top = state.row_y_offsets[display_idx];
        let bottom = if display_idx + 1 < state.row_y_offsets.len() {
            state.row_y_offsets[display_idx + 1]
        } else {
            top + row_height
        };
        (top, bottom)
    } else {
        let top = display_idx as f32 * row_height;
        (top, top + row_height)
    };
    if row_top < state.scroll_y {
        state.scroll_y = row_top;
    } else if row_bottom > state.scroll_y + data_area_height {
        state.scroll_y = row_bottom - data_area_height;
    }
    state.scroll_y = state.scroll_y.clamp(0.0, max_scroll_y);
}

/// Scroll horizontally so the given column index stays visible. Frozen
/// columns are always visible, so they never move the scroll; for the rest
/// the visible window starts after the frozen band.
pub(super) fn scroll_col_into_view(
    state: &mut TableViewState,
    col_idx: usize,
    view_width: f32,
    max_scroll_x: f32,
    frozen_cols: usize,
    frozen_width: f32,
) {
    if col_idx < frozen_cols {
        state.scroll_x = state.scroll_x.clamp(0.0, max_scroll_x);
        return;
    }
    let col_left: f32 = state.col_widths[frozen_cols.min(col_idx)..col_idx]
        .iter()
        .sum();
    let col_right = col_left
        + state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);
    let window = view_width - state.row_number_width - frozen_width;
    if col_left < state.scroll_x {
        state.scroll_x = col_left;
    } else if col_right > state.scroll_x + window {
        state.scroll_x = col_right - window;
    }
    state.scroll_x = state.scroll_x.clamp(0.0, max_scroll_x);
}

/// Width of the frozen band: the effective painted widths (hidden columns
/// count 0) of the first `frozen_cols` columns.
pub(super) fn frozen_band_width(
    col_widths: &[f32],
    hidden_columns: &HashSet<usize>,
    frozen_cols: usize,
) -> f32 {
    col_widths
        .iter()
        .enumerate()
        .take(frozen_cols)
        .map(|(i, w)| if hidden_columns.contains(&i) { 0.0 } else { *w })
        .sum()
}

/// Map a pointer x (relative to the data-area origin, i.e. just right of the
/// row-number gutter) to the drag-drop target column, honouring the frozen
/// band: frozen columns sit at fixed positions, scrolled ones shift by
/// `scroll_x`. Uses the same midpoint rule the drag-reorder code always used;
/// with `frozen_cols == 0` it reproduces the original arithmetic exactly.
pub(super) fn drag_target_at_x(
    rel_x: f32,
    col_widths: &[f32],
    frozen_cols: usize,
    frozen_width: f32,
    scroll_x: f32,
) -> usize {
    let n = col_widths.len();
    if n == 0 {
        return 0;
    }
    let frozen_cols = frozen_cols.min(n);
    if frozen_cols > 0 && rel_x < frozen_width {
        let mut acc = 0.0f32;
        let mut target = frozen_cols - 1;
        for (i, &cw) in col_widths.iter().enumerate().take(frozen_cols) {
            if rel_x < acc + cw / 2.0 {
                target = i;
                break;
            }
            acc += cw;
            target = i;
        }
        return target;
    }
    // Content-space x measured from the first scrolled column's left edge.
    let content_x = rel_x - frozen_width + scroll_x;
    let mut acc = 0.0f32;
    let mut target = n - 1;
    for (i, &cw) in col_widths.iter().enumerate().skip(frozen_cols) {
        if content_x < acc + cw / 2.0 {
            target = i;
            break;
        }
        acc += cw;
        target = i;
    }
    target
}

/// Binary search in prefix-sum array to find the row containing a given scroll offset.
pub(super) fn row_at_offset(offsets: &[f32], scroll_y: f32) -> usize {
    let mut lo = 0usize;
    let mut hi = offsets.len().saturating_sub(2);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if offsets[mid] <= scroll_y {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// What measuring a row costs beyond the table itself. Bundled so
/// [`ensure_row_y_offsets`] stays under clippy's argument limit; the four
/// always travel together and all four come from the same settings block.
#[derive(Clone, Copy)]
pub(super) struct RowHeightOpts {
    pub(super) font_size: f32,
    pub(super) base_row_height: f32,
    pub(super) binary_display_mode: BinaryDisplayMode,
    /// Cell line breaks. With them off every unadjusted row is exactly
    /// `base_row_height`, so no measuring pass is needed at all.
    pub(super) wrap: bool,
}

/// Rebuild the prefix-sum of row heights if the cache is stale.
pub(super) fn ensure_row_y_offsets(
    ui: &Ui,
    state: &mut TableViewState,
    table: &DataTable,
    filtered_rows: &[usize],
    opts: RowHeightOpts,
) {
    if state.row_heights_cached_generation == state.row_heights_generation
        && state.row_y_offsets.len() == filtered_rows.len() + 1
    {
        return;
    }
    let col_widths = state.col_widths.clone();
    let overrides = state.row_heights.clone();
    let mut offsets = Vec::with_capacity(filtered_rows.len() + 1);
    offsets.push(0.0);
    let mut cumulative = 0.0f32;
    for &actual_row in filtered_rows {
        // A height the user dragged wins outright. Otherwise measure only when
        // wrapping is on: `compute_row_height` lays out every cell of the row,
        // so running it with wrap off - where every unadjusted row is exactly
        // `base_row_height` - would be an O(rows x cols) text-layout pass for
        // an answer already known.
        let h = match overrides.get(&actual_row) {
            Some(&h) => h,
            None if opts.wrap => compute_row_height(
                ui,
                table,
                actual_row,
                &col_widths,
                opts.font_size,
                opts.base_row_height,
                opts.binary_display_mode,
            ),
            None => opts.base_row_height,
        };
        cumulative += h;
        offsets.push(cumulative);
    }
    state.row_y_offsets = offsets;
    state.row_heights_cached_generation = state.row_heights_generation;
}

/// Compute the height of a row by measuring wrapped text in each cell.
pub(super) fn compute_row_height(
    ui: &Ui,
    table: &DataTable,
    actual_row: usize,
    col_widths: &[f32],
    font_size: f32,
    base_row_height: f32,
    binary_display_mode: BinaryDisplayMode,
) -> f32 {
    let mut max_height = base_row_height;
    let font_id = egui::FontId::new(font_size, egui::FontFamily::Monospace);
    for col_idx in 0..table.col_count() {
        if let Some(value) = table.get(actual_row, col_idx) {
            let text = value.display_with_binary_mode(binary_display_mode);
            let col_width = col_widths
                .get(col_idx)
                .copied()
                .unwrap_or(DEFAULT_COL_WIDTH);
            let wrap_width = (col_width - 12.0).max(20.0); // account for cell padding
            let galley =
                ui.fonts_mut(|f| f.layout(text, font_id.clone(), egui::Color32::WHITE, wrap_width));
            let text_height = galley.size().y + 4.0; // small vertical padding
            max_height = max_height.max(text_height);
        }
    }
    max_height
}
