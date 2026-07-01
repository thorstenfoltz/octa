//! Implements `eframe::App::update`. This is the top-level frame orchestrator:
//! it calls the individual render/handle methods in the same order the old
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
        self.drive_pending_load(&ctx);
        self.drain_pending_open_queue();
        self.drain_pending_tab_edits();
        self.drain_cloud_pending_open();
        self.drain_cloud_sign_ins(&ctx);
        self.expire_sql_diff_highlights(&ctx);

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
        self.render_chat_panel(ui);
        self.render_christmas_overlay(&ctx);
        self.render_central_panel(ui);
        self.render_window_resize_handles(&ctx);
        self.render_confetti(&ctx);
        self.render_snowfall(&ctx);
        self.render_new_year_overlay(&ctx);
        self.render_crash_offer(&ctx);
    }

    /// Cleanup on shutdown: persist the live chat session and stop any Ollama
    /// server Octa started (a user-launched server is left running).
    fn on_exit(&mut self) {
        self.persist_current_session();
        self.chat.ollama.stop_server();
        octa::diagnostics::crash::clear_running();
    }
}

impl OctaApp {
    /// One-shot dialog offering a debug report after an unclean prior exit.
    fn render_crash_offer(&mut self, ctx: &egui::Context) {
        if !self.pending_crash_offer {
            return;
        }
        let mut close = false;
        egui::Window::new(octa::i18n::t("diagnostics.crash_title"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(octa::i18n::t("diagnostics.crash_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(octa::i18n::t("diagnostics.export")).clicked() {
                        self.export_debug_report_now();
                        close = true;
                    }
                    if ui.button(octa::i18n::t("diagnostics.dismiss")).clicked() {
                        // Discard the waiting crash file so this fires only once.
                        let _ = octa::diagnostics::crash::take_last_crash();
                        close = true;
                    }
                });
            });
        if close {
            self.pending_crash_offer = false;
        }
    }
}
