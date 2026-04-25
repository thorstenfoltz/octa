//! Implements `eframe::App::update`. This is the top-level frame orchestrator
//! — it calls the individual render/handle methods in the same order the old
//! monolithic `update()` used.

use eframe::egui;

use super::state::OctaApp;

impl eframe::App for OctaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Load file from CLI on first frame.
        if let Some(path) = self.initial_file.take() {
            self.load_file(path);
        }

        self.handle_shortcuts(ctx);
        self.drain_background_rows(ctx);

        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }

        let search_active = !self.tabs[self.active_tab].search_text.is_empty();
        let filtered_count = self.tabs[self.active_tab].filtered_rows.len();

        self.render_toolbar(ctx);
        self.render_tab_bar(ctx);
        self.render_sidebar(ctx);
        self.render_dialogs(ctx);
        self.render_status_bar(ctx, filtered_count, search_active);
        self.render_sql_panel(ctx);
        self.render_central_panel(ctx);
    }
}
