//! Implements `eframe::App::update`. This is the top-level frame orchestrator
//! — it calls the individual render/handle methods in the same order the old
//! monolithic `update()` used.

use eframe::egui;

use super::state::OctaApp;

impl eframe::App for OctaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Load CLI-provided files on first frame. Multiple paths are queued so
        // the standard drain logic creates one tab per file. Pinned tabs from
        // a previous session enqueue alongside (de-duplicated against the
        // CLI args), and missing pinned paths are pruned from settings so
        // the list doesn't keep failing.
        if !self.initial_files.is_empty() || !self.startup_pin_load_done {
            let files = std::mem::take(&mut self.initial_files);
            let already: std::collections::HashSet<std::path::PathBuf> =
                files.iter().cloned().collect();
            let mut to_enqueue = files;
            let mut pruned = false;
            let mut surviving = Vec::with_capacity(self.settings.pinned_tabs.len());
            for path_str in std::mem::take(&mut self.settings.pinned_tabs) {
                let path = std::path::PathBuf::from(&path_str);
                if path.exists() {
                    if !already.contains(&path) {
                        to_enqueue.push(path);
                    }
                    surviving.push(path_str);
                } else {
                    pruned = true;
                }
            }
            self.settings.pinned_tabs = surviving;
            if pruned {
                self.settings.save();
            }
            if !to_enqueue.is_empty() {
                self.enqueue_open_files(to_enqueue);
            }
            self.startup_pin_load_done = true;
        }

        // Re-sync `tab.pinned` against `settings.pinned_tabs`. Cheap and
        // idempotent; runs once per frame so freshly-loaded pinned files
        // pick up their flag without a dedicated callback.
        for tab in &mut self.tabs {
            let want_pinned = tab
                .table
                .source_path
                .as_ref()
                .map(|p| self.settings.pinned_tabs.iter().any(|q| q == p))
                .unwrap_or(false);
            if tab.pinned != want_pinned {
                tab.pinned = want_pinned;
            }
        }

        self.handle_shortcuts(&ctx);
        self.update_easter_egg_inputs(&ctx);
        self.drain_background_rows(&ctx);
        self.drain_pending_open_queue();

        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }

        let search_active = !self.tabs[self.active_tab].search_text.is_empty();
        let filtered_count = self.tabs[self.active_tab].filtered_rows.len();

        self.render_toolbar(ui);
        self.render_tab_bar(ui);
        self.render_sidebar(ui);
        self.render_dialogs(&ctx);
        self.render_status_bar(ui, filtered_count, search_active);
        self.render_sql_panel(ui);
        self.render_multi_search_panel(ui);
        self.render_christmas_overlay(&ctx);
        self.render_central_panel(ui);
        self.render_confetti(&ctx);
        self.render_snowfall(&ctx);
    }
}
