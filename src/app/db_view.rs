//! A partial database tab sorts and filters on the server: its sort, value
//! filters, comparison chips and search box become the `ORDER BY` and
//! `WHERE` of the query that pages it (`octa::db::pushdown::view`). This
//! file decides which parts go there and which stay on the loaded rows,
//! keeps the tab in step, and asks before a re-query drops unsaved edits.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use eframe::egui;

use octa::data::SearchMode;
use octa::data::predicate_filter::PredicateFilter;
use octa::data::value_frequency::ValueFrequency;
use octa::db::DbConnection;
use octa::db::pushdown::ServerSource;
use octa::db::pushdown::view::{ServerView, SortKey, ViewFilter};
use octa::i18n::t;

use super::pushdown::{ServerTask, TaskPoll};
use super::state::{DbOrigin, OctaApp, TabState};

/// The whole table `o` names, unfiltered.
pub(crate) fn origin_source(conn: &DbConnection, o: &DbOrigin) -> ServerSource {
    ServerSource {
        conn: conn.clone(),
        catalog: o.catalog.clone(),
        schema: o.schema.clone(),
        table: o.table.clone(),
        filter: None,
        derived: Vec::new(),
    }
}

/// Whether `tab` sorts and filters on the server: a live-database tab (not
/// in large-file mode), the setting on, its connection saved, and either
/// rows it does not hold or a view already applied. The second half matters:
/// once a filter narrows the result until every row fits, the next change
/// must still go to the server, not to the few rows that happen to be here.
pub(crate) fn view_source_for(
    tab: &TabState,
    pushdown_on: bool,
    conns: &[DbConnection],
) -> Option<ServerSource> {
    let conn = server_conn(tab, pushdown_on, conns)?;
    Some(origin_source(conn, tab.db_origin.as_ref()?))
}

/// [`view_source_for`]'s test without building the source, for callers that
/// ask every frame.
pub(crate) fn server_conn<'a>(
    tab: &TabState,
    pushdown_on: bool,
    conns: &'a [DbConnection],
) -> Option<&'a DbConnection> {
    if !pushdown_on || tab.large.is_some() {
        return None;
    }
    let o = tab.db_origin.as_ref()?;
    if !(tab.source_has_more() || tab.server_view.is_some() || tab.view_task.is_some()) {
        return None;
    }
    conns.iter().find(|c| c.id == o.conn_id)
}

/// The search box hides rows: there is text and the tab is in Filter mode.
pub(crate) fn search_hides_rows(tab: &TabState, mode: octa::data::SearchResultMode) -> bool {
    !tab.search_text.is_empty() && !super::state::effective_highlight(tab.view_mode, mode)
}

/// The parts of a tab's filters the server cannot apply exactly. They keep
/// filtering the loaded rows, and the banner names them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LocalLeftovers {
    /// The search box: Regex mode, or whole-word matching.
    pub(crate) search: bool,
    /// Indices into `predicate_filters`.
    pub(crate) predicates: Vec<usize>,
}

/// What the tab asks for now: the server's part, and what stays local.
pub(crate) fn wanted_view(tab: &TabState, search_hides: bool) -> (ServerView, LocalLeftovers) {
    let cols = &tab.table.columns;
    let mut view = ServerView {
        order: tab.server_sort.clone(),
        filters: Vec::new(),
        derived: tab.server_hashes.clone(),
    };
    let mut local = LocalLeftovers::default();
    // By column name, so the same settings always give an equal view: moving
    // a column changes no filter, and must not re-query.
    let mut keyed: Vec<(&str, &HashSet<String>)> = tab
        .column_filters
        .iter()
        .filter_map(|(&c, allowed)| cols.get(c).map(|ci| (ci.name.as_str(), allowed)))
        .collect();
    keyed.sort_by_key(|(name, _)| *name);
    for (name, allowed) in keyed {
        view.filters
            .push(ViewFilter::values(name, allowed.iter().cloned()));
    }
    for (i, p) in tab.predicate_filters.iter().enumerate() {
        let f = cols.get(p.col).and_then(|ci| {
            ViewFilter::compare(&ci.name, &ci.data_type, p.op, &p.value, p.case_sensitive)
        });
        match f {
            Some(f) => view.filters.push(f),
            None => local.predicates.push(i),
        }
    }
    if search_hides {
        // As `recompute_filter`: a scope past the last column searches all.
        let columns: Vec<String> = match tab.search_scope_col.and_then(|i| cols.get(i)) {
            Some(c) => vec![c.name.clone()],
            // By name, not display order: see the value filters above.
            None => {
                let mut all: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
                all.sort();
                all
            }
        };
        let case_sensitive = tab.search_case_sensitive;
        match (tab.search_mode, tab.search_whole_word) {
            (SearchMode::Plain, false) => view.filters.push(ViewFilter::Contains {
                columns,
                needle: tab.search_text.clone(),
                case_sensitive,
            }),
            (SearchMode::Wildcard, false) => view.filters.push(ViewFilter::Wildcard {
                columns,
                pattern: tab.search_text.clone(),
                case_sensitive,
            }),
            _ => local.search = true,
        }
    }
    (view, local)
}

/// `Some(leftovers)` when the server applies `tab`'s filters (the tab is on
/// the server and the user has not chosen the loaded rows), so only the
/// leftovers filter locally; `None` filters everything locally, as today.
pub(crate) fn server_leftovers(
    tab: &TabState,
    pushdown_on: bool,
    conns: &[DbConnection],
    mode: octa::data::SearchResultMode,
) -> Option<LocalLeftovers> {
    // A refusal on show: the chips say filtered, so the loaded rows are, until
    // the user picks Try again.
    if tab.view_hold.is_some()
        || tab.view_error.is_some()
        || server_conn(tab, pushdown_on, conns).is_none()
    {
        return None;
    }
    Some(wanted_view(tab, search_hides_rows(tab, mode)).1)
}

/// The user's answer for one view while the tab has unsaved edits.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ViewHold {
    /// "Only the loaded rows": sort and filter what is loaded while the tab
    /// asks for exactly this view.
    LoadedRows(ServerView),
    /// "Save first": the write-back is under way; query once it lands.
    AwaitSave(ServerView),
}

/// The settings a view is made from, as they were when it was applied.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ViewUiSnapshot {
    pub(crate) column_filters: HashMap<usize, HashSet<String>>,
    pub(crate) predicate_filters: Vec<PredicateFilter>,
    pub(crate) search_text: String,
    pub(crate) search_mode: octa::data::SearchMode,
    pub(crate) search_case_sensitive: bool,
    pub(crate) search_whole_word: bool,
    pub(crate) search_scope_col: Option<usize>,
    pub(crate) server_sort: Vec<SortKey>,
}

impl ViewUiSnapshot {
    pub(crate) fn of(tab: &TabState) -> Self {
        Self {
            column_filters: tab.column_filters.clone(),
            predicate_filters: tab.predicate_filters.clone(),
            search_text: tab.search_text.clone(),
            search_mode: tab.search_mode,
            search_case_sensitive: tab.search_case_sensitive,
            search_whole_word: tab.search_whole_word,
            search_scope_col: tab.search_scope_col,
            server_sort: tab.server_sort.clone(),
        }
    }

    /// Follow a column insert, delete or move (`remap`: old index to new), as
    /// `sync_column_keys` does for the live settings. What pointed at a
    /// deleted column goes.
    pub(crate) fn remap(&mut self, remap: &HashMap<usize, usize>) {
        self.column_filters = std::mem::take(&mut self.column_filters)
            .into_iter()
            .filter_map(|(c, v)| remap.get(&c).map(|&n| (n, v)))
            .collect();
        self.predicate_filters
            .retain_mut(|p| match remap.get(&p.col) {
                Some(&n) => {
                    p.col = n;
                    true
                }
                None => false,
            });
        self.search_scope_col = self.search_scope_col.and_then(|c| remap.get(&c).copied());
    }

    pub(crate) fn restore(&self, tab: &mut TabState) {
        tab.column_filters = self.column_filters.clone();
        tab.predicate_filters = self.predicate_filters.clone();
        tab.search_text = self.search_text.clone();
        tab.search_mode = self.search_mode;
        tab.search_case_sensitive = self.search_case_sensitive;
        tab.search_whole_word = self.search_whole_word;
        tab.search_scope_col = self.search_scope_col;
        tab.server_sort = self.server_sort.clone();
        tab.filter_dirty = true;
    }
}

/// `page`'s rows in the order of the tab's `columns`, matched by name: the
/// server answers `SELECT *` in its own column order, while a column moved in
/// the tab keeps its place there. A column the page lacks reads as Null.
pub(crate) fn rows_in_column_order(
    page: octa::data::DataTable,
    columns: &[String],
) -> Vec<Vec<octa::data::CellValue>> {
    let at: Vec<Option<usize>> = columns
        .iter()
        .map(|n| page.columns.iter().position(|c| &c.name == n))
        .collect();
    if page.columns.len() == columns.len() && at.iter().enumerate().all(|(i, j)| *j == Some(i)) {
        return page.rows;
    }
    page.rows
        .into_iter()
        .map(|mut row| {
            at.iter()
                .map(|j| {
                    j.and_then(|j| row.get_mut(j))
                        .map(|c| std::mem::replace(c, octa::data::CellValue::Null))
                        .unwrap_or(octa::data::CellValue::Null)
                })
                .collect()
        })
        .collect()
}

/// The names of `tab`'s columns, in the tab's order.
pub(crate) fn column_names(tab: &TabState) -> Vec<String> {
    tab.table.columns.iter().map(|c| c.name.clone()).collect()
}

/// The page of `view` starting at `offset`. The table's key, when it has
/// one, breaks ties so a row cannot move from one page to the next.
pub(crate) fn view_page_sql(
    src: &ServerSource,
    identity: Option<&octa::db::write_back::RowIdentity>,
    view: &ServerView,
    limit: usize,
    offset: usize,
) -> String {
    let tie: Vec<String> = match identity {
        Some(octa::db::write_back::RowIdentity::Key(cols)) => cols.clone(),
        _ => Vec::new(),
    };
    let from = view.from_item(src.engine(), &src.table_sql());
    octa::db::pushdown::view::page_sql(src.engine(), view, &from, &tie, limit, offset)
}

impl TabState {
    /// The first page of `view`, the server's answer to the tab's sort and
    /// filters, replaces the loaded rows. What pointed at the old rows goes
    /// with them: row and cell marks, bookmarks, outlier and kept-as-text
    /// cells, the undo history, the selection and an open editor, row
    /// heights, the duplicate and timeline caches, the scroll position.
    /// (The edits prompt made sure nothing unsaved is lost, and the tab is
    /// read-only while the query runs.)
    pub(crate) fn apply_view_page(
        &mut self,
        view: ServerView,
        page: octa::data::DataTable,
        page_rows: usize,
    ) {
        let writable = self.table.db_meta.is_some();
        self.table.rows = rows_in_column_order(page, &column_names(self));
        self.table.row_offset = 0;
        self.table.edits.clear();
        self.table.undo_stack.clear();
        self.table.redo_stack.clear();
        // The recipe's steps point into the undo history just cleared: start
        // over at its new bottom, or the next hand edits go unrecorded.
        for step in &mut self.recipe {
            step.undo_mark = 0;
        }
        self.recipe_undone.clear();
        self.recipe_seen = 0;
        // A whole-table download of the old view ends with it.
        self.loading_all = false;
        self.table
            .marks
            .retain(|k, _| matches!(k, octa::data::MarkKey::Column(_)));
        self.table.clear_modified();
        // A full page may have more behind it: the "more may exist" flag the
        // status bar and the scroll path read.
        let n = self.table.rows.len();
        self.table.total_rows = (n >= page_rows).then_some(n);
        if writable && let Some(o) = &self.db_origin {
            let (name, schema) = (o.table.clone(), o.schema.clone());
            super::db_browser::baseline_db_meta(&mut self.table, &name, &schema);
        }
        // Fresh flags: a worker of the old view must not reach the new rows.
        self.bg_can_load_more = self.table.total_rows.is_some();
        self.bg_row_buffer = None;
        self.bg_loading_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.bg_file_exhausted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.server_view = (!view.is_empty()).then_some(view);
        // The settings may have moved on while the query ran; the sync takes
        // a snapshot once they match the applied view again.
        self.view_ui = None;
        self.view_error = None;
        self.view_hold = None;
        self.view_settle = None;
        self.table_state.selected_cell = None;
        self.table_state.selected_cells.clear();
        self.table_state.selected_rows.clear();
        self.table_state.selection_anchor_display = None;
        self.table_state.editing_cell = None;
        self.table_state.row_heights.clear();
        self.table_state.invalidate_row_heights();
        self.table_state.set_scroll_y(0.0);
        self.bookmarks.clear();
        self.outlier_cells.clear();
        self.retype_kept_as_text.clear();
        // Both key on (row count, edits, ...), which a same-size page matches.
        self.duplicate_filter_cache = None;
        self.timeline.forget_built();
        self.filter_dirty = true;
    }
}

/// How long the search box must stay unchanged before its text is sent.
pub(crate) const SETTLE: Duration = Duration::from_millis(400);

/// The per-frame sync's next step for a tab on the server.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SyncStep {
    /// In step, busy, or waiting for the user.
    Idle,
    /// The search text may still be changing: look again after this long.
    Settle(Duration),
    /// A re-query would drop unsaved edits: ask.
    Ask(ServerView),
    /// Fetch this view.
    Query(ServerView),
}

/// The search filters of a view: the only part typing changes.
fn search_part(v: &ServerView) -> Vec<&ViewFilter> {
    v.filters
        .iter()
        .filter(|f| matches!(f, ViewFilter::Contains { .. } | ViewFilter::Wildcard { .. }))
        .collect()
}

/// Whether the rows loaded are `wanted`'s, without cloning the view.
fn applied_is(tab: &TabState, wanted: &ServerView) -> bool {
    tab.server_view
        .as_ref()
        .map_or(wanted.is_empty(), |v| v == wanted)
}

/// How much longer the search text must stay as it is before `wanted` goes
/// out; `None` when it has settled or the search did not change.
fn settle_left(tab: &TabState, wanted: &ServerView, now: Instant) -> Option<Duration> {
    let applied: Vec<&ViewFilter> = tab
        .server_view
        .as_ref()
        .map(search_part)
        .unwrap_or_default();
    if search_part(wanted) == applied {
        return None;
    }
    match &tab.view_settle {
        Some((v, since)) if v == wanted => SETTLE
            .checked_sub(now.saturating_duration_since(*since))
            .filter(|d| !d.is_zero()),
        _ => Some(SETTLE),
    }
}

/// A re-query, a page or a whole-table download is under way, or a refusal
/// waits for the user's answer: the sync leaves the tab alone.
fn sync_waits(tab: &TabState) -> bool {
    let page_in_flight = tab.bg_row_buffer.is_some()
        && !tab
            .bg_loading_done
            .load(std::sync::atomic::Ordering::Relaxed);
    tab.view_task.is_some() || tab.view_error.is_some() || tab.loading_all || page_in_flight
}

/// Decide the sync's next step. `unsaved`: a re-query would lose something;
/// `saving`: a write-back is pending or running. Pure apart from `tab`'s own
/// bookkeeping, so the whole state machine is tested without an app.
pub(crate) fn sync_step(
    tab: &mut TabState,
    wanted: ServerView,
    unsaved: bool,
    saving: bool,
    now: Instant,
) -> SyncStep {
    if sync_waits(tab) {
        return SyncStep::Idle;
    }
    if applied_is(tab, &wanted) {
        // The settings behind what is loaded, for Cancel to return to; kept
        // current as local-only settings (highlight search, leftover
        // predicates) change.
        if tab.view_ui.is_none() || tab.filter_dirty {
            tab.view_ui = Some(ViewUiSnapshot::of(tab));
        }
        tab.view_settle = None;
        tab.view_hold = None;
        return SyncStep::Idle;
    }
    // A write-back is pending or running: replacing the rows now would drop
    // the edits it is about to write (or that its Cancel keeps).
    if saving {
        return SyncStep::Idle;
    }
    match tab.view_hold.clone() {
        Some(ViewHold::LoadedRows(v)) if v == wanted => return SyncStep::Idle,
        Some(ViewHold::AwaitSave(v)) if v == wanted && unsaved => {
            // The save was cancelled or failed: keep the loaded rows
            // rather than asking again every frame.
            tab.hold_loaded_rows(wanted);
            return SyncStep::Idle;
        }
        _ => {}
    }
    if let Some(wait) = settle_left(tab, &wanted, now) {
        if !matches!(&tab.view_settle, Some((v, _)) if *v == wanted) {
            tab.view_settle = Some((wanted, now));
        }
        return SyncStep::Settle(wait);
    }
    if unsaved {
        return SyncStep::Ask(wanted);
    }
    SyncStep::Query(wanted)
}

impl TabState {
    /// "Only the loaded rows" (and the banner's "Use the loaded rows"):
    /// filter and sort what is loaded while the tab asks for `wanted`.
    /// Filtering follows from the hold (`server_leftovers` is `None`); the
    /// sort is applied here, once, as an ordinary local sort.
    pub(crate) fn hold_loaded_rows(&mut self, wanted: ServerView) {
        let applied = self
            .server_view
            .as_ref()
            .map(|v| v.order.clone())
            .unwrap_or_default();
        if !wanted.order.is_empty() && wanted.order != applied {
            let keys: Vec<(usize, bool)> = wanted
                .order
                .iter()
                .filter_map(|k| {
                    self.table
                        .columns
                        .iter()
                        .position(|c| c.name == k.column)
                        .map(|i| (i, k.ascending))
                })
                .collect();
            self.table.sort_rows_by_columns(&keys);
        }
        self.view_hold = Some(ViewHold::LoadedRows(wanted));
        self.view_error = None;
        self.filter_dirty = true;
    }

    /// Cancel in the edits prompt: back to the settings behind the loaded
    /// rows. Without a snapshot there is nothing to go back to, so the
    /// loaded rows are kept instead.
    pub(crate) fn cancel_view_change(&mut self, wanted: ServerView) {
        match self.view_ui.clone() {
            Some(snap) => snap.restore(self),
            None => self.hold_loaded_rows(wanted),
        }
    }
}

/// The edits question, for one tab and the view it asked for.
pub(crate) struct ViewEditPrompt {
    pub(crate) tab: usize,
    pub(crate) wanted: ServerView,
    /// The edits can be written back from this tab.
    pub(crate) can_save: bool,
}

impl OctaApp {
    /// Whether replacing tab `idx`'s rows would lose something: an edit not
    /// yet merged, or merged rows the database has not seen. A local sort
    /// alone (`structural_changes`) loses nothing.
    pub(crate) fn has_unsaved_rows(&self, idx: usize) -> bool {
        let Some(tab) = self.tabs.get(idx) else {
            return false;
        };
        if !tab.table.is_modified() {
            return false;
        }
        if !tab.table.edits.is_empty() {
            return true;
        }
        match self.db_plan_for_tab(idx) {
            Ok(plan) => plan.is_some(),
            // Work the write-back refuses (a column dropped or renamed, a
            // SQL panel rewrite, a NULL key) is still the user's: ask, with
            // Save first greyed out.
            Err(_) => tab
                .db_origin
                .as_ref()
                .is_some_and(|o| self.db_origin_writable(o)),
        }
    }

    /// Once per frame: take in every tab's finished re-query, then, for the
    /// active tab, send the next one when its sort or filters changed.
    pub(crate) fn sync_db_view(&mut self, ctx: &egui::Context) {
        if let Some(msg) = self
            .tabs
            .get_mut(self.active_tab)
            .and_then(TabState::close_hash_editor)
        {
            self.status_message = Some((msg, Instant::now()));
        }
        self.poll_values(ctx);
        for i in 0..self.tabs.len() {
            self.poll_view_task(i, ctx);
        }
        let idx = self.active_tab;
        if self.view_edit_prompt.is_some() {
            return;
        }
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        // Busy, or a refusal waits for the user's answer (retrying it every
        // frame would hammer the server).
        if sync_waits(tab) {
            return;
        }
        // Switched off with a view applied: back to the plain table, as if the
        // setting had never been on. Not over local changes, nor once the
        // user chose to keep the loaded rows: the tab then keeps paging its
        // view. `is_modified`, not the write-back plan: this runs every frame.
        if !self.settings.db_pushdown && tab.server_view.is_some() {
            if tab.table.is_modified() || tab.view_hold.is_some() {
                return;
            }
            let plain = tab.db_origin.as_ref().and_then(|o| {
                self.settings
                    .db_connections
                    .iter()
                    .find(|c| c.id == o.conn_id)
                    .map(|conn| origin_source(conn, o))
            });
            if let Some(src) = plain {
                // The hash columns go with the view: what stays is an empty
                // column of the user's, still never written back.
                self.tabs[idx].server_hashes.clear();
                self.start_view_query(idx, src, ServerView::default());
            }
            return;
        }
        if server_conn(
            tab,
            self.settings.db_pushdown,
            &self.settings.db_connections,
        )
        .is_none()
        {
            return;
        }
        let (wanted, _) = wanted_view(tab, search_hides_rows(tab, self.search_result_mode));
        let saving = self.pending_db_write_back.is_some() || self.db_write_back_job.is_some();
        // Diffing the edits clones the table: only when an answer is needed,
        // not every frame a kept view stands or the search text settles.
        let now = Instant::now();
        let held = matches!(&tab.view_hold, Some(ViewHold::LoadedRows(v)) if *v == wanted);
        let unsaved = !applied_is(tab, &wanted)
            && !saving
            && !held
            && settle_left(tab, &wanted, now).is_none()
            && self.has_unsaved_rows(idx);
        match sync_step(&mut self.tabs[idx], wanted, unsaved, saving, now) {
            SyncStep::Idle => {}
            SyncStep::Settle(wait) => ctx.request_repaint_after(wait),
            SyncStep::Ask(wanted) => {
                let can_save = matches!(self.db_plan_for_tab(idx), Ok(Some(_)));
                self.view_edit_prompt = Some(ViewEditPrompt {
                    tab: idx,
                    wanted,
                    can_save,
                });
            }
            SyncStep::Query(wanted) => {
                if let Some(src) = view_source_for(
                    &self.tabs[idx],
                    self.settings.db_pushdown,
                    &self.settings.db_connections,
                ) {
                    self.start_view_query(idx, src, wanted);
                }
            }
        }
    }

    fn poll_view_task(&mut self, idx: usize, ctx: &egui::Context) {
        let page_rows = self.settings.db_page_size();
        let on_screen = idx == self.active_tab;
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let Some((_, task)) = &tab.view_task else {
            return;
        };
        match task.poll() {
            // Nothing wakes the UI when the worker ends: keep looking, less
            // often for a tab not on screen.
            TaskPoll::Pending if on_screen => ctx.request_repaint(),
            TaskPoll::Pending => ctx.request_repaint_after(Duration::from_millis(250)),
            TaskPoll::Ready(page) => {
                if let Some((view, _)) = tab.view_task.take() {
                    tab.apply_view_page(view, page, page_rows);
                }
            }
            TaskPoll::Cancelled => {
                tab.view_task = None;
                tab.view_error = Some(t("pushdown.cancelled"));
            }
            TaskPoll::Failed(e) => {
                tab.view_task = None;
                tab.view_error = Some(e);
            }
        }
    }

    fn start_view_query(&mut self, idx: usize, src: ServerSource, wanted: ServerView) {
        let page_rows = self.settings.db_page_size();
        let identity = self.tabs[idx]
            .db_origin
            .as_ref()
            .and_then(|o| o.identity.clone());
        let sql = view_page_sql(&src, identity.as_ref(), &wanted, page_rows, 0);
        let label = self.tabs[idx].title_display();
        let task = self.spawn_server_task(src.conn.clone(), label, move |c, _| c.query(&sql));
        self.tabs[idx].view_task = Some((wanted, task));
    }
}

/// The edits question: Save first / Only the loaded rows / Cancel.
pub(crate) fn render_view_edit_prompt(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(prompt) = app.view_edit_prompt.as_ref() else {
        return;
    };
    let idx = prompt.tab;
    if idx >= app.tabs.len() {
        app.view_edit_prompt = None;
        return;
    }
    let loaded = octa::ui::status_bar::format_number(app.tabs[idx].table.rows.len());
    let can_save = prompt.can_save;
    #[derive(Clone, Copy)]
    enum Answer {
        Save,
        Loaded,
        Cancel,
    }
    let mut answer: Option<Answer> = None;
    egui::Window::new("octa_view_edits_prompt")
        .id(egui::Id::new("octa_view_edits_prompt"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .default_pos(octa::ui::settings::center_on_first_show(
            ctx,
            egui::vec2(440.0, 180.0),
        ))
        .default_width(440.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(t("dbview.edits_title"))
                    .strong()
                    .size(16.0),
            );
            ui.add_space(6.0);
            ui.label(t("dbview.edits_body"));
            ui.add_space(8.0);
            octa::ui::control_row::control_row(ui, |ui| {
                let save = ui
                    .add_enabled(can_save, egui::Button::new(t("dbview.save_first")))
                    .on_hover_text(t("dbview.save_first_hint"))
                    .on_disabled_hover_text(t("dbview.save_first_disabled_hint"));
                if save.clicked() {
                    answer = Some(Answer::Save);
                }
                if ui
                    .button(t("dbview.loaded_only"))
                    .on_hover_text(t("dbview.loaded_only_hint").replace("{loaded}", &loaded))
                    .clicked()
                {
                    answer = Some(Answer::Loaded);
                }
                if ui
                    .button(t("common.cancel"))
                    .on_hover_text(t("dbview.edits_cancel_hint"))
                    .clicked()
                {
                    answer = Some(Answer::Cancel);
                }
            });
        });
    let Some(answer) = answer else {
        return;
    };
    let Some(prompt) = app.view_edit_prompt.take() else {
        return;
    };
    match answer {
        Answer::Save => {
            app.tabs[idx].view_hold = Some(ViewHold::AwaitSave(prompt.wanted));
            app.begin_db_write_back(idx);
        }
        Answer::Loaded => app.tabs[idx].hold_loaded_rows(prompt.wanted),
        Answer::Cancel => {
            app.tabs[idx].cancel_view_change(prompt.wanted);
            // The Filter/Highlight switch and the view mode are not the tab's
            // to restore: if they still ask for another view, keep the loaded
            // rows rather than asking again on the next frame.
            let tab = &app.tabs[idx];
            let wanted = wanted_view(tab, search_hides_rows(tab, app.search_result_mode)).0;
            if !applied_is(tab, &wanted) && app.tabs[idx].view_hold.is_none() {
                app.tabs[idx].hold_loaded_rows(wanted);
            }
        }
    }
}

impl OctaApp {
    /// Above the grid of a tab on the server: the query running (with
    /// Cancel), its refusal (with Try again / Use the loaded rows), or what
    /// the server applied (with Clear sort), then what stays on the loaded
    /// rows and why.
    pub(crate) fn render_view_banner(&mut self, ui: &mut egui::Ui) {
        let idx = self.active_tab;
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        let on_server = server_conn(
            tab,
            self.settings.db_pushdown,
            &self.settings.db_connections,
        )
        .is_some();
        // With the setting off a view can still be applied, reverting, or
        // refused: say so, with Cancel and Try again.
        if !on_server
            && tab.view_hold.is_none()
            && tab.server_view.is_none()
            && tab.view_task.is_none()
            && tab.view_error.is_none()
        {
            return;
        }
        #[derive(Clone, Copy)]
        enum Act {
            Cancel,
            Retry,
            UseLoaded,
            ClearSort,
        }
        let mut act: Option<Act> = None;
        let colors = octa::ui::theme::ThemeColors::for_mode(self.theme_mode);
        if tab.view_task.is_some() {
            octa::ui::control_row::control_row(ui, |ui| {
                ui.add_space(8.0);
                ui.spinner();
                ui.label(egui::RichText::new(t("dbview.running")).color(colors.text_muted));
                if ui
                    .small_button(t("common.cancel"))
                    .on_hover_text(t("dbview.cancel_hint"))
                    .clicked()
                {
                    act = Some(Act::Cancel);
                }
            });
        } else if let Some(e) = &tab.view_error {
            let cancelled = *e == t("pushdown.cancelled");
            let msg = if cancelled {
                e.clone()
            } else {
                format!("{} {e}", t("dbview.failed"))
            };
            octa::ui::message::selectable_message(ui, ui.visuals().error_fg_color, &msg);
            octa::ui::control_row::control_row(ui, |ui| {
                ui.add_space(8.0);
                if ui
                    .small_button(t("dbview.retry"))
                    .on_hover_text(t("dbview.retry_hint"))
                    .clicked()
                {
                    act = Some(Act::Retry);
                }
                if ui
                    .small_button(t("dbview.use_loaded"))
                    .on_hover_text(t("dbview.use_loaded_hint"))
                    .clicked()
                {
                    act = Some(Act::UseLoaded);
                }
            });
        } else if tab.view_hold.is_none()
            && let Some(view) = &tab.server_view
            && !(view.order.is_empty() && view.filters.is_empty())
        {
            let columns = view
                .order
                .iter()
                .map(|k| k.column.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let line = match (view.order.is_empty(), view.filters.is_empty()) {
                (false, true) => t("dbview.note_sorted"),
                (true, false) => t("dbview.note_filtered"),
                _ => t("dbview.note_both"),
            }
            .replace("{columns}", &columns);
            octa::ui::control_row::control_row(ui, |ui| {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(line)
                        .color(colors.text_muted)
                        .size(12.0),
                );
                if !view.order.is_empty()
                    && ui
                        .small_button(t("dbview.clear_sort"))
                        .on_hover_text(t("dbview.clear_sort_hint"))
                        .clicked()
                {
                    act = Some(Act::ClearSort);
                }
            });
        }
        let mut notes: Vec<String> = Vec::new();
        let applied: Vec<&str> = tab
            .server_view
            .iter()
            .flat_map(|v| v.derived.iter().map(|h| h.name.as_str()))
            .collect();
        if !applied.is_empty() && tab.view_task.is_none() {
            notes.push(t("dbview.note_hash").replace("{columns}", &applied.join(", ")));
        }
        // Kept the loaded rows over a hash the database never computed.
        if let Some(ViewHold::LoadedRows(v)) = &tab.view_hold {
            let missing: Vec<&str> = v
                .derived
                .iter()
                .map(|h| h.name.as_str())
                .filter(|n| !applied.contains(n))
                .collect();
            if !missing.is_empty() {
                notes.push(t("dbview.local_hash").replace("{columns}", &missing.join(", ")));
            }
        }
        if tab.view_hold.is_some() {
            // Kept for unsaved edits, or chosen after the database refused.
            notes.push(t(if tab.table.is_modified() {
                "dbview.local_hold"
            } else {
                "dbview.local_refused"
            }));
        } else if on_server && tab.view_task.is_none() {
            let (_, left) = wanted_view(tab, search_hides_rows(tab, self.search_result_mode));
            if left.search {
                notes.push(t("dbview.local_search"));
            }
            if !left.predicates.is_empty() {
                let names: Vec<String> = left
                    .predicates
                    .iter()
                    .filter_map(|&i| tab.predicate_filters.get(i))
                    .map(|p| p.label(&tab.table))
                    .collect();
                notes.push(t("dbview.local_predicates").replace("{filters}", &names.join(", ")));
            }
            if tab.duplicate_filter.is_some() {
                notes.push(t("dbview.local_duplicates"));
            }
        }
        for note in &notes {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                octa::ui::message::partial_note_label(ui, note);
            });
        }
        let Some(act) = act else {
            return;
        };
        let wanted = wanted_view(
            &self.tabs[idx],
            search_hides_rows(&self.tabs[idx], self.search_result_mode),
        )
        .0;
        let tab = &mut self.tabs[idx];
        match act {
            // Dropping the task cancels the statement on the server.
            Act::Cancel => {
                tab.view_task = None;
                tab.view_error = Some(t("pushdown.cancelled"));
            }
            Act::Retry => tab.view_error = None,
            Act::UseLoaded => tab.hold_loaded_rows(wanted),
            Act::ClearSort => {
                tab.server_sort.clear();
                tab.filter_dirty = true;
            }
        }
    }
}

impl TabState {
    /// Close a cell editor opened on a hash column the database computes
    /// (typing, F2, the record view all open one), with the message why.
    pub(crate) fn close_hash_editor(&mut self) -> Option<String> {
        let (_, c, _) = self.table_state.editing_cell.as_ref()?;
        let name = &self.table.columns.get(*c)?.name;
        if !self.server_hashes.iter().any(|h| &h.name == name) {
            return None;
        }
        let msg = t("hashcols.locked").replace("{name}", name);
        self.table_state.editing_cell = None;
        Some(msg)
    }

    /// The server sort for `keys` (column index, ascending); unknown columns
    /// are dropped. The rows move when the server answers.
    pub(crate) fn sort_on_server(&mut self, keys: &[(usize, bool)]) {
        self.server_sort = keys
            .iter()
            .filter_map(|&(c, ascending)| {
                self.table.columns.get(c).map(|ci| SortKey {
                    column: ci.name.clone(),
                    ascending,
                    text: octa::db::pushdown::view::is_text_type(&ci.data_type),
                })
            })
            .collect();
    }
}

impl OctaApp {
    /// Sort tab `idx` by `keys`: on the server when the tab sorts there (the
    /// order becomes the view's `ORDER BY` and the sync re-queries), else the
    /// loaded rows, as before. The header, the Edit menu, Multi-sort and Ask
    /// all come through here.
    pub(crate) fn sort_tab_rows(&mut self, idx: usize, keys: &[(usize, bool)]) {
        let server = self.tabs.get(idx).is_some_and(|t| {
            view_source_for(t, self.settings.db_pushdown, &self.settings.db_connections).is_some()
        });
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        if server {
            tab.sort_on_server(keys);
        } else {
            tab.table.sort_rows_by_columns(keys);
            // A local sort replaces a server one left from before the setting
            // was switched off, which would otherwise return with it.
            tab.server_sort.clear();
        }
        tab.filter_dirty = true;
    }
}

/// The Column Filter window lists this many of a column's most common values
/// from the server; its own search narrows that list.
pub(crate) const FILTER_WINDOW_TOP_N: usize = 10_000;

/// Which list a server value query fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValuesSlot {
    Popup,
    Window,
}

/// A value list from the server for one `(column, search)`.
pub(crate) struct ServerValues {
    pub(crate) key: (usize, String),
    pub(crate) task: Option<ServerTask<ValueFrequency>>,
    pub(crate) result: Option<Result<ValueFrequency, String>>,
    /// When the key was first asked for: a search waits to settle.
    pub(crate) since: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValuesDue {
    /// A new key with no search: record it and send now.
    SendNew,
    /// A new key with a search: record it and wait for typing to settle.
    Start,
    Wait(Duration),
    /// The search has settled: send.
    Send,
    /// In flight or answered.
    Done,
}

pub(crate) fn values_due(
    slot: Option<&ServerValues>,
    key: &(usize, String),
    now: Instant,
) -> ValuesDue {
    match slot {
        Some(v) if &v.key == key => {
            if v.task.is_some() || v.result.is_some() {
                return ValuesDue::Done;
            }
            let waited = now.saturating_duration_since(v.since);
            if waited < SETTLE {
                ValuesDue::Wait(SETTLE - waited)
            } else {
                ValuesDue::Send
            }
        }
        _ if key.1.trim().is_empty() => ValuesDue::SendNew,
        _ => ValuesDue::Start,
    }
}

impl TabState {
    fn values_slot(&mut self, which: ValuesSlot) -> &mut Option<ServerValues> {
        match which {
            ValuesSlot::Popup => &mut self.facet_values,
            ValuesSlot::Window => &mut self.filter_window_values,
        }
    }
}

impl OctaApp {
    /// Ask the server for the active tab's values of column `key.0`
    /// containing `key.1`, the `top_n` most common. Once per key; a search
    /// waits for typing to settle. A new key replaces the old one, whose
    /// task is dropped (and its statement cancelled).
    pub(crate) fn want_values(
        &mut self,
        which: ValuesSlot,
        key: (usize, String),
        top_n: usize,
        ctx: &egui::Context,
    ) {
        let idx = self.active_tab;
        if idx >= self.tabs.len() {
            return;
        }
        let now = Instant::now();
        match values_due(self.tabs[idx].values_slot(which).as_ref(), &key, now) {
            ValuesDue::Done => return,
            ValuesDue::Wait(wait) => {
                ctx.request_repaint_after(wait);
                return;
            }
            ValuesDue::Start => {
                *self.tabs[idx].values_slot(which) = Some(ServerValues {
                    key,
                    task: None,
                    result: None,
                    since: now,
                });
                ctx.request_repaint_after(SETTLE);
                return;
            }
            ValuesDue::SendNew | ValuesDue::Send => {}
        }
        let Some(src) = view_source_for(
            &self.tabs[idx],
            self.settings.db_pushdown,
            &self.settings.db_connections,
        ) else {
            return;
        };
        let Some(col) = self.tabs[idx].table.columns.get(key.0).cloned() else {
            return;
        };
        // The hash columns as loaded, so their values can be listed too.
        let mut src = src;
        if let Some(v) = &self.tabs[idx].server_view {
            src.derived = v.derived.clone();
        }
        let search = key.1.trim().to_string();
        let task = self.spawn_server_task(src.conn.clone(), col.name.clone(), move |c, stop| {
            octa::db::pushdown::facets::run(c, &src, &col, &search, top_n, stop)
        });
        *self.tabs[idx].values_slot(which) = Some(ServerValues {
            key,
            task: Some(task),
            result: None,
            since: now,
        });
    }

    /// Take in finished value lists for the active tab, and hand the popup's
    /// to the popup. A closed popup drops its list (cancelling a count still
    /// running), so each opening counts afresh.
    pub(crate) fn poll_values(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        if tab.table_state.facet_col.is_none() {
            tab.facet_values = None;
        }
        // Same for the Column Filter window: each opening counts afresh.
        if !tab.show_column_filter {
            tab.filter_window_values = None;
        }
        for which in [ValuesSlot::Popup, ValuesSlot::Window] {
            let Some(v) = tab.values_slot(which).as_mut() else {
                continue;
            };
            let Some(task) = &v.task else {
                continue;
            };
            let outcome = match task.poll() {
                TaskPoll::Pending => {
                    ctx.request_repaint();
                    continue;
                }
                TaskPoll::Ready(vf) => Ok(vf),
                TaskPoll::Cancelled => Err(t("pushdown.cancelled")),
                TaskPoll::Failed(e) => Err(e),
            };
            v.task = None;
            v.result = Some(outcome);
        }
        tab.take_popup_values();
    }
}

impl TabState {
    /// Hand the popup the server's answer for what it shows now. An answer
    /// for another column or search (the user moved on while it ran) is
    /// dropped, cancelling it if still running, so it can never land.
    pub(crate) fn take_popup_values(&mut self) {
        let st = &mut self.table_state;
        let Some(v) = &self.facet_values else {
            return;
        };
        let current = st
            .facet_col
            .map(|c| octa::ui::table_view::facet_request_key(st, c));
        if current.as_ref() != Some(&v.key) {
            self.facet_values = None;
            return;
        }
        let Some(res) = &v.result else {
            return;
        };
        if st.facet_cache_key.as_ref() == Some(&v.key) {
            return;
        }
        // The whole column's distinct count, and the "counted over all rows"
        // note, come from the unsearched list only: a search answers for its
        // matches.
        let unsearched = v.key.1.is_empty();
        match res {
            Ok(vf) => {
                st.facet_rows = vf.rows.iter().map(|r| (r.label.clone(), r.count)).collect();
                if unsearched {
                    st.facet_unique = vf.unique_count;
                    st.facet_external_note = Some(Ok(t("dbview.facet_server").replace(
                        "{total}",
                        &octa::ui::status_bar::format_number(vf.nulls + vf.total_non_null),
                    )));
                }
            }
            Err(e) => {
                st.facet_rows.clear();
                st.facet_external_note = Some(Err(format!("{} {e}", t("dbview.facet_failed"))));
            }
        }
        st.facet_cache_key = Some(v.key.clone());
    }
}

#[cfg(test)]
#[path = "db_view_tests.rs"]
pub(crate) mod tests;
