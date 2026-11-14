//! "Calculated column" dialog: rank, dense rank, row number, running total,
//! percent of total, difference and percent change to the previous row, the
//! previous or next row's value, visits from timestamps and ABC classes,
//! optionally restarting per group. The work is `octa::data::calculated`,
//! shared with `--calculate`, the MCP tool and the recipe step.
//!
//! A database tab whose analyses run on the database (the predicate Hash
//! columns uses) gets the column from the server, over every row, when the
//! engine can say it (`calc_sql`) and the spec names only the table's own
//! columns; otherwise the column is computed on the loaded rows and the
//! status line says so.

use std::time::Instant;

use eframe::egui;
use egui::RichText;

use octa::data::CellValue;
use octa::data::calculated::{self as engine, CalcByName, CalcExtras, CalcKind, CalcSpec};
use octa::data::recipe::{Calculated, RecipeStep};
use octa::db::pushdown::calc::{ServerCalc, calc_sql};
use octa::db::pushdown::derived::ServerColumn;
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, center_on_first_show, dialog_header, remember_dialog_rect, size_dialog_window,
};

use super::super::db_view::server_conn;
use super::super::state::{OctaApp, TabState};
use super::widgets::{col_combo, multi_col_picker};

const PREVIEW_ROWS: usize = 8;

type Preview = (CalcSpec, Result<Vec<CellValue>, String>);

#[derive(Default)]
pub(crate) struct CalcState {
    pub(crate) spec: CalcSpec,
    pub(crate) name: String,
    pub(crate) name_edited: bool,
    /// What is typed into the fields only some kinds show. Empty = default.
    pub(crate) offset_buf: String,
    pub(crate) gap_buf: String,
    pub(crate) abc_a_buf: String,
    pub(crate) abc_b_buf: String,
    pub(crate) size: DialogSize,
    /// The spec the preview was computed for and its answer. Recomputed only
    /// when the spec changes. ponytail: a rank sorts the whole table, about a
    /// second at 2M rows, once per change; a sampled preview if it bites.
    preview: Option<Preview>,
    pub(crate) error: Option<String>,
}

impl OctaApp {
    /// Entry from Columns > Calculated column... and the shortcut.
    pub(crate) fn open_calculated_column(&mut self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        if tab.table.col_count() == 0 || self.is_readonly() {
            return;
        }
        let spec = CalcSpec {
            descending: CalcKind::Rank.default_descending(),
            ..CalcSpec::default()
        };
        self.calculated_column_dialog = Some(CalcState {
            spec,
            ..CalcState::default()
        });
    }

    /// The server's version of `spec`, when this tab and engine can compute it.
    fn calc_on_server(&self, tab: &TabState, spec: &CalcSpec, name: &str) -> Option<ServerCalc> {
        let conn = server_conn(
            tab,
            self.settings.db_pushdown,
            &self.settings.db_connections,
        )?;
        let calc = ServerCalc::of(spec, &tab.table.columns, name);
        // A column the server adds itself cannot be named in the same SELECT.
        let derived = calc
            .columns()
            .any(|c| tab.server_columns.iter().any(|d| d.name() == c));
        (!derived && calc_sql(conn.engine, &calc).is_some()).then_some(calc)
    }
}

/// A typed number, comma or dot as the decimal mark. Empty is `Ok(None)`,
/// the default; anything else that is no number is `Err`.
fn typed_number(buf: &str) -> Result<Option<f64>, ()> {
    let b = buf.trim();
    if b.is_empty() {
        return Ok(None);
    }
    b.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(Some)
        .ok_or(())
}

/// The fields of the dialog as the engine's settings, `None` when one of
/// them does not read as a number.
fn typed_extras(st: &CalcState) -> Option<CalcExtras> {
    let offset = typed_number(&st.offset_buf).ok()?;
    if offset.is_some_and(|v| v < 0.0 || v.fract() != 0.0) {
        return None;
    }
    Some(CalcExtras {
        offset: offset.map(|v| v as usize),
        gap_minutes: typed_number(&st.gap_buf).ok()?,
        abc_a: typed_number(&st.abc_a_buf).ok()?,
        abc_b: typed_number(&st.abc_b_buf).ok()?,
    })
}

/// One label / number field pair of the grid, the default shown as the hint.
fn number_row(ui: &mut egui::Ui, key: &str, buf: &mut String, default: &str) {
    let hint = t(&format!("{key}_hint"));
    ui.label(t(key)).on_hover_text(&hint);
    octa::ui::control_row::control_text_edit(
        ui,
        80.0,
        egui::TextEdit::singleline(buf).hint_text(default),
    )
    .on_hover_text(hint);
    ui.end_row();
}

pub(crate) fn render_calculated_column_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(mut st) = app.calculated_column_dialog.take() else {
        return;
    };
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };
    let table = &tab.table;
    let col_names: Vec<String> = table.columns.iter().map(|c| c.name.clone()).collect();
    let n = col_names.len();
    // A column deleted while the dialog was open drops out of the pick.
    st.spec.value = st.spec.value.filter(|&c| c < n);
    st.spec.order = st.spec.order.filter(|&c| c < n);
    st.spec.groups.retain(|&c| c < n);
    let typed = typed_extras(&st);
    st.spec.extras = typed.unwrap_or_default();
    if !st.name_edited {
        st.name = engine::default_name(table, &st.spec);
    }
    let name = st.name.trim().to_string();
    let server = app.calc_on_server(tab, &st.spec, &name);
    let db_tab = server_conn(tab, app.settings.db_pushdown, &app.settings.db_connections).is_some();
    let unsaved = server.is_some() && table.is_modified();
    let kind = st.spec.kind;

    // The preview, computed again only when the spec changed.
    if st.preview.as_ref().is_none_or(|(s, _)| *s != st.spec) {
        st.preview = Some((st.spec.clone(), engine::compute(table, &st.spec)));
    }
    let preview_err = st
        .preview
        .as_ref()
        .and_then(|(_, r)| r.as_ref().err().cloned())
        .or_else(|| typed.is_none().then(|| t("calc.not_a_number")));
    let name_taken = col_names.contains(&name);
    let ready = (!kind.needs_value() || st.spec.value.is_some())
        && !name.is_empty()
        && !name_taken
        && !unsaved
        && preview_err.is_none();

    let mut close = false;
    let mut apply = false;
    let dialog_id = egui::Id::new("octa_calculated_column_dialog");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or(st.size));
    let minimized = size == DialogSize::Minimized;

    let center = center_on_first_show(ctx, egui::vec2(520.0, 560.0));
    let window = egui::Window::new("octa_calculated_column")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(560.0)
            .min_width(360.0)
            .default_pos(center)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "calculated_column_header",
            RichText::new(t("calc.title")),
            &mut size,
        ) {
            close = true;
        }
        if minimized {
            return;
        }

        egui::Panel::bottom("calculated_column_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                octa::ui::control_row::control_row(ui, |ui| {
                    let b = ui.add_enabled(ready, egui::Button::new(t("calc.apply")));
                    if b.clicked() {
                        apply = true;
                    }
                    if ready {
                        b.on_hover_text(t("calc.apply_hint"));
                    } else if name_taken {
                        b.on_disabled_hover_text(t("hashcols.name_taken"));
                    } else if unsaved {
                        b.on_disabled_hover_text(t("hashcols.unsaved_hint"));
                    } else if let Some(e) = &preview_err {
                        b.on_disabled_hover_text(e.as_str());
                    } else {
                        b.on_disabled_hover_text(t("calc.apply_disabled_hint"));
                    }
                    if ui
                        .button(t("common.cancel"))
                        .on_hover_text(t("calc.cancel_hint"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(
                    RichText::new(t("calc.intro"))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
                if db_tab {
                    octa::ui::message::partial_note_label(ui, &t("calc.server_intro"));
                }
                ui.add_space(6.0);

                octa::ui::control_row::control_grid(ui, "calculated_column_grid", |ui| {
                    ui.label(t("calc.kind")).on_hover_text(t("calc.kind_hint"));
                    egui::ComboBox::from_id_salt("calculated_column_kind")
                        .selected_text(t(&st.spec.kind.label_key()))
                        .show_ui(ui, |ui| {
                            for k in CalcKind::ALL {
                                let r = ui
                                    .selectable_label(st.spec.kind == k, t(&k.label_key()))
                                    .on_hover_text(t(&k.hint_key()));
                                if r.clicked() && st.spec.kind != k {
                                    st.spec.kind = k;
                                    st.spec.descending = k.default_descending();
                                }
                            }
                        })
                        .response
                        .on_hover_text(t("calc.kind_hint"));
                    ui.end_row();

                    ui.label(t("calc.value"))
                        .on_hover_text(t("calc.value_hint"));
                    if st.spec.kind.needs_value() {
                        col_combo(
                            ui,
                            "calculated_column_value",
                            &mut st.spec.value,
                            &col_names,
                        )
                        .on_hover_text(t("calc.value_hint"));
                    } else {
                        ui.weak("-");
                    }
                    ui.end_row();

                    // Ranks order by their value; the others take a chosen order.
                    if st.spec.kind.uses_order() {
                        ui.label(t("calc.order"))
                            .on_hover_text(t("calc.order_hint"));
                        let text = st
                            .spec
                            .order
                            .and_then(|i| col_names.get(i).cloned())
                            .unwrap_or_else(|| t("calc.table_order"));
                        octa::ui::control_row::control_line(ui, |ui| {
                            egui::ComboBox::from_id_salt("calculated_column_order")
                                .selected_text(text)
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut st.spec.order,
                                        None,
                                        t("calc.table_order"),
                                    );
                                    for (i, nm) in col_names.iter().enumerate() {
                                        ui.selectable_value(&mut st.spec.order, Some(i), nm);
                                    }
                                })
                                .response
                                .on_hover_text(t("calc.order_hint"));
                            ui.checkbox(&mut st.spec.descending, t("calc.descending"))
                                .on_hover_text(t("calc.descending_hint"));
                        });
                        ui.end_row();
                    } else if matches!(st.spec.kind, CalcKind::Rank | CalcKind::DenseRank) {
                        ui.label("");
                        ui.checkbox(&mut st.spec.descending, t("calc.descending"))
                            .on_hover_text(t("calc.descending_hint"));
                        ui.end_row();
                    }

                    match st.spec.kind {
                        CalcKind::Previous | CalcKind::Next => {
                            number_row(ui, "calc.offset", &mut st.offset_buf, "1");
                        }
                        CalcKind::Session => {
                            number_row(ui, "calc.gap", &mut st.gap_buf, "30");
                        }
                        CalcKind::Abc => {
                            number_row(ui, "calc.abc_a", &mut st.abc_a_buf, "80");
                            number_row(ui, "calc.abc_b", &mut st.abc_b_buf, "95");
                        }
                        _ => {}
                    }

                    ui.label(t("calc.name")).on_hover_text(t("calc.name_hint"));
                    if ui
                        .add(egui::TextEdit::singleline(&mut st.name).desired_width(220.0))
                        .on_hover_text(t("calc.name_hint"))
                        .changed()
                    {
                        st.name_edited = true;
                    }
                    ui.end_row();
                });
                if name_taken {
                    ui.colored_label(ui.visuals().warn_fg_color, t("hashcols.name_taken"));
                }

                ui.add_space(6.0);
                ui.label(t("calc.groups"))
                    .on_hover_text(t("calc.groups_hint"));
                multi_col_picker(ui, "calc_groups", &mut st.spec.groups, &col_names);

                ui.add_space(8.0);
                ui.label(t("calc.preview"))
                    .on_hover_text(t("calc.preview_hint"));
                match st.preview.as_ref().map(|(_, r)| r) {
                    Some(Ok(vals)) => {
                        let shown = |r: usize| match st.spec.value.filter(|_| kind.needs_value()) {
                            Some(c) => table.get(r, c).map(|v| v.to_string()).unwrap_or_default(),
                            None => (r + 1).to_string(),
                        };
                        octa::ui::control_row::control_grid(
                            ui,
                            "calculated_column_preview",
                            |ui| {
                                for (r, v) in vals.iter().enumerate().take(PREVIEW_ROWS) {
                                    ui.label(shown(r));
                                    ui.label(v.to_string());
                                    ui.end_row();
                                }
                            },
                        );
                    }
                    Some(Err(_)) | None if typed.is_none() => {
                        octa::ui::message::selectable_message(
                            ui,
                            ui.visuals().error_fg_color,
                            &t("calc.not_a_number"),
                        )
                    }
                    Some(Err(e)) => {
                        octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e)
                    }
                    None => {}
                }
                if let Some(e) = &st.error {
                    octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, e);
                }
            });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;
    ctx.data_mut(|d| d.insert_temp(size_key, size));

    if apply && !app.is_readonly() {
        let tab = &mut app.tabs[app.active_tab];
        let by = CalcByName::of(&st.spec, &tab.table);
        let step = |name: String| {
            RecipeStep::Calculated(Calculated {
                kind: by.kind,
                value: by.value.clone(),
                order: by.order.clone(),
                descending: by.descending,
                groups: by.groups.clone(),
                extras: by.extras,
                name,
            })
        };
        if let Some(mut calc) = server {
            // Same rule as the local path: a taken name gets a number.
            let name = octa::data::unique_name(&name, |n| {
                tab.table.columns.iter().any(|c| c.name == n)
                    || tab.server_columns.iter().any(|d| d.name() == n)
            });
            calc.name = name.clone();
            // The database fills the column: the sync reads the table again.
            tab.server_columns.push(ServerColumn::Calc(calc));
            if !tab.server_column_seen.contains(&name) {
                tab.server_column_seen.push(name.clone());
            }
            let idx = tab.table.col_count();
            let data_type = engine::result_type(&tab.table, &st.spec);
            tab.table.insert_column(idx, name.clone(), data_type);
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
            app.status_message = Some((
                t("calc.server_done").replace("{name}", &name),
                Instant::now(),
            ));
            app.record_step(step(name));
            return;
        }
        let start = tab.table.undo_stack.len();
        match engine::add_calculated_column(&mut tab.table, &st.spec, Some(&name)) {
            Ok((name, _)) => {
                tab.table.coalesce_undo_since(start);
                tab.filter_dirty = true;
                tab.table_state.widths_initialized = false;
                let key = if db_tab {
                    "calc.server_local"
                } else if tab.source_has_more() {
                    "calc.done_partial"
                } else {
                    "calc.done"
                };
                app.status_message = Some((t(key).replace("{name}", &name), Instant::now()));
                app.record_step(step(name));
                return;
            }
            Err(e) => st.error = Some(format!("{e:#}")),
        }
    }
    if !close {
        app.calculated_column_dialog = Some(st);
    }
}

#[cfg(test)]
#[path = "calculated_column_tests.rs"]
mod tests;
