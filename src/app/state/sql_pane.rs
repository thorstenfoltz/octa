//! One SQL editor with its own query, result and run state. A tab's SQL
//! panel shows any number of them side by side; the workspace, the run
//! target and the Ask box stay shared per tab.
//!
//! The active pane lives in `TabState.sql`, so every path that works on "the"
//! editor (Run, Format, shortcuts, the write-back dialog, Ask) reads one plain
//! field and the borrow checker sees it as disjoint from the rest of the tab.
//! The others wait in `TabState.sql_panes`, whose slot at `sql_active_pane`
//! holds a hollow placeholder while its pane is out in `sql`. The panel draws
//! pane `i` by swapping it in, drawing the unchanged single-editor code, and
//! swapping back.

use std::sync::atomic::{AtomicU64, Ordering};

use octa::data::DataTable;

use super::TabState;

/// Stable identity of a pane. Keys its egui widgets (the editor's caret must
/// not jump to another pane when the order changes) and lets a finished
/// server query find the pane it was started from after panes were closed.
/// 0 is the placeholder; real panes start at 1.
static NEXT_PANE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(crate) struct SqlPane {
    pub(crate) id: u64,
    pub(crate) query: String,
    pub(crate) result: Option<DataTable>,
    pub(crate) error: Option<String>,
    /// Selected cells, rows and columns in the result grid: highlighted and
    /// used as the Ctrl+C copy target.
    pub(crate) result_sel: crate::view_modes::sql::SqlResultSelection,
    /// Rows the last result has in total, exact, from a `count(*)` over the
    /// materialised result. `result` holds only the pages scrolled to so far;
    /// `None` means the statement was not paged and `result` is everything.
    pub(crate) result_total: Option<usize>,
    /// The editor grabs keyboard focus on the next frame, so the user can
    /// type without clicking first. Consumed by `draw_sql_editor`.
    pub(crate) focus_pending: bool,
    /// Autocomplete popup: highlighted suggestion (clamped every frame).
    pub(crate) ac_selected: usize,
    /// Autocomplete popup: `false` after Escape until the user types again.
    pub(crate) ac_visible: bool,
    /// Last successfully executed SELECT, verbatim, so the write-back dialog
    /// has a source query to compose `CREATE TABLE AS ...` from.
    pub(crate) last_query: String,
    /// How long the last query took, shown beside the row count.
    pub(crate) last_duration_ms: Option<u64>,
    /// Set when a query starts, cleared when it ends in any way. While set,
    /// the result area shows a running state instead of the previous result,
    /// which otherwise looked like the answer to the new query.
    pub(crate) running_since: Option<std::time::Instant>,
    /// A local query parked until the running state has been painted once:
    /// the (synchronous) DuckDB call blocks the window, so it must not start
    /// in the frame that is about to show that state. Carries the frame it
    /// was parked in.
    pub(crate) pending_run: Option<(String, u64)>,
}

impl SqlPane {
    pub(crate) fn new() -> Self {
        Self {
            id: NEXT_PANE_ID.fetch_add(1, Ordering::Relaxed),
            ac_visible: true,
            ..Default::default()
        }
    }

    /// The run is starting: drop what the last one showed.
    pub(crate) fn start_run(&mut self) {
        self.result = None;
        self.result_total = None;
        self.result_sel = Default::default();
        self.error = None;
        self.running_since = Some(std::time::Instant::now());
    }
}

impl TabState {
    pub(crate) fn sql_pane_count(&self) -> usize {
        self.sql_panes.len()
    }

    /// Make pane `i` the one in `self.sql`.
    pub(crate) fn activate_sql_pane(&mut self, i: usize) {
        let active = self.sql_active_pane;
        if i == active || i >= self.sql_panes.len() {
            return;
        }
        // Put the active pane back into its slot, then take pane `i` out.
        std::mem::swap(&mut self.sql, &mut self.sql_panes[active]);
        std::mem::swap(&mut self.sql, &mut self.sql_panes[i]);
        self.sql_active_pane = i;
    }

    /// A new, empty editor to the right of the others, focused.
    pub(crate) fn add_sql_pane(&mut self) {
        self.sql_panes.push(SqlPane::new());
        self.activate_sql_pane(self.sql_panes.len() - 1);
        self.sql.focus_pending = true;
    }

    /// Remove pane `i`. The last remaining pane stays.
    pub(crate) fn close_sql_pane(&mut self, i: usize) {
        let n = self.sql_panes.len();
        if n <= 1 || i >= n {
            return;
        }
        if i == self.sql_active_pane {
            self.activate_sql_pane(if i == 0 { 1 } else { i - 1 });
        }
        let closed = self.sql_panes.remove(i);
        if let Some(ws) = self.sql_workspace.as_ref() {
            ws.drop_result(closed.id);
        }
        if self.sql_active_pane > i {
            self.sql_active_pane -= 1;
        }
    }

    /// Id of the pane at position `i`.
    pub(crate) fn sql_pane_id(&self, i: usize) -> u64 {
        if i == self.sql_active_pane {
            self.sql.id
        } else {
            self.sql_panes[i].id
        }
    }

    pub(crate) fn has_sql_pane(&self, id: u64) -> bool {
        id != 0 && (self.sql.id == id || self.sql_panes.iter().any(|p| p.id == id))
    }

    /// The pane with this id, wherever it sits.
    pub(crate) fn sql_pane_by_id_mut(&mut self, id: u64) -> Option<&mut SqlPane> {
        if self.sql.id == id {
            return Some(&mut self.sql);
        }
        self.sql_panes.iter_mut().find(|p| p.id == id && id != 0)
    }

    /// Every editor's text is blank.
    pub(crate) fn sql_editors_empty(&self) -> bool {
        self.sql.query.trim().is_empty() && self.sql_panes.iter().all(|p| p.query.trim().is_empty())
    }

    /// Each editor's text, left to right.
    pub(crate) fn sql_queries(&self) -> Vec<String> {
        (0..self.sql_panes.len())
            .map(|i| {
                if i == self.sql_active_pane {
                    self.sql.query.clone()
                } else {
                    self.sql_panes[i].query.clone()
                }
            })
            .collect()
    }

    /// Take over the queries of a closed tab's editors: only when every
    /// editor here is blank (never overwrite what the user typed), and only
    /// the non-blank ones.
    pub(crate) fn adopt_sql_queries(&mut self, queries: Vec<String>) {
        let queries: Vec<String> = queries
            .into_iter()
            .filter(|q| !q.trim().is_empty())
            .collect();
        if self.sql_editors_empty() && !queries.is_empty() {
            self.set_sql_queries(queries);
        }
    }

    /// Replace the editors with one per query (at least one), first active.
    pub(crate) fn set_sql_queries(&mut self, queries: Vec<String>) {
        let mut panes: Vec<SqlPane> = queries
            .into_iter()
            .map(|q| SqlPane {
                query: q,
                ..SqlPane::new()
            })
            .collect();
        if panes.is_empty() {
            panes.push(SqlPane::new());
        }
        self.sql = std::mem::take(&mut panes[0]);
        self.sql_panes = panes;
        self.sql_active_pane = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab() -> TabState {
        TabState::new(Default::default())
    }

    fn typed(tab: &mut TabState, texts: &[&str]) {
        tab.set_sql_queries(texts.iter().map(|t| t.to_string()).collect());
    }

    #[test]
    fn activating_swaps_panes_without_losing_text() {
        let mut t = tab();
        typed(&mut t, &["a", "b", "c"]);
        t.activate_sql_pane(2);
        assert_eq!(t.sql.query, "c");
        t.activate_sql_pane(0);
        assert_eq!(t.sql.query, "a");
        assert_eq!(t.sql_queries(), ["a", "b", "c"]);
    }

    #[test]
    fn closing_the_active_pane_activates_a_neighbour_and_keeps_the_rest() {
        let mut t = tab();
        typed(&mut t, &["a", "b", "c"]);
        t.activate_sql_pane(1);
        t.close_sql_pane(1);
        assert_eq!(t.sql_queries(), ["a", "c"]);
        assert_eq!(t.sql.query, "a");
        // Closing one left of the active pane shifts its index.
        t.activate_sql_pane(1);
        t.close_sql_pane(0);
        assert_eq!(t.sql_active_pane, 0);
        assert_eq!(t.sql.query, "c");
        // The last pane stays.
        t.close_sql_pane(0);
        assert_eq!(t.sql_pane_count(), 1);
    }

    #[test]
    fn a_pane_is_found_by_id_wherever_it_sits() {
        let mut t = tab();
        typed(&mut t, &["a", "b"]);
        let id_b = t.sql_pane_id(1);
        t.sql_pane_by_id_mut(id_b).expect("b").query.push('!');
        assert_eq!(t.sql_queries(), ["a", "b!"]);
        assert!(t.has_sql_pane(id_b));
        assert!(!t.has_sql_pane(0), "the placeholder is never a pane");
    }

    #[test]
    fn a_closed_tabs_queries_land_only_in_blank_editors() {
        let mut t = tab();
        t.adopt_sql_queries(vec!["select 1".into(), "  ".into(), "select 2".into()]);
        assert_eq!(t.sql_queries(), ["select 1", "select 2"]);

        let mut busy = tab();
        busy.sql.query = "mine".into();
        busy.adopt_sql_queries(vec!["select 1".into()]);
        assert_eq!(busy.sql_queries(), ["mine"]);
    }

    #[test]
    fn starting_a_run_clears_what_the_last_one_showed() {
        let mut p = SqlPane::new();
        p.error = Some("boom".into());
        p.result_total = Some(3);
        p.start_run();
        assert!(p.error.is_none() && p.result.is_none() && p.result_total.is_none());
        assert!(p.running_since.is_some());
    }
}
