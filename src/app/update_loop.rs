//! Implements `eframe::App::update`. This is the top-level frame orchestrator:
//! it calls the individual render/handle methods in the same order the old
//! monolithic `update()` used.

use eframe::egui;

use super::state::{OctaApp, UpdateState};

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
                } else if std::fs::read_dir(
                    // A bare file name has an empty parent, which means the
                    // working directory.
                    path.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(std::path::Path::new(".")),
                )
                .is_ok()
                {
                    // The folder is there and readable, so the file really is
                    // gone.
                    pruned = true;
                } else {
                    // The folder is not reachable: an unmounted NAS, a volume
                    // still locked at login, a slow autofs mount. "Missing" is
                    // not the same as "deleted", and this prune is persisted
                    // on frame one with no undo.
                    surviving.push(path_str);
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

        // One background request per launch, opt-out in Settings. Kicked off
        // from the first frame rather than `OctaApp::new` because it needs an
        // `egui::Context` to wake the UI when the answer arrives.
        if !self.startup_update_started && self.settings.check_updates_on_start {
            self.startup_update_started = true;
            self.check_for_updates(&ctx);
        }
        self.drain_startup_update_check();
        // Workers spawned from paths that carry no context (a cloud save-back
        // starts three calls below `save_tab`) borrow this one to wake the UI.
        if self.cloud_browser.repaint.is_none() {
            self.cloud_browser.repaint = Some(ctx.clone());
        }

        // Recording a binding takes the keyboard away from every shortcut for
        // that frame - see `ui::shortcuts::set_capture_mode`.
        octa::ui::shortcuts::set_capture_mode(self.settings_dialog.is_recording_shortcut());

        self.handle_shortcuts(&ctx);
        self.update_easter_egg_inputs(&ctx);
        self.drain_background_rows(&ctx);
        self.drive_pending_load(&ctx);
        self.drive_union_prep(&ctx);
        // The cloud union phases (listing, downloading) run on their own threads
        // and only notify when finished, so keep frames coming while any union
        // job is live - otherwise its spinner and counter sit frozen.
        if self.union_progress.is_some() {
            ctx.request_repaint();
        }
        self.drain_pending_open_queue();
        self.drain_pending_tab_edits();
        self.drain_cloud_pending_open();
        self.drain_cloud_sign_ins(&ctx);
        self.drain_db_pending_open();
        self.drain_sql_server_job();
        self.drain_db_write_back_job();
        self.drain_batch_convert();
        self.drain_open_url(&ctx);
        self.drain_schema_drift();
        self.drain_harmonise();
        self.drain_report();
        self.drain_fuzzy_join();
        self.drain_ask_filter();
        self.drain_ask_sql();
        self.expire_sql_diff_highlights(&ctx);
        self.drive_auto_save(&ctx);

        // Columns may have been inserted, deleted or reordered since the last
        // frame; the filters and hidden columns are keyed by index and have to
        // follow. Runs before the filter recompute that consumes them.
        if self.tabs[self.active_tab].sync_column_keys() {
            self.tabs[self.active_tab].filter_dirty = true;
        }
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
        self.render_cleanup_panel(ui);
        self.render_chat_panel(ui);
        self.render_christmas_overlay(&ctx);
        self.render_central_panel(ui);
        self.render_window_resize_handles(&ctx);
        self.render_confetti(&ctx);
        self.render_snowfall(&ctx);
        self.render_new_year_overlay(&ctx);
        self.render_crash_offer(&ctx);
        // Last, so the overlays it switches on describe the frame just built.
        if self.settings.debug_mode {
            octa::diagnostics::input_trace::trace(&ctx);
        }
    }

    /// Cleanup on shutdown: persist the live chat session and stop any Ollama
    /// server Octa started (a user-launched server is left running).
    ///
    /// The `glow::Context` parameter exists only on the glow renderer, for
    /// freeing GPU resources. Octa allocates none directly (egui owns its own
    /// textures), so it is ignored.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_current_session();
        self.chat.ollama.stop_server();
        octa::diagnostics::crash::clear_running();
    }
}

impl OctaApp {
    /// Act on the startup check exactly once. An available version says so in
    /// the status bar - the check would otherwise be a silent no-op. "Up to
    /// date" and a failed request stay quiet: neither is worth interrupting a
    /// launch for. Release notes are not this function's business; they come
    /// from the binary at startup (see `dialogs::release_notes`).
    fn drain_startup_update_check(&mut self) {
        if self.startup_update_seen || !self.startup_update_started {
            return;
        }
        let state = self.update_state.lock().unwrap().clone();
        match state {
            UpdateState::Available { version } => {
                self.startup_update_seen = true;
                self.status_message = Some((
                    octa::i18n::t("release.toast").replace("{version}", &version),
                    std::time::Instant::now(),
                ));
            }
            UpdateState::UpToDate | UpdateState::Error(_) => {
                self.startup_update_seen = true;
            }
            _ => {}
        }
    }

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
