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
use octa::ui::settings::db_secrets::get_db_secret;

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
        pk_cols: Vec<String>,
    },
    /// A finished table-metadata load ("Show metadata..."): opened as a plain
    /// read-only detached tab (no db_origin, no PK, not editable).
    MetadataReady {
        table: Box<octa::data::DataTable>,
        label: String,
    },
    Failed(String),
}

pub(crate) struct DbBrowserState {
    /// Whether the sidebar's Databases section is shown.
    pub(crate) visible: bool,
    /// Cached per-node listings, written by list workers.
    pub(crate) listings: Arc<Mutex<HashMap<ConnSchema, DbListState>>>,
    /// Which nodes the user has expanded (connection roots + schemas).
    pub(crate) expanded: HashSet<ConnSchema>,
    /// Finished/failed table loads, drained on the main thread per frame.
    pub(crate) pending_open: Arc<Mutex<Vec<DbOpenResult>>>,
}

impl Default for DbBrowserState {
    fn default() -> Self {
        Self {
            visible: false,
            listings: Arc::new(Mutex::new(HashMap::new())),
            expanded: HashSet::new(),
            pending_open: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl OctaApp {
    pub(crate) fn toggle_db_browser(&mut self) {
        self.db_browser.visible = !self.db_browser.visible;
    }

    fn find_db_conn(&self, conn_id: &str) -> Option<DbConnection> {
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
            let secret = get_db_secret(&conn.id, &settings);
            let result = cache.with_conn(&conn, secret.as_deref(), |c| {
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

    /// Load a table's first rows on a worker and queue it for opening as a
    /// read-only tab. The row cap is the streaming initial-load cap, same as
    /// opening a large file.
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
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<(octa::data::DataTable, Vec<String>)> {
                let secret = get_db_secret(&conn.id, &settings);
                let sql = db::select_sample_sql(
                    conn.engine,
                    catalog.as_deref(),
                    &schema,
                    &table_name,
                    octa::formats::initial_load_rows(),
                );
                let (mut table, pk_cols) = cache.with_conn(&conn, secret.as_deref(), |c| {
                    let table = c.query(&sql)?;
                    // Catalog engines expose no discoverable PK: skip the lookup
                    // so the tab opens read-only and no unqualified
                    // information_schema query runs. Otherwise best effort: a
                    // failed PK lookup just means read-only.
                    let pk_cols = if conn.engine.has_catalogs() {
                        Vec::new()
                    } else {
                        let pk_sql = db::primary_key_sql(
                            conn.engine,
                            catalog.as_deref(),
                            &schema,
                            &table_name,
                        );
                        c.query(&pk_sql)
                            .map(|t| {
                                t.rows
                                    .iter()
                                    .filter_map(|r| r.first().map(|v| v.to_string()))
                                    .collect::<Vec<String>>()
                            })
                            .unwrap_or_default()
                    };
                    Ok((table, pk_cols))
                })?;
                // A writable tab needs row identity for the diff-based
                // write-back: tag every loaded row and snapshot it as the
                // baseline (same shape as the SQLite/DuckDB file readers).
                if conn.allow_writes && !pk_cols.is_empty() {
                    let original: std::collections::HashMap<i64, Vec<octa::data::CellValue>> =
                        table
                            .rows
                            .iter()
                            .enumerate()
                            .map(|(i, r)| (i as i64, r.clone()))
                            .collect();
                    table.db_meta = Some(octa::data::DbRowMeta {
                        table_name: table_name.clone(),
                        schema: Some(schema.clone()),
                        row_tags: (0..table.rows.len()).map(|i| Some(i as i64)).collect(),
                        original,
                        original_columns: table.columns.iter().map(|c| c.name.clone()).collect(),
                    });
                }
                Ok((table, pk_cols))
            })();
            let item = match result {
                Ok((table, pk_cols)) => DbOpenResult::Ready {
                    table: Box::new(table),
                    label,
                    conn_id,
                    catalog,
                    schema,
                    table_name,
                    pk_cols,
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
            let sql = db::table_metadata_sql(conn.engine, catalog.as_deref(), &schema, &table_name);
            let result = cache.with_conn(&conn, secret.as_deref(), |c| c.query(&sql));
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
                    pk_cols,
                } => {
                    let mut new_tab =
                        super::state::TabState::new(self.settings.default_search_mode);
                    new_tab.table = *table;
                    new_tab.custom_tab_label = Some(label);
                    // SQL on this tab targets the server by default.
                    new_tab.sql_run_on_server = true;
                    let origin = DbOrigin {
                        conn_id,
                        catalog,
                        schema,
                        table: table_name,
                        pk_cols,
                    };
                    // Dismissible note explaining the tab's editability:
                    // writable -> none; connection read-only -> the standard
                    // note; writes allowed but no PK -> why it stays locked.
                    let writable = self.db_origin_writable(&origin);
                    let conn_allows = self
                        .settings
                        .db_connections
                        .iter()
                        .any(|c| c.id == origin.conn_id && c.allow_writes);
                    new_tab.parse_error_banner = if writable {
                        None
                    } else if conn_allows {
                        Some(octa::i18n::t("db.tab_no_pk_note"))
                    } else {
                        Some(octa::i18n::t("db.tab_readonly_note"))
                    };
                    new_tab.db_origin = Some(origin);
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
    /// completely blank (the tab Octa starts with), else push a new one. Same
    /// stray-"Untitled" guard as `load_file_in_new_tab`.
    fn open_db_result_tab(&mut self, new_tab: super::state::TabState) {
        let blank = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.table.col_count() == 0 && t.raw_content.is_none() && !t.is_modified())
            .unwrap_or(false);
        if blank {
            self.tabs[self.active_tab] = new_tab;
        } else {
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }
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
