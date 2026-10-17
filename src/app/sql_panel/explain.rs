//! Explain: show the plan of the editor's query on the pane's target, without
//! running it. Local plans are fast (DuckDB only plans), so they are made on
//! this thread once `data` is current; when it is not, the copy runs as a
//! background job first and the plan follows when it lands. A server plan
//! goes through the same worker as Run, with its spinner and Cancel.

use super::*;

impl OctaApp {
    /// The panel's **Explain** button and the `ExplainQuery` shortcut: the
    /// marked text, else the whole editor, the same rule Run follows.
    pub(crate) fn explain_query(&mut self, ctx: &egui::Context) {
        let tab = &self.tabs[self.active_tab];
        let query = crate::view_modes::sql::query_to_run(ctx, &tab.sql);
        if query.trim().is_empty() {
            return;
        }
        if tab.sql_target.is_some() {
            self.run_server_query(ctx, query, true);
        } else {
            self.explain_local(query);
        }
    }

    fn explain_local(&mut self, query: String) {
        let tab = &mut self.tabs[self.active_tab];
        // A copy of `data` or a query is still running on the workspace.
        if tab.sql_job.is_some() {
            tab.sql.error = Some(octa::i18n::t("sql.busy"));
            return;
        }
        // `data` must hold the live edits, exactly as for a Run. Copying them
        // is a job; the plan is made when it lands (`land_sql_job`).
        ensure_workspace(tab);
        if tab.large.is_none() && super::job::data_is_stale(tab) {
            let pane_id = tab.sql.id;
            tab.sql.start_run();
            let idx = self.active_tab;
            self.start_sql_job(idx, None);
            let tab = &mut self.tabs[idx];
            match tab.sql_job.as_mut() {
                Some(job) => job.then_explain = Some((pane_id, query)),
                // The copy could not start; its reason is in the pane.
                None => tab.sql.running_since = None,
            }
            return;
        }
        let registered = super::register_active_data(tab);
        // No workspace: `ensure_workspace` has put the reason in the pane.
        let Some(ws) = tab.sql_workspace.as_mut() else {
            return;
        };
        let planned = registered.and_then(|()| ws.explain(&query));
        tab.sql.start_run();
        show_planned(&mut tab.sql, planned);
    }
}

/// Put an Explain outcome into `pane`.
pub(super) fn show_planned(
    pane: &mut crate::app::state::SqlPane,
    planned: anyhow::Result<octa::sql::explain::Plan>,
) {
    match planned {
        Ok(plan) => pane.show_plan(plan),
        Err(e) => {
            pane.running_since = None;
            pane.error = Some(format!("{e:#}"));
        }
    }
}
