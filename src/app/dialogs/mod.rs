//! Modal dialogs rendered over the central panel. Each submodule exposes a
//! single `render_*_dialog` free function that takes `&mut OctaApp` and an
//! `egui::Context`.

use eframe::egui;

use super::state::OctaApp;

pub(crate) mod about;
pub(crate) mod add_column;
pub(crate) mod column_inspector;
pub(crate) mod date_ambiguity;
pub(crate) mod delete_columns;
pub(crate) mod documentation;
pub(crate) mod reload_confirm;
pub(crate) mod settings;
pub(crate) mod table_picker;
pub(crate) mod unsaved_changes;
pub(crate) mod update_dialog;

impl OctaApp {
    /// Render every modal dialog in the order the old `update()` body
    /// rendered them. Each dialog early-returns if its visibility flag is
    /// false, so calling all of them every frame is cheap.
    pub(crate) fn render_dialogs(&mut self, ctx: &egui::Context) {
        add_column::render_add_column_dialog(self, ctx);
        column_inspector::render_column_inspector_dialog(self, ctx);
        delete_columns::render_delete_columns_dialog(self, ctx);
        unsaved_changes::render_close_confirm_dialog(self, ctx);
        unsaved_changes::render_open_confirm_dialog(self, ctx);
        table_picker::render_table_picker(self, ctx);
        date_ambiguity::render_date_ambiguity_dialog(self, ctx);
        settings::render_settings_dialog(self, ctx);
        documentation::render_documentation_dialog(self, ctx);
        reload_confirm::render_unalign_confirm_dialog(self, ctx);
        reload_confirm::render_reload_confirm_dialog(self, ctx);
        about::render_about_dialog(self, ctx);
        update_dialog::render_update_dialog(self, ctx);
    }
}
