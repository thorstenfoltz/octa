//! Small widgets shared by the column-picking dialogs (Pivot, Time series).
//!
//! Extracted from `pivot.rs` when the second dialog needed the same three
//! pieces. Every caller passes its own `id_salt`: two of these dialogs can be
//! open at once, and egui ids that collide make the second one's scroll state
//! fight the first's.

use eframe::egui;
use egui::RichText;

/// Cap on how many source rows a live preview runs against, so previewing an
/// operation on a huge table stays instant.
pub(crate) const PREVIEW_SOURCE_ROWS: usize = 1000;
/// Cap on how many result rows a preview shows.
pub(crate) const PREVIEW_RESULT_ROWS: usize = 10;
/// Cap on how many result columns a preview shows (a pivot can be very wide).
pub(crate) const PREVIEW_MAX_COLS: usize = 12;

/// A single-column dropdown writing into `sel`.
pub(crate) fn col_combo(ui: &mut egui::Ui, id: &str, sel: &mut Option<usize>, cols: &[String]) {
    col_combo_ordered(ui, id, sel, cols, None);
}

/// [`col_combo`] with an explicit listing order, for pickers that want the
/// likely candidates first (the Time series time column offers the date-typed
/// columns before the rest). `order` holds indices into `cols`; `None` lists
/// them in table order.
pub(crate) fn col_combo_ordered(
    ui: &mut egui::Ui,
    id: &str,
    sel: &mut Option<usize>,
    cols: &[String],
    order: Option<&[usize]>,
) {
    let text = sel
        .and_then(|i| cols.get(i).cloned())
        .unwrap_or_else(|| octa::i18n::t("dialog.pv_pick"));
    egui::ComboBox::from_id_salt(id)
        .selected_text(text)
        .width(180.0)
        .show_ui(ui, |ui| {
            let indices: Vec<usize> = match order {
                Some(o) => o.to_vec(),
                None => (0..cols.len()).collect(),
            };
            for i in indices {
                let Some(name) = cols.get(i) else { continue };
                if ui.selectable_label(*sel == Some(i), name).clicked() {
                    *sel = Some(i);
                }
            }
        });
}

/// A checkbox picker writing into `sel` (preserving pick order). Checkboxes
/// wrap across the full available width (using the empty space beside a single
/// narrow column) and the area has a bounded height: it grows with the number
/// of columns up to a cap, then scrolls, so the dialog never stretches to the
/// bottom of the screen.
pub(crate) fn multi_col_picker(ui: &mut egui::Ui, id: &str, sel: &mut Vec<usize>, cols: &[String]) {
    // Frame the picker so the wrap region reads as one panel.
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            // `auto_shrink([false, true])`: take the full width, but shrink to
            // the content height up to `max_height` (then scroll). This is the
            // bound that stops the list running to the end of the screen.
            .auto_shrink([false, true])
            .max_height(220.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, name) in cols.iter().enumerate() {
                        let mut on = sel.contains(&i);
                        if ui.checkbox(&mut on, name).changed() {
                            if on {
                                if !sel.contains(&i) {
                                    sel.push(i);
                                }
                            } else {
                                sel.retain(|c| *c != i);
                            }
                        }
                    }
                });
            });
    });
}

/// Render a small, bounded preview table (header + a few rows, capped columns).
pub(crate) fn render_preview_grid(ui: &mut egui::Ui, id: &str, table: &octa::data::DataTable) {
    let ncols = table.col_count().min(PREVIEW_MAX_COLS);
    let more_cols = table.col_count() > PREVIEW_MAX_COLS;
    egui::ScrollArea::horizontal()
        .id_salt(format!("{id}_preview_scroll"))
        .max_height(160.0)
        .show(ui, |ui| {
            egui::Grid::new(format!("{id}_preview_grid"))
                .striped(true)
                .show(ui, |ui| {
                    for c in 0..ncols {
                        let name = table.columns.get(c).map(|c| c.name.as_str()).unwrap_or("");
                        ui.label(RichText::new(name).strong());
                    }
                    if more_cols {
                        ui.label(RichText::new("...").strong());
                    }
                    ui.end_row();
                    for r in 0..table.row_count() {
                        for c in 0..ncols {
                            let cell = table.get(r, c).map(|v| v.to_string()).unwrap_or_default();
                            let cell: String = cell.chars().take(30).collect();
                            ui.label(cell);
                        }
                        if more_cols {
                            ui.label("...");
                        }
                        ui.end_row();
                    }
                });
        });
}
