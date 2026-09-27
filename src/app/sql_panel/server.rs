//! Server-mode queries: running a query on a live connection, paging and cancelling it.

use super::*;

impl OctaApp {
    /// Append the next page of the SQL result the workspace materialised.
    ///
    /// Called when the result grid scrolls near the end of what it holds. The
    /// rows never left DuckDB, so this is a `LIMIT/OFFSET` over a temp table,
    /// not a re-run of the user's query.
    pub(super) fn load_more_sql_result_rows(&mut self) {
        let page_rows = self.settings.sql_result_page_rows;
        if page_rows == 0 {
            return;
        }
        let tab = &mut self.tabs[self.active_tab];
        let (Some(total), Some(loaded)) = (
            tab.sql_result_total,
            tab.sql_result.as_ref().map(|r| r.row_count()),
        ) else {
            return;
        };
        if loaded >= total {
            return;
        }
        let page = match tab.sql_workspace.as_ref() {
            Some(ws) => ws.result_page(loaded, page_rows),
            None => return,
        };
        match page {
            Ok(page) => {
                if let Some(result) = tab.sql_result.as_mut() {
                    result.rows.extend(page.rows);
                }
            }
            // `{e:#}`: the outermost context alone would just say "query".
            // Clearing the total stops the grid asking again next frame,
            // which would otherwise re-raise the same error forever.
            Err(e) => {
                tab.sql_error = Some(format!("{e:#}"));
                tab.sql_result_total = None;
            }
        }
    }

    /// Run the editor's query on the live database server the active tab was
    /// opened from, on a worker thread (network). One in-flight query at a
    /// time; the result lands via `drain_sql_server_job`.
    pub(crate) fn run_server_query(&mut self, ctx: &egui::Context) {
        if self.sql_server_job.is_some() {
            return;
        }
        let tab_idx = self.active_tab;
        let Some(origin) = self.tabs[tab_idx].db_origin.clone() else {
            return;
        };
        let query = self.tabs[tab_idx].sql_query.clone();
        if query.trim().is_empty() {
            return;
        }
        let Some(conn) = self
            .settings
            .db_connections
            .iter()
            .find(|c| c.id == origin.conn_id)
            .cloned()
        else {
            self.tabs[tab_idx].sql_error = Some(octa::i18n::t("sql.server_conn_gone"));
            return;
        };
        self.tabs[tab_idx].sql_error = None;
        let result = std::sync::Arc::new(std::sync::Mutex::new(None));
        let cancel = std::sync::Arc::new(std::sync::Mutex::new(None));
        self.sql_server_job = Some(SqlServerJob {
            started: std::time::Instant::now(),
            tab_idx,
            conn_id: conn.id.clone(),
            query: query.clone(),
            result: result.clone(),
            cancel: cancel.clone(),
        });
        let settings = self.settings.clone();
        let ctx = ctx.clone();
        let cache = self.db_conn_cache.clone();
        std::thread::spawn(move || {
            let done = (|| -> Result<ServerQueryDone, String> {
                let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                let ssh_secret =
                    octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                if octa::sql::is_mutation(&query) {
                    octa::db::ensure_write_allowed(&conn, Some(&query))
                        .map_err(|e| format!("{e:#}"))?;
                }
                // No `with_conn` auto-retry here: a user cancel surfaces as a
                // query error, and a retry would silently re-run the very
                // statement that was just cancelled. Single attempt; drop the
                // cached connector on failure so the next run reconnects.
                let (shared, _) = cache
                    .get_or_connect(&conn, secret.as_deref(), ssh_secret.as_deref())
                    .map_err(|e| format!("{e:#}"))?;
                let res = {
                    let mut c = crate::app::db_conn_cache::lock_connector(&shared);
                    if let Ok(mut slot) = cancel.lock() {
                        *slot = c.cancel_handle();
                    }
                    if octa::sql::is_mutation(&query) {
                        c.execute(&query).map(ServerQueryDone::Affected)
                    } else {
                        c.query(&query).map(|t| ServerQueryDone::Rows(Box::new(t)))
                    }
                };
                res.map_err(|e| {
                    cache.invalidate(&conn.id);
                    format!("{e:#}")
                })
            })();
            let done = done.unwrap_or_else(ServerQueryDone::Failed);
            if let Ok(mut r) = result.lock() {
                *r = Some(done);
            }
            ctx.request_repaint();
        });
    }

    /// Best-effort cancel of the in-flight server query (Postgres only; the
    /// other engines have no cross-thread cancel).
    pub(super) fn cancel_server_query(&mut self) {
        if let Some(job) = &self.sql_server_job
            && let Ok(c) = job.cancel.lock()
            && let Some(f) = c.as_ref()
        {
            f();
        }
    }

    /// Apply a finished server query to its tab. Called once per frame from
    /// the update loop.
    pub(crate) fn drain_sql_server_job(&mut self) {
        let Some(job) = &self.sql_server_job else {
            return;
        };
        let done = {
            let Ok(mut r) = job.result.lock() else {
                return;
            };
            r.take()
        };
        let Some(done) = done else {
            return;
        };
        let job = self.sql_server_job.take().expect("job checked above");
        // Read before the tab borrow: `record_sql_history` runs while the tab
        // is held mutably.
        let history_on = self.settings.sql_history_enabled;
        let history_limit = self.settings.sql_history_limit;
        // Only apply if the originating tab still shows the same connection
        // (tabs may have been closed/reordered while the query ran).
        let Some(tab) = self.tabs.get_mut(job.tab_idx).filter(|t| {
            t.db_origin
                .as_ref()
                .is_some_and(|o| o.conn_id == job.conn_id)
        }) else {
            self.status_message = Some((
                octa::i18n::t("sql.server_tab_gone"),
                std::time::Instant::now(),
            ));
            return;
        };
        tab.sql_last_duration_ms = Some(job.started.elapsed().as_millis() as u64);
        match done {
            ServerQueryDone::Rows(t) => {
                let rows = t.row_count();
                tab.sql_result = Some(*t);
                tab.sql_result_total = None;
                tab.sql_error = None;
                record_sql_history(
                    tab,
                    &job.query,
                    job.started,
                    rows,
                    history_on,
                    history_limit,
                );
                tab.sql_last_query = job.query;
            }
            ServerQueryDone::Affected(n) => {
                record_sql_history(
                    tab,
                    &job.query,
                    job.started,
                    n as usize,
                    history_on,
                    history_limit,
                );
                tab.sql_result = None;
                tab.sql_result_total = None;
                tab.sql_error = None;
                self.status_message = Some((
                    format!("SQL applied on server: {n} row(s) affected"),
                    std::time::Instant::now(),
                ));
            }
            ServerQueryDone::Failed(msg) => {
                tab.sql_error = Some(msg);
            }
        }
    }
}
