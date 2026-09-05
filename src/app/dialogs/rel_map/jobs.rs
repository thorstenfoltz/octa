//! Scanning for relationships in the background, and applying the result:
//! the `ScanResult` / `RelMapJob` plumbing on `OctaApp`.
//!
//! Split out of `app/dialogs/rel_map.rs` (1,434 lines). Code moved unchanged.

use super::*;

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
    pub(super) fn spawn_rel_map_meta(&self, st: &mut RelMapState, ctx: &egui::Context) {
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
            let ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
            let outcome = cache
                .with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
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
    pub(super) fn spawn_rel_map_score(&self, st: &mut RelMapState, ctx: &egui::Context) {
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
                let ssh_secret =
                    octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                // One connection for every table, not one per table: a map of
                // twenty boxes would otherwise open twenty connections.
                let tables =
                    cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
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

    pub(super) fn spawn_rel_map_scan(&self, st: &mut RelMapState, ctx: &egui::Context) {
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
                    let ssh_secret =
                        octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                    let (columns, fks) =
                        cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
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
