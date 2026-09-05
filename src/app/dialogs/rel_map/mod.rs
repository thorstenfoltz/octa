//! "Relationship map" dialog: which of these tables are linked, and by what?
//!
//! The ranking, the orphan counts and the folder walk are all
//! `octa::data::rel_map`, the same engine behind `--relationships` and the
//! `suggest_join_keys` tool, so a line drawn here and a row printed there carry
//! the same numbers. This module is the picker, the worker and the drawing.
//!
//! The diagram is hand-drawn into one allocated painter, and every box is
//! interacted with through `ui.interact(rect, ..)`. Deliberately **not**
//! `egui::Area`: an Area derives its own rect from its contents and can settle
//! somewhere else entirely, which is what made buttons unclickable on Linux
//! Mint (see the custom title bar note in CLAUDE.md).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use egui::{FontId, Pos2, Rect, RichText, Stroke, Vec2};

use octa::data::join::{JoinOp, JoinType};
use octa::data::join_keys::DEFAULT_SAMPLE_ROWS;
use octa::data::rel_map::{
    DEFAULT_MAX_FILES, RelMap, RelMapOptions, build_map, collect_tables, score_edges,
};
use octa::data::rel_map_export::{EdgeLine, MapColors, MapLayout, NodeBox, RelMapExportFormat};
use octa::db::relationships::{ColumnRow, ForeignKey, build_db_map, scan as scan_db};
use octa::i18n::t;
use octa::ui::settings::{
    DialogSize, draw_result_message, draw_window_controls, remember_dialog_rect, size_dialog_window,
};

use super::super::state::{JoinCondDraft, JoinState, OctaApp};

/// Box geometry. Fixed rather than measured: the boxes are draggable, so an
/// exact fit buys nothing and a stable size keeps the starting grid tidy.
const NODE_W: f32 = 190.0;
const HEADER_H: f32 = 24.0;
const ROW_H: f32 = 16.0;
/// Columns listed inside a box before it says how many more there are.
const MAX_LISTED_COLS: usize = 12;
const GRID_X: f32 = 280.0;
const GRID_Y: f32 = 240.0;

/// What the map-building worker hands back.
struct ScanResult {
    map: RelMap,
    /// The folder scan stopped at the file cap, or the database scan at the
    /// table cap.
    truncated: bool,
    /// Declared keys whose other side is not drawn (database source only).
    skipped_edges: usize,
    /// Everything the database scan read, kept so re-ticking a table redraws
    /// without another round trip.
    db_data: Option<(Vec<ColumnRow>, Vec<ForeignKey>)>,
}

/// One worker, three answers: the catalog/schema listing that fills the
/// pickers, the map itself, and the optional pass that measures a declared
/// map against the rows.
enum RelMapJob {
    Meta {
        catalogs: Vec<String>,
        schemas: Vec<String>,
    },
    Map(Box<ScanResult>),
    Scored(RelMap),
}

type ScanSlot = Arc<Mutex<Option<Result<RelMapJob, String>>>>;

/// Where the tables come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelMapSource {
    Tabs,
    Folder,
    Database,
}

pub(crate) struct RelMapState {
    pub(crate) size: DialogSize,
    pub(crate) source: RelMapSource,
    /// Ticked tab indices, in map-node order when `source` is `Tabs`.
    pub(crate) tabs: Vec<usize>,
    pub(crate) folder: Option<PathBuf>,
    pub(crate) recursive: bool,
    /// Top-left of each node, in canvas coordinates. Seeded as a grid on the
    /// first result, then owned by the user's dragging.
    pub(crate) positions: Vec<Pos2>,
    /// How far each edge is bent away from straight, as the offset of its
    /// score chip from the line's midpoint. Index-parallel to `map.edges`;
    /// `ZERO` is a straight line. The chip is the curve's handle: dragging it
    /// bends the whole connection to follow, rather than sliding a label
    /// along a line that stays put.
    pub(crate) chip_offsets: Vec<Vec2>,
    /// Text buffer for the threshold, so any value in 0..1 can be typed
    /// exactly rather than picked off a stepped slider.
    pub(crate) min_score_buf: String,
    /// Lowest score drawn. Exposed because "is this a relationship?" is a
    /// judgement about the data, not a constant: 0.50 hides real but partial
    /// links, and lowering it is how you find them.
    pub(crate) min_score: f64,
    pub(crate) map: Option<RelMap>,
    pub(crate) truncated: bool,
    pub(crate) error: Option<String>,
    /// Saved connection id for `RelMapSource::Database`.
    pub(crate) conn_id: Option<String>,
    /// Catalog for the three-level engines; `None` until one is picked.
    pub(crate) catalog: Option<String>,
    /// Catalogs offered by the connection, empty for a two-level engine.
    pub(crate) catalogs: Vec<String>,
    /// Schemas offered by the connection, with the ticked ones marked.
    pub(crate) schemas: Vec<(String, bool)>,
    /// `schema.table` in the scanned schemas, with the drawn ones ticked.
    /// Filled by the scan, so adding an unlinked table costs no query.
    pub(crate) db_tables: Vec<(String, bool)>,
    /// Columns and declared keys the scan read, kept for the client-side
    /// redraw when the table ticks change.
    db_columns: Vec<ColumnRow>,
    db_fks: Vec<ForeignKey>,
    /// Declared keys with no box on one end.
    pub(crate) skipped_edges: usize,
    /// The map on screen came from declared keys rather than from values.
    pub(crate) declared: bool,
    /// Node indices whose box lists every column instead of the first
    /// `MAX_LISTED_COLS`. Session-only, and cleared whenever the map is
    /// rebuilt, since the indices belong to that map.
    pub(crate) expanded: HashSet<usize>,
    /// Outcome of the last export, `(ok, message)`.
    pub(crate) export_result: Option<(bool, String)>,
    job: Option<(ScanSlot, Arc<AtomicBool>)>,
}

mod draw;
mod jobs;
mod layout;

use draw::draw_map;
use layout::{apply_map, build_layout, export_map, parse_score};

pub(crate) fn render_rel_map_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    if app.rel_map_dialog.is_none() {
        return;
    }
    let mut close = false;
    let mut scan = false;
    let mut cancel = false;
    let mut pick_folder = false;
    // Ask the server for its catalogs or schemas, measure a declared map
    // against the rows, redraw after a table was ticked.
    let mut meta = false;
    let mut score = false;
    let mut redraw = false;
    let mut export = false;
    let mut format = app.settings.rel_map_export_format;
    let mut use_edge: Option<usize> = None;
    let mut st = app.rel_map_dialog.take().unwrap();
    let connections: Vec<(String, String, octa::db::DbEngine)> = app
        .settings
        .db_connections
        .iter()
        .map(|c| (c.id.clone(), c.name.clone(), c.engine))
        .collect();
    let engine = st
        .conn_id
        .as_ref()
        .and_then(|id| connections.iter().find(|(cid, _, _)| cid == id))
        .map(|(_, _, e)| *e);
    let mut size = st.size;
    let minimized = size == DialogSize::Minimized;

    // Drain a finished worker.
    if let Some((slot, _)) = &st.job
        && let Some(res) = slot.lock().ok().and_then(|mut g| g.take())
    {
        st.job = None;
        match res {
            // A catalog list arrives first on the three-level engines, so
            // picking one can then ask for its schemas.
            Ok(RelMapJob::Meta { catalogs, schemas }) => {
                // The second reply carries schemas and an empty catalog list.
                // Assigning it blindly would empty the combo the user just
                // picked from; the list is cleared explicitly when the
                // connection changes, which is the only time it goes stale.
                if !catalogs.is_empty() {
                    st.catalogs = catalogs;
                }
                st.schemas = schemas.into_iter().map(|s| (s, false)).collect();
            }
            Ok(RelMapJob::Map(out)) => {
                st.truncated = out.truncated;
                st.skipped_edges = out.skipped_edges;
                st.declared = out.db_data.is_some();
                if let Some((columns, fks)) = out.db_data {
                    // Every table in the scanned schemas, ticked when the map
                    // drew it. Adding an unlinked one is then a redraw, not a
                    // second round trip.
                    let drawn: HashSet<&str> =
                        out.map.nodes.iter().map(|n| n.name.as_str()).collect();
                    let mut seen = HashSet::new();
                    st.db_tables = columns
                        .iter()
                        .map(|(s, t, _)| format!("{s}.{t}"))
                        .filter(|l| seen.insert(l.clone()))
                        .map(|l| {
                            let on = drawn.contains(l.as_str());
                            (l, on)
                        })
                        .collect();
                    st.db_columns = columns;
                    st.db_fks = fks;
                }
                apply_map(&mut st, out.map);
            }
            // Same nodes and edges, only the numbers changed, so the boxes
            // the user dragged stay where they were put.
            Ok(RelMapJob::Scored(map)) => st.map = Some(map),
            Err(e) => st.error = Some(e),
        }
    }
    let running = st.job.is_some();

    let tab_labels: Vec<(usize, String)> = (0..app.tabs.len())
        .filter(|&i| app.tabs[i].table.col_count() > 0)
        .map(|i| (i, app.tabs[i].title_display()))
        .collect();

    let dialog_id = egui::Id::new("octa_rel_map_dialog");
    let window = egui::Window::new("octa_rel_map")
        .title_bar(false)
        .collapsible(false);
    let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
        w.resizable(true)
            .default_width(760.0)
            .default_height(560.0)
            .min_width(460.0)
            .min_height(300.0)
    });

    let inner = window.show(ctx, |ui| {
        egui::Panel::top("rel_map_header")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("relmap.title")).strong().size(16.0));
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

        egui::Panel::bottom("rel_map_footer")
            .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // A ComboBox sizes itself against `interact_size`, a Button
                    // against its own text, so left alone the format picker
                    // comes out a different height from the buttons beside it
                    // and the row visibly steps. Same fix the chart control bar
                    // uses.
                    let row_h = ui.text_style_height(&egui::TextStyle::Button)
                        + 2.0 * ui.spacing().button_padding.y;
                    ui.spacing_mut().interact_size.y = row_h;
                    ui.set_min_height(row_h);
                    if running {
                        ui.spinner();
                        ui.label(t("relmap.scanning"));
                        if ui
                            .button(t("relmap.cancel"))
                            .on_hover_text(t("relmap.cancel_hint"))
                            .clicked()
                        {
                            cancel = true;
                        }
                    } else {
                        let ready = match st.source {
                            RelMapSource::Tabs => st.tabs.len() >= 2,
                            RelMapSource::Folder => st.folder.is_some(),
                            RelMapSource::Database => {
                                engine.is_some_and(|e| e.has_foreign_keys())
                                    && st.schemas.iter().any(|(_, on)| *on)
                            }
                        };
                        let btn = ui.add_enabled(ready, egui::Button::new(t("relmap.scan")));
                        if btn.clicked() {
                            scan = true;
                        }
                        if ready {
                            btn.on_hover_text(match st.source {
                                RelMapSource::Database => t("relmap.db_scan_hint"),
                                _ => t("relmap.scan_hint"),
                            });
                        } else {
                            btn.on_disabled_hover_text(match st.source {
                                RelMapSource::Tabs => t("joinkeys.need_two"),
                                RelMapSource::Folder => t("relmap.source_folder_hint"),
                                RelMapSource::Database => {
                                    if st.conn_id.is_none() {
                                        t("relmap.db_pick_conn")
                                    } else if engine.is_some_and(|e| !e.has_foreign_keys()) {
                                        t("relmap.db_no_fks")
                                    } else {
                                        t("relmap.db_pick_schema")
                                    }
                                }
                            });
                        }
                        // Only a declared map can be measured: a value-sampled
                        // one already carries the numbers this would compute.
                        if st.declared && st.map.is_some() {
                            let m = ui.button(t("relmap.measure"));
                            if m.clicked() {
                                score = true;
                            }
                            m.on_hover_text(t("relmap.measure_hint"));
                        }

                        // Writes the map as it stands right now, so the file
                        // keeps the arrangement rather than a fresh layout.
                        let has_map = st.map.is_some();
                        egui::ComboBox::from_id_salt("rel_map_export_format")
                            .selected_text(format.label())
                            .width(78.0)
                            .show_ui(ui, |ui| {
                                for f in RelMapExportFormat::ALL {
                                    ui.selectable_value(&mut format, f, f.label())
                                        .on_hover_text(t("relmap.export_format_hint"));
                                }
                            })
                            .response
                            .on_hover_text(t("relmap.export_format_hint"));
                        let ex = ui.add_enabled(has_map, egui::Button::new(t("relmap.export")));
                        if ex.clicked() {
                            export = true;
                        }
                        if has_map {
                            ex.on_hover_text(t("relmap.export_hint"));
                        } else {
                            ex.on_disabled_hover_text(t("relmap.export_needs_map"));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("common.close")).clicked() {
                            close = true;
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(t("relmap.hint"));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.radio_value(&mut st.source, RelMapSource::Tabs, t("relmap.source_tabs"))
                    .on_hover_text(t("relmap.source_tabs_hint"));
                ui.radio_value(
                    &mut st.source,
                    RelMapSource::Folder,
                    t("relmap.source_folder"),
                )
                .on_hover_text(t("relmap.source_folder_hint"));
                ui.radio_value(
                    &mut st.source,
                    RelMapSource::Database,
                    t("relmap.source_db"),
                )
                .on_hover_text(t("relmap.source_db_hint"));
            });

            match st.source {
                RelMapSource::Tabs => {
                    ui.horizontal_wrapped(|ui| {
                        for (i, name) in &tab_labels {
                            let mut on = st.tabs.contains(i);
                            if ui.checkbox(&mut on, name).changed() {
                                if on {
                                    st.tabs.push(*i);
                                    st.tabs.sort_unstable();
                                } else {
                                    st.tabs.retain(|x| x != i);
                                }
                            }
                        }
                    });
                }
                RelMapSource::Folder => {
                    ui.horizontal(|ui| {
                        if ui
                            .button(t("relmap.source_folder"))
                            .on_hover_text(t("relmap.source_folder_hint"))
                            .clicked()
                        {
                            pick_folder = true;
                        }
                        if let Some(d) = &st.folder {
                            ui.label(
                                RichText::new(d.display().to_string())
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                        ui.checkbox(&mut st.recursive, t("relmap.recursive"))
                            .on_hover_text(t("relmap.recursive_hint"));
                    });
                }
                RelMapSource::Database => {
                    // The connection and catalog pickers reuse the File vs
                    // database compare dialog's labels: one wording for the
                    // same two questions.
                    egui::Grid::new("rel_map_db_grid")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(t("dbcmp.connection"))
                                .on_hover_text(t("relmap.db_connection_hint"));
                            let mut picked = st.conn_id.clone();
                            egui::ComboBox::from_id_salt("rel_map_conn")
                                .selected_text(
                                    connections
                                        .iter()
                                        .find(|(id, _, _)| Some(id) == st.conn_id.as_ref())
                                        .map(|(_, name, _)| name.clone())
                                        .unwrap_or_default(),
                                )
                                .show_ui(ui, |ui| {
                                    for (id, name, _) in &connections {
                                        ui.selectable_value(&mut picked, Some(id.clone()), name);
                                    }
                                })
                                .response
                                .on_hover_text(t("relmap.db_connection_hint"));
                            if picked != st.conn_id {
                                st.conn_id = picked;
                                st.catalog = None;
                                st.catalogs.clear();
                                st.schemas.clear();
                                meta = true;
                            }
                            ui.end_row();

                            if engine.is_some_and(|e| e.has_catalogs()) {
                                ui.label(t("dbcmp.catalog"))
                                    .on_hover_text(t("dbcmp.catalog_hint"));
                                let mut picked = st.catalog.clone();
                                egui::ComboBox::from_id_salt("rel_map_catalog")
                                    .selected_text(st.catalog.clone().unwrap_or_default())
                                    .show_ui(ui, |ui| {
                                        for c in &st.catalogs {
                                            ui.selectable_value(&mut picked, Some(c.clone()), c);
                                        }
                                    })
                                    .response
                                    .on_hover_text(t("dbcmp.catalog_hint"));
                                if picked != st.catalog {
                                    st.catalog = picked;
                                    st.schemas.clear();
                                    meta = true;
                                }
                                ui.end_row();
                            }
                        });

                    ui.label(t("relmap.db_schemas"))
                        .on_hover_text(t("relmap.db_schemas_hint"));
                    ui.horizontal_wrapped(|ui| {
                        for (name, on) in &mut st.schemas {
                            ui.checkbox(on, name.as_str())
                                .on_hover_text(t("relmap.db_schemas_hint"));
                        }
                    });

                    // Everything the scan saw, ticked when the map drew it.
                    // Untangling a crowded map, or adding a table nobody
                    // declared a key for, is then a redraw and no round trip.
                    if !st.db_tables.is_empty() {
                        ui.add_space(4.0);
                        ui.label(t("relmap.db_tables"))
                            .on_hover_text(t("relmap.db_tables_hint"));
                        egui::ScrollArea::vertical()
                            .id_salt("rel_map_db_tables")
                            .max_height(88.0)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    for (name, on) in &mut st.db_tables {
                                        if ui
                                            .checkbox(on, name.as_str())
                                            .on_hover_text(t("relmap.db_tables_hint"))
                                            .changed()
                                        {
                                            redraw = true;
                                        }
                                    }
                                });
                            });
                    }
                }
            }

            // The threshold applies to both sources, so it sits outside the
            // per-source block. Takes effect on the next Scan, which the hint
            // says, because a slider that silently did nothing would be worse
            // than no slider.
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("relmap.min_score"))
                    .on_hover_text(t("relmap.min_score_hint"));
                // No value box on the slider: that is a `DragValue`, whose
                // horizontal-resize cursor reads as a window edge. Same
                // reason `find_fuzzy_duplicates` renders its threshold as
                // slider + label. Unstepped, so the slider can reach any
                // value, and an editable field beside it for typing an exact
                // one - which the slider alone cannot do at pixel precision.
                if ui
                    .add(egui::Slider::new(&mut st.min_score, 0.0..=1.0).show_value(false))
                    .on_hover_text(t("relmap.min_score_hint"))
                    .changed()
                {
                    st.min_score_buf = format!("{:.2}", st.min_score);
                }
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut st.min_score_buf)
                            .desired_width(48.0)
                            .hint_text("0.50"),
                    )
                    .on_hover_text(t("relmap.min_score_hint"));
                if edit.changed()
                    && let Some(v) = parse_score(&st.min_score_buf)
                {
                    st.min_score = v;
                }
                // Snap the text back to the live value once the user leaves
                // the field, so a half-typed or nonsense entry does not sit
                // there disagreeing with the slider.
                if edit.lost_focus() {
                    st.min_score_buf = format!("{:.2}", st.min_score);
                }
                if ui
                    .small_button(t("relmap.min_score_reset"))
                    .on_hover_text(t("relmap.min_score_reset_hint"))
                    .clicked()
                {
                    st.min_score = RelMapOptions::default().min_score;
                    st.min_score_buf = format!("{:.2}", st.min_score);
                }
            });

            if let Some(e) = &st.error {
                ui.add_space(6.0);
                draw_result_message(ui, false, e);
            }
            if let Some((ok, msg)) = &st.export_result {
                ui.add_space(6.0);
                draw_result_message(ui, *ok, msg);
            }
            if st.truncated {
                let key = if st.declared {
                    "relmap.db_capped"
                } else {
                    "relmap.capped_files"
                };
                ui.label(
                    RichText::new(t(key).replace("{n}", &DEFAULT_MAX_FILES.to_string()))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
            // Tied to the edges rather than to the source: Measure fills the
            // numbers in, and a redraw brings back unmeasured ones.
            if st
                .map
                .as_ref()
                .is_some_and(|m| m.edges.iter().any(|e| !e.scored))
            {
                ui.label(
                    RichText::new(t("relmap.declared_note"))
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
            if st.skipped_edges > 0 {
                ui.label(
                    RichText::new(
                        t("relmap.db_skipped").replace("{n}", &st.skipped_edges.to_string()),
                    )
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
                );
            }

            ui.add_space(6.0);
            ui.separator();

            if let Some(map) = st.map.take() {
                if map.edges.is_empty() {
                    ui.add_space(8.0);
                    ui.label(t("relmap.no_links"));
                }
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        use_edge = draw_map(ui, &mut st, &map);
                    });
                st.map = Some(map);
            }
        });
    });

    if let Some(inner) = inner.as_ref() {
        remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
    }
    st.size = size;

    if cancel && let Some((_, flag)) = &st.job {
        flag.store(true, Ordering::Relaxed);
    }
    if pick_folder && let Some(dir) = rfd::FileDialog::new().pick_folder() {
        st.folder = Some(dir);
    }
    // Remembering the pick is the whole "default PDF, set it to something
    // else" behaviour: chosen once, kept for next time.
    if format != app.settings.rel_map_export_format {
        app.settings.rel_map_export_format = format;
        app.settings.save();
    }
    if export && let Some(map) = &st.map {
        // Built before the file dialog opens, so what is written is the map
        // as it was when the button was clicked.
        let visuals = ctx.style_of(ctx.theme()).visuals.clone();
        let layout = build_layout(&st, map, &visuals);
        if let Some(result) = export_map(&layout, format) {
            st.export_result = Some(result);
        }
    }
    if scan {
        app.spawn_rel_map_scan(&mut st, ctx);
    }
    if meta {
        app.spawn_rel_map_meta(&mut st, ctx);
    }
    if score {
        app.spawn_rel_map_score(&mut st, ctx);
    }
    // Pure and instant: the scan already read every column and every declared
    // key, so re-picking which tables are drawn touches no server.
    if redraw {
        let wanted: HashSet<String> = st
            .db_tables
            .iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n.clone())
            .collect();
        let built = build_db_map(&st.db_columns, &st.db_fks, Some(&wanted), DEFAULT_MAX_FILES);
        st.truncated = built.truncated;
        st.skipped_edges = built.skipped_edges;
        apply_map(&mut st, built.map);
    }

    // Handing a pair to the Join dialog rather than joining here: one join
    // implementation, one place where its options live. Only open tabs can be
    // joined, so a folder scan draws the map and stops there.
    if let Some(ei) = use_edge
        && st.source == RelMapSource::Tabs
        && let Some(map) = &st.map
        && let Some(e) = map.edges.get(ei)
        && let (Some(&ltab), Some(&rtab)) = (st.tabs.get(e.left_table), st.tabs.get(e.right_table))
    {
        app.join_dialog = Some(JoinState {
            left_tab: ltab,
            right_tab: rtab,
            conds: vec![JoinCondDraft {
                left_col: e.left_col,
                op: JoinOp::Eq,
                right_col: e.right_col,
            }],
            join_type: JoinType::Left,
            error: None,
            size: DialogSize::default(),
        });
        return; // this dialog closes; the Join dialog takes over
    }

    if !close {
        app.rel_map_dialog = Some(st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half the locales Octa ships write a decimal comma, and a threshold is
    /// exactly the kind of field someone types by hand.
    #[test]
    fn parse_score_takes_either_decimal_mark() {
        assert_eq!(parse_score("0.35"), Some(0.35));
        assert_eq!(parse_score("0,35"), Some(0.35));
        assert_eq!(parse_score("  .5 "), Some(0.5));
        assert_eq!(parse_score("1"), Some(1.0));
    }

    /// Out of range clamps rather than erroring: holding a digit key is an
    /// easy way to reach 11, and "show nothing" is a coherent answer.
    #[test]
    fn parse_score_clamps_instead_of_refusing() {
        assert_eq!(parse_score("11"), Some(1.0));
        assert_eq!(parse_score("-2"), Some(0.0));
    }

    /// A half-typed field must not move the threshold: `None` leaves the
    /// slider where it was until the text becomes a number again.
    #[test]
    fn parse_score_rejects_what_is_not_a_number() {
        assert_eq!(parse_score(""), None);
        assert_eq!(parse_score("   "), None);
        assert_eq!(parse_score("-"), None);
        assert_eq!(parse_score("abc"), None);
    }
}
