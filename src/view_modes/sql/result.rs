//! The query-result grid and its TSV copy helper.
//!
//! Split out of `view_modes/sql.rs` (1,555 lines). Code moved unchanged.

use super::*;

pub(super) fn render_result_table(
    ui: &mut egui::Ui,
    table: &octa::data::DataTable,
    selected: &mut Option<(usize, usize)>,
) {
    use egui_extras::{Column, TableBuilder};

    if table.col_count() == 0 {
        ui.label(egui::RichText::new(octa::i18n::t("sql.no_columns")).weak());
        return;
    }

    egui::ScrollArea::horizontal()
        .id_salt("sql_result_scroll")
        .show(ui, |ui| {
            let mut builder = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
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
                .body(|mut body| {
                    for r in 0..table.row_count() {
                        body.row(20.0, |mut row| {
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
                        });
                    }
                });
        });
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
