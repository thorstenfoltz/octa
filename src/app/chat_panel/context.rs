//! Chat tool-context assembly and the pending-tab-edit drain that applies assistant edits back onto tabs. Split out of chat_panel/mod.rs.

use crate::app::state::OctaApp;
use crate::mcp::tools::{TableSnapshot, ToolContext};
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
            open_tabs.push(TableSnapshot {
                handle: format!("#{}", open_tabs.len() + 1),
                display_name,
                source_path,
                table: snapshot,
            });
        }

        let export_dir = {
            let raw = self.settings.chat_export_dir.trim();
            if raw.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(raw))
            }
        };

        ToolContext {
            open_tabs,
            active_tab: active_index,
            // Cap how many rows a tool result puts into the model's context.
            // User-configurable (Settings > Chat); the Unlimited checkbox sends
            // `None` (no cap).
            default_row_limit: if self.settings.chat_result_row_limit_unlimited {
                None
            } else {
                Some(self.settings.chat_result_row_limit)
            },
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
            read_only: !allow_writes,
        }
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

        for batch in batches {
            // Map the handle (#N) to a live non-chart tab, same numbering as
            // build_tool_context.
            let tab_idx = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, t)| !t.is_chart_tab)
                .enumerate()
                .find(|(pos, _)| format!("#{}", pos + 1) == batch.tab_handle)
                .map(|(_, (i, _))| i);
            let Some(tab_idx) = tab_idx else {
                self.status_message = Some((
                    format!(
                        "Assistant edit skipped: tab {} is no longer open",
                        batch.tab_handle
                    ),
                    std::time::Instant::now(),
                ));
                continue;
            };
            if self.is_readonly() || !chat_profiles::active_profile(&self.settings).allow_writes {
                self.status_message = Some((
                    "Assistant edit skipped: editing is currently disabled".to_string(),
                    std::time::Instant::now(),
                ));
                continue;
            }
            let tab = &mut self.tabs[tab_idx];
            if tab.table.row_count() != batch.snapshot_rows {
                self.status_message = Some((
                    "Assistant edit skipped: the table changed while the assistant was working"
                        .to_string(),
                    std::time::Instant::now(),
                ));
                continue;
            }

            let start = tab.table.undo_stack.len();
            for op in &batch.ops {
                match op {
                    crate::mcp::tools::ResolvedOp::AddColumn {
                        name,
                        type_name,
                        values,
                    } => {
                        let idx = tab.table.col_count();
                        tab.table
                            .insert_column(idx, name.clone(), type_name.clone());
                        for (r, v) in values.iter().enumerate() {
                            if r < tab.table.row_count() {
                                tab.table.set(r, idx, v.clone());
                            }
                        }
                    }
                    crate::mcp::tools::ResolvedOp::InsertRows { at, rows } => {
                        for row in rows {
                            let at_i = at
                                .unwrap_or_else(|| tab.table.row_count())
                                .min(tab.table.row_count());
                            tab.table.insert_row(at_i);
                            for (c, v) in row.iter().enumerate() {
                                tab.table.set(at_i, c, v.clone());
                            }
                        }
                    }
                    crate::mcp::tools::ResolvedOp::SetCells(cells) => {
                        for (r, c, v) in cells {
                            tab.table.set(*r, *c, v.clone());
                        }
                    }
                    crate::mcp::tools::ResolvedOp::DeleteRows(idxs) => {
                        let mut sorted = idxs.clone();
                        sorted.sort_unstable();
                        sorted.dedup();
                        for &i in sorted.iter().rev() {
                            if i < tab.table.row_count() {
                                tab.table.delete_row(i);
                            }
                        }
                    }
                    crate::mcp::tools::ResolvedOp::DropColumns(idxs) => {
                        let mut sorted = idxs.clone();
                        sorted.sort_unstable();
                        sorted.dedup();
                        for &c in sorted.iter().rev() {
                            if c < tab.table.col_count() {
                                tab.table.delete_column(c);
                            }
                        }
                    }
                }
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
