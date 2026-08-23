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

impl OctaApp {
    /// Entry from Analyse -> Relationship map...
    pub(crate) fn open_rel_map_dialog(&mut self) {
        // Pre-tick every tab with columns: the map is about all of them, not a
        // chosen pair, so an empty list would just mean more clicking.
        let ticked: Vec<usize> = (0..self.tabs.len())
            .filter(|&i| self.tabs[i].table.col_count() > 0)
            .collect();
        let source = if ticked.len() >= 2 {
            RelMapSource::Tabs
        } else {
            RelMapSource::Folder
        };
        self.rel_map_dialog = Some(RelMapState {
            size: DialogSize::Normal,
            source,
            tabs: ticked,
            folder: None,
            recursive: false,
            positions: Vec::new(),
            chip_offsets: Vec::new(),
            min_score_buf: format!("{:.2}", RelMapOptions::default().min_score),
            min_score: RelMapOptions::default().min_score,
            map: None,
            truncated: false,
            error: None,
            conn_id: None,
            catalog: None,
            catalogs: Vec::new(),
            schemas: Vec::new(),
            db_tables: Vec::new(),
            db_columns: Vec::new(),
            db_fks: Vec::new(),
            skipped_edges: 0,
            declared: false,
            expanded: HashSet::new(),
            export_result: None,
            job: None,
        });
    }

    /// The connection picked for the database source, if it still exists.
    fn rel_map_conn(&self, st: &RelMapState) -> Option<octa::db::DbConnection> {
        st.conn_id
            .as_ref()
            .and_then(|id| self.settings.db_connections.iter().find(|c| &c.id == id))
            .cloned()
    }

    /// List the catalogs, or the schemas of the chosen catalog. Two steps for
    /// the three-level engines because a schema list needs a catalog first;
    /// one step for everything else.
    fn spawn_rel_map_meta(&self, st: &mut RelMapState, ctx: &egui::Context) {
        let Some(conn) = self.rel_map_conn(st) else {
            return;
        };
        let slot: ScanSlot = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        st.job = Some((slot.clone(), cancel));
        st.error = None;
        let catalog = st.catalog.clone();
        let cache = self.db_conn_cache.clone();
        let settings = self.settings.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
            let outcome = cache
                .with_conn(&conn, secret.as_deref(), |c| {
                    if conn.engine.has_catalogs() && catalog.is_none() {
                        return Ok(RelMapJob::Meta {
                            catalogs: c.list_catalogs()?,
                            schemas: Vec::new(),
                        });
                    }
                    Ok(RelMapJob::Meta {
                        catalogs: Vec::new(),
                        schemas: c.list_schemas(catalog.as_deref())?,
                    })
                })
                .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Read the rows of every drawn table and put real numbers on the lines.
    ///
    /// Separate from the scan and never automatic: the scan reads catalog
    /// metadata only, and this reads table data over the wire. On a server
    /// that enforces its foreign keys the answer is always "no orphans"; on
    /// Redshift, Snowflake, Databricks and BigQuery, which do not, it is the
    /// only way to find out.
    fn spawn_rel_map_score(&self, st: &mut RelMapState, ctx: &egui::Context) {
        let (Some(conn), Some(map)) = (self.rel_map_conn(st), st.map.clone()) else {
            return;
        };
        let slot: ScanSlot = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        st.job = Some((slot.clone(), cancel.clone()));
        st.error = None;
        let catalog = st.catalog.clone();
        let cache = self.db_conn_cache.clone();
        let settings = self.settings.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<RelMapJob> {
                let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                // One connection for every table, not one per table: a map of
                // twenty boxes would otherwise open twenty connections.
                let tables = cache.with_conn(&conn, secret.as_deref(), |c| {
                    let mut out: Vec<(String, octa::data::DataTable)> = Vec::new();
                    for node in &map.nodes {
                        if cancel.load(Ordering::Relaxed) {
                            anyhow::bail!("cancelled");
                        }
                        let (schema, table) = node
                            .name
                            .split_once('.')
                            .unwrap_or(("", node.name.as_str()));
                        let sql = octa::db::select_sample_sql(
                            conn.engine,
                            catalog.as_deref(),
                            schema,
                            table,
                            DEFAULT_SAMPLE_ROWS,
                        );
                        out.push((node.name.clone(), c.query(&sql)?));
                    }
                    Ok(out)
                })?;
                let mut scored = map;
                score_edges(&tables, &mut scored, DEFAULT_SAMPLE_ROWS);
                Ok(RelMapJob::Scored(scored))
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    fn spawn_rel_map_scan(&self, st: &mut RelMapState, ctx: &egui::Context) {
        // Resolve the source on the UI thread: nothing is spawned for a
        // request that cannot run.
        enum Work {
            Tables(Vec<(String, octa::data::DataTable)>),
            Folder(PathBuf, bool),
            Database(Box<octa::db::DbConnection>, Option<String>, Vec<String>),
        }
        let work = match st.source {
            RelMapSource::Tabs => {
                let mut named = Vec::new();
                for &i in &st.tabs {
                    let Some(tab) = self.tabs.get(i) else {
                        continue;
                    };
                    // Snapshot with edits applied: the map must describe what
                    // is on screen, not what was last saved.
                    let mut snap = tab.table.clone();
                    snap.apply_edits();
                    snap.rows.truncate(DEFAULT_SAMPLE_ROWS);
                    named.push((tab.title_display(), snap));
                }
                if named.len() < 2 {
                    st.error = Some(t("joinkeys.need_two"));
                    return;
                }
                Work::Tables(named)
            }
            RelMapSource::Folder => {
                let Some(dir) = st.folder.clone() else {
                    st.error = Some(t("relmap.source_folder_hint"));
                    return;
                };
                Work::Folder(dir, st.recursive)
            }
            RelMapSource::Database => {
                let Some(conn) = self.rel_map_conn(st) else {
                    st.error = Some(t("relmap.db_pick_conn"));
                    return;
                };
                let picked: Vec<String> = st
                    .schemas
                    .iter()
                    .filter(|(_, on)| *on)
                    .map(|(n, _)| n.clone())
                    .collect();
                if picked.is_empty() {
                    st.error = Some(t("relmap.db_pick_schema"));
                    return;
                }
                Work::Database(Box::new(conn), st.catalog.clone(), picked)
            }
        };

        let slot: ScanSlot = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        st.job = Some((slot.clone(), cancel.clone()));
        st.error = None;
        st.map = None;
        st.positions.clear();
        st.chip_offsets.clear();
        st.db_tables.clear();
        st.db_columns.clear();
        st.db_fks.clear();
        st.skipped_edges = 0;
        let min_score = st.min_score;
        let cache = self.db_conn_cache.clone();
        let settings = self.settings.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let outcome = (|| -> anyhow::Result<RelMapJob> {
                // The database source builds its map from declared keys, so it
                // never reaches the value scorer below.
                if let Work::Database(conn, catalog, schemas) = work {
                    let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                    let (columns, fks) = cache.with_conn(&conn, secret.as_deref(), |c| {
                        scan_db(c, catalog.as_deref(), &schemas)
                    })?;
                    let built = build_db_map(&columns, &fks, None, DEFAULT_MAX_FILES);
                    return Ok(RelMapJob::Map(Box::new(ScanResult {
                        map: built.map,
                        truncated: built.truncated,
                        skipped_edges: built.skipped_edges,
                        db_data: Some((columns, fks)),
                    })));
                }

                let (named, truncated) = match work {
                    Work::Tables(t) => (t, false),
                    Work::Folder(dir, recursive) => {
                        let stop = || cancel.load(Ordering::Relaxed);
                        let found = collect_tables(
                            &dir,
                            recursive,
                            DEFAULT_MAX_FILES,
                            DEFAULT_SAMPLE_ROWS,
                            &stop,
                        );
                        if found.tables.len() < 2 {
                            anyhow::bail!("{} has fewer than two readable tables", dir.display());
                        }
                        (found.tables, found.truncated)
                    }
                    Work::Database(..) => unreachable!("handled above"),
                };
                let refs: Vec<(String, &octa::data::DataTable)> =
                    named.iter().map(|(n, t)| (n.clone(), t)).collect();
                Ok(RelMapJob::Map(Box::new(ScanResult {
                    map: build_map(
                        &refs,
                        &RelMapOptions {
                            min_score,
                            ..RelMapOptions::default()
                        },
                    ),
                    truncated,
                    skipped_edges: 0,
                    db_data: None,
                })))
            })()
            .map_err(|e| format!("{e:#}"));
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }
}

/// How many columns a box lists: all of them once the user has clicked its
/// `+N` row open, otherwise the first `MAX_LISTED_COLS`.
fn listed_cols(cols: usize, expanded: bool) -> usize {
    if expanded {
        cols
    } else {
        cols.min(MAX_LISTED_COLS)
    }
}

/// Height of a node box, given how many columns it lists.
fn node_height(cols: usize, listed: usize) -> f32 {
    // One extra row for the "+N more" line when the list was cut short.
    let extra = if listed < cols { 1.0 } else { 0.0 };
    HEADER_H + (listed as f32 + extra) * ROW_H + 6.0
}

/// Where a column's line should attach, on the given side of a node.
///
/// A column past the end of the list attaches to the last visible row, which
/// is why `listed` has to be passed rather than assumed: expanding a box
/// moves the line to the column it actually names.
fn anchor(top_left: Pos2, col: usize, right_side: bool, listed: usize) -> Pos2 {
    let row = col.min(listed.saturating_sub(1));
    let y = top_left.y + HEADER_H + row as f32 * ROW_H + ROW_H / 2.0;
    let x = if right_side {
        top_left.x + NODE_W
    } else {
        top_left.x
    };
    Pos2::new(x, y)
}

/// Both ends of one edge and its chip, in map space (no scroll offset).
///
/// The drawing adds the painter's origin to these and the export does not,
/// which is the only difference between what is on screen and what is
/// written to a file. Sharing the arithmetic is what stops the two drifting.
fn edge_geometry(st: &RelMapState, map: &RelMap, ei: usize) -> Option<(Pos2, Pos2, Pos2, bool)> {
    let e = map.edges.get(ei)?;
    let lp = st.positions.get(e.left_table).copied()?;
    let rp = st.positions.get(e.right_table).copied()?;
    let ln = map.nodes.get(e.left_table)?;
    let rn = map.nodes.get(e.right_table)?;
    // Attach to whichever sides face each other, so the line does not cross
    // back over its own box.
    let left_first = lp.x <= rp.x;
    let a = anchor(
        lp,
        e.left_col,
        left_first,
        listed_cols(ln.columns.len(), st.expanded.contains(&e.left_table)),
    );
    let b = anchor(
        rp,
        e.right_col,
        !left_first,
        listed_cols(rn.columns.len(), st.expanded.contains(&e.right_table)),
    );
    let straight_mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let bend = st.chip_offsets.get(ei).copied().unwrap_or_default();
    Some((a, b, straight_mid + bend, bend != Vec2::ZERO))
}

/// Parse a typed threshold. Accepts a comma decimal mark, because half of
/// the locales Octa ships write `0,35`, and clamps rather than rejecting a
/// value outside 0..1 - a threshold above 1 is reachable by holding a key
/// down and means "show nothing", which is not worth an error message.
fn parse_score(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().map(|v| v.clamp(0.0, 1.0))
}

/// Seed a grid so the first render is readable before anyone drags anything.
fn seed_positions(count: usize) -> Vec<Pos2> {
    let per_row = (count as f32).sqrt().ceil().max(1.0) as usize;
    (0..count)
        .map(|i| {
            Pos2::new(
                20.0 + (i % per_row) as f32 * GRID_X,
                20.0 + (i / per_row) as f32 * GRID_Y,
            )
        })
        .collect()
}

/// Draw the diagram. Returns the edge index the user clicked, if any.
fn draw_map(ui: &mut egui::Ui, st: &mut RelMapState, map: &RelMap) -> Option<usize> {
    let mut clicked_edge = None;

    let extent = st
        .positions
        .iter()
        .enumerate()
        .fold(Vec2::new(600.0, 400.0), |acc, (i, p)| {
            let cols = map.nodes[i].columns.len();
            let h = node_height(cols, listed_cols(cols, st.expanded.contains(&i)));
            Vec2::new(acc.x.max(p.x + NODE_W + 40.0), acc.y.max(p.y + h + 40.0))
        });

    let (resp, painter) = ui.allocate_painter(extent, egui::Sense::hover());
    let origin = resp.rect.min.to_vec2();
    let visuals = ui.visuals().clone();
    let text_color = visuals.text_color();
    let weak = visuals.weak_text_color();
    let line_color = visuals.hyperlink_color;

    // Edges first, so the boxes sit on top of the lines rather than under.
    for (ei, e) in map.edges.iter().enumerate() {
        let Some((a, b, mid, bent)) = edge_geometry(st, map, ei) else {
            continue;
        };
        // Map space to screen space; the export skips exactly this step.
        let (a, b, mid) = (a + origin, b + origin, mid + origin);

        // The chip is the curve's handle: it sits wherever it was dragged and
        // the connection bends to pass through it. A quadratic Bezier's
        // midpoint is (a + 2c + b) / 4, so to make the curve pass through the
        // chip at t=0.5 the control point is 2*chip - (a + b) / 2. With no
        // offset that lands back on the midpoint and the curve is a straight
        // line, so an untouched map looks exactly as it did.
        let straight_mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        if !bent {
            painter.line_segment([a, b], Stroke::new(1.5, line_color));
        } else {
            let control = Pos2::new(2.0 * mid.x - straight_mid.x, 2.0 * mid.y - straight_mid.y);
            painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
                [a, control, b],
                false,
                egui::Color32::TRANSPARENT,
                Stroke::new(1.5, line_color),
            ));
        }

        // The chip is the edge's hit target too: a click on a hairline is not
        // something anyone should have to aim at.
        // A declared foreign key has no score to show until it is measured,
        // and a chip reading 0.00 would say the opposite of what is true.
        let label = if e.scored {
            format!("{:.2}", e.score)
        } else {
            t("relmap.chip_declared")
        };
        let chip = Rect::from_center_size(mid, Vec2::new(46.0, 18.0));
        painter.rect_filled(chip, 4.0, visuals.extreme_bg_color);
        painter.rect_stroke(
            chip,
            4.0,
            Stroke::new(1.0, line_color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            mid,
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.0),
            text_color,
        );

        // Click to prefill the Join dialog, drag to move the chip: egui's
        // click/drag split keeps both on one response.
        let hit = ui.interact(
            chip,
            ui.id().with(("relmap_edge", ei)),
            egui::Sense::click_and_drag(),
        );
        if hit.dragged()
            && let Some(o) = st.chip_offsets.get_mut(ei)
        {
            *o += hit.drag_delta();
        }
        let hit = hit.on_hover_text(edge_tooltip(map, e));
        if hit.clicked() {
            clicked_edge = Some(ei);
        }
    }

    // Boxes, and the dragging that moves them.
    for (i, node) in map.nodes.iter().enumerate() {
        let Some(pos) = st.positions.get(i).copied() else {
            continue;
        };
        let top_left = pos + origin;
        let expanded = st.expanded.contains(&i);
        let listed = listed_cols(node.columns.len(), expanded);
        let rect = Rect::from_min_size(
            top_left,
            Vec2::new(NODE_W, node_height(node.columns.len(), listed)),
        );
        painter.rect_filled(rect, 6.0, visuals.faint_bg_color);
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            top_left + Vec2::new(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            &node.name,
            FontId::proportional(13.0),
            text_color,
        );
        for (ci, col) in node.columns.iter().take(listed).enumerate() {
            painter.text(
                top_left + Vec2::new(10.0, HEADER_H + ci as f32 * ROW_H),
                egui::Align2::LEFT_TOP,
                col,
                FontId::proportional(11.0),
                weak,
            );
        }
        // `+N` folded, `-N` expanded: a signed count needs no words, so it
        // reads the same in every language. The tooltip says it is clickable.
        let hidden = node.columns.len() - listed;
        let toggle_row = if expanded {
            (node.columns.len() > MAX_LISTED_COLS).then(|| {
                (
                    format!("-{}", node.columns.len() - MAX_LISTED_COLS),
                    node.columns.len(),
                )
            })
        } else {
            (hidden > 0).then(|| (format!("+{hidden}"), listed))
        };
        if let Some((label, row)) = &toggle_row {
            painter.text(
                top_left + Vec2::new(10.0, HEADER_H + *row as f32 * ROW_H),
                egui::Align2::LEFT_TOP,
                label,
                FontId::proportional(11.0),
                line_color,
            );
        }

        let drag = ui.interact(rect, ui.id().with(("relmap_node", i)), egui::Sense::drag());
        debug_assert_eq!(drag.interact_rect, rect);
        if drag.dragged()
            && let Some(p) = st.positions.get_mut(i)
        {
            *p += drag.drag_delta();
        }

        // Registered AFTER the box, so it wins the click; it senses clicks
        // only and the box senses drags only, so dragging the box by this
        // row still works. Same split the custom title bar relies on.
        if let Some((_, row)) = &toggle_row {
            let strip = Rect::from_min_size(
                top_left + Vec2::new(0.0, HEADER_H + *row as f32 * ROW_H),
                Vec2::new(NODE_W, ROW_H),
            );
            let hit = ui.interact(
                strip,
                ui.id().with(("relmap_cols", i)),
                egui::Sense::click(),
            );
            if hit.clicked() {
                if expanded {
                    st.expanded.remove(&i);
                } else {
                    st.expanded.insert(i);
                }
            }
            hit.on_hover_text(t("relmap.cols_toggle_hint"));
        }
        drag.on_hover_text(format!(
            "{}\n{}",
            t("relmap.node_tooltip")
                .replace("{name}", &node.name)
                .replace("{cols}", &node.columns.len().to_string())
                .replace("{rows}", &node.rows.to_string()),
            t("relmap.drag_hint")
        ));
    }

    clicked_edge
}

/// The sentence a line explains itself with, on hover and in the export's
/// SVG `<title>`. One wording, so a map read in a browser says what the map
/// in Octa said.
fn edge_tooltip(map: &RelMap, e: &octa::data::rel_map::Relationship) -> String {
    let left = format!(
        "{}.{}",
        map.nodes[e.left_table].name, map.nodes[e.left_table].columns[e.left_col]
    );
    let right = format!(
        "{}.{}",
        map.nodes[e.right_table].name, map.nodes[e.right_table].columns[e.right_col]
    );
    if e.scored {
        // Both directions, one line each. Only one of them can break a tie
        // between two candidates that score the same, and which one depends on
        // which side is the child - something the map cannot know for a scan of
        // tabs or files. Showing one direction alone left `orders.id` and
        // `orders.customer_id` looking identical.
        let direction = |matched: usize, total: usize, orphans: usize, from: &str, to: &str| {
            t("relmap.edge_direction")
                .replace("{matched}", &matched.to_string())
                .replace("{total}", &total.to_string())
                .replace("{from}", from)
                .replace("{to}", to)
                .replace("{orphans}", &orphans.to_string())
        };
        format!(
            "{}\n{}\n\n{}",
            direction(
                e.matched(),
                e.left_distinct_values,
                e.left_orphans,
                &left,
                &right
            ),
            direction(
                e.right_matched(),
                e.right_distinct_values,
                e.right_orphans,
                &right,
                &left
            ),
            t("relmap.score_tooltip")
        )
    } else {
        t("relmap.edge_declared_tooltip")
            .replace("{name}", e.constraint.as_deref().unwrap_or("-"))
            .replace("{left}", &left)
            .replace("{right}", &right)
    }
}

/// Snapshot the map exactly as it stands, ready to be written to a file.
///
/// Uses the same `edge_geometry`, `node_height` and `listed_cols` the drawing
/// does, minus the painter's origin, so what lands in the PDF is what was on
/// screen: the boxes where they were dragged, the lines where they were bent,
/// the column lists as far as they were opened.
fn build_layout(st: &RelMapState, map: &RelMap, visuals: &egui::Visuals) -> MapLayout {
    let nodes = map
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let pos = st.positions.get(i).copied().unwrap_or_default();
            NodeBox {
                name: n.name.clone(),
                columns: n.columns.clone(),
                listed: listed_cols(n.columns.len(), st.expanded.contains(&i)),
                x: pos.x,
                y: pos.y,
                rows: n.rows,
            }
        })
        .collect();

    let edges = map
        .edges
        .iter()
        .enumerate()
        .filter_map(|(ei, e)| {
            let (a, b, mid, bent) = edge_geometry(st, map, ei)?;
            Some(EdgeLine {
                left_table: e.left_table,
                left_col: e.left_col,
                right_table: e.right_table,
                right_col: e.right_col,
                ax: a.x,
                ay: a.y,
                bx: b.x,
                by: b.y,
                mx: mid.x,
                my: mid.y,
                bent,
                label: if e.scored {
                    format!("{:.2}", e.score)
                } else {
                    t("relmap.chip_declared")
                },
                tooltip: edge_tooltip(map, e),
            })
        })
        .collect();

    MapLayout {
        title: t("relmap.title"),
        nodes,
        edges,
        colors: MapColors {
            background: visuals.panel_fill.to_array(),
            node_fill: visuals.faint_bg_color.to_array(),
            node_stroke: visuals.widgets.noninteractive.fg_stroke.color.to_array(),
            text: visuals.text_color().to_array(),
            weak_text: visuals.weak_text_color().to_array(),
            line: visuals.hyperlink_color.to_array(),
            chip_fill: visuals.extreme_bg_color.to_array(),
        },
        node_w: NODE_W,
        header_h: HEADER_H,
        row_h: ROW_H,
    }
}

/// Ask for a path and write the map to it. Returns what to tell the user.
fn export_map(layout: &MapLayout, format: RelMapExportFormat) -> Option<(bool, String)> {
    let ext = format.extension();
    let path = rfd::FileDialog::new()
        .set_title(t("relmap.export"))
        .add_filter(format.label(), &[ext])
        .set_file_name(format!("relationship-map.{ext}"))
        .save_file()?;
    match octa::data::rel_map_export::render(layout, format)
        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
    {
        Ok(()) => Some((
            true,
            t("relmap.export_done").replace("{path}", &path.display().to_string()),
        )),
        Err(e) => Some((false, e)),
    }
}

/// Put a freshly built map on screen: reset the layout, straighten every line.
///
/// Shared by the scan and by the redraw that follows a table being ticked,
/// because both change how many boxes and lines there are, and both index
/// vectors have to stay parallel to them.
fn apply_map(st: &mut RelMapState, map: RelMap) {
    st.positions = seed_positions(map.nodes.len());
    // Node indices belong to the map that produced them, so a new map cannot
    // inherit which boxes were open.
    st.expanded.clear();
    st.export_result = None;
    // Index-parallel to the edges, so `get(ei)` is always in range. ZERO
    // means every edge starts out straight.
    st.chip_offsets = vec![Vec2::ZERO; map.edges.len()];
    st.map = Some(map);
}

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
