//! The query-result grid, its selection and the TSV copy helpers.

use std::collections::BTreeSet;

use super::*;

/// What is selected in the result grid: single cells, whole rows (the number
/// gutter) and whole columns (the headers), in any mix. Rows and columns stay
/// sets of their own, so selecting a column of a million-row result does not
/// materialise a million cells.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SqlResultSelection {
    pub cells: BTreeSet<(usize, usize)>,
    pub rows: BTreeSet<usize>,
    pub cols: BTreeSet<usize>,
    /// Last plainly or Ctrl-clicked cell: where a Shift+click range starts.
    pub anchor: Option<(usize, usize)>,
}

impl SqlResultSelection {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() && self.rows.is_empty() && self.cols.is_empty()
    }

    pub fn contains(&self, r: usize, c: usize) -> bool {
        self.rows.contains(&r) || self.cols.contains(&c) || self.cells.contains(&(r, c))
    }

    /// A click on a cell, the way a spreadsheet takes it: plain replaces the
    /// selection, Ctrl (Cmd) toggles the one cell, Shift adds the rectangle
    /// from the anchor.
    pub fn click_cell(&mut self, r: usize, c: usize, m: egui::Modifiers) {
        if m.shift
            && let Some((ar, ac)) = self.anchor
        {
            for rr in ar.min(r)..=ar.max(r) {
                for cc in ac.min(c)..=ac.max(c) {
                    self.cells.insert((rr, cc));
                }
            }
            return;
        }
        if m.command {
            if !self.cells.remove(&(r, c)) {
                self.cells.insert((r, c));
            }
        } else {
            *self = Self::default();
            self.cells.insert((r, c));
        }
        self.anchor = Some((r, c));
    }

    /// A click on a row number (`is_row`) or a column header. Ctrl adds or
    /// removes it; plain replaces the selection.
    pub fn click_line(&mut self, idx: usize, is_row: bool, m: egui::Modifiers) {
        if !m.command {
            *self = Self::default();
        }
        let set = if is_row {
            &mut self.rows
        } else {
            &mut self.cols
        };
        if !set.remove(&idx) {
            set.insert(idx);
        }
    }
}

/// The selected cells as TSV, rows top to bottom, each row's selected cells
/// left to right, joined by tabs. No header row: this is what a spreadsheet
/// puts on the clipboard for the same selection. Cells past the loaded rows
/// are skipped.
pub fn selection_to_tsv(table: &octa::data::DataTable, sel: &SqlResultSelection) -> String {
    let loaded = table.row_count();
    let ncols = table.col_count();
    let mut rows: BTreeSet<usize> = sel.rows.iter().copied().filter(|&r| r < loaded).collect();
    rows.extend(sel.cells.iter().map(|&(r, _)| r).filter(|&r| r < loaded));
    if sel.cols.iter().any(|&c| c < ncols) {
        rows.extend(0..loaded);
    }
    let mut out = String::new();
    for r in rows {
        let line = (0..ncols)
            .filter(|&c| sel.contains(r, c))
            .map(|c| table.get(r, c).map(|v| v.to_string()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\t");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Starting width of a result column: its longest text among the header and
/// the first rows, clamped. Chars times an average glyph width rather than a
/// real layout, because it runs every frame and only has to be about right;
/// the user can drag from there.
fn initial_col_width(table: &octa::data::DataTable, c: usize) -> f32 {
    let header = table.columns[c].name.chars().count();
    let longest = (0..table.row_count().min(200))
        .filter_map(|r| table.get(r, c))
        .map(|v| v.to_string().chars().count())
        .max()
        .unwrap_or(0)
        .max(header);
    (longest as f32 * 7.5 + 20.0).clamp(60.0, 360.0)
}

/// A single row or column of the result as TSV, for the context menu.
fn line_to_tsv(table: &octa::data::DataTable, idx: usize, is_row: bool) -> String {
    let mut sel = SqlResultSelection::default();
    if is_row {
        sel.rows.insert(idx);
    } else {
        sel.cols.insert(idx);
    }
    selection_to_tsv(table, &sel)
}

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
    selected: &mut SqlResultSelection,
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
            // Nothing in the grid may change size while it scrolls: hovered
            // widgets grow by `expansion` in several themes, and that reads as
            // the whole grid twitching under the pointer.
            ui.style_mut().visuals.widgets.hovered.expansion = 0.0;
            ui.style_mut().visuals.widgets.active.expansion = 0.0;
            // Keyed on the column names, so a new query with other columns
            // starts from fresh widths and a re-run keeps the user's drags.
            let mut salt = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(
                &table.columns.iter().map(|c| &c.name).collect::<Vec<_>>(),
                &mut salt,
            );
            // Rows as tall as a selectable cell actually is under the active
            // theme, plus a little air. A fixed 20px was shorter than a themed
            // button, and the clipped column cut the selection highlight off
            // at the top and bottom.
            let row_h =
                octa::ui::control_row::control_height(ui).max(ui.spacing().interact_size.y) + 4.0;
            let pad_x = ui.spacing().button_padding.x;
            let mut builder = TableBuilder::new(ui)
                .id_salt(std::hash::Hasher::finish(&salt))
                .striped(true)
                .resizable(true)
                .auto_shrink([false, false])
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center));
            // Row-number gutter: click selects the row, Ctrl adds it. Sized
            // for the whole result, not the rows loaded so far, so it does not
            // widen when the next page lands.
            let digits = total.unwrap_or(loaded).max(1).to_string().len() as f32;
            // Wide enough for the number plus the button's own padding, so its
            // highlight is not cut at the sides either.
            builder = builder.column(Column::exact(digits * 8.0 + 2.0 * pad_x + 8.0));
            // Fixed widths, measured once from the header and the first rows.
            // `Column::auto` re-fitted every column to whatever rows were on
            // screen, so scrolling past a longer value resized the grid.
            for c in 0..table.col_count() {
                builder = builder.column(
                    Column::initial(initial_col_width(table, c))
                        .at_least(40.0)
                        .clip(true)
                        .resizable(true),
                );
            }
            builder
                .header(row_h, |mut header| {
                    header.col(|_| {});
                    for (c, col) in table.columns.iter().enumerate() {
                        header.col(|ui| {
                            let resp = ui
                                .add(
                                    egui::Button::selectable(
                                        selected.cols.contains(&c),
                                        egui::RichText::new(&col.name).strong(),
                                    )
                                    .truncate(),
                                )
                                // The name first: a narrow column truncates it.
                                .on_hover_text(format!(
                                    "{}\n\n{}",
                                    col.name,
                                    octa::i18n::t("sql.result_header_hint")
                                ));
                            if resp.clicked() {
                                selected.click_line(c, false, ui.input(|i| i.modifiers));
                                resp.request_focus();
                            }
                            resp.context_menu(|ui| {
                                if ui.button(octa::i18n::t("sql.copy_column")).clicked() {
                                    ui.ctx().copy_text(line_to_tsv(table, c, false));
                                    ui.close();
                                }
                            });
                        });
                    }
                })
                .body(|body| {
                    // Virtualised: the old `for r in 0..row_count()` laid out
                    // every row of every result on every frame.
                    body.rows(row_h, loaded, |mut row| {
                        let r = row.index();
                        if has_more && r + PREFETCH_ROWS >= loaded {
                            want_more = true;
                        }
                        row.col(|ui| {
                            let resp = ui.selectable_label(
                                selected.rows.contains(&r),
                                egui::RichText::new((r + 1).to_string()).weak(),
                            );
                            if resp.clicked() {
                                selected.click_line(r, true, ui.input(|i| i.modifiers));
                                resp.request_focus();
                            }
                            resp.context_menu(|ui| {
                                if ui.button(octa::i18n::t("sql.copy_row")).clicked() {
                                    ui.ctx().copy_text(line_to_tsv(table, r, true));
                                    ui.close();
                                }
                            });
                        });
                        {
                            for c in 0..table.col_count() {
                                row.col(|ui| {
                                    let v = table.get(r, c).cloned().unwrap_or(CellValue::Null);
                                    let text = v.to_string();
                                    // Click selects the cell like the main
                                    // table; Ctrl / Shift extend it. Ctrl+C -
                                    // handled in `render_sql_view` - copies the
                                    // selection.
                                    // Truncated: a long value ends at its
                                    // column instead of sizing the cell.
                                    let resp = ui.add(
                                        egui::Button::selectable(selected.contains(r, c), &text)
                                            .truncate(),
                                    );
                                    if resp.clicked() {
                                        selected.click_cell(r, c, ui.input(|i| i.modifiers));
                                        // Take keyboard focus from the editor so
                                        // its TextEdit doesn't swallow Ctrl+C.
                                        resp.request_focus();
                                    }
                                    resp.context_menu(|ui| {
                                        if ui.button(octa::i18n::t("header.copy")).clicked() {
                                            ui.ctx().copy_text(text.clone());
                                            ui.close();
                                        }
                                        if ui.button(octa::i18n::t("sql.copy_row")).clicked() {
                                            ui.ctx().copy_text(line_to_tsv(table, r, true));
                                            ui.close();
                                        }
                                        if ui.button(octa::i18n::t("sql.copy_column")).clicked() {
                                            ui.ctx().copy_text(line_to_tsv(table, c, false));
                                            ui.close();
                                        }
                                        if !selected.is_empty()
                                            && ui
                                                .button(octa::i18n::t("sql.copy_selection"))
                                                .clicked()
                                        {
                                            ui.ctx().copy_text(selection_to_tsv(table, selected));
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
