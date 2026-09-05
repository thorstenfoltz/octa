//! Dispatch for the Analyse menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::analyse_menu`, so a menu entry and
//! the code it runs sit in matching files.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every Analyse-menu action set on `action`.
    pub(super) fn dispatch_analyse_menu(
        &mut self,
        ctx: &egui::Context,
        action: &ui::toolbar::ToolbarAction,
    ) {
        if action.toggle_sql_panel {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_panel_open = !tab.sql_panel_open;
            if tab.sql_panel_open {
                tab.sql_editor_focus_pending = true;
            }
        }
        if action.open_correlation && self.tabs[self.active_tab].table.col_count() > 0 {
            self.correlation_dialog = Some(crate::app::state::CorrelationState {
                method: octa::data::correlation::CorrMethod::Pearson,
                size: octa::ui::settings::DialogSize::default(),
            });
        }
        if action.open_referential && self.tabs[self.active_tab].table.col_count() > 0 {
            self.referential_dialog = Some(crate::app::state::ReferentialState {
                parent_tab: self.active_tab,
                parent_col: None,
                child_tab: self.active_tab,
                child_col: None,
                size: octa::ui::settings::DialogSize::default(),
            });
        }
        if action.open_dist_compare && self.tabs[self.active_tab].table.col_count() > 0 {
            // Both sides start on the active tab: comparing two of its columns
            // is as common as comparing one column across two files, and the
            // tab picker is right there for the other case.
            self.dist_compare_dialog = Some(crate::app::state::DistCompareState {
                tab_a: self.active_tab,
                col_a: None,
                tab_b: self.active_tab,
                col_b: None,
                size: octa::ui::settings::DialogSize::default(),
            });
        }
        if action.toggle_chat_panel {
            self.toggle_chat_panel();
        }
        if action.explain_file {
            self.explain_active_file(ctx);
        }
        if action.open_chart_tab {
            self.open_chart_tab();
        }
        if action.open_value_frequency {
            let tab = &mut self.tabs[self.active_tab];
            if tab.table.col_count() > 0 {
                tab.value_frequency_pick = true;
            }
        }
        if action.open_join_diag {
            self.open_join_diag_dialog();
        }
        if action.open_join_keys {
            self.open_join_keys_dialog();
        }
        if action.open_drift {
            self.open_drift_dialog();
        }
        if action.open_rel_map {
            self.open_rel_map_dialog();
        }
        if action.open_db_compare {
            self.open_db_compare_dialog(None, None);
        }
        if action.open_file_internals {
            self.open_file_internals_tab();
        }
        if action.open_describe_tab {
            self.open_describe_tab();
        }
        if action.open_pivot && self.tabs[self.active_tab].table.col_count() > 0 {
            self.pivot_dialog = Some(crate::app::state::PivotState::default());
        }
        if action.open_timeseries && self.tabs[self.active_tab].table.col_count() > 0 {
            self.timeseries_dialog = Some(crate::app::state::TimeseriesState::default());
        }
        if action.open_cleanup_panel {
            self.toggle_cleanup_panel();
        }
        if action.open_quality {
            self.open_quality_tab();
        }
        if action.open_transpose {
            self.open_transpose_tab();
        }
        if action.open_row_compare {
            self.open_row_compare_tab();
        }
        if action.open_random_sample && self.tabs[self.active_tab].table.col_count() > 0 {
            self.random_sample_dialog = Some(crate::app::state::RandomSampleState::default());
        }
        if action.open_outliers && self.tabs[self.active_tab].table.col_count() > 0 {
            self.outlier_dialog = Some(crate::app::state::OutlierState::for_table(
                &self.tabs[self.active_tab].table,
            ));
        }
        if action.open_pii && self.tabs[self.active_tab].table.col_count() > 0 {
            self.open_pii_dialog();
        }
    }
}
