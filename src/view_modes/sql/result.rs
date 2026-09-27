//! The query-result grid and its TSV copy helper.
//!
//! Split out of `view_modes/sql.rs` (1,555 lines). Code moved unchanged.

use super::*;

/// How close to the end of the loaded rows the grid has to scroll before the
/// next page is fetched. One page-ish of lead time, so the fetch lands before
/// the user reaches the gap.
const PREFETCH_ROWS: usize = 200;

/// Render the result grid. Returns whether the view has scrolled close enough
/// to the end of the rows it holds that the next page should be fetched.
///
/// `total` is the exact row count of the whole result; `table` holds only the
/// pages loaded so far.
pub(super) fn render_result_table(
    ui: &mut egui::Ui,
    table: &octa::data::DataTable,
    selected: &mut Option<(usize, usize)>,
    total: Option<usize>,
) -> bool {
    use egui_extras::{Column, TableBuilder};

    if table.col_count() == 0 {
        ui.label(egui::RichText::new(octa::i18n::t("sql.no_columns")).weak());
        return false;
    }
    let loaded = table.row_count();
    let has_more = total.is_some_and(|t| loaded < t);
    let mut want_more = false;

    // `auto_shrink` off on both: the grid sits in a resizable pane, and
    // `egui::Panel` persists the rect its content produced. A grid that
    // shrinks to a short result rewrites the pane height the user dragged to.
    egui::ScrollArea::horizontal()
        .id_salt("sql_result_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .auto_shrink([false, false])
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            for _ in &table.columns {
                builder = builder.column(Column::auto().at_least(80.0).resizable(true));
            }
            builder
                .header(22.0, |mut header| {
                    for col in &table.columns {
                        header.col(|ui| {
                            ui.strong(&col.name);
                        });
                    }
                })
                .body(|body| {
                    // Virtualised: the old `for r in 0..row_count()` laid out
                    // every row of every result on every frame.
                    body.rows(20.0, loaded, |mut row| {
                        let r = row.index();
                        if has_more && r + PREFETCH_ROWS >= loaded {
                            want_more = true;
                        }
                        {
                            for c in 0..table.col_count() {
                                row.col(|ui| {
                                    let v = table.get(r, c).cloned().unwrap_or(CellValue::Null);
                                    let text = v.to_string();
                                    // Click selects the whole cell (highlighted)
                                    // like the main table, so Ctrl+C - handled in
                                    // `render_sql_view` - copies exactly it. The
                                    // context menu adds Copy cell / Copy all.
                                    let is_sel = *selected == Some((r, c));
                                    let resp = ui.selectable_label(is_sel, &text);
                                    if resp.clicked() {
                                        *selected = Some((r, c));
                                        // Take keyboard focus from the editor so
                                        // its TextEdit doesn't swallow Ctrl+C.
                                        resp.request_focus();
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button(octa::i18n::t("header.copy")).clicked() {
                                            ui.ctx().copy_text(text.clone());
                                            ui.close();
                                        }
                                        if ui.button(octa::i18n::t("view.copy_all")).clicked() {
                                            ui.ctx().copy_text(result_to_tsv(table));
                                            ui.close();
                                        }
                                    });
                                });
                            }
                        }
                    });
                });
        });
    want_more
}

/// Serialise a result table to TSV (header row + one row per record, cells
/// joined by tabs, rows by newlines) for the result table's "Copy all" action.
pub(super) fn result_to_tsv(table: &octa::data::DataTable) -> String {
    let mut out = String::new();
    out.push_str(
        &table
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join("\t"),
    );
    out.push('\n');
    for r in 0..table.row_count() {
        let line = (0..table.col_count())
            .map(|c| {
                table
                    .get(r, c)
                    .cloned()
                    .unwrap_or(CellValue::Null)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\t");
        out.push_str(&line);
        out.push('\n');
    }
    out
}
