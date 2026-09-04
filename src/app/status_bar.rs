//! Render the bottom status bar and handle its navigation action
//! ("Go to Cell" input).

use eframe::egui;

use octa::ui;

use super::state::{OctaApp, UpdateState};

impl OctaApp {
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

        // Busy indicator state: a long-running operation is either a
        // background row-load draining into the active tab or an
        // update-check / install in flight. We surface a small spinner +
        // one-word reason so the user knows the app is intentionally
        // doing work (and so the WM's startup cursor, which we disabled
        // in octa.desktop, isn't replaced by a different mystery).
        let bg_loading = !self.tabs[self.active_tab]
            .bg_loading_done
            .load(std::sync::atomic::Ordering::Relaxed);
        // Split from `Updating`: the startup check runs on every launch, and
        // a spinner labelled "Updating..." for a read-only version query
        // reads as "something is being installed behind my back".
        let update_checking = matches!(*self.update_state.lock().unwrap(), UpdateState::Checking);
        let update_busy = matches!(*self.update_state.lock().unwrap(), UpdateState::Updating);
        let file_loading = self.pending_load.is_some();
        let db_writing = self.db_write_back_job.is_some();
        // The only busy job that offers a Cancel, because it is the only one
        // that can be stopped: a warehouse read can take minutes, and the
        // connector hands out a thread-safe cancel closure for it. The button
        // waits for that closure rather than appearing with the spinner, so
        // it is never shown while connecting, nor at all on Oracle, which has
        // no cancel to give.
        let db_load_hint = self.db_load_job.as_ref().map(|j| j.hint.clone());
        let db_load_cancellable = self.db_load_job.as_ref().is_some_and(|j| j.can_cancel());
        let asking = self.ask_filter_job.is_some() || self.ask_sql_job.is_some();
        // Union: cloud listing/download and the local read phase all report
        // through one progress object, so the spinner spans them.
        let union_hint = self.union_progress.as_ref().map(|p| p.hint());
        let busy = db_load_hint.is_some()
            || bg_loading
            || update_busy
            || update_checking
            || file_loading
            || db_writing
            || asking
            || union_hint.is_some();
        let db_writing_hint = octa::i18n::t("db.writing_back");
        let asking_hint = octa::i18n::t("search.ask_running");
        let checking_hint = octa::i18n::t("dialog.ud_checking");
        let busy_hint = if let Some(hint) = db_load_hint.as_deref() {
            Some(hint)
        } else if let Some(hint) = union_hint.as_deref() {
            Some(hint)
        } else if asking {
            Some(asking_hint.as_str())
        } else if update_busy {
            Some("Updating...")
        } else if update_checking {
            Some(checking_hint.as_str())
        } else if db_writing {
            Some(db_writing_hint.as_str())
        } else if file_loading {
            Some("Loading file...")
        } else if bg_loading {
            Some("Loading rows...")
        } else {
            None
        };

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
                        readonly,
                        busy,
                        busy_hint,
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
