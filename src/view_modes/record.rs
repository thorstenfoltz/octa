//! Record view: one row of the active table shown vertically as
//! field-name / value pairs, for tables too wide to read in the grid.
//!
//! Navigation walks `TabState.filtered_rows` (the display order), not raw row
//! indices, so it steps through exactly the rows the active search and column
//! filters left visible. That stepping is the pure part and lives at the top of
//! this file so it can be unit-tested without a renderer.

use eframe::egui;

use crate::app::state::{NavDir, TabState};
use crate::ui;
use octa::data::CellValue;
use octa::i18n::t;
use ui::theme::{ThemeColors, ThemeMode};

/// Width of the field-name column, in points.
const NAME_WIDTH: f32 = 180.0;

/// The next data-row index to show, given the currently shown row and the
/// filtered display order.
///
/// Returns `None` when there is nowhere to go: an empty filter, or `current`
/// already sits at the end being stepped towards. When `current` is not in
/// `filtered` at all (a filter changed under the view) it lands on the first
/// visible row in either direction rather than giving up, so the view can
/// always recover.
pub(crate) fn next_record_index(filtered: &[usize], current: usize, dir: NavDir) -> Option<usize> {
    if filtered.is_empty() {
        return None;
    }
    match (filtered.iter().position(|&r| r == current), dir) {
        (Some(pos), NavDir::Next) => filtered.get(pos + 1).copied(),
        (Some(pos), NavDir::Prev) => pos.checked_sub(1).and_then(|p| filtered.get(p).copied()),
        (None, _) => filtered.first().copied(),
    }
}

/// Render one row of `tab.table` as a vertical field-name / value list.
///
/// The shown row is `tab.table_state.selected_cell`'s row when one is
/// selected, otherwise the first entry of `filtered_rows`. The selection is
/// written back, so switching between Table and Record keeps the user's place
/// in both directions.
pub fn render_record_view(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    theme_mode: ThemeMode,
    readonly: bool,
) {
    if tab.table.col_count() == 0 || tab.filtered_rows.is_empty() {
        ui.add_space(8.0);
        ui.label(t("record.empty"));
        return;
    }

    // Resolve the row to show, snapping back into the filtered set if the
    // selection points at a row the current filter hides.
    let Some(row) = tab
        .table_state
        .selected_cell
        .map(|(r, _)| r)
        .filter(|r| tab.filtered_rows.contains(r))
        .or_else(|| tab.filtered_rows.first().copied())
    else {
        ui.label(t("record.empty"));
        return;
    };

    let position = tab
        .filtered_rows
        .iter()
        .position(|&r| r == row)
        .unwrap_or(0);
    let total = tab.filtered_rows.len();

    // Header strip: navigation plus position.
    let mut jump: Option<NavDir> = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(position > 0, egui::Button::new("<").small())
            .on_hover_text(t("record.prev"))
            .clicked()
        {
            jump = Some(NavDir::Prev);
        }
        if ui
            .add_enabled(position + 1 < total, egui::Button::new(">").small())
            .on_hover_text(t("record.next"))
            .clicked()
        {
            jump = Some(NavDir::Next);
        }
        ui.label(
            t("record.position")
                .replace("{n}", &(position + 1).to_string())
                .replace("{total}", &total.to_string()),
        );
        ui.separator();
        ui.weak(if readonly {
            t("record.readonly")
        } else {
            t("record.edit_hint")
        });
    });
    ui.separator();

    // Arrow keys navigate when no text edit has focus.
    if tab.table_state.editing_cell.is_none() && ui.ctx().memory(|m| m.focused()).is_none() {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            jump = Some(NavDir::Next);
        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            jump = Some(NavDir::Prev);
        }
    }

    if let Some(dir) = jump
        && let Some(next) = next_record_index(&tab.filtered_rows, row, dir)
    {
        let col = tab.table_state.selected_cell.map_or(0, |(_, c)| c);
        tab.table_state.selected_cell = Some((next, col));
        tab.table_state.editing_cell = None;
        return;
    }

    render_fields(ui, tab, row, theme_mode, readonly);
}

/// The scrolling field list for one row. Split out so `render_record_view`
/// stays readable: it does resolution and navigation, this does painting.
fn render_fields(
    ui: &mut egui::Ui,
    tab: &mut TabState,
    row: usize,
    theme_mode: ThemeMode,
    readonly: bool,
) {
    let col_count = tab.table.col_count();
    let row_height = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;

    // Search highlighting reuses the shared engine every other text view uses.
    let matcher = (!tab.search_text.is_empty()).then(|| tab.search_matcher());
    let (normal, active) =
        ui::search_highlight::highlight_colors(&ThemeColors::for_mode(theme_mode));

    let mut commit: Option<(usize, String)> = None;
    let mut begin: Option<(usize, String)> = None;

    // `show_rows` reads `item_spacing.y` from the **outer** `ui`, not the inner
    // one it hands the closure, so `row_height` above must already account for
    // the spacing the outer context applies. Getting this wrong drifts the
    // virtualised window out of alignment further down a long field list.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, row_height, col_count, |ui, range| {
            for col in range {
                let field = tab
                    .table
                    .columns
                    .get(col)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let text = tab
                    .table
                    .get(row, col)
                    .map(|v| v.to_string())
                    .unwrap_or_default();

                ui.horizontal(|ui| {
                    ui.add_sized(
                        [NAME_WIDTH, row_height],
                        egui::Label::new(egui::RichText::new(field).weak()).truncate(),
                    );

                    match tab.table_state.editing_cell.as_mut() {
                        Some((r, c, buf)) if *r == row && *c == col => {
                            let resp = ui
                                .add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
                            let text = buf.clone();
                            // Focus once, exactly like the grid's inline
                            // editor. Re-requesting every frame would mean the
                            // field never loses focus and so never commits.
                            if tab.table_state.edit_needs_focus {
                                resp.request_focus();
                                tab.table_state.edit_needs_focus = false;
                            }
                            // A singleline TextEdit gives up focus on Enter, so
                            // lost_focus covers Enter and clicking away alike.
                            if resp.lost_focus() {
                                commit = Some((col, text));
                            }
                        }
                        _ => {
                            let mut job = egui::text::LayoutJob::simple_singleline(
                                text.clone(),
                                egui::TextStyle::Body.resolve(ui.style()),
                                ui.visuals().text_color(),
                            );
                            if let Some(m) = &matcher {
                                let ranges = m.find_ranges(&text);
                                ui::search_highlight::apply_highlight(
                                    &mut job, &ranges, None, normal, active,
                                );
                            }
                            let resp = ui
                                .add(egui::Label::new(job).truncate().sense(egui::Sense::click()));
                            if !readonly && resp.clicked() {
                                begin = Some((col, text.clone()));
                            }
                        }
                    }
                });
            }
        });

    if let Some((col, text)) = begin {
        tab.table_state.begin_edit(row, col, text);
    }
    if let Some((col, buf)) = commit {
        if let Some(old) = tab.table.get(row, col) {
            let new_val = CellValue::parse_like(old, &buf);
            if new_val != *old {
                tab.table.set(row, col, new_val);
                tab.filter_dirty = true;
            }
        }
        tab.table_state.editing_cell = None;
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
