//! Sidebar live-database browser: list schemas/tables of saved DB connections
//! and open a table into a read-only tab, all on background workers so the
//! egui update thread never blocks on the network. Mirrors
//! [`super::cloud_browser`]: shared `Arc<Mutex<_>>` state the workers write
//! and the panel reads each frame, plus a per-frame drain of finished loads
//! (`drain_db_pending_open`). Secrets resolve *inside* the worker because the
//! IAM/AD auth modes shell out to the aws/az CLIs, which blocks.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::db::{self, DbConnection};
use octa::ui::settings::db_secrets::{get_db_secret, get_ssh_secret};
use octa::ui::tree_filter::{self, SearchSlot, TreeSearch};

use super::state::{DbOrigin, OctaApp};

/// (connection id, path) key into the listings cache. The path is a
/// [`PATH_SEP`]-joined list of the node's parts below the connection root:
/// `""` is the root, `"cat"` a catalog's schemas, `"cat\x1fsch"` a schema's
/// tables. Two-level engines never have a catalog part.
pub(crate) type ConnSchema = (String, String);

/// Separator between path parts in a node key's second element. The unit
/// separator cannot occur in a real identifier.
const PATH_SEP: char = '\u{1f}';

/// Split a node path into its parts (`""` -> empty).
pub(crate) fn split_path(path: &str) -> Vec<&str> {
    if path.is_empty() {
        Vec::new()
    } else {
        path.split(PATH_SEP).collect()
    }
}

/// Join path parts into a node key's second element.
pub(crate) fn join_path(parts: &[&str]) -> String {
    parts.join(&PATH_SEP.to_string())
}

/// Tag every loaded row `0..n` and snapshot it as the write-back baseline:
/// the diff-based write-back pairs `rows[i]` with `row_tags[i]`. A writable
/// tab gets it when it opens, when a sort or filter re-queries it, and after
/// a save.
pub(crate) fn baseline_db_meta(table: &mut octa::data::DataTable, table_name: &str, schema: &str) {
    let original: std::collections::HashMap<i64, Vec<octa::data::CellValue>> = table
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (i as i64, r.clone()))
        .collect();
    table.db_meta = Some(octa::data::DbRowMeta {
        table_name: table_name.to_string(),
        schema: Some(schema.to_string()),
        row_tags: (0..table.rows.len()).map(|i| Some(i as i64)).collect(),
        original,
        original_columns: table.columns.iter().map(|c| c.name.clone()).collect(),
    });
}

/// Cached state of one expanded node's listing.
pub(crate) enum DbListState {
    Loading,
    /// The connection root of a `has_catalogs` engine: its catalogs.
    Catalogs(Vec<String>),
    /// The connection root (two-level engine) or one catalog: its schemas.
    Schemas(Vec<String>),
    /// One schema: its tables (and views).
    Tables(Vec<String>),
    Error(String),
}

/// Turns a listing node that is still `Loading` when its worker ends into an
/// error, so a panicking worker cannot leave the node spinning for the rest of
/// the session. A no-op on the normal path, where the result is already in.
struct ListingGuard {
    listings: Arc<Mutex<HashMap<ConnSchema, DbListState>>>,
    key: ConnSchema,
}

impl Drop for ListingGuard {
    fn drop(&mut self) {
        if let Ok(mut m) = self.listings.lock()
            && matches!(m.get(&self.key), Some(DbListState::Loading))
        {
            m.insert(
                self.key.clone(),
                DbListState::Error("listing did not finish".to_string()),
            );
        }
    }
}

/// A finished table load waiting to be opened on the main thread (workers
/// must not touch tabs/egui), or a load that failed.
pub(crate) enum DbOpenResult {
    Ready {
        /// Boxed: a `DataTable` inline would dwarf the `Failed` variant.
        table: Box<octa::data::DataTable>,
        label: String,
        conn_id: String,
        /// Catalog for three-level engines, else None.
        catalog: Option<String>,
        schema: String,
        table_name: String,
        /// Primary-key column names (ordinal order); empty = none found.
        identity: Option<octa::db::write_back::RowIdentity>,
    },
    /// A finished table-metadata load ("Show metadata..."): opened as a plain
    /// read-only detached tab (no db_origin, no PK, not editable).
    MetadataReady {
        table: Box<octa::data::DataTable>,
        label: String,
    },
    Failed(String),
}

/// One table a deep search found.
#[derive(Debug, Clone)]
pub(crate) struct DbHit {
    pub(crate) conn_id: String,
    pub(crate) conn_name: String,
    pub(crate) catalog: Option<String>,
    pub(crate) schema: String,
    pub(crate) table: String,
}

/// How many schemas one deep search lists tables for before it stops. A
/// warehouse can hold thousands, and each is a round trip.
const SEARCH_MAX_SCHEMAS: usize = 500;

pub(crate) struct DbBrowserState {
    /// Whether the sidebar's Databases section is shown.
    pub(crate) visible: bool,
    /// Cached per-node listings, written by list workers.
    pub(crate) listings: Arc<Mutex<HashMap<ConnSchema, DbListState>>>,
    /// Which nodes the user has expanded (connection roots + schemas).
    pub(crate) expanded: HashSet<ConnSchema>,
    /// Finished/failed table loads, drained on the main thread per frame.
    pub(crate) pending_open: Arc<Mutex<Vec<DbOpenResult>>>,
    /// The search box's text.
    pub(crate) search_query: String,
    /// The deep search, written by its worker.
    pub(crate) search: Arc<Mutex<SearchSlot<DbHit>>>,
}

impl Default for DbBrowserState {
    fn default() -> Self {
        Self {
            visible: false,
            listings: Arc::new(Mutex::new(HashMap::new())),
            expanded: HashSet::new(),
            pending_open: Arc::new(Mutex::new(Vec::new())),
            search_query: String::new(),
            search: Arc::new(Mutex::new(SearchSlot::default())),
        }
    }
}

impl OctaApp {
    pub(crate) fn toggle_db_browser(&mut self) {
        self.db_browser.visible = !self.db_browser.visible;
    }

    pub(crate) fn find_db_conn(&self, conn_id: &str) -> Option<DbConnection> {
        self.settings
            .db_connections
            .iter()
            .find(|c| c.id == conn_id)
            .cloned()
    }

    /// Expand (and lazily list) or collapse a database node.
    pub(crate) fn toggle_db_node(&mut self, ctx: &egui::Context, conn_id: String, schema: String) {
        let key = (conn_id.clone(), schema.clone());
        if self.db_browser.expanded.contains(&key) {
            self.db_browser.expanded.remove(&key);
            return;
        }
        self.db_browser.expanded.insert(key.clone());
        let cached = self
            .db_browser
            .listings
            .lock()
            .map(|m| m.contains_key(&key))
            .unwrap_or(false);
        if !cached {
            self.start_db_list(ctx, conn_id, schema);
        }
    }

    /// Drop a connection's cached listings, collapse its schemas, and re-list
    /// its root (Refresh button).
    pub(crate) fn refresh_db_conn(&mut self, ctx: &egui::Context, conn_id: String) {
        if let Ok(mut m) = self.db_browser.listings.lock() {
            m.retain(|(c, _), _| c != &conn_id);
        }
        self.db_browser
            .expanded
            .retain(|(c, s)| c != &conn_id || s.is_empty());
        self.db_browser
            .expanded
            .insert((conn_id.clone(), String::new()));
        self.start_db_list(ctx, conn_id, String::new());
    }

    /// Populate a node's listing if nobody has yet, without touching the
    /// sidebar's expansion state. The SQL panel's attach menu uses this to get
    /// a connection's catalogs off the same cache and the same worker the
    /// sidebar uses, rather than blocking the interface thread on the network.
    pub(crate) fn ensure_db_listing(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        schema: String,
    ) {
        let key = (conn_id.clone(), schema.clone());
        let cached = self
            .db_browser
            .listings
            .lock()
            .map(|m| m.contains_key(&key))
            .unwrap_or(false);
        if !cached {
            self.start_db_list(ctx, conn_id, schema);
        }
    }

    fn start_db_list(&mut self, ctx: &egui::Context, conn_id: String, schema: String) {
        let Some(conn) = self.find_db_conn(&conn_id) else {
            return;
        };
        let settings = self.settings.clone();
        let key = (conn_id, schema.clone());
        let listings = self.db_browser.listings.clone();
        if let Ok(mut m) = listings.lock() {
            m.insert(key.clone(), DbListState::Loading);
        }
        let ctx = ctx.clone();
        let cache = self.db_conn_cache.clone();
        std::thread::spawn(move || {
            // A panicking worker would leave this node on `Loading` forever:
            // the expand path only starts a worker when the key is absent, so
            // collapsing and re-expanding never retries.
            let _guard = ListingGuard {
                listings: listings.clone(),
                key: key.clone(),
            };
            let secret = get_db_secret(&conn.id, &settings);
            let ssh_secret = get_ssh_secret(&conn.id, &settings);
            let result = cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                let parts = split_path(&schema);
                let state = if conn.engine.has_catalogs() {
                    match parts.as_slice() {
                        [] => DbListState::Catalogs(c.list_catalogs()?),
                        [cat] => DbListState::Schemas(c.list_schemas(Some(cat))?),
                        [cat, sch] => DbListState::Tables(c.list_tables(Some(cat), sch)?),
                        _ => DbListState::Error("unexpected node depth".into()),
                    }
                } else {
                    match parts.as_slice() {
                        [] => DbListState::Schemas(c.list_schemas(None)?),
                        [sch] => DbListState::Tables(c.list_tables(None, sch)?),
                        _ => DbListState::Error("unexpected node depth".into()),
                    }
                };
                Ok(state)
            });
            let state = result.unwrap_or_else(|e| DbListState::Error(format!("{e:#}")));
            if let Ok(mut m) = listings.lock() {
                m.insert(key, state);
            }
            ctx.request_repaint();
        });
    }

    /// Search every schema of the expanded connections for tables whose name
    /// contains the search box's text, on a worker. Only expanded
    /// connections: listing one means connecting to it, and a connection the
    /// user has not opened may well ask for a sign-in.
    pub(crate) fn start_db_search(&mut self, ctx: &egui::Context) {
        let query = self.db_browser.search_query.clone();
        let Some(needle) = tree_filter::needle(&query) else {
            return;
        };
        let conns: Vec<DbConnection> = self
            .settings
            .db_connections
            .iter()
            .filter(|c| {
                self.db_browser
                    .expanded
                    .contains(&(c.id.clone(), String::new()))
            })
            .cloned()
            .collect();
        let slot = self.db_browser.search.clone();
        let Ok(mut s) = slot.lock() else {
            return;
        };
        let stop = s.start(&query);
        if conns.is_empty() {
            s.state = TreeSearch::Failed(octa::i18n::t("treesearch.need_expand"));
            return;
        }
        drop(s);
        let settings = self.settings.clone();
        let cache = self.db_conn_cache.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut hits = Vec::new();
            let mut errors = Vec::new();
            let mut schemas_seen = 0usize;
            let mut stopped_at = None;
            for conn in &conns {
                if stopped_at.is_some() || stop.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let secret = get_db_secret(&conn.id, &settings);
                let ssh_secret = get_ssh_secret(&conn.id, &settings);
                let walked = cache.with_conn(conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                    let catalogs: Vec<Option<String>> = if conn.engine.has_catalogs() {
                        c.list_catalogs()?.into_iter().map(Some).collect()
                    } else {
                        vec![None]
                    };
                    let mut schemas: Vec<(Option<String>, String)> = Vec::new();
                    for catalog in catalogs {
                        if stop.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        // One catalogue query per catalog; walking schema by
                        // schema is one round trip each, minutes on a warehouse.
                        let found =
                            octa::db::table_search_sql(conn.engine, catalog.as_deref(), &needle)
                                .and_then(|sql| c.query(&sql).ok());
                        if let Some(t) = found {
                            hits.extend(t.rows.iter().filter(|r| r.len() >= 2).map(|r| DbHit {
                                conn_id: conn.id.clone(),
                                conn_name: conn.name.clone(),
                                catalog: catalog.clone(),
                                schema: r[0].to_string(),
                                table: r[1].to_string(),
                            }));
                            continue;
                        }
                        // A catalog the account cannot read is skipped, not
                        // the end of the search.
                        match c.list_schemas(catalog.as_deref()) {
                            Ok(names) => {
                                schemas.extend(names.into_iter().map(|s| (catalog.clone(), s)))
                            }
                            Err(e) if catalog.is_none() => return Err(e),
                            Err(_) => {}
                        }
                    }
                    for (catalog, schema) in schemas {
                        // Cancel: one schema's listing is the longest wait.
                        if stop.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        if schemas_seen >= SEARCH_MAX_SCHEMAS {
                            stopped_at = Some(schemas_seen);
                            break;
                        }
                        schemas_seen += 1;
                        let Ok(tables) = c.list_tables(catalog.as_deref(), &schema) else {
                            continue;
                        };
                        hits.extend(
                            tables
                                .into_iter()
                                .filter(|t| tree_filter::matches(t, &needle))
                                .map(|table| DbHit {
                                    conn_id: conn.id.clone(),
                                    conn_name: conn.name.clone(),
                                    catalog: catalog.clone(),
                                    schema: schema.clone(),
                                    table,
                                }),
                        );
                    }
                    Ok(())
                });
                if let Err(e) = walked {
                    errors.push(format!("{}: {e:#}", conn.name));
                }
            }
            // A connection that failed does not hide what the others found.
            let state = if hits.is_empty() && !errors.is_empty() {
                TreeSearch::Failed(
                    octa::i18n::t("treesearch.failed").replace("{error}", &errors.join("; ")),
                )
            } else {
                TreeSearch::Done { hits, stopped_at }
            };
            if let Ok(mut s) = slot.lock() {
                s.finish(&stop, state);
            }
            ctx.request_repaint();
        });
    }

    /// Claim the database-load slot for a read that is about to start, and
    /// hand the worker the three things it needs: a flag to raise on the way
    /// out, the slot to publish its cancel closure into, and the flag Cancel
    /// sets. Replacing an in-flight job is deliberate - the status bar has one
    /// spinner, so it names the most recent read.
    pub(crate) fn begin_db_load(
        &mut self,
        hint: String,
    ) -> (
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        super::sql_panel::SharedCancel,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        let job = super::state::DbLoadJob::new(hint);
        let handles = (
            job.finished.clone(),
            job.cancel.clone(),
            job.cancelled.clone(),
        );
        self.db_load_job = Some(job);
        handles
    }

    /// The database job the status bar's spinner and Cancel speak for: a
    /// table read first, else a running analysis on the server (a pushdown
    /// result tab, then a value-frequency count, then a key analysis in its
    /// dialog). Each lives in its own slot, so a sidebar open or a page
    /// scroll during a long analysis never takes the analysis' Cancel away.
    ///
    /// A table read that cannot cancel yet yields to the analysis: on the
    /// same connection it is waiting for the analysis to release the shared
    /// connector, so the analysis is what Cancel has to stop.
    pub(crate) fn busy_db_job(&self) -> Option<&super::state::DbLoadJob> {
        let analysis = self.pushdown_job.as_ref().map(|j| &j.load);
        // A value-frequency count still running on any tab's dialog.
        let vf_count = || {
            self.tabs
                .iter()
                .filter_map(|t| t.vf_server.as_ref().map(|v| &v.load))
                .find(|l| !l.finished.load(std::sync::atomic::Ordering::Relaxed))
        };
        // A key analysis still running in its dialog.
        let dialog_task = || {
            self.dialog_tasks()
                .into_iter()
                .find(|l| !l.finished.load(std::sync::atomic::Ordering::Relaxed))
        };
        self.db_load_job
            .as_ref()
            .filter(|j| j.can_cancel())
            .or(analysis)
            .or_else(vf_count)
            .or_else(dialog_task)
            .or(self.db_load_job.as_ref())
    }

    /// Stop the database read in flight. Best effort on the server (the
    /// vendor cancel may arrive after the statement finished), but the local
    /// wait always ends, which is what the user actually asked for.
    pub(crate) fn cancel_db_load(&mut self) {
        let Some(job) = self.busy_db_job() else {
            return;
        };
        job.cancel_now();
    }

    /// Retire the load slot once its worker has exited. Called once per frame
    /// from the update loop.
    pub(crate) fn drain_db_load_job(&mut self) {
        if self
            .db_load_job
            .as_ref()
            .is_some_and(|j| j.finished.load(std::sync::atomic::Ordering::Relaxed))
        {
            self.db_load_job = None;
        }
    }

    /// Load a table's first page on a worker and queue it for opening as a
    /// tab. The page is `db_page_rows`, not the streaming initial-load cap:
    /// that cap sizes a local file read, and the same number over a database
    /// connection is megabytes of JSON (Databricks refuses a result over
    /// 25 MiB outright). Further pages arrive from `central_panel`'s
    /// scroll-to-load-more path, the same way a large file's do.
    pub(crate) fn open_db_table(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        catalog: Option<String>,
        schema: String,
        table_name: String,
    ) {
        let Some(conn) = self.find_db_conn(&conn_id) else {
            return;
        };
        let label = format!("{table_name} @ {}", conn.name);
        let settings = self.settings.clone();
        let pending = self.db_browser.pending_open.clone();
        let ctx = ctx.clone();
        self.status_message = Some((
            format!("{} {label}", octa::i18n::t("db.loading")),
            std::time::Instant::now(),
        ));
        let cache = self.db_conn_cache.clone();
        let (finished, cancel_slot, cancelled) =
            self.begin_db_load(format!("{} {label}", octa::i18n::t("db.loading")));
        std::thread::spawn(move || {
            let _finished = crate::app::flag_guard::FlagOnDrop::new(finished, true);
            let result = (|| -> anyhow::Result<(
                octa::data::DataTable,
                Option<octa::db::write_back::RowIdentity>,
            )> {
                let secret = get_db_secret(&conn.id, &settings);
                let ssh_secret = get_ssh_secret(&conn.id, &settings);
                let page_rows = settings.db_page_size();
                let sql = db::select_sample_sql(
                    conn.engine,
                    catalog.as_deref(),
                    &schema,
                    &table_name,
                    page_rows,
                );
                let (mut table, identity) = cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                    // `with_conn` retries a cached connector once to heal a
                    // dead socket. After a Cancel that retry would silently
                    // re-run the statement the user just stopped, so refuse
                    // the second attempt outright.
                    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                        anyhow::bail!("{}", octa::i18n::t("db.load_cancelled"));
                    }
                    if let Ok(mut slot) = cancel_slot.lock() {
                        *slot = c.cancel_handle();
                    }
                    let table = c.query(&sql)?;
                    // Catalog engines expose no discoverable PK: skip the lookup
                    // so the tab opens read-only and no unqualified
                    // information_schema query runs. Otherwise best effort: a
                    // failed PK lookup just means read-only.
                    let pk_cols = if conn.engine.has_catalogs() {
                        Vec::new()
                    } else {
                        let key_sql =
                            db::row_key_sql(conn.engine, catalog.as_deref(), &schema, &table_name);
                        c.query(&key_sql)
                            .map(|t| {
                                let rows: Vec<db::RowKeyCandidate> = t
                                    .rows
                                    .iter()
                                    .filter_map(|r| {
                                        Some(db::RowKeyCandidate {
                                            constraint_type: r.first()?.to_string(),
                                            constraint_name: r.get(1)?.to_string(),
                                            column_name: r.get(2)?.to_string(),
                                            nullable: r
                                                .get(3)
                                                .map(|v| v.to_string().eq_ignore_ascii_case("YES"))
                                                .unwrap_or(true),
                                        })
                                    })
                                    .collect();
                                db::choose_row_key(&rows)
                            })
                            .unwrap_or_default()
                    };
                    // A key when the server guarantees one. Failing that,
                    // and only on engines whose plain UPDATE edits one row,
                    // fall back to matching every column of the baseline row -
                    // safe because `apply_write_back` then refuses any
                    // statement that did not touch exactly one row.
                    let identity = if !pk_cols.is_empty() {
                        Some(octa::db::write_back::RowIdentity::Key(pk_cols))
                    } else if conn.engine.supports_row_update() && !table.columns.is_empty() {
                        Some(octa::db::write_back::RowIdentity::FullRow(
                            table.columns.iter().map(|c| c.name.clone()).collect(),
                        ))
                    } else {
                        None
                    };
                    Ok((table, identity))
                })?;
                // A page that came back exactly full may have more behind
                // it. `total_rows` is read only as a "more may exist" flag
                // (the status bar prints the loaded count with a `+`), so an
                // exact server-side count is neither needed nor worth a
                // second query.
                if table.rows.len() >= page_rows {
                    table.total_rows = Some(table.rows.len());
                }
                // A writable tab needs row identity for the diff-based
                // write-back: tag every loaded row and snapshot it as the
                // baseline (same shape as the SQLite/DuckDB file readers).
                if conn.allow_writes && identity.is_some() {
                    baseline_db_meta(&mut table, &table_name, &schema);
                }
                Ok((table, identity))
            })();
            let item = match result {
                Ok((table, identity)) => DbOpenResult::Ready {
                    table: Box::new(table),
                    label,
                    conn_id,
                    catalog,
                    schema,
                    table_name,
                    identity,
                },
                Err(e) => DbOpenResult::Failed(format!(
                    "{} {label}: {e:#}",
                    octa::i18n::t("db.open_failed")
                )),
            };
            if let Ok(mut p) = pending.lock() {
                p.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Load a table's metadata (columns + table details) on a worker and queue
    /// it for opening as a plain read-only tab. Mirrors `open_db_table` but runs
    /// the engine's `table_metadata_sql` and carries no row identity.
    pub(crate) fn open_db_metadata(
        &mut self,
        ctx: &egui::Context,
        conn_id: String,
        catalog: Option<String>,
        schema: String,
        table_name: String,
    ) {
        let Some(conn) = self.find_db_conn(&conn_id) else {
            return;
        };
        let label = format!(
            "{table_name} {} @ {}",
            octa::i18n::t("db.metadata_label"),
            conn.name
        );
        let settings = self.settings.clone();
        let pending = self.db_browser.pending_open.clone();
        let ctx = ctx.clone();
        self.status_message = Some((
            format!("{} {label}", octa::i18n::t("db.loading")),
            std::time::Instant::now(),
        ));
        let cache = self.db_conn_cache.clone();
        std::thread::spawn(move || {
            let secret = get_db_secret(&conn.id, &settings);
            let ssh_secret = get_ssh_secret(&conn.id, &settings);
            let sql = db::table_metadata_sql(conn.engine, catalog.as_deref(), &schema, &table_name);
            let result = cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                c.query(&sql)
            });
            let item = match result {
                Ok(table) => DbOpenResult::MetadataReady {
                    table: Box::new(table),
                    label,
                },
                Err(e) => DbOpenResult::Failed(format!(
                    "{} {label}: {e:#}",
                    octa::i18n::t("db.open_failed")
                )),
            };
            if let Ok(mut p) = pending.lock() {
                p.push(item);
            }
            ctx.request_repaint();
        });
    }

    /// Open any table loads that finished since last frame. Runs on the main
    /// thread (touches tabs/egui); called from the update loop.
    pub(crate) fn drain_db_pending_open(&mut self) {
        let drained: Vec<DbOpenResult> = {
            let Ok(mut p) = self.db_browser.pending_open.lock() else {
                return;
            };
            if p.is_empty() {
                return;
            }
            std::mem::take(&mut *p)
        };
        for item in drained {
            match item {
                DbOpenResult::Ready {
                    table,
                    label,
                    conn_id,
                    catalog,
                    schema,
                    table_name,
                    identity,
                } => {
                    let mut new_tab =
                        super::state::TabState::new(self.settings.default_search_mode);
                    new_tab.table = *table;
                    new_tab.custom_tab_label = Some(label);
                    // SQL on this tab targets the server by default.
                    new_tab.sql_target = Some(conn_id.clone());
                    let origin = DbOrigin {
                        conn_id,
                        catalog,
                        schema,
                        table: table_name,
                        identity,
                    };
                    // Note explaining the tab's editability: writable by a
                    // key -> none; writable only by matching whole rows ->
                    // say so, because that has a ceiling the user has to know
                    // about; connection read-only or no way to address a row
                    // -> why it stays locked.
                    //
                    // A status message, not the tab's banner. It reports what
                    // just happened when a tab opened, which is what every
                    // other such report in the app is, so it gets that look,
                    // that dismiss button, that hover pause and the one
                    // lifetime in Settings > Appearance rather than a second
                    // style that sits there until clicked.
                    let writable = self.db_origin_writable(&origin);
                    let full_row = matches!(
                        origin.identity,
                        Some(octa::db::write_back::RowIdentity::FullRow(_))
                    );
                    let conn_allows = self
                        .settings
                        .db_connections
                        .iter()
                        .any(|c| c.id == origin.conn_id && c.allow_writes);
                    let note = if writable && full_row {
                        Some(octa::i18n::t("db.tab_full_row_note"))
                    } else if writable {
                        None
                    } else if conn_allows {
                        Some(octa::i18n::t("db.tab_no_pk_note"))
                    } else {
                        Some(octa::i18n::t("db.tab_readonly_note"))
                    };
                    if let Some(note) = note {
                        self.status_message = Some((note, std::time::Instant::now()));
                    }
                    self.take_reload_slot(&super::refresh::db_source(
                        &origin.conn_id,
                        origin.catalog.as_deref(),
                        &origin.schema,
                        &origin.table,
                    ));
                    new_tab.db_origin = Some(origin);
                    // Arm the scroll-to-load-more path, the same four fields
                    // the file open sets (`file_io::mod`). `total_rows` is
                    // what the worker set above, so a table that fitted in one
                    // page leaves this off.
                    if new_tab.table.total_rows.is_some() {
                        new_tab.bg_can_load_more = true;
                        new_tab.bg_row_buffer = None;
                        new_tab
                            .bg_loading_done
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        new_tab
                            .bg_file_exhausted
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    self.open_db_result_tab(new_tab);
                }
                DbOpenResult::MetadataReady { table, label } => {
                    let mut new_tab =
                        super::state::TabState::new(self.settings.default_search_mode);
                    new_tab.table = *table;
                    new_tab.custom_tab_label = Some(label);
                    self.open_db_result_tab(new_tab);
                }
                DbOpenResult::Failed(msg) => {
                    self.status_message = Some((msg, std::time::Instant::now()));
                }
            }
        }
    }

    /// Place a finished DB result tab: reuse the active tab when it is
    /// completely blank (the tab Octa starts with), else push a new one.
    fn open_db_result_tab(&mut self, new_tab: super::state::TabState) {
        self.push_result_tab(new_tab);
    }
}

#[cfg(test)]
mod tests {
    use super::{join_path, split_path};

    #[test]
    fn path_roundtrip() {
        assert_eq!(split_path(""), Vec::<&str>::new());
        assert_eq!(split_path("main"), vec!["main"]);
        assert_eq!(split_path("main\u{1f}sales"), vec!["main", "sales"]);
        assert_eq!(join_path(&["main", "sales"]), "main\u{1f}sales");
        assert_eq!(join_path(&["public"]), "public");
    }
}
