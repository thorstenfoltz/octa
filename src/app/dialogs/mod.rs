//! Modal dialogs rendered over the central panel. Each submodule exposes a
//! single `render_*_dialog` free function that takes `&mut OctaApp` and an
//! `egui::Context`.

use eframe::egui;

use super::state::OctaApp;

pub(crate) mod about;
pub(crate) mod add_column;
pub(crate) mod column_filter;
pub(crate) mod column_inspector;
pub(crate) mod date_ambiguity;
pub(crate) mod delete_columns;
pub(crate) mod documentation;
pub(crate) mod find_duplicates;
pub(crate) mod parse_in_new_tab;
pub(crate) mod raw_perf_prompt;
pub(crate) mod readonly_notice;
pub(crate) mod reload_confirm;
pub(crate) mod schema_export;
pub(crate) mod settings;
pub(crate) mod table_picker;
pub(crate) mod unsaved_changes;
pub(crate) mod update_dialog;
pub(crate) mod value_frequency;

impl OctaApp {
    /// Render every modal dialog in the order the old `update()` body
    /// rendered them. Each dialog early-returns if its visibility flag is
    /// false, so calling all of them every frame is cheap.
    pub(crate) fn render_dialogs(&mut self, ctx: &egui::Context) {
        add_column::render_add_column_dialog(self, ctx);
        column_filter::render_column_filter_dialog(self, ctx);
        column_inspector::render_column_inspector_dialog(self, ctx);
        delete_columns::render_delete_columns_dialog(self, ctx);
        unsaved_changes::render_close_confirm_dialog(self, ctx);
        unsaved_changes::render_open_confirm_dialog(self, ctx);
        table_picker::render_table_picker(self, ctx);
        raw_perf_prompt::render_raw_perf_prompt_dialog(self, ctx);
        readonly_notice::render_readonly_notice_dialog(self, ctx);
        date_ambiguity::render_date_ambiguity_dialog(self, ctx);
        settings::render_settings_dialog(self, ctx);
        documentation::render_documentation_dialog(self, ctx);
        reload_confirm::render_unalign_confirm_dialog(self, ctx);
        reload_confirm::render_reload_confirm_dialog(self, ctx);
        about::render_about_dialog(self, ctx);
        update_dialog::render_update_dialog(self, ctx);
        parse_in_new_tab::render_parse_in_new_tab_dialog(self, ctx);
        value_frequency::render_value_frequency_dialog(self, ctx);
        find_duplicates::render_find_duplicates_dialog(self, ctx);
        schema_export::render_schema_export_dialog(self, ctx);
    }
}
