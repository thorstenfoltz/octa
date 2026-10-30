//! Render the bottom status bar and handle its navigation action
//! ("Go to Cell" input).

use eframe::egui;

use octa::ui;

use super::state::{OctaApp, UpdateState};

impl OctaApp {
    /// What the status bar's spinner says, or `None` when nothing it watches
    /// is running. The finish notification asks the same question first
    /// (`long_task_label`), so the two can never disagree.
    ///
    /// A long-running operation is a background row-load draining into the
    /// active tab, an update check / install in flight, or one of the jobs
    /// below. The spinner + one-word reason tell the user the app is
    /// intentionally doing work (and the WM's startup cursor, which we
    /// disabled in octa.desktop, isn't replaced by a different mystery).
    pub(crate) fn status_busy_hint(&self) -> Option<String> {
        let bg_loading = !self.tabs[self.active_tab]
            .bg_loading_done
            .load(std::sync::atomic::Ordering::Relaxed);
        // Split from `Updating`: the startup check runs on every launch, and
        // a spinner labelled "Updating..." for a read-only version query
        // reads as "something is being installed behind my back".
        let update_checking = matches!(*self.update_state.lock().unwrap(), UpdateState::Checking);
        let update_busy = matches!(*self.update_state.lock().unwrap(), UpdateState::Updating);
        if let Some(job) = self.busy_db_job() {
            Some(job.hint.clone())
        } else if let Some(progress) = self.union_progress.as_ref() {
            // Union: cloud listing/download and the local read phase all
            // report through one progress object, so the spinner spans them.
            Some(progress.hint())
        } else if self.ask_filter_job.is_some() || self.ask_sql_job.is_some() {
            Some(octa::i18n::t("search.ask_running"))
        } else if update_busy {
            Some("Updating...".to_string())
        } else if update_checking {
            Some(octa::i18n::t("dialog.ud_checking"))
        } else if self.db_write_back_job.is_some() {
            Some(octa::i18n::t("db.writing_back"))
        } else if self.pending_load.is_some() {
            Some("Loading file...".to_string())
        } else if let Some(job) = &self.large_open_job {
            Some(octa::i18n::t("largefile.converting").replace("{name}", &job.name))
        } else if self.tabs[self.active_tab].large_fetch.is_some() {
            Some(octa::i18n::t("largefile.reading"))
        } else if let Some(job) = self.tabs.iter().find_map(|t| t.saving.as_ref()) {
            Some(octa::i18n::t("saving.task").replace("{name}", &job.file_name()))
        } else if let Some(job) = self.tabs.iter().find_map(|t| t.large_export.as_ref()) {
            Some(octa::i18n::t("saving.task").replace("{name}", &job.file_name()))
        } else if self.tabs.iter().any(|t| t.date_detect.is_some()) {
            Some(octa::i18n::t("detect_dates.task"))
        } else if self.tabs.iter().any(|t| !t.column_casts.is_empty()) {
            Some(octa::i18n::t("column_cast.task"))
        } else if self
            .tabs
            .iter()
            .any(|t| t.sql_job.as_ref().is_some_and(|j| !j.runs_query()))
        {
            Some(octa::i18n::t("sql.copying"))
        } else if self.tabs.iter().any(|t| t.sql_other_tabs.is_some()) {
            Some(octa::i18n::t("sql.copying_tabs"))
        } else if bg_loading {
            Some("Loading rows...".to_string())
        } else {
            None
        }
    }

    pub(crate) fn render_status_bar(
        &mut self,
        parent_ui: &mut egui::Ui,
        filtered_count: usize,
        search_active: bool,
    ) {
        let status_colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let status_frame = egui::Frame::new()
            .fill(status_colors.bg_header)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0_f32, status_colors.border_subtle));

        // The only busy job that offers a Cancel, because it is the only one
        // that can be stopped: a warehouse read can take minutes, and the
        // connector hands out a thread-safe cancel closure for it. The button
        // waits for that closure rather than appearing with the spinner, so
        // it is never shown while connecting, nor at all on Oracle, which has
        // no cancel to give.
        let db_load_cancellable = self.busy_db_job().is_some_and(|j| j.can_cancel());
        let busy_hint = self.status_busy_hint();
        let busy = busy_hint.is_some();

        let column_filter_count = self.tabs[self.active_tab].column_filters.len();
        let first_filtered_col = self.tabs[self.active_tab]
            .column_filters
            .keys()
            .min()
            .copied();
        let selected_rows = self.tabs[self.active_tab].table_state.selected_rows.clone();
        let selected_cells = self.tabs[self.active_tab]
            .table_state
            .selected_cells
            .clone();
        let readonly = self.is_readonly();

        let status_action = egui::Panel::bottom("status_bar")
            .exact_size(28.0)
            .frame(status_frame)
            .show(parent_ui, |ui| {
                ui::status_bar::draw_status_bar(
                    ui,
                    ui::status_bar::StatusBarCtx {
                        table: &self.tabs[self.active_tab].table,
                        state: &self.tabs[self.active_tab].table_state,
                        theme_mode: self.theme_mode,
                        filtered_count,
                        search_active,
                        nav_focus_requested: std::mem::take(&mut self.nav_focus_requested),
                        zoom_percent: self.zoom_percent,
                        zoom_step: self.settings.zoom_step(),
                        readonly,
                        busy,
                        busy_hint: busy_hint.as_deref(),
                        busy_cancellable: db_load_cancellable,
                        column_filter_count,
                        first_filtered_col,
                        selected_rows: &selected_rows,
                        selected_cells: &selected_cells,
                    },
                    &mut self.nav_input,
                )
            })
            .inner;

        if status_action.cancel_busy {
            self.cancel_db_load();
        }

        if let Some(percent) = status_action.zoom_to {
            self.set_zoom(parent_ui.ctx(), percent);
        }

        if let Some(preselect) = status_action.open_column_filter {
            self.open_column_filter_dialog(Some(preselect));
        }

        if let Some((row, col)) = status_action.navigate_to {
            let tab = &mut self.tabs[self.active_tab];
            tab.table_state.selected_cell = Some((row, col));
            tab.table_state.selected_rows.clear();
            tab.table_state.selected_cols.clear();
            // Auto-scroll to the target cell
            let row_height =
                (self.settings.font_size * self.zoom_percent as f32 / 100.0 * 2.0).max(26.0);
            tab.table_state.set_scroll_y(row as f32 * row_height);
            let col_left: f32 = tab.table_state.col_widths[..col].iter().sum();
            tab.table_state.set_scroll_x(col_left);
        }

        if status_action.kraken_summoned {
            // Easter egg: typing "kraken" into the nav input wakes the beast.
            // Prefixed with "\u{1f419}" so the central-panel renderer paints
            // the message in the accent color instead of error-red.
            self.status_message = Some((
                "\u{1f419} The kraken stirs from the depths...".to_string(),
                std::time::Instant::now(),
            ));
        }
    }
}
