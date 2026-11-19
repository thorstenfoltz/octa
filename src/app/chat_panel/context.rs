//! Chat tool-context assembly and the pending-tab-edit drain that applies assistant edits back onto tabs. Split out of chat_panel/mod.rs.

use crate::app::pushdown::server_source_for;
use crate::app::state::OctaApp;
use crate::mcp::tools::{SnapshotDb, SnapshotWindow, TableSnapshot, ToolContext};
use crate::ui::settings::chat_profiles;

use super::CHAT_CELL_CAP;
use super::helpers::{snapshot_table, tab_display_name};

impl OctaApp {
    /// Snapshot every open (non-chart) tab into a sandboxed `ToolContext`:
    /// the agent may read only these files (and the other sheets/tables of an
    /// open workbook/database) and writes are confined to the export dir.
    pub(crate) fn build_tool_context(&self, allow_writes: bool) -> ToolContext {
        let mut open_tabs: Vec<TableSnapshot> = Vec::new();
        let mut active_index: Option<usize> = None;
        let mut allowed_read_paths: Vec<std::path::PathBuf> = Vec::new();

        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.is_chart_tab {
                continue;
            }
            let is_active = i == self.active_tab;
            let snapshot = snapshot_table(&tab.table);
            let display_name = tab_display_name(tab, i);
            let source_path = tab.table.source_path.clone();
            if let Some(sp) = &source_path {
                let p = std::path::Path::new(sp);
                let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
                if !allowed_read_paths.contains(&canon) {
                    allowed_read_paths.push(canon);
                }
            }
            if is_active {
                active_index = Some(open_tabs.len());
            }
            // A large-file tab's table is one page read from disk, not the
            // file. Say so, or every tool answers from the window.
            let window = match (&tab.large_page_key, &tab.large) {
                (Some(key), Some(handle)) => Some(SnapshotWindow {
                    offset: key.offset,
                    len: snapshot.row_count(),
                    total: snapshot.total_rows.unwrap_or_else(|| handle.row_count()),
                }),
                _ => None,
            };
            let db = tab
                .db_origin
                .as_ref()
                .filter(|_| tab.source_has_more())
                .map(|origin| {
                    let catalog = origin.catalog.as_deref().unwrap_or_default();
                    let edited = tab.is_modified();
                    SnapshotDb {
                        table: [catalog, &origin.schema, &origin.table]
                            .into_iter()
                            .filter(|part| !part.is_empty())
                            .collect::<Vec<_>>()
                            .join("."),
                        connection: self
                            .find_db_conn(&origin.conn_id)
                            .map_or_else(|| origin.conn_id.clone(), |conn| conn.name),
                        source: server_source_for(
                            tab,
                            self.settings.db_pushdown,
                            &self.settings.db_connections,
                        )
                        .filter(|_| !edited),
                        edited,
                    }
                });
            open_tabs.push(TableSnapshot {
                handle: format!("#{}", open_tabs.len() + 1),
                display_name,
                source_path,
                table: snapshot,
                window,
                read_only: self.tab_is_readonly(i),
                db,
            });
        }

        // The limits and the folder are the active profile's, each falling
        // back to the general setting.
        let profile = octa::ui::settings::chat_profiles::active_profile(&self.settings);
        let export_dir =
            octa::ui::settings::chat_profiles::export_dir_for(&profile, &self.settings);

        ToolContext {
            loaded_rows_note: Default::default(),
            open_tabs,
            active_tab: active_index,
            // Cap how many rows a tool result puts into the model's context.
            // User-configurable (Settings > Chat); the Unlimited checkbox sends
            // `None` (no cap).
            large_file_min_bytes: self.settings.large_file_min_bytes,
            default_row_limit: octa::ui::settings::chat_profiles::result_row_limit_for(
                &profile,
                &self.settings,
            ),
            cell_byte_cap: CHAT_CELL_CAP,
            restrict_filesystem: true,
            allowed_read_paths,
            export_dir,
            allow_existing_writes: allow_writes,
            allow_schema_changes: allow_writes,
            backup_before_modify: self.settings.backup_before_modify,
            pending_tab_edits: Some(self.pending_tab_edits.clone()),
            // The assistant may reach saved cloud connections (creds resolved
            // lazily on the worker thread).
            cloud_settings: Some(self.settings.clone()),
            db_connections: self.settings.db_connections.clone(),
            api_connections: self.settings.api_connections.clone(),
            read_only: !allow_writes,
        }
    }

    /// Map a tool-facing tab handle (`#1`, ...) to an index into `tabs`.
    ///
    /// The numbering is `build_tool_context`'s: chart tabs are skipped, so the
    /// handle a tool was given and the tab it lands on cannot drift apart.
    pub(crate) fn tab_index_for_handle(&self, handle: &str) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_chart_tab)
            .enumerate()
            .find(|(pos, _)| format!("#{}", pos + 1) == handle)
            .map(|(_, (i, _))| i)
    }

    /// Apply any live-tab edits the chat agent queued. Each batch is applied on
    /// the UI thread through the normal undoable table mutations, coalesced into
    /// one undo entry. Aborts a batch (with a status message) if the target tab
    /// is gone or its row count drifted from the snapshot the ops were computed
    /// against, so data can never misalign.
    pub(crate) fn drain_pending_tab_edits(&mut self) {
        let batches: Vec<crate::mcp::tools::PendingTabEdit> = {
            let mut q = self.pending_tab_edits.lock().unwrap();
            if q.is_empty() {
                return;
            }
            std::mem::take(&mut *q)
        };

        let order = octa::data::TextOrder::from_natural(self.settings.natural_sort);
        for batch in batches {
            // Plan mode: hold the batch for review instead of applying it. The
            // flag rides on the batch, so flipping the panel's mode mid-turn
            // cannot send an already-computed batch down the wrong path.
            tracing::info!(
                tab = %batch.tab_handle,
                ops = batch.ops.len(),
                "assistant tab-edit batch drained"
            );
            // Map the handle (#N) to a live non-chart tab, same numbering as
            // build_tool_context.
            let tab_idx = self.tab_index_for_handle(&batch.tab_handle);
            let Some(tab_idx) = tab_idx else {
                self.status_message = Some((
                    octa::i18n::t("gui.chat_skip_closed")
                        .replace("{tab}", &batch.tab_handle.to_string()),
                    std::time::Instant::now(),
                ));
                continue;
            };
            // The tab the edit is for, not the active one. A database sort or
            // filter running on it would replace its rows, edit and all.
            if self.tab_is_readonly(tab_idx)
                || !chat_profiles::active_profile(&self.settings).allow_writes
            {
                self.status_message = Some((
                    octa::i18n::t("gui.chat_skip_disabled"),
                    std::time::Instant::now(),
                ));
                continue;
            }
            let tab = &mut self.tabs[tab_idx];
            if tab.table.row_count() != batch.snapshot_rows {
                self.status_message = Some((
                    octa::i18n::t("gui.chat_skip_changed"),
                    std::time::Instant::now(),
                ));
                continue;
            }

            let start = tab.table.undo_stack.len();
            for op in &batch.ops {
                op.apply_to(&mut tab.table, order);
            }
            tab.table.coalesce_undo_since(start);
            // Remember the assistant touched this tab, so the next manual save
            // backs up the original file first (the user's own edits don't).
            tab.assistant_modified = true;
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
        }
    }
}
