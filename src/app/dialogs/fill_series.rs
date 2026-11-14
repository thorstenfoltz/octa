//! "Fill series" dialog: fill the selected cells with numbers, an ID's
//! counter, dates or a repeated pattern. The values come from
//! `octa::data::fill_series`; writing goes through `CellValue::parse_like`,
//! the paste path, so a column's type rules still apply, and the whole fill
//! is one undo step.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::fill_series::{self as engine, DateUnit, FillError, FillKind, FillSpec};
use octa::data::{CellValue, DataTable};
use octa::i18n::t;
use octa::ui::control_row::{control_grid, control_row, control_text_edit};
use octa::ui::menu_entry::disabled_tip;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::state::OctaApp;

/// How many values the preview line shows.
const PREVIEW: usize = 5;

pub(crate) struct FillSeriesState {
    pub(crate) spec: FillSpec,
    /// Column and rows, in display order, fixed when the dialog opened.
    pub(crate) targets: Vec<(usize, Vec<usize>)>,
    /// The pattern length as typed; parsed into `spec.pattern`.
    pattern_buf: String,
    error: Option<String>,
    size: DialogSize,
}

impl OctaApp {
    /// Edit > Fill series..., the cell menu and the shortcut.
    pub(crate) fn open_fill_series(&mut self) {
        if self.is_readonly() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let st = &tab.table_state;
        let targets = engine::targets(&st.selected_cols, &st.selected_cells, &tab.filtered_rows);
        let Some((col, rows)) = targets.first() else {
            self.status_message = Some((t("greyed.need_two_cells"), Instant::now()));
            return;
        };
        let spec = engine::guess(&own_cells(&tab.table, *col, rows));
        self.fill_series_dialog = Some(FillSeriesState {
            pattern_buf: spec.pattern.to_string(),
            spec,
            targets,
            error: None,
            size: DialogSize::default(),
        });
    }
}

/// A column's current cells at `rows`, in that order.
fn own_cells(table: &DataTable, col: usize, rows: &[usize]) -> Vec<CellValue> {
    rows.iter()
        .map(|&r| table.get(r, col).cloned().unwrap_or(CellValue::Null))
        .collect()
}

/// The error in words. A Numbers step may have decimals, so its complaint
/// differs from the whole-number one Counter and Dates share.
fn error_text(spec: &FillSpec, e: FillError) -> String {
    if spec.kind == FillKind::Numbers && e == FillError::BadStep {
        t("fill.err_step_number")
    } else {
        t(e.key())
    }
}

/// Write the series into every target column as one undo step.
pub(crate) fn apply_fill(app: &mut OctaApp, st: &FillSeriesState) -> Result<usize, FillError> {
    let tab = &mut app.tabs[app.active_tab];
    // Compute everything first, so a bad spec writes nothing.
    let mut plan: Vec<(usize, &[usize], Vec<String>)> = Vec::new();
    for (col, rows) in &st.targets {
        let own = own_cells(&tab.table, *col, rows);
        plan.push((*col, rows, engine::values(&st.spec, &own, rows.len())?));
    }
    let start = tab.table.undo_stack.len();
    let (row_count, col_count) = (tab.table.row_count(), tab.table.col_count());
    let mut n = 0;
    for (col, rows, vals) in plan {
        // A row or column deleted while the dialog was open is skipped.
        if col >= col_count {
            continue;
        }
        let ty = tab.table.columns[col].data_type.clone();
        for (&r, v) in rows.iter().zip(vals) {
            if r >= row_count {
                continue;
            }
            let new = match tab.table.get(r, col) {
                Some(CellValue::Null) | None => {
                    octa::data::convert_value(&CellValue::String(v), &ty)
                }
                Some(existing) => CellValue::parse_like(existing, &v),
            };
            tab.table.set(r, col, new);
            n += 1;
        }
    }
    tab.table.coalesce_undo_since(start);
    tab.filter_dirty = true;
    Ok(n)
}

/// A text field that is greyed with a reason instead of hidden.
fn field(ui: &mut egui::Ui, text: &mut String, enabled: bool, hint: &str, why: &str) {
    let r = ui
        .add_enabled_ui(enabled, |ui| {
            control_text_edit(ui, 160.0, egui::TextEdit::singleline(text))
        })
        .inner;
    if enabled {
        r.on_hover_text(t(hint));
    } else {
        r.on_disabled_hover_text(disabled_tip(&t(hint), &t(why)));
    }
}

pub(crate) fn render_fill_series_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.fill_series_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };

    // The first values of the first target column, for the preview line.
    let preview = st.targets.first().map(|(col, rows)| {
        let rows = &rows[..rows.len().min(PREVIEW)];
        engine::values(&st.spec, &own_cells(&tab.table, *col, rows), rows.len())
    });
    let preview_err = match &preview {
        Some(Err(e)) => Some(error_text(&st.spec, *e)),
        _ => None,
    };

    let mut close = false;
    let mut apply = false;
    let dialog_id = egui::Id::new("octa_fill_series_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(440.0, 340.0));
    let window = egui::Window::new("octa_fill_series")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(440.0)
            .default_height(340.0)
            .min_width(340.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "fill_series_header",
            RichText::new(t("fill.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("fill_series_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                control_row(ui, |ui| {
                    let b =
                        ui.add_enabled(preview_err.is_none(), egui::Button::new(t("fill.fill")));
                    if b.clicked() {
                        apply = true;
                    }
                    match &preview_err {
                        None => {
                            b.on_hover_text(t("fill.fill_hint"));
                        }
                        Some(e) => {
                            b.on_disabled_hover_text(disabled_tip(&t("fill.fill_hint"), e));
                        }
                    }
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("fill.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let kind = st.spec.kind;
            let repeat = kind == FillKind::Repeat;
            control_grid(ui, "fill_series_grid", |ui| {
                ui.label(t("fill.kind")).on_hover_text(t("fill.kind_hint"));
                egui::ComboBox::from_id_salt("fill_series_kind")
                    .selected_text(t(kind.key()))
                    .show_ui(ui, |ui| {
                        for k in FillKind::ALL {
                            ui.selectable_value(&mut st.spec.kind, k, t(k.key()));
                        }
                    })
                    .response
                    .on_hover_text(t("fill.kind_hint"));
                ui.end_row();

                ui.label(t("fill.start"))
                    .on_hover_text(t("fill.start_hint"));
                field(
                    ui,
                    &mut st.spec.start,
                    !repeat,
                    "fill.start_hint",
                    "fill.start_repeat_why",
                );
                ui.end_row();

                ui.label(t("fill.step")).on_hover_text(t("fill.step_hint"));
                field(
                    ui,
                    &mut st.spec.step,
                    !repeat,
                    "fill.step_hint",
                    "fill.start_repeat_why",
                );
                ui.end_row();

                ui.label(t("fill.unit")).on_hover_text(t("fill.unit_hint"));
                let dates = kind == FillKind::Dates;
                let r = ui
                    .add_enabled_ui(dates, |ui| {
                        egui::ComboBox::from_id_salt("fill_series_unit")
                            .selected_text(t(st.spec.unit.key()))
                            .show_ui(ui, |ui| {
                                for u in DateUnit::ALL {
                                    ui.selectable_value(&mut st.spec.unit, u, t(u.key()));
                                }
                            })
                            .response
                    })
                    .inner;
                if dates {
                    r.on_hover_text(t("fill.unit_hint"));
                } else {
                    r.on_disabled_hover_text(disabled_tip(
                        &t("fill.unit_hint"),
                        &t("fill.unit_why"),
                    ));
                }
                ui.end_row();

                ui.label(t("fill.pattern"))
                    .on_hover_text(t("fill.pattern_hint"));
                field(
                    ui,
                    &mut st.pattern_buf,
                    repeat,
                    "fill.pattern_hint",
                    "fill.pattern_why",
                );
                ui.end_row();
            });
            // An unreadable length means "no pattern", which the preview says.
            st.spec.pattern = st.pattern_buf.trim().parse().unwrap_or(0);

            ui.add_space(8.0);
            ui.label(t("fill.preview"));
            match (&preview, &preview_err) {
                (_, Some(e)) => {
                    octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e)
                }
                (Some(Ok(vals)), None) => {
                    ui.monospace(vals.join(", "));
                }
                _ => {}
            }
            if let Some(e) = &st.error {
                octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
            }
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if apply && !app.is_readonly() {
        match apply_fill(app, &st) {
            Ok(n) => {
                app.status_message = Some((
                    t("fill.done").replace("{n}", &n.to_string()),
                    Instant::now(),
                ));
                return;
            }
            Err(e) => st.error = Some(error_text(&st.spec, e)),
        }
    }
    if !close {
        app.fill_series_dialog = Some(st);
    }
}
