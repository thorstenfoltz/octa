//! Render the SQL editor panel and apply the user's actions: run query,
//! clear result, export result. The panel is only visible while the active
//! tab is in Table view.

use eframe::egui;

use octa::data::ViewMode;
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};
use crate::view_modes;

impl OctaApp {
    pub(crate) fn render_sql_panel(&mut self, ctx: &egui::Context) {
        let sql_panel_visible = {
            let tab = &self.tabs[self.active_tab];
            tab.sql_panel_open && tab.table.col_count() > 0 && tab.view_mode == ViewMode::Table
        };
        if !sql_panel_visible {
            return;
        }
        let position = self.settings.sql_panel_position;
        let mut sql_action = view_modes::SqlAction::default();
        let tab = &mut self.tabs[self.active_tab];
        let partial_rows = tab.table.total_rows.and_then(|total| {
            let loaded = tab.table.row_count();
            if loaded < total {
                Some((loaded, total))
            } else {
                None
            }
        });
        let render = |ui: &mut egui::Ui,
                      tab: &mut TabState,
                      autocomplete: bool,
                      row_limit: usize|
         -> view_modes::SqlAction {
            view_modes::render_sql_view(ui, tab, autocomplete, row_limit, position, partial_rows)
        };
        let autocomplete = self.settings.sql_autocomplete;
        let row_limit = self.settings.sql_default_row_limit;
        match position {
            ui::settings::SqlPanelPosition::Bottom => {
                egui::TopBottomPanel::bottom("sql_panel")
                    .resizable(true)
                    .default_height(280.0)
                    .min_height(140.0)
                    .show(ctx, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
            ui::settings::SqlPanelPosition::Top => {
                egui::TopBottomPanel::top("sql_panel")
                    .resizable(true)
                    .default_height(280.0)
                    .min_height(140.0)
                    .show(ctx, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
            ui::settings::SqlPanelPosition::Left => {
                egui::SidePanel::left("sql_panel")
                    .resizable(true)
                    .default_width(440.0)
                    .min_width(280.0)
                    .show(ctx, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
            ui::settings::SqlPanelPosition::Right => {
                egui::SidePanel::right("sql_panel")
                    .resizable(true)
                    .default_width(440.0)
                    .min_width(280.0)
                    .show(ctx, |ui| {
                        sql_action = render(ui, tab, autocomplete, row_limit);
                    });
            }
        }
        if sql_action.clear {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_result = None;
            tab.sql_error = None;
        }
        if sql_action.run {
            let tab = &mut self.tabs[self.active_tab];
            let query = tab.sql_query.clone();
            let mut snapshot = tab.table.clone();
            snapshot.apply_edits();
            match octa::sql::run_query(&snapshot, &query) {
                Ok(outcome) => match outcome.kind {
                    octa::sql::QueryKind::Select => {
                        tab.sql_result = Some(outcome.table);
                        tab.sql_error = None;
                    }
                    octa::sql::QueryKind::Mutation => {
                        // Apply the mutation to the base table directly so
                        // INSERT / UPDATE / DELETE affect the data, not just
                        // a result set. Selection / widths / per-tab UI state
                        // are reset because row/column identity may have changed.
                        tab.table = outcome.table;
                        tab.table_state = TableViewState::default();
                        tab.filter_dirty = true;
                        tab.sql_result = None;
                        tab.sql_error = None;
                        let rows = tab.table.row_count();
                        let affected = outcome.affected.unwrap_or(0);
                        self.status_message = Some((
                            format!(
                                "SQL applied: {} row(s) affected — table now {} row(s)",
                                affected, rows
                            ),
                            std::time::Instant::now(),
                        ));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                            self.tabs[self.active_tab].title_display(),
                        ));
                    }
                },
                Err(e) => {
                    tab.sql_error = Some(e.to_string());
                }
            }
        }
        if sql_action.export {
            self.export_sql_result();
        }
    }
}
