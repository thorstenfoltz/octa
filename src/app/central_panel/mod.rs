//! Central panel: status banner, view-mode dispatch (Notebook/Markdown/
//! Raw/JsonTree), the table renderer, and the table interaction handling
//! (column rename, type change, sort, context menu, lazy row loading).

mod banners;
mod chips;
mod interaction;
mod status;
mod tiles;
mod views;

use eframe::egui;

use octa::ui;

use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn render_central_panel(&mut self, parent_ui: &mut egui::Ui) {
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        // A maximised Assistant takes this area, and comes before a maximised
        // SQL panel: Restore then shows that one again.
        if self.chat_maximised() {
            egui::CentralPanel::default().show(parent_ui, |ui| {
                self.render_status_message(ui);
                self.render_chat_body(ui);
            });
            return;
        }
        // A maximised SQL panel takes this area instead of the table.
        if self.sql_panel_visible() && self.tabs[self.active_tab].sql_maximised {
            egui::CentralPanel::default().show(parent_ui, |ui| {
                self.render_status_message(ui);
                self.draw_sql_panel(ui, true);
            });
            return;
        }
        egui::CentralPanel::default().show(parent_ui, |ui| {
            // Per-theme background decoration (e.g. Manga's halftone field).
            // Painted before any content so widgets sit on top.
            ui::theme::paint_background_decoration(ui.painter(), ui.max_rect(), self.theme_mode);

            self.render_status_message(ui);
            match self.active_tile_members() {
                Some((set_id, members)) => self.render_tiles(ctx, ui, set_id, &members),
                None => self.render_tab_body(ctx, ui),
            }
        });
    }

    /// Everything the active tab shows below the status line: banners, chips,
    /// its view and the find box. A tile draws it with `active_tab` pointed
    /// at its own tab (see `tiles::render_tiles`).
    pub(super) fn render_tab_body(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.render_load_banners(ui);
        self.render_filter_chips(ui);

        // A tab whose rows were dropped by View -> Tab memory reads its
        // file again the moment it is the one on screen. The flag is
        // cleared first: a failed read must not retry every frame.
        if self.tabs[self.active_tab].needs_reload {
            self.tabs[self.active_tab].needs_reload = false;
            self.reload_active_file();
        }

        // A switch between a text view and a table view is when each
        // catches up with the other's unsaved edits. Before the filter:
        // text read back into the table dirties it.
        self.tabs[self.active_tab].sync_views(&self.registry, &self.settings.write_options);

        // Recompute filter before drawing (toolbar actions earlier in the
        // frame may have dirtied it).
        if self.tabs[self.active_tab].filter_dirty {
            self.recompute_filter();
        }

        let corner = egui::pos2(ui.max_rect().right(), ui.cursor().top());
        if !self.render_non_table_view(ctx, ui) {
            self.render_table_view(ctx, ui);
        }
        self.draw_find_box(ui, corner);
    }
}
