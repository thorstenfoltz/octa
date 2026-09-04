//! Unit / currency split: the question the clean-up suggestion raises.
//!
//! **Nothing is adjusted silently, and nothing is adjusted at all until
//! Apply.** The dialog states what was found, shows the first rows split the
//! way they would be, and offers three answers; the default is to change
//! nothing.
//!
//! The original column is never touched. New columns are added beside it, so
//! the value as it was written stays in the file.

use eframe::egui;
use egui::RichText;

use octa::data::units::{Detection, parse_value_with};
use octa::data::{CellValue, DataTable};
use octa::ui::settings::{
    DialogSize, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use crate::app::state::{OctaApp, UnitsAction, UnitsState};

/// How many rows the preview shows. Enough to recognise the pattern, few
/// enough to build every frame without thinking about it.
const PREVIEW_ROWS: usize = 8;

impl OctaApp {
    /// Open the dialog for `col`, with the detection and preview already done.
    pub(crate) fn open_units_dialog(&mut self, col: usize) {
        let table = &self.tabs[self.active_tab].table;
        let values: Vec<Option<&str>> = (0..table.row_count())
            .map(|r| match table.get(r, col) {
                Some(CellValue::String(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        let Some(detection) = octa::data::units::detect_column(&values) else {
            return;
        };
        let preview = values
            .iter()
            .flatten()
            .filter_map(|v| {
                parse_value_with(v, detection.style)
                    .map(|p| ((*v).to_string(), fmt_number(p.number), p.unit))
            })
            .take(PREVIEW_ROWS)
            .collect();
        self.units_dialog = Some(UnitsState {
            col,
            action: UnitsAction::default(),
            detection,
            preview,
            size: DialogSize::default(),
        });
    }

    /// Add the chosen columns, in one undo step.
    fn apply_units(&mut self, st: &UnitsState) {
        if st.action == UnitsAction::LeaveAsText {
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        let base = tab
            .table
            .columns
            .get(st.col)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let with_unit = st.action == UnitsAction::AddNumberAndUnit;

        // Read every value first: `insert_column` shifts indices, and reading
        // afterwards would read the wrong column.
        let split: Vec<Option<(f64, String)>> = (0..tab.table.row_count())
            .map(|r| match tab.table.get(r, st.col) {
                Some(CellValue::String(s)) => {
                    parse_value_with(s, st.detection.style).map(|p| (p.number, p.unit))
                }
                _ => None,
            })
            .collect();

        let undo_from = tab.table.undo_stack.len();
        let value_name = unique_name(&tab.table, &format!("{base}_value"));
        insert_values(
            &mut tab.table,
            st.col + 1,
            &value_name,
            "Float64",
            split
                .iter()
                .map(|v| match v {
                    Some((n, _)) => CellValue::Float(*n),
                    None => CellValue::Null,
                })
                .collect(),
        );
        if with_unit {
            let unit_name = unique_name(&tab.table, &format!("{base}_unit"));
            insert_values(
                &mut tab.table,
                st.col + 2,
                &unit_name,
                "Utf8",
                split
                    .iter()
                    .map(|v| match v {
                        Some((_, u)) if !u.is_empty() => CellValue::String(u.clone()),
                        _ => CellValue::Null,
                    })
                    .collect(),
            );
        }
        // One Ctrl+Z undoes the whole split, both columns together.
        tab.table.coalesce_undo_since(undo_from);
        tab.filter_dirty = true;
    }
}

/// Insert one column and fill it, through the same undoable path the Add
/// column dialog uses.
fn insert_values(table: &mut DataTable, at: usize, name: &str, ty: &str, values: Vec<CellValue>) {
    table.insert_column(at, name.to_string(), ty.to_string());
    for (row, v) in values.into_iter().enumerate() {
        if !matches!(v, CellValue::Null) {
            table.set(row, at, v);
        }
    }
}

/// `amount_value`, then `amount_value_2`, so a second run does not collide.
fn unique_name(table: &DataTable, base: &str) -> String {
    if !table.columns.iter().any(|c| c.name == base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !table.columns.iter().any(|col| col.name == *c))
        .unwrap_or_else(|| base.to_string())
}

fn fmt_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

pub(crate) fn render_units_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.units_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut apply = false;
    let mut st = app.units_dialog.take().unwrap();
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    let dialog_id = egui::Id::new("octa_units_dialog");
    let window = egui::Window::new("octa_units")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(520.0)
            .default_height(360.0)
            .min_width(400.0)
            .min_height(260.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("units_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(octa::i18n::t("units.title"))
                            .strong()
                            .size(16.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draw_window_controls(ui, &mut size) {
                            close = true;
                        }
                    });
                });
            });
        if minimized {
            return;
        }
        egui::Panel::bottom("units_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            st.action != UnitsAction::LeaveAsText,
                            egui::Button::new(octa::i18n::t("common.apply")),
                        )
                        .on_hover_text(octa::i18n::t("units.apply_hint"))
                        .on_disabled_hover_text(octa::i18n::t("units.nothing_to_do"))
                        .clicked()
                    {
                        apply = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(found_sentence(&st.detection));
            if st.detection.mixed_units {
                let colour = ui.visuals().warn_fg_color;
                octa::ui::message::selectable_message(
                    ui,
                    colour,
                    &octa::i18n::t("units.mixed_warning"),
                );
            }
            ui.add_space(6.0);

            ui.radio_value(
                &mut st.action,
                UnitsAction::LeaveAsText,
                octa::i18n::t("units.leave"),
            );
            ui.radio_value(
                &mut st.action,
                UnitsAction::AddNumber,
                octa::i18n::t("units.add_number"),
            );
            ui.radio_value(
                &mut st.action,
                UnitsAction::AddNumberAndUnit,
                octa::i18n::t("units.add_both"),
            );

            ui.add_space(8.0);
            ui.label(
                RichText::new(octa::i18n::t("units.preview"))
                    .size(10.0)
                    .color(ui.visuals().weak_text_color()),
            );
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("units_preview")
                    .num_columns(3)
                    .striped(true)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        for (raw, number, unit) in &st.preview {
                            ui.label(RichText::new(raw).monospace());
                            ui.label(RichText::new(number).monospace());
                            ui.label(RichText::new(unit).monospace().weak());
                            ui.end_row();
                        }
                    });
            });
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if apply {
        app.apply_units(&st);
        return;
    }
    if !close {
        app.units_dialog = Some(st);
    }
}

/// "Found 412 values that look like a currency in EUR." The unit is data, so
/// it is substituted rather than translated.
fn found_sentence(d: &Detection) -> String {
    let key = match d.flavour.id() {
        "magnitude" => "units.found_magnitude",
        "currency" => "units.found_currency",
        "percent" => "units.found_percent",
        _ => "units.found_unit",
    };
    octa::i18n::t(key)
        .replace("{n}", &d.matched.to_string())
        .replace("{total}", &d.total.to_string())
        .replace("{unit}", &d.unit)
}
