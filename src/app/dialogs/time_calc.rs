//! "Date/Time calculation" modal dialog. Builds a new column from one of five
//! operations over existing date / datetime / numeric columns:
//!
//! - **Difference** between two date columns (in a chosen unit),
//! - **Add / subtract** an amount of a unit to one date column,
//! - **Convert** a numeric duration between units (ms / s / min / h / days),
//! - **Extract** a component (year, month, weekday, ...) from a date column,
//! - **Unix convert** between an epoch timestamp and a date/datetime,
//! - **Duration between** two times or dates, **Add a duration** to one, and
//!   **Duration to a number** / **Number to a duration**.
//!
//! The compute lives in `octa::data::time_calc`; this file is the UI plus the
//! materialisation loop (mirrors `add_column.rs`).

use eframe::egui;
use egui::RichText;

use octa::data::time_calc::{DateComponent, TimeCalcOp, TimeUnit, UnixDirection, UnixUnit};
use octa::data::{self, CellValue};
use octa::ui::settings::{DialogSize, dialog_header, remember_dialog_rect, size_dialog_window};

use super::super::state::{OctaApp, TimeCalcDialog, TimeCalcKind};

const KINDS: &[TimeCalcKind] = &[
    TimeCalcKind::Difference,
    TimeCalcKind::DurationBetween,
    TimeCalcKind::AddSubtract,
    TimeCalcKind::AddDuration,
    TimeCalcKind::ConvertDuration,
    TimeCalcKind::DurationToNumber,
    TimeCalcKind::NumberToDuration,
    TimeCalcKind::Extract,
    TimeCalcKind::UnixConvert,
    TimeCalcKind::ConvertTimezone,
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
        // It adds a column. The menu and the shortcut both land here.
        if self.is_readonly() {
            return;
        }
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
            tz_from: chrono_tz::UTC,
            tz_to: chrono_tz::UTC,
            tz_filter: String::new(),
            next_day: false,
            fixed_amount: false,
            duration_buf: "01:00:00".to_string(),
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

    let mut apply = false;
    let mut close = false;

    let dialog_id = egui::Id::new("octa_time_calc_dialog_v2");
    let size_key = dialog_id.with("octa_dlg_size");
    let mut size = ctx.data_mut(|d| d.get_temp::<DialogSize>(size_key).unwrap_or_default());
    let minimized = size == DialogSize::Minimized;

    let window = egui::Window::new("octa_time_calc")
        .id(dialog_id)
        .title_bar(false)
        .collapsible(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center());
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable([true, true])
            .default_width(360.0)
            .default_height(420.0)
            .min_width(300.0)
            .min_height(220.0)
    });

    let inner = window.show(ctx, |ui| {
        if dialog_header(
            ui,
            "time_calc_header",
            RichText::new(octa::i18n::t("dialog.tc_title")),
            &mut size,
        ) {
            close = true;
        }

        if minimized {
            return;
        }

        egui::CentralPanel::default().show(ui, |ui| {
            // Fill the window in both axes so the resize handles drag freely
            // instead of the window snapping back to content height.
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let Some(state) = app.tabs[app.active_tab].time_calc.as_mut() else {
                        return;
                    };

                    // Operation kind.
                    octa::ui::control_row::control_line(ui, |ui| {
                        ui.label(octa::i18n::t("dialog.tc_operation"));
                        egui::ComboBox::from_id_salt("time_calc_kind")
                            .selected_text(kind_label(state.kind))
                            .show_ui(ui, |ui| {
                                for kind in KINDS {
                                    let entry = ui.selectable_value(
                                        &mut state.kind,
                                        *kind,
                                        kind_label(*kind),
                                    );
                                    if let Some(hint) = kind_hint(*kind) {
                                        entry.on_hover_text(octa::i18n::t(hint));
                                    }
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
                            octa::ui::control_row::control_line(ui, |ui| {
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
                        TimeCalcKind::DurationBetween => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_start_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_end_column"),
                                "tc_col_b",
                                &mut state.col_b,
                                &col_names,
                            );
                            ui.checkbox(&mut state.next_day, octa::i18n::t("dialog.tc_next_day"))
                                .on_hover_text(octa::i18n::t("dialog.tc_next_day_hint"));
                        }
                        TimeCalcKind::AddDuration => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_start_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            ui.checkbox(
                                &mut state.fixed_amount,
                                octa::i18n::t("dialog.tc_fixed_amount"),
                            )
                            .on_hover_text(octa::i18n::t("dialog.tc_fixed_amount_hint"));
                            if state.fixed_amount {
                                octa::ui::control_row::control_line(ui, |ui| {
                                    ui.label(octa::i18n::t("dialog.tc_amount"));
                                    ui.add(
                                        egui::TextEdit::singleline(&mut state.duration_buf)
                                            .desired_width(120.0),
                                    )
                                    .on_hover_text(octa::i18n::t("dialog.tc_fixed_amount_hint"));
                                });
                            } else {
                                column_picker(
                                    ui,
                                    &octa::i18n::t("dialog.tc_duration_column"),
                                    "tc_col_b",
                                    &mut state.col_b,
                                    &col_names,
                                );
                            }
                        }
                        TimeCalcKind::DurationToNumber => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_duration_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            unit_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_to"),
                                "tc_to_unit",
                                &mut state.to_unit,
                                CONVERT_UNITS,
                            );
                        }
                        TimeCalcKind::NumberToDuration => {
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
                        }
                        TimeCalcKind::Extract => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_date_column"),
                                "tc_col_a",
                                &mut state.col_a,
                                &col_names,
                            );
                            octa::ui::control_row::control_line(ui, |ui| {
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
                        TimeCalcKind::ConvertTimezone => {
                            column_picker(
                                ui,
                                &octa::i18n::t("dialog.tc_date_column"),
                                "tc_tz_col",
                                &mut state.col_a,
                                &col_names,
                            );
                            ui.horizontal(|ui| {
                                ui.label(octa::i18n::t("dialog.tc_tz_filter"))
                                    .on_hover_text(octa::i18n::t("dialog.tc_tz_filter_hint"));
                                ui.text_edit_singleline(&mut state.tz_filter);
                            });
                            // 597 IANA zones is far too many to scroll, so both
                            // pickers narrow together from one filter box.
                            let needle = state.tz_filter.to_ascii_lowercase();
                            let zones: Vec<chrono_tz::Tz> = chrono_tz::TZ_VARIANTS
                                .iter()
                                .copied()
                                .filter(|z| {
                                    needle.is_empty()
                                        || z.name().to_ascii_lowercase().contains(&needle)
                                })
                                .collect();
                            for (label, hint, id, target) in [
                                (
                                    octa::i18n::t("dialog.tc_tz_from"),
                                    octa::i18n::t("dialog.tc_tz_from_hint"),
                                    "tc_tz_from",
                                    &mut state.tz_from,
                                ),
                                (
                                    octa::i18n::t("dialog.tc_tz_to"),
                                    octa::i18n::t("dialog.tc_tz_to_hint"),
                                    "tc_tz_to",
                                    &mut state.tz_to,
                                ),
                            ] {
                                octa::ui::control_row::control_line(ui, |ui| {
                                    ui.label(&label).on_hover_text(&hint);
                                    egui::ComboBox::from_id_salt(id)
                                        .selected_text(target.name())
                                        .height(420.0)
                                        .show_ui(ui, |ui| {
                                            for z in &zones {
                                                ui.selectable_value(target, *z, z.name());
                                            }
                                        });
                                });
                            }
                            ui.label(
                                RichText::new(octa::i18n::t("dialog.tc_tz_note"))
                                    .italics()
                                    .weak(),
                            );
                        }
                        TimeCalcKind::UnixConvert => {
                            octa::ui::control_row::control_line(ui, |ui| {
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
                            octa::ui::control_row::control_line(ui, |ui| {
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
                    octa::ui::control_row::control_line(ui, |ui| {
                        ui.label(octa::i18n::t("dialog.insert_at_position"));
                        ui.add(
                            egui::TextEdit::singleline(&mut state.insert_at_text)
                                .desired_width(48.0),
                        );
                        ui.label(format!("/ {}", col_count + 1));
                    });

                    ui.add_space(8.0);
                    octa::ui::control_row::control_line(ui, |ui| {
                        if ui.button(octa::i18n::t("dialog.tc_add_column")).clicked()
                            && !state.new_name.trim().is_empty()
                        {
                            apply = true;
                        }
                        if ui.button(octa::i18n::t("common.cancel")).clicked() {
                            close = true;
                        }
                    });
                });
        });
    });

    if let Some(inner) = inner {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            size_key,
            if close || apply {
                DialogSize::Normal
            } else {
                size
            },
        )
    });

    if apply {
        apply_time_calc(app);
        app.tabs[app.active_tab].time_calc = None;
    }
    if close {
        app.tabs[app.active_tab].time_calc = None;
    }
}

fn kind_label(kind: TimeCalcKind) -> String {
    octa::i18n::t(match kind {
        TimeCalcKind::Difference => "dialog.tc_kind_difference",
        TimeCalcKind::DurationBetween => "dialog.tc_kind_duration_between",
        TimeCalcKind::AddSubtract => "dialog.tc_kind_addsub",
        TimeCalcKind::AddDuration => "dialog.tc_kind_add_duration",
        TimeCalcKind::ConvertDuration => "dialog.tc_kind_convert",
        TimeCalcKind::DurationToNumber => "dialog.tc_kind_duration_to_number",
        TimeCalcKind::NumberToDuration => "dialog.tc_kind_number_to_duration",
        TimeCalcKind::Extract => "dialog.tc_kind_extract",
        TimeCalcKind::UnixConvert => "dialog.tc_kind_unix",
        TimeCalcKind::ConvertTimezone => "dialog.tc_kind_timezone",
    })
}

/// Tooltip for an operation entry; only the Duration ones need one.
fn kind_hint(kind: TimeCalcKind) -> Option<&'static str> {
    match kind {
        TimeCalcKind::DurationBetween => Some("dialog.tc_kind_duration_between_hint"),
        TimeCalcKind::AddDuration => Some("dialog.tc_kind_add_duration_hint"),
        TimeCalcKind::DurationToNumber => Some("dialog.tc_kind_duration_to_number_hint"),
        TimeCalcKind::NumberToDuration => Some("dialog.tc_kind_number_to_duration_hint"),
        _ => None,
    }
}

fn unix_direction_label(dir: UnixDirection) -> String {
    octa::i18n::t(match dir {
        UnixDirection::ToDateTime => "dialog.tc_unix_to_dt",
        UnixDirection::FromDateTime => "dialog.tc_unix_from_dt",
    })
}

fn column_picker(ui: &mut egui::Ui, label: &str, id: &str, selected: &mut usize, names: &[String]) {
    octa::ui::control_row::control_line(ui, |ui| {
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
    octa::ui::control_row::control_line(ui, |ui| {
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
        TimeCalcKind::DurationBetween => TimeCalcOp::DurationBetween {
            next_day: state.next_day,
        },
        TimeCalcKind::AddDuration if !state.fixed_amount => {
            TimeCalcOp::AddDuration { amount: None }
        }
        TimeCalcKind::AddDuration => {
            match octa::data::times::TimeKind::Duration.parse(&state.duration_buf) {
                Some(ns) => TimeCalcOp::AddDuration { amount: Some(ns) },
                None => {
                    app.tabs[app.active_tab].parse_error_banner =
                        Some(octa::i18n::t("dialog.tc_duration_error"));
                    return;
                }
            }
        }
        TimeCalcKind::DurationToNumber => TimeCalcOp::DurationToNumber {
            unit: state.to_unit,
        },
        TimeCalcKind::NumberToDuration => TimeCalcOp::NumberToDuration {
            unit: state.from_unit,
        },
        TimeCalcKind::Extract => TimeCalcOp::Extract {
            component: state.component,
        },
        TimeCalcKind::UnixConvert => TimeCalcOp::UnixConvert {
            direction: state.unix_direction,
            unit: state.unix_unit,
        },
        TimeCalcKind::ConvertTimezone => TimeCalcOp::ConvertTimezone {
            from: state.tz_from,
            to: state.tz_to,
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

    // ConvertTimezone goes through the column helper rather than the generic
    // per-row loop below: `evaluate_cell` collapses "DST-ambiguous" and "not a
    // datetime" into a single `None`, and the whole point of the ambiguity
    // report is that those two are different. Read before `insert_column` so an
    // insert position at or left of the source cannot shift it underneath us.
    let tz_result = if let TimeCalcOp::ConvertTimezone { from, to } = op {
        let values: Vec<CellValue> = (0..row_count)
            .map(|row| {
                tab.table
                    .get(row, state.col_a)
                    .cloned()
                    .unwrap_or(CellValue::Null)
            })
            .collect();
        Some(data::time_calc::convert_timezone_column(&values, from, to))
    } else {
        None
    };

    tab.table.insert_column(
        idx,
        state.new_name.trim().to_string(),
        type_hint.to_string(),
    );

    if let Some((converted, ambiguous)) = tz_result {
        for (row, value) in converted.into_iter().enumerate() {
            tab.table.set(row, idx, value);
        }
        if ambiguous > 0 {
            tab.parse_error_banner = Some(format!(
                "{ambiguous} {}",
                octa::i18n::t("dialog.tc_tz_ambiguous")
            ));
        }
        tab.table_state.widths_initialized = false;
        tab.filter_dirty = true;
        return;
    }

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
                // The first value types the column, unless a later time or
                // duration has digits below a microsecond, which only a
                // nanosecond column keeps on save.
                let t = data::time_calc::cell_arrow_type(&value);
                if produced_type.is_none() || t.ends_with("(Nanosecond)") {
                    produced_type = Some(t);
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
