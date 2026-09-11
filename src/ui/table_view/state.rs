//! Public `impl` block on [`TableViewState`]. Split out of
//! [`super`] for navigability; the struct definition and its private fields
//! live in `mod.rs`. No behaviour change.

use egui::Ui;

use crate::data::{BinaryDisplayMode, DataTable};

use super::{
    DEFAULT_COL_WIDTH, MIN_COL_WIDTH, NumFmtCtx, SORT_ARROW_SIZE, TableViewState,
    compute_optimal_col_width,
};

impl TableViewState {
    /// Ensure column widths are initialized for the given table.
    pub fn ensure_widths(&mut self, table: &DataTable) {
        if !self.widths_initialized || self.col_widths.len() != table.col_count() {
            self.col_widths = vec![DEFAULT_COL_WIDTH; table.col_count()];
            for (i, col) in table.columns.iter().enumerate() {
                let name_width = col.name.len() as f32 * 8.0 + 32.0 + SORT_ARROW_SIZE * 2.0 + 8.0;
                let type_width = col.data_type.len() as f32 * 6.5 + 32.0;
                let mut max_width = name_width.max(type_width);

                let sample_count = table.row_count().min(50);
                for row in 0..sample_count {
                    if let Some(val) = table.get(row, i) {
                        let text = val.to_string();
                        let text_width = text.len() as f32 * 7.5 + 20.0;
                        max_width = max_width.max(text_width);
                    }
                }

                self.col_widths[i] = max_width.max(MIN_COL_WIDTH);
            }
            self.widths_initialized = true;
            self.invalidate_row_heights();
        }
    }

    /// Give every row the height its content needs: drop every hand-set
    /// height (per row and the `#` corner's uniform one) so the offsets pass
    /// measures the wrapped cells again. The row-side twin of
    /// [`Self::fit_all_columns`]; needs cell line breaks on to show anything,
    /// which the app switches on when it calls this.
    pub fn fit_all_rows(&mut self) {
        self.row_heights.clear();
        self.uniform_row_height = None;
        self.invalidate_row_heights();
    }

    /// Mark the row-height cache as stale. Call after any change that could
    /// affect row heights: cell edits, column resize, data load, sort, filter,
    /// zoom, undo/redo, row insert/delete.
    pub fn invalidate_row_heights(&mut self) {
        self.row_heights_generation = self.row_heights_generation.wrapping_add(1);
    }

    /// Make the vertical scrollbar stand for a whole file rather than for the
    /// loaded page: `(first row of the page, rows in the file)`. Set per frame
    /// by large-file mode and left `None` everywhere else, where the page IS
    /// the table. Dragging it emits [`TableInteraction::jump_to_row`].
    pub fn set_virtual_rows(&mut self, offset_and_total: Option<(usize, usize)>) {
        self.virtual_rows = offset_and_total;
        if offset_and_total.is_none() {
            // No file behind the bar means no drag in progress against it.
            self.virtual_drag_row = None;
        }
    }

    /// Ask for the viewport to be moved onto `display_idx` on the next frame.
    /// Use this rather than `set_scroll_y` whenever the target is a *row*: the
    /// row height is only known inside `draw_table`, and guessing it lands the
    /// viewport short (a 24px guess against a 26px row misses the end of a
    /// 2,000-row page by 130 rows).
    pub fn scroll_to_row(&mut self, display_idx: usize) {
        self.pending_scroll_row = Some(display_idx);
    }

    /// Set the vertical scroll offset (used for navigation).
    pub fn set_scroll_y(&mut self, y: f32) {
        self.scroll_y = y;
    }

    /// The vertical scroll offset. Read by large-file mode to notice that the
    /// user scrolled off the top of the loaded page.
    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    /// Set the horizontal scroll offset (used for navigation).
    pub fn set_scroll_x(&mut self, x: f32) {
        self.scroll_x = x;
    }

    /// Start editing the given cell with the given initial buffer.
    pub fn begin_edit(&mut self, row: usize, col: usize, text: String) {
        self.selected_cell = Some((row, col));
        self.editing_cell = Some((row, col, text));
        self.edit_needs_focus = true;
    }

    /// Resize every column to its best-fit width using the same algorithm
    /// as the double-click-the-header-seam gesture. Sample is capped at
    /// `AUTOFIT_MAX_ROWS` rows per column so multi-million-row tables stay
    /// snappy.
    pub(crate) fn fit_all_columns(
        &mut self,
        ui: &Ui,
        table: &DataTable,
        filtered_rows: &[usize],
        font_size: f32,
        binary_display_mode: BinaryDisplayMode,
        num_fmt: NumFmtCtx<'_>,
    ) {
        self.ensure_widths(table);
        for col_idx in 0..table.col_count() {
            let optimal = compute_optimal_col_width(
                ui,
                table,
                filtered_rows,
                col_idx,
                font_size,
                binary_display_mode,
                num_fmt,
            );
            if let Some(width) = self.col_widths.get_mut(col_idx) {
                *width = optimal;
            }
        }
        self.invalidate_row_heights();
    }
}
