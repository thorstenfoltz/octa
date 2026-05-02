//! Read-only Column Inspector modal: lists every column with its type plus
//! a few quick stats (numeric min/max, not-null, all-unique). Has its own
//! view-only A→Z / Z→A toggle that does **not** mutate the underlying
//! column order — the destructive sort lives on the Edit menu.

use std::collections::HashSet;

use eframe::egui;
use egui::RichText;
use egui_extras::{Column, TableBuilder};

use octa::data::{self, CellValue, is_numeric_data_type};
use octa::ui::settings::{DialogSize, draw_window_controls};

use super::super::state::{ColumnInspectorSort, OctaApp};

#[derive(Debug, Clone)]
struct ColumnStat {
    name: String,
    data_type: String,
    numeric_min: Option<f64>,
    numeric_max: Option<f64>,
    has_null: bool,
    all_unique: bool,
}

fn compute_stats(table: &data::DataTable, binary_mode: data::BinaryDisplayMode) -> Vec<ColumnStat> {
    let row_count = table.row_count();
    table
        .columns
        .iter()
        .enumerate()
        .map(|(col_idx, col)| {
            let numeric = is_numeric_data_type(&col.data_type);
            let mut min: Option<f64> = None;
            let mut max: Option<f64> = None;
            let mut has_null = false;
            let mut seen: HashSet<String> = HashSet::with_capacity(row_count);
            let mut all_unique = true;

            for row in 0..row_count {
                match table.get(row, col_idx) {
                    None | Some(CellValue::Null) => {
                        has_null = true;
                    }
                    Some(CellValue::String(s)) if s.is_empty() => {
                        has_null = true;
                    }
                    Some(value) => {
                        if numeric {
                            let n = match value {
                                CellValue::Int(n) => Some(*n as f64),
                                CellValue::Float(f) => Some(*f),
                                _ => None,
                            };
                            if let Some(n) = n {
                                min = Some(min.map_or(n, |m: f64| m.min(n)));
                                max = Some(max.map_or(n, |m: f64| m.max(n)));
                            }
                        }
                        if all_unique {
                            let key = value.display_with_binary_mode(binary_mode);
                            if !seen.insert(key) {
                                all_unique = false;
                            }
                        }
                    }
                }
            }

            // An empty column (all null) is trivially unique-free; report it
            // as not unique to avoid misleading "Yes" answers on empty data.
            if row_count == 0 || seen.is_empty() {
                all_unique = false;
            }

            ColumnStat {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                numeric_min: min,
                numeric_max: max,
                has_null,
                all_unique,
            }
        })
        .collect()
}

fn format_num(n: Option<f64>) -> String {
    match n {
        None => String::new(),
        Some(v) if v.fract() == 0.0 && v.abs() < 1e15 => format!("{:.0}", v),
        Some(v) => format!("{}", v),
    }
}

fn yes_no(b: bool) -> &'static str {
    if b { "Yes" } else { "No" }
}

const HEADERS: &[&str] = &["#", "Column", "Type", "Min", "Max", "Not Null", "Unique"];

/// Build the seven cell strings for the inspector row at `display_pos`
/// (1-indexed). Shared by the live row renderer, Ctrl+C / "Copy" (which join
/// with tabs), and the per-column copy submenu (which picks one element).
fn stat_row_cells(s: &ColumnStat, display_pos: usize) -> [String; 7] {
    let numeric = is_numeric_data_type(&s.data_type);
    [
        format!("{}", display_pos),
        s.name.clone(),
        s.data_type.clone(),
        if numeric { format_num(s.numeric_min) } else { String::new() },
        if numeric { format_num(s.numeric_max) } else { String::new() },
        yes_no(!s.has_null).to_string(),
        yes_no(s.all_unique).to_string(),
    ]
}

/// Build a single TSV row for the inspector's display position `display_pos`
/// (1-indexed) and the column stat at index `i`. Used both by Ctrl+C and the
/// right-click "Copy" context-menu entry.
fn stat_row_to_tsv(s: &ColumnStat, display_pos: usize) -> String {
    stat_row_cells(s, display_pos).join("\t")
}

/// Build the TSV payload for the current selection (or all rows when none),
/// including the header row.
fn build_tsv(stats: &[ColumnStat], order: &[usize], selected: &HashSet<usize>) -> String {
    let mut lines = Vec::with_capacity(order.len() + 1);
    lines.push(HEADERS.join("\t"));
    let use_all = selected.is_empty();
    for (display_pos, &i) in order.iter().enumerate() {
        if use_all || selected.contains(&display_pos) {
            lines.push(stat_row_to_tsv(&stats[i], display_pos + 1));
        }
    }
    lines.join("\n")
}

/// Copy the values of one inspector column (`column_idx` ∈ 0..7) for every
/// row, one value per line. The right-click action always returns the whole
/// column — selection state is irrelevant here, since the common need is
/// "give me every column name" / "give me every type". Returns `(payload,
/// count)`.
fn build_single_column_copy(
    stats: &[ColumnStat],
    order: &[usize],
    column_idx: usize,
) -> (String, usize) {
    let mut lines = Vec::with_capacity(order.len());
    for (display_pos, &i) in order.iter().enumerate() {
        let cells = stat_row_cells(&stats[i], display_pos + 1);
        lines.push(cells[column_idx].clone());
    }
    let count = lines.len();
    (lines.join("\n"), count)
}

/// Estimate a comfortable dialog width from the longest column name / type.
fn estimate_width(stats: &[ColumnStat]) -> f32 {
    let max_name = stats
        .iter()
        .map(|s| s.name.chars().count())
        .max()
        .unwrap_or(8)
        .max("Column".len());
    let max_type = stats
        .iter()
        .map(|s| s.data_type.chars().count())
        .max()
        .unwrap_or(6)
        .max("Type".len());
    let max_num = stats
        .iter()
        .flat_map(|s| {
            [
                format_num(s.numeric_min).chars().count(),
                format_num(s.numeric_max).chars().count(),
            ]
        })
        .max()
        .unwrap_or(4)
        .max(5);

    let char_w = 7.5_f32;
    let pad_per_col = 24.0_f32;
    let row_num_w = 44.0;
    let name_w = (max_name as f32 * char_w) + pad_per_col;
    let type_w = (max_type as f32 * char_w) + pad_per_col;
    let num_w = (max_num as f32 * char_w) + pad_per_col;
    let yesno_w = 64.0 + pad_per_col;
    let total = row_num_w + name_w + type_w + num_w * 2.0 + yesno_w * 2.0 + 80.0;
    total.clamp(460.0, 1100.0)
}

pub(crate) fn render_column_inspector_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if !app.tabs[app.active_tab].show_column_inspector {
        return;
    }
    let stats = {
        let tab = &app.tabs[app.active_tab];
        compute_stats(&tab.table, app.settings.binary_display_mode)
    };
    let mut sort = app.tabs[app.active_tab].column_inspector_sort;
    let mut size = app.tabs[app.active_tab].column_inspector_size;
    let mut selected = app.tabs[app.active_tab].column_inspector_selected.clone();
    let mut anchor = app.tabs[app.active_tab].column_inspector_anchor;
    let mut close_requested = false;
    let dialog_width = estimate_width(&stats);
    let dialog_height = (stats.len() as f32 * 22.0 + 160.0).clamp(240.0, 640.0);

    // --- Build the display order for the current sort. We need this both for
    // rendering and for clipboard/context-menu actions. ---
    let mut order: Vec<usize> = (0..stats.len()).collect();
    match sort {
        ColumnInspectorSort::Default => {}
        ColumnInspectorSort::Asc => order.sort_by(|&a, &b| {
            stats[a]
                .name
                .to_lowercase()
                .cmp(&stats[b].name.to_lowercase())
        }),
        ColumnInspectorSort::Desc => order.sort_by(|&a, &b| {
            stats[b]
                .name
                .to_lowercase()
                .cmp(&stats[a].name.to_lowercase())
        }),
    }

    // --- Drain the OS Copy event before the central panel sees it, so Ctrl+C
    // inside the inspector copies inspector rows instead of the table cells
    // underneath. Same Ctrl+A consumption guards against the table's
    // Select-All firing while the inspector is in front. ---
    let mut copy_requested = false;
    let mut select_all_requested = false;
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            egui::Event::Copy => {
                copy_requested = true;
                false
            }
            _ => true,
        });
        if i.consume_key(egui::Modifiers::COMMAND, egui::Key::A) {
            select_all_requested = true;
        }
    });

    let mut window = egui::Window::new("Column Inspector")
        .title_bar(false)
        .collapsible(false);
    window = match size {
        DialogSize::Maximized => window.fixed_rect(ctx.screen_rect().shrink(8.0)),
        DialogSize::Minimized => window.resizable(false),
        DialogSize::Normal => window
            .resizable(true)
            .default_width(dialog_width)
            .default_height(dialog_height)
            .min_width(380.0)
            .min_height(180.0),
    };
    let minimized = size == DialogSize::Minimized;

    let mut copy_payload: Option<String> = None;
    let mut status_message: Option<String> = None;
    // Original column indices to select in the underlying table once the
    // dialog finishes rendering. Filled by double-click only — the
    // right-click menu no longer reaches into the underlying table.
    let mut select_in_table: Option<Vec<usize>> = None;
    let mut double_clicked_outer: Option<usize> = None;

    window.show(ctx, |ui| {
        egui::TopBottomPanel::top("column_inspector_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Column Inspector").strong().size(16.0));
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if draw_window_controls(ui, &mut size) {
                                close_requested = true;
                            }
                        },
                    );
                });
            });

        if minimized {
            return;
        }

        egui::TopBottomPanel::bottom("column_inspector_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        close_requested = true;
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} columns · {} rows · {} selected",
                            stats.len(),
                            app.tabs[app.active_tab].table.row_count(),
                            selected.len()
                        ))
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Sort:");
                    let prev_sort = sort;
                    ui.selectable_value(&mut sort, ColumnInspectorSort::Default, "Default");
                    ui.selectable_value(&mut sort, ColumnInspectorSort::Asc, "A -> Z");
                    ui.selectable_value(&mut sort, ColumnInspectorSort::Desc, "Z -> A");
                    if sort != prev_sort {
                        // Sort changed — prior display-position selections no
                        // longer point at the same columns.
                        selected.clear();
                        anchor = None;
                    }
                    ui.label(
                        RichText::new("(view-only — does not change the table)")
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
                ui.separator();

                let visuals = ui.visuals().clone();
                let row_height = 22.0;
                let body_height = ui.available_height();
                let mut clicked: Option<(usize, egui::Modifiers)> = None;
                let mut context_action: Option<ContextAction> = None;

                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::initial(40.0).at_least(32.0))
                    .column(Column::initial(160.0).at_least(80.0).clip(true))
                    .column(Column::initial(110.0).at_least(60.0).clip(true))
                    .column(Column::initial(90.0).at_least(50.0))
                    .column(Column::initial(90.0).at_least(50.0))
                    .column(Column::initial(80.0).at_least(60.0))
                    .column(Column::initial(80.0).at_least(60.0))
                    .max_scroll_height(body_height)
                    .header(24.0, |mut header| {
                        for h in HEADERS {
                            header.col(|ui| {
                                ui.label(RichText::new(*h).strong());
                            });
                        }
                    })
                    .body(|mut body| {
                        for (display_pos, &i) in order.iter().enumerate() {
                            let s = &stats[i];
                            let row_selected = selected.contains(&display_pos);
                            body.row(row_height, |mut row| {
                                row.set_selected(row_selected);

                                let cells = stat_row_cells(s, display_pos + 1);

                                // Build a single per-row response by unioning
                                // each cell's response. Attaching `context_menu`
                                // once per row (instead of once per cell) avoids
                                // the egui menu-state-leak that left the
                                // context menu unopenable after the first use.
                                let mut row_response: Option<egui::Response> = None;
                                for (idx, cell) in cells.iter().enumerate() {
                                    let resp = row.col(|ui| {
                                        let weak = idx == 0 || idx == 2;
                                        let mut text = RichText::new(cell.clone());
                                        if weak {
                                            text = text.color(visuals.weak_text_color());
                                        }
                                        ui.add(
                                            egui::Label::new(text)
                                                .selectable(false)
                                                .truncate(),
                                        );
                                    });
                                    let (_rect, response) = resp;
                                    if response.clicked() {
                                        clicked = Some((display_pos, response.ctx.input(|i| i.modifiers)));
                                    }
                                    if response.double_clicked() {
                                        // Double-click jumps to the column in
                                        // the underlying table — the only
                                        // remaining "reach into the main view"
                                        // gesture from the inspector.
                                        double_clicked_outer = Some(display_pos);
                                    }
                                    row_response = Some(match row_response {
                                        None => response,
                                        Some(prev) => prev.union(response),
                                    });
                                }

                                if let Some(resp) = row_response {
                                    resp.context_menu(|ui| {
                                        if !row_selected {
                                            // Treat right-click on an unselected
                                            // row like a regular click first, so
                                            // the menu's actions target an
                                            // unambiguous row set.
                                            clicked = Some((display_pos, egui::Modifiers::default()));
                                        }
                                        ui.menu_button("Copy column", |ui| {
                                            for (col_idx, header) in HEADERS.iter().enumerate() {
                                                if ui.button(format!("Copy '{}'", header)).clicked() {
                                                    context_action = Some(
                                                        ContextAction::CopyInspectorColumn(col_idx),
                                                    );
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                        ui.separator();
                                        if ui.button("Copy").clicked() {
                                            context_action = Some(ContextAction::CopySelection);
                                            ui.close_menu();
                                        }
                                        if ui.button("Copy all rows").clicked() {
                                            context_action = Some(ContextAction::CopyAll);
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui.button("Select all").clicked() {
                                            context_action = Some(ContextAction::SelectAll);
                                            ui.close_menu();
                                        }
                                        if ui.button("Clear selection").clicked() {
                                            context_action = Some(ContextAction::ClearSelection);
                                            ui.close_menu();
                                        }
                                    });
                                }
                            });
                        }
                    });

                if let Some((row, mods)) = clicked {
                    apply_click(&mut selected, &mut anchor, row, mods, order.len());
                }

                if let Some(act) = context_action {
                    match act {
                        ContextAction::CopySelection => {
                            copy_payload = Some(build_tsv(&stats, &order, &selected));
                            status_message = Some(if selected.is_empty() {
                                format!("Copied {} columns", order.len())
                            } else {
                                format!("Copied {} columns", selected.len())
                            });
                        }
                        ContextAction::CopyAll => {
                            let empty = HashSet::new();
                            copy_payload = Some(build_tsv(&stats, &order, &empty));
                            status_message = Some(format!("Copied {} columns", order.len()));
                        }
                        ContextAction::SelectAll => {
                            selected = (0..order.len()).collect();
                            anchor = order.last().map(|_| order.len() - 1);
                        }
                        ContextAction::ClearSelection => {
                            selected.clear();
                            anchor = None;
                        }
                        ContextAction::CopyInspectorColumn(col_idx) => {
                            let (payload, count) =
                                build_single_column_copy(&stats, &order, col_idx);
                            copy_payload = Some(payload);
                            let header = HEADERS.get(col_idx).copied().unwrap_or("?");
                            status_message =
                                Some(format!("Copied {} value(s) from '{}'", count, header));
                        }
                    }
                }
            });
    });

    // Double-click on an inspector row jumps to that column in the
    // underlying table.
    if let Some(pos) = double_clicked_outer {
        if let Some(&col_idx) = order.get(pos) {
            select_in_table = Some(vec![col_idx]);
        }
    }

    if select_all_requested {
        selected = (0..order.len()).collect();
        anchor = order.last().map(|_| order.len() - 1);
    }

    if copy_requested && copy_payload.is_none() {
        copy_payload = Some(build_tsv(&stats, &order, &selected));
        status_message = Some(if selected.is_empty() {
            format!("Copied {} columns", order.len())
        } else {
            format!("Copied {} columns", selected.len())
        });
    }

    if let Some(payload) = copy_payload {
        ctx.copy_text(payload);
    }

    let tab = &mut app.tabs[app.active_tab];
    tab.column_inspector_sort = sort;
    tab.column_inspector_size = size;
    tab.column_inspector_selected = selected;
    tab.column_inspector_anchor = anchor;
    if close_requested {
        tab.show_column_inspector = false;
        tab.column_inspector_size = DialogSize::Normal;
        tab.column_inspector_selected.clear();
        tab.column_inspector_anchor = None;
    }
    if let Some(cols) = select_in_table {
        if !cols.is_empty() {
            let n = cols.len();
            apply_select_in_table(tab, &cols);
            status_message = Some(if n == 1 {
                let name = tab
                    .table
                    .columns
                    .get(cols[0])
                    .map(|c| c.name.as_str())
                    .unwrap_or("?")
                    .to_string();
                format!("Selected column '{}' in table", name)
            } else {
                format!("Selected {} columns in table", n)
            });
        }
    }
    if let Some(msg) = status_message {
        app.status_message = Some((msg, std::time::Instant::now()));
    }
}

/// Mirror an inspector selection onto the underlying table view: highlight
/// every chosen column, place the cell cursor on the first one, and scroll
/// the table horizontally so the cursor is on screen. Mirrors the
/// `R5:C3`-nav handling in `app::status_bar`.
fn apply_select_in_table(tab: &mut super::super::state::TabState, cols: &[usize]) {
    let total_cols = tab.table.col_count();
    if total_cols == 0 {
        return;
    }
    tab.table_state.selected_rows.clear();
    tab.table_state.selected_cols.clear();
    tab.table_state.selected_cells.clear();
    for &c in cols {
        if c < total_cols {
            tab.table_state.selected_cols.insert(c);
        }
    }
    let first = cols
        .iter()
        .copied()
        .filter(|&c| c < total_cols)
        .min()
        .unwrap_or(0);
    tab.table_state.selected_cell = Some((0, first));
    let col_left: f32 = tab
        .table_state
        .col_widths
        .iter()
        .take(first)
        .sum();
    tab.table_state.set_scroll_x(col_left);
    tab.table_state.set_scroll_y(0.0);
}

#[derive(Debug, Clone, Copy)]
enum ContextAction {
    CopySelection,
    CopyAll,
    SelectAll,
    ClearSelection,
    /// Copy a single inspector column (by display index 0..7) for every
    /// selected row, or every row when the selection is empty.
    CopyInspectorColumn(usize),
}

/// Update `selected` / `anchor` for a click at `row`, applying the standard
/// plain / Ctrl / Shift modifiers.
fn apply_click(
    selected: &mut HashSet<usize>,
    anchor: &mut Option<usize>,
    row: usize,
    mods: egui::Modifiers,
    total: usize,
) {
    if mods.shift && anchor.is_some() {
        let a = anchor.unwrap().min(total.saturating_sub(1));
        let lo = a.min(row);
        let hi = a.max(row);
        if !mods.command {
            selected.clear();
        }
        for r in lo..=hi {
            selected.insert(r);
        }
    } else if mods.command {
        if !selected.insert(row) {
            selected.remove(&row);
        }
        *anchor = Some(row);
    } else {
        selected.clear();
        selected.insert(row);
        *anchor = Some(row);
    }
}
