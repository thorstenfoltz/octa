//! "Date/Time calculation" modal dialog. Builds a new column from one of five
//! operations over existing date / datetime / numeric columns:
//!
//! - **Difference** between two date columns (in a chosen unit),
//! - **Add / subtract** an amount of a unit to one date column,
//! - **Convert** a numeric duration between units (ms / s / min / h / days),
//! - **Extract** a component (year, month, weekday, ...) from a date column,
//! - **Unix convert** between an epoch timestamp and a date/datetime.
//!
//! The compute lives in `octa::data::time_calc`; this file is the UI plus the
//! materialisation loop (mirrors `add_column.rs`).

use eframe::egui;
use egui::RichText;

use octa::data::time_calc::{DateComponent, TimeCalcOp, TimeUnit, UnixDirection, UnixUnit};
use octa::data::{self, CellValue};

use super::super::state::{OctaApp, TimeCalcDialog, TimeCalcKind};

const KINDS: &[TimeCalcKind] = &[
    TimeCalcKind::Difference,
    TimeCalcKind::AddSubtract,
    TimeCalcKind::ConvertDuration,
    TimeCalcKind::Extract,
    TimeCalcKind::UnixConvert,
];

const UNIX_UNITS: &[UnixUnit] = &[
    UnixUnit::Seconds,
    UnixUnit::Milliseconds,
    UnixUnit::Microseconds,
    UnixUnit::Nanoseconds,
];

const DIFF_UNITS: &[TimeUnit] = &[
    TimeUnit::Milliseconds,
    TimeUnit::Seconds,
    TimeUnit::Minutes,
    TimeUnit::Hours,
    TimeUnit::Days,
    TimeUnit::Months,
    TimeUnit::Years,
];

// Conversion is only meaningful between fixed-length units.
const CONVERT_UNITS: &[TimeUnit] = &[
    TimeUnit::Milliseconds,
    TimeUnit::Seconds,
    TimeUnit::Minutes,
    TimeUnit::Hours,
    TimeUnit::Days,
];

const COMPONENTS: &[DateComponent] = &[
    DateComponent::Year,
    DateComponent::Month,
    DateComponent::Day,
    DateComponent::Hour,
    DateComponent::Minute,
    DateComponent::Second,
    DateComponent::Weekday,
];

impl OctaApp {
    /// Seed and open the dialog. Primary column defaults to the selected
    /// column; the second column (for differences) defaults to the next one.
    pub(crate) fn open_time_calc_dialog(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        let col_count = tab.table.col_count();
        let selected_col = tab.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
        let col_a = selected_col.min(col_count.saturating_sub(1));
        let col_b = (col_a + 1).min(col_count.saturating_sub(1));
        tab.time_calc = Some(TimeCalcDialog {
            kind: TimeCalcKind::Difference,
            unit: TimeUnit::Days,
            from_unit: TimeUnit::Milliseconds,
            to_unit: TimeUnit::Seconds,
            amount_buf: "1".to_string(),
            component: DateComponent::Year,
            unix_direction: UnixDirection::ToDateTime,
            unix_unit: UnixUnit::Seconds,
            col_a,
            col_b,
            new_name: "calc".to_string(),
            insert_at_text: (col_count + 1).to_string(),
        });
    }
}

pub(crate) fn render_time_calc_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.tabs[app.active_tab].time_calc.is_none() {
        return;
    }
    let col_names: Vec<String> = app.tabs[app.active_tab]
        .table
        .columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let col_count = col_names.len();

    let mut open = true;
    let mut apply = false;
    let mut cancel = false;

    egui::Window::new(octa::i18n::t("dialog.tc_title"))
        .id(egui::Id::new("octa_time_calc_dialog_v2"))
        .open(&mut open)
        .resizable([true, true])
        .collapsible(false)
        .default_width(360.0)
        .default_height(420.0)
        .min_width(300.0)
        .min_height(220.0)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            // Fill the window in both axes so the resize handles drag freely
            // instead of the window snapping back to content height.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let Some(state) = app.tabs[app.active_tab].time_calc.as_mut() else {
                        return;
                    };

                    // Operation kind.
                    ui.horizontal(|ui| {
                        ui.label(octa::i18n::t("dialog.tc_operation"));
                        egui::ComboBox::from_id_salt("time_calc_kind")
                            .selected_text(kind_label(state.kind))
                            .show_ui(ui, |ui| {
                                for kind in KINDS {
                                    ui.selectable_value(&mut state.kind, *kind, kind_label(*kind));
                                }
                            });
                    });
                    ui.separator();

                    match state.kind {
                        TimeCalcKind::Difference => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_first_date"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_second_date"),
                                "tc_col_b",
                                &mut state.col_b,
                                &col_names,
                            );
                            unit_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_result_unit"),
                                "tc_diff_unit",
                                &mut state.unit,
                                DIFF_UNITS,
                            );
                            ui.label(
                                RichText::new(octa::i18n::t("dialog.tc_computed_as"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        TimeCalcKind::AddSubtract => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_date_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            ui.horizontal(|ui| {
                                ui.label(octa::i18n::t("dialog.tc_amount"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.amount_buf)
                                        .desired_width(80.0)
                                        .hint_text(octa::i18n::t("dialog.tc_amount_hint")),
                                );
                            });
                            unit_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_unit"),
                                "tc_add_unit",
                                &mut state.unit,
                                DIFF_UNITS,
                            );
                            ui.label(
                                RichText::new(octa::i18n::t("dialog.tc_addsub_note"))
                                    .size(10.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        TimeCalcKind::ConvertDuration => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_number_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            unit_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_from"),
                                "tc_from_unit",
                                &mut state.from_unit,
                                CONVERT_UNITS,
                            );
                            unit_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_to"),
                                "tc_to_unit",
                                &mut state.to_unit,
                                CONVERT_UNITS,
                            );
                        }
                        TimeCalcKind::Extract => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_date_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            ui.horizontal(|ui| {
                                ui.label(octa::i18n::t("dialog.tc_component"));
                                egui::ComboBox::from_id_salt("tc_component")
                                    .selected_text(state.component.label_t())
                                    .show_ui(ui, |ui| {
                                        for comp in COMPONENTS {
                                            ui.selectable_value(
                                                &mut state.component,
                                                *comp,
                                                comp.label_t(),
                                            );
                                        }
                                    });
                            });
                        }
                        TimeCalcKind::UnixConvert => {
                            ui.horizontal(|ui| {
                                ui.label(octa::i18n::t("dialog.tc_unix_direction"));
                                egui::ComboBox::from_id_salt("tc_unix_dir")
                                    .selected_text(unix_direction_label(state.unix_direction))
                                    .show_ui(ui, |ui| {
                                        for dir in
                                            [UnixDirection::ToDateTime, UnixDirection::FromDateTime]
                                        {
                                            ui.selectable_value(
                                                &mut state.unix_direction,
                                                dir,
                                                unix_direction_label(dir),
                                            );
                                        }
                                    });
                            });
                            // Column label tracks the direction: a number column feeds
                            // ToDateTime, a date column feeds FromDateTime.
                            let col_label = match state.unix_direction {
                                UnixDirection::ToDateTime => {
                                    octa::i18n::t("dialog.tc_number_column")
                                }
                                UnixDirection::FromDateTime => {
                                    octa::i18n::t("dialog.tc_date_column")
                                }
                            };
                            column_picker(ui, &col_label, "tc_col_a", &mut state.col_a, &col_names);
                            ui.horizontal(|ui| {
                                ui.label(octa::i18n::t("dialog.tc_unix_unit"));
                                egui::ComboBox::from_id_salt("tc_unix_unit")
                                    .selected_text(state.unix_unit.label_t())
                                    .show_ui(ui, |ui| {
                                        for unit in UNIX_UNITS {
                                            ui.selectable_value(
                                                &mut state.unix_unit,
                                                *unit,
                                                unit.label_t(),
                                            );
                                        }
                                    });
                            });
                        }
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(octa::i18n::t("dialog.tc_new_name"));
                        ui.text_edit_singleline(&mut state.new_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label(octa::i18n::t("dialog.insert_at_position"));
                        ui.add(
                            egui::TextEdit::singleline(&mut state.insert_at_text)
                                .desired_width(48.0),
                        );
                        ui.label(format!("/ {}", col_count + 1));
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button(octa::i18n::t("dialog.tc_add_column")).clicked()
                            && !state.new_name.trim().is_empty()
                        {
                            apply = true;
                        }
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
        });

    if apply {
        apply_time_calc(app);
        app.tabs[app.active_tab].time_calc = None;
    }
    if cancel || !open {
        app.tabs[app.active_tab].time_calc = None;
    }
}

fn kind_label(kind: TimeCalcKind) -> String {
    octa::i18n::t(match kind {
        TimeCalcKind::Difference => "dialog.tc_kind_difference",
        TimeCalcKind::AddSubtract => "dialog.tc_kind_addsub",
        TimeCalcKind::ConvertDuration => "dialog.tc_kind_convert",
        TimeCalcKind::Extract => "dialog.tc_kind_extract",
        TimeCalcKind::UnixConvert => "dialog.tc_kind_unix",
    })
}

fn unix_direction_label(dir: UnixDirection) -> String {
    octa::i18n::t(match dir {
        UnixDirection::ToDateTime => "dialog.tc_unix_to_dt",
        UnixDirection::FromDateTime => "dialog.tc_unix_from_dt",
    })
}

fn column_picker(ui: &mut egui::Ui, label: &str, id: &str, selected: &mut usize, names: &[String]) {
    ui.horizontal(|ui| {
        ui.label(label);
        let current = names.get(*selected).map(String::as_str).unwrap_or("");
        egui::ComboBox::from_id_salt(id)
            .selected_text(current)
            // Tables can have many columns; show a tall popup so the user
            // does not have to scroll a 3-4 row dropdown to find one.
            .height(420.0)
            .show_ui(ui, |ui| {
                for (i, name) in names.iter().enumerate() {
                    ui.selectable_value(selected, i, name);
                }
            });
    });
}

fn unit_picker(
    ui: &mut egui::Ui,
    label: &str,
    id: &str,
    selected: &mut TimeUnit,
    units: &[TimeUnit],
) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(id)
            .selected_text(selected.label_t())
            .show_ui(ui, |ui| {
                for unit in units {
                    ui.selectable_value(selected, *unit, unit.label_t());
                }
            });
    });
}

/// Build the `TimeCalcOp` from the dialog state, insert the new column, and
/// fill it row by row. Rows whose inputs can't be interpreted are left null
/// and counted into a banner.
fn apply_time_calc(app: &mut OctaApp) {
    let Some(state) = app.tabs[app.active_tab].time_calc.clone() else {
        return;
    };
    let op = match state.kind {
        TimeCalcKind::Difference => TimeCalcOp::Difference { unit: state.unit },
        TimeCalcKind::AddSubtract => {
            let amount: i64 = match state.amount_buf.trim().parse() {
                Ok(n) => n,
                Err(_) => {
                    app.tabs[app.active_tab].parse_error_banner =
                        Some(octa::i18n::t("dialog.tc_amount_error"));
                    return;
                }
            };
            TimeCalcOp::AddSubtract {
                unit: state.unit,
                amount,
            }
        }
        TimeCalcKind::ConvertDuration => TimeCalcOp::ConvertDuration {
            from: state.from_unit,
            to: state.to_unit,
        },
        TimeCalcKind::Extract => TimeCalcOp::Extract {
            component: state.component,
        },
        TimeCalcKind::UnixConvert => TimeCalcOp::UnixConvert {
            direction: state.unix_direction,
            unit: state.unix_unit,
        },
    };

    let tab = &mut app.tabs[app.active_tab];
    let col_count = tab.table.col_count();
    let idx = state
        .insert_at_text
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|v| (1..=col_count + 1).contains(v))
        .map(|v| v - 1)
        .unwrap_or(col_count);

    let row_count = tab.table.row_count();
    let type_hint = data::time_calc::result_type_name(op);
    tab.table.insert_column(
        idx,
        state.new_name.trim().to_string(),
        type_hint.to_string(),
    );

    let mut skipped = 0usize;
    let mut produced_type: Option<&'static str> = None;
    for row in 0..row_count {
        let a = tab
            .table
            .get(row, state.col_a)
            .cloned()
            .unwrap_or(CellValue::Null);
        let b = if op.needs_second_input() {
            tab.table.get(row, state.col_b).cloned()
        } else {
            None
        };
        match data::time_calc::evaluate_cell(op, &a, b.as_ref()) {
            Some(value) => {
                if produced_type.is_none() {
                    produced_type = Some(data::time_calc::cell_arrow_type(&value));
                }
                tab.table.set(row, idx, value);
            }
            None => skipped += 1,
        }
    }

    // Refine the column type from what was actually produced (e.g. AddSubtract
    // on a datetime column yields datetimes, not the Date32 default).
    if let Some(t) = produced_type
        && let Some(col) = tab.table.columns.get_mut(idx)
    {
        col.data_type = t.to_string();
    }

    if skipped > 0 {
        tab.parse_error_banner = Some(format!(
            "{} {skipped}/{row_count} {} {}",
            octa::i18n::t("dialog.tc_skipped"),
            octa::i18n::t("dialog.formula_rows"),
            octa::i18n::t("dialog.tc_skipped_suffix")
        ));
    }

    tab.table_state.widths_initialized = false;
    tab.filter_dirty = true;
}
