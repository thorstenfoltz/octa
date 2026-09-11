//! The SQL panel's "Ask" path: send the question box's sentence to a chat
//! profile and splice the answer into the editor.
//!
//! One request, no tools, no agent loop (see `chat::ask_sql`). The network
//! call runs on a worker thread and the update loop drains the slot, the same
//! shape `ask_filter_job` uses. The query is never run: the user reads it and
//! presses Run.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::app::chat::ask_sql;
use crate::app::chat::providers;
use crate::app::state::{AskSqlJob, OctaApp};

impl OctaApp {
    /// Fire one request for the SQL panel's question box.
    pub(crate) fn start_ask_sql(&mut self, ctx: &eframe::egui::Context, question: String) {
        if self.ask_sql_job.is_some() {
            return; // one at a time; the spinner is already up
        }
        let question = question.trim().to_string();
        if question.is_empty() {
            return;
        }
        let tab_idx = self.active_tab;
        let Some(tab) = self.tabs.get(tab_idx) else {
            return;
        };
        // Everything the prompt can describe counts, not just the tab's own
        // table: an empty tab whose workspace has a server ATTACHed is exactly
        // the case this box is for. Mirrors the button's own gate in
        // `view_modes::sql`.
        let workspace_tables: Vec<(String, Vec<(String, String)>)> = tab
            .sql_workspace
            .as_ref()
            .map(|ws| {
                ws.schema_overview(ask_sql::MAX_WORKSPACE_TABLES)
                    .into_iter()
                    .map(|(name, cols)| {
                        (
                            name,
                            cols.into_iter().map(|c| (c.name, c.data_type)).collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        if tab.table.col_count() == 0 && workspace_tables.is_empty() {
            self.status_message = Some((
                octa::i18n::t("sql.ask_no_columns"),
                std::time::Instant::now(),
            ));
            return;
        }

        // Where the answer goes. Same `load_state` read the autocomplete uses,
        // and the same fall back to the end of the text when the editor has
        // never held the cursor.
        let editor_id = crate::view_modes::sql::editor_id();
        let insert_at = eframe::egui::TextEdit::load_state(ctx, editor_id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let char_idx = r.primary.index.0;
                tab.sql_query
                    .char_indices()
                    .nth(char_idx)
                    .map(|(i, _)| i)
                    .unwrap_or(tab.sql_query.len())
            })
            .unwrap_or(tab.sql_query.len());

        // Where the query will run decides how it must be spelled. Same
        // columns either way; only the address and the dialect change.
        let (table_name, dialect) = match (&tab.db_origin, tab.sql_run_on_server) {
            (Some(origin), true) => {
                let engine = self
                    .settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == origin.conn_id)
                    .map(|c| c.engine.label())
                    .unwrap_or("SQL");
                let name = if origin.schema.is_empty() {
                    origin.table.clone()
                } else {
                    format!("{}.{}", origin.schema, origin.table)
                };
                (name, engine.to_string())
            }
            _ => ("data".to_string(), "DuckDB".to_string()),
        };

        // Whether the neighbours can be asked about at all. Only a
        // server-mode query can join them: the local DuckDB workspace holds
        // this tab's table alone, so naming a sibling there would produce SQL
        // that cannot run. An unqualified table (no schema) is skipped too,
        // because `relationships::scan` addresses everything as
        // `schema.table` and nothing would match. So is an engine with no
        // foreign keys at all: `scan` reads the columns first and only then
        // discovers it has no keys to read, which would be a wasted catalog
        // sweep on every question asked against ClickHouse, Trino or Athena.
        let rel_scan = match (&tab.db_origin, tab.sql_run_on_server) {
            (Some(origin), true) if !origin.schema.is_empty() => self
                .settings
                .db_connections
                .iter()
                .find(|c| c.id == origin.conn_id && c.engine.has_foreign_keys())
                .cloned()
                .map(|conn| {
                    (
                        conn,
                        origin.catalog.clone(),
                        origin.schema.clone(),
                        format!("{}.{}", origin.schema, origin.table),
                    )
                }),
            _ => None,
        };

        // The tab's own Ask profile, picked in the combo beside the box. It
        // is seeded from `chat_active_profile`, so an untouched tab still
        // asks whatever the chat panel is set to.
        // `tab_idx`, not `active_tab`: the rest of this function already
        // reads the tab the question was typed on, and they must not diverge.
        let profile_id = self.tabs[tab_idx].sql_ask_profile.clone();
        let Some(profile) = self
            .settings
            .chat_profiles
            .iter()
            .find(|p| p.id == profile_id)
            .or_else(|| self.settings.chat_profiles.first())
            .cloned()
        else {
            self.status_message = Some((
                octa::i18n::t("sql.ask_needs_profile"),
                std::time::Instant::now(),
            ));
            return;
        };

        let api_key =
            crate::app::chat_panel::profile_api_key(&profile, &self.settings).unwrap_or_default();
        let cfg = providers::config_for_profile(
            &profile,
            &match profile.kind {
                octa::ui::settings::ChatProviderKind::Ollama => {
                    self.settings.chat_ollama_url.clone()
                }
                _ => self.settings.chat_base_url.clone(),
            },
            api_key,
            if self.settings.chat_max_tokens_unlimited {
                None
            } else {
                Some(self.settings.chat_max_tokens)
            },
        );
        let provider_kind = profile.kind;

        // Snapshot the schema, not the table. `total_rows` is the file's own
        // size; `row_count()` is whatever is loaded, which for a live database
        // tab is one page. Sending a page size as a table size told the model
        // a 40-million-row table had 100,000 rows, so send nothing instead.
        let columns = tab.table.columns.clone();
        let row_count = tab.table.total_rows.or(if tab.db_origin.is_some() {
            None
        } else {
            Some(tab.table.row_count())
        });

        let slot: Arc<Mutex<Option<Result<String, String>>>> = Arc::new(Mutex::new(None));
        self.ask_sql_job = Some(AskSqlJob {
            tab_idx,
            insert_at,
            result: Arc::clone(&slot),
        });
        let ctx = ctx.clone();
        let cache = self.db_conn_cache.clone();
        let settings = self.settings.clone();
        // The Ask boxes are real API requests and were the one thing the token
        // meter never saw, which is what made it look as if tool traffic was
        // being left out. Same shared state `agent.rs` adds to.
        let session = std::sync::Arc::clone(&self.chat.session);
        std::thread::spawn(move || {
            // Two catalog queries, no table data, on the connection the tab
            // already uses. It runs here rather than on the UI thread because
            // it is a round trip, and it is allowed to fail: a catalog the
            // user cannot read is no reason to refuse the question, so the
            // fallback is the prompt as it was before neighbours existed.
            let related = rel_scan
                .and_then(|(conn, catalog, schema, label)| {
                    let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, &settings);
                    let ssh_secret =
                        octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, &settings);
                    let scanned =
                        cache.with_conn(&conn, secret.as_deref(), ssh_secret.as_deref(), |c| {
                            octa::db::relationships::scan(
                                c,
                                catalog.as_deref(),
                                std::slice::from_ref(&schema),
                            )
                        });
                    match scanned {
                        Ok((cols, fks)) => Some(ask_sql::related_tables_block(&label, &cols, &fks)),
                        Err(e) => {
                            tracing::warn!("ask sql: reading the foreign keys failed: {e:#}");
                            None
                        }
                    }
                })
                .unwrap_or_default();
            let provider = providers::make_provider(provider_kind);
            let cancel = AtomicBool::new(false);
            let facts = ask_sql::AskSqlFacts {
                table_name,
                dialect,
                columns,
                row_count,
                related,
                workspace: ask_sql::workspace_block(&workspace_tables),
            };
            let system = ask_sql::build_prompt(&facts, &question);
            let mut usage = (0u32, 0u32);
            let outcome = ask_sql::ask(
                provider.as_ref(),
                &cfg,
                &system,
                &question,
                &cancel,
                &mut usage,
            );
            if let Ok(mut s) = session.lock() {
                s.input_tokens = s.input_tokens.saturating_add(usage.0);
                s.output_tokens = s.output_tokens.saturating_add(usage.1);
            }
            if let Ok(mut g) = slot.lock() {
                *g = Some(outcome);
            }
            ctx.request_repaint();
        });
    }

    /// Poll the in-flight request. Called once per frame from the update loop.
    pub(crate) fn drain_ask_sql(&mut self) {
        let Some(job) = &self.ask_sql_job else {
            return;
        };
        let Some(outcome) = job.result.lock().ok().and_then(|mut g| g.take()) else {
            return;
        };
        let tab_idx = job.tab_idx;
        let insert_at = job.insert_at;
        self.ask_sql_job = None;

        match outcome {
            Ok(sql) => {
                let Some(tab) = self.tabs.get_mut(tab_idx) else {
                    return;
                };
                tab.sql_query = ask_sql::splice_at(&tab.sql_query, insert_at, &sql);
                tab.sql_editor_focus_pending = true;
                tab.sql_ask_input.clear();
            }
            Err(e) => {
                // The editor belongs to the user: on failure it is left
                // exactly as it was.
                self.status_message = Some((
                    format!("{}: {e}", octa::i18n::t("sql.ask_failed")),
                    std::time::Instant::now(),
                ));
            }
        }
    }
}
