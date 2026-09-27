//! Timeline view: one bar per row from its start to its end, one band per
//! lane, overlaps inside a lane outlined in the warning colour.
//!
//! The bars come from [`octa::data::timeline::build`] over the tab's
//! filtered rows, cached until the columns, the filter or the data change.
//! Overlapping bars in a lane are stacked on their own tracks so neither
//! hides the other. Clicking a bar selects its row, so the Table and Record
//! views open on it.

use std::collections::HashSet;

use eframe::egui;
use egui::{RichText, Stroke};
use egui_plot::{Bar, BarChart, GridMark, Plot};

use octa::data::DataTable;
use octa::data::timeline::{
    Timeline, build, format_secs, lanes_with_overlaps, overlaps, overlaps_columns, overlaps_table,
    time_columns,
};
use octa::i18n::t;
use octa::ui::control_row::control_row;

use crate::app::state::TabState;

/// Bars drawn at most; overlaps are still found on every row.
/// ponytail: a fixed cap, a level-of-detail pass if someone needs millions.
const MAX_BARS: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimelineCols {
    start: usize,
    end: Option<usize>,
    label: Option<usize>,
    lane: Option<usize>,
}

/// What the cache was built from.
type CacheKey = (TimelineCols, usize, usize, usize, Option<usize>);

struct Built {
    key: CacheKey,
    timeline: Timeline,
    pairs: Vec<(usize, usize)>,
    /// Bar indices that take part in any overlap.
    overlapping: HashSet<usize>,
}

/// Per-tab timeline state (session only).
#[derive(Default)]
pub(crate) struct TimelineState {
    cols: Option<TimelineCols>,
    built: Option<Built>,
}

/// Whether a tab offers the Timeline view: a column typed as a date or a
/// datetime. By type only, because the toolbar asks every frame; text
/// columns holding dates still appear in the view's pickers.
pub(crate) fn offered(table: &DataTable) -> bool {
    table.columns.iter().any(|c| {
        let ty = c.data_type.to_ascii_lowercase();
        ty.contains("date") || ty.contains("timestamp")
    })
}

/// A column dropdown labelled by i18n `key`, hovered by `key_hint`.
fn col_combo(
    ui: &mut egui::Ui,
    key: &str,
    sel: &mut Option<usize>,
    choices: &[usize],
    names: &[String],
    optional: bool,
) {
    let hint = t(&format!("{key}_hint"));
    ui.label(t(key)).on_hover_text(&hint);
    let text = sel
        .and_then(|c| names.get(c).cloned())
        .unwrap_or_else(|| t("timeline.none"));
    egui::ComboBox::from_id_salt(key)
        .selected_text(text)
        .width(140.0)
        .show_ui(ui, |ui| {
            if optional {
                ui.selectable_value(sel, None, t("timeline.none"));
            }
            for &c in choices {
                ui.selectable_value(sel, Some(c), &names[c]);
            }
        })
        .response
        .on_hover_text(hint);
}

/// Hover text for each column of the overlaps table, in
/// [`overlaps_columns`] order.
fn overlaps_hints(tl: &Timeline) -> Vec<String> {
    let names = overlaps_columns(tl);
    let (a, b) = (t("timeline.col_side_a"), t("timeline.col_side_b"));
    let keys = ["row", "label", "start", "end"];
    let mut out = vec![t("timeline.col_lane_hint")];
    for side in [&a, &b] {
        out.extend(
            keys.iter()
                .map(|k| format!("{}\n{side}", t(&format!("timeline.col_{k}_hint")))),
        );
    }
    debug_assert_eq!(out.len(), names.len());
    out
}

/// Draw the view. Returns the overlaps table and its column hover texts
/// when the user asked to open it.
pub fn render_timeline_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
) -> Option<(DataTable, Vec<String>)> {
    let names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
    let time_cols = time_columns(&tab.table);
    let Some(&first) = time_cols.first() else {
        ui.add_space(8.0);
        ui.label(t("timeline.empty"));
        return None;
    };
    let state = &mut tab.timeline;
    let cols = state.cols.get_or_insert(TimelineCols {
        start: first,
        end: time_cols.get(1).copied(),
        label: None,
        lane: None,
    });
    if !time_cols.contains(&cols.start) {
        cols.start = first;
    }

    // Column pickers.
    let all: Vec<usize> = (0..names.len()).collect();
    control_row(ui, |ui| {
        let mut start = Some(cols.start);
        col_combo(ui, "timeline.start", &mut start, &time_cols, &names, false);
        cols.start = start.unwrap_or(first);
        col_combo(ui, "timeline.end", &mut cols.end, &time_cols, &names, true);
        col_combo(ui, "timeline.label", &mut cols.label, &all, &names, true);
        col_combo(ui, "timeline.lane", &mut cols.lane, &all, &names, true);
    });
    let cols = *cols;

    let key: CacheKey = (
        cols,
        tab.table.row_count(),
        tab.table.undo_stack.len() + tab.table.edits.len(),
        tab.filtered_rows.len(),
        tab.filtered_rows.last().copied(),
    );
    if state.built.as_ref().is_none_or(|b| b.key != key) {
        let timeline = build(
            &tab.table,
            &tab.filtered_rows,
            cols.start,
            cols.end,
            cols.label,
            cols.lane,
        );
        let pairs = overlaps(&timeline);
        let overlapping = pairs.iter().flat_map(|&(a, b)| [a, b]).collect();
        state.built = Some(Built {
            key,
            timeline,
            pairs,
            overlapping,
        });
    }
    let built = state.built.as_ref().expect("built above");
    let tl = &built.timeline;

    // Summary line.
    let mut open = false;
    ui.horizontal_wrapped(|ui| {
        let warn = ui.visuals().warn_fg_color;
        if built.pairs.is_empty() {
            ui.label(t("timeline.no_overlaps"));
        } else {
            ui.label(
                RichText::new(
                    t("timeline.overlaps")
                        .replace("{n}", &built.pairs.len().to_string())
                        .replace(
                            "{lanes}",
                            &lanes_with_overlaps(&built.pairs, tl).to_string(),
                        ),
                )
                .color(warn),
            );
            if ui
                .button(t("timeline.open_overlaps"))
                .on_hover_text(t("timeline.open_overlaps_hint"))
                .clicked()
            {
                open = true;
            }
        }
        if !tl.bad_rows.is_empty() {
            let rows: Vec<String> = tl
                .bad_rows
                .iter()
                .take(20)
                .map(|r| (r + 1).to_string())
                .collect();
            ui.label(
                RichText::new(t("timeline.bad").replace("{n}", &tl.bad_rows.len().to_string()))
                    .color(warn),
            )
            .on_hover_text(format!("{}\n{}", t("timeline.bad_hint"), rows.join(", ")));
        }
        if tl.skipped > 0 {
            ui.label(
                RichText::new(t("timeline.skipped").replace("{n}", &tl.skipped.to_string())).weak(),
            );
        }
        if tl.bars.len() > MAX_BARS {
            ui.label(
                RichText::new(t("timeline.capped").replace("{n}", &MAX_BARS.to_string())).weak(),
            );
        }
    });
    ui.label(RichText::new(t("timeline.hint")).weak().size(10.0));

    // Lane bands from the top: lane l starts at row `base[l]`.
    let mut base = Vec::with_capacity(tl.lanes.len());
    let mut y = 0usize;
    for &n in &tl.tracks {
        base.push(y);
        y += n + 1; // a free row between lanes
    }
    let span = tl.bars.iter().fold((f64::MAX, f64::MIN), |(lo, hi), b| {
        (lo.min(b.start), hi.max(b.end))
    });
    let min_len = ((span.1 - span.0) * 0.004).max(60.0);

    let visuals = ui.visuals().clone();
    let normal = visuals.selection.bg_fill;
    let warn = visuals.warn_fg_color;
    let selected_row = tab.table_state.selected_cell.map(|(r, _)| r);
    let bars: Vec<Bar> = tl
        .bars
        .iter()
        .enumerate()
        .take(MAX_BARS)
        .map(|(i, b)| {
            let pos = -((base[b.lane] + b.track) as f64);
            let len = (b.end - b.start).max(min_len);
            let hot = built.overlapping.contains(&i);
            let mut stroke = Stroke::new(1.0, if hot { warn } else { normal });
            if selected_row == Some(b.row) {
                stroke = Stroke::new(2.5, visuals.strong_text_color());
            }
            Bar::new(pos, len)
                .base_offset(b.start)
                .width(0.8)
                .fill(if hot {
                    warn.linear_multiply(0.45)
                } else {
                    normal.linear_multiply(0.7)
                })
                .stroke(stroke)
        })
        .collect();

    let lane_rows: Vec<(f64, String)> = tl
        .lanes
        .iter()
        .enumerate()
        .map(|(l, name)| (-(base[l] as f64), name.clone()))
        .collect();
    let short = span.1 - span.0 < 3.0 * 86_400.0;
    // One y grid mark per lane, so every lane gets its name; the default
    // log-10 spacer skips rows once there are many and the names vanish.
    let marks: Vec<GridMark> = lane_rows
        .iter()
        .map(|(y, _)| GridMark {
            value: *y,
            step_size: 1.0,
        })
        .collect();
    // The wheel scrolls both ways (Shift for sideways), Ctrl+wheel zooms.
    let plot = Plot::new(("timeline_plot", tab.table.source_path.clone()))
        .allow_boxed_zoom(false)
        .show_y(false)
        .y_grid_spacer(move |_| marks.clone())
        .x_axis_formatter(move |mark, _| {
            let s = format_secs(mark.value);
            if short {
                s
            } else {
                s.chars().take(10).collect()
            }
        })
        .y_axis_formatter(move |mark, _| {
            lane_rows
                .iter()
                .find(|(y, _)| (mark.value - y).abs() < 1e-6)
                .map(|(_, n)| n.clone())
                .unwrap_or_default()
        })
        .label_formatter(|_| None);
    // No in-plot hover label: egui_plot paints it above the bar and clips it
    // to the plot, so the top lanes' labels were cut off. A tooltip below.
    let chart = BarChart::new("timeline", bars)
        .horizontal()
        .allow_hover(false);
    let resp = plot.show(ui, |plot_ui| {
        plot_ui.bar_chart(chart);
        (plot_ui.response().clicked(), plot_ui.pointer_coordinate())
    });

    let (clicked, pointer) = resp.inner;
    let hit = pointer.and_then(|p| {
        tl.bars.iter().take(MAX_BARS).find(|b| {
            let pos = -((base[b.lane] + b.track) as f64);
            (p.y - pos).abs() <= 0.4
                && p.x >= b.start
                && p.x <= b.start + (b.end - b.start).max(min_len)
        })
    });
    if let Some(b) = hit {
        let when = if b.end > b.start {
            format!("{} - {}", format_secs(b.start), format_secs(b.end))
        } else {
            format_secs(b.start)
        };
        resp.response
            .on_hover_text_at_pointer(format!("{}\n{when}\n#{}", b.label, b.row + 1));
        // A click selects the bar's row.
        if clicked {
            let col = tab.table_state.selected_cell.map(|(_, c)| c).unwrap_or(0);
            tab.table_state.selected_cell = Some((b.row, col));
        }
    }

    open.then(|| (overlaps_table(tl, &built.pairs), overlaps_hints(tl)))
}
