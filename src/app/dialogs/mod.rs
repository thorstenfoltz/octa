//! Modal dialogs rendered over the central panel. Each submodule exposes a
//! single `render_*_dialog` free function that takes `&mut OctaApp` and an
//! `egui::Context`.

use eframe::egui;

use super::state::OctaApp;

pub(crate) mod about;
pub(crate) mod add_column;
pub(crate) mod anonymize;
pub(crate) mod bookmark;
pub(crate) mod chat_prompt;
pub(crate) mod column_filter;
pub(crate) mod column_format;
pub(crate) mod conditional_column;
pub(crate) mod conditional_format;
pub(crate) mod correlation;
pub(crate) mod date_ambiguity;
pub(crate) mod dedupe;
pub(crate) mod delete_columns;
pub(crate) mod documentation;
pub(crate) mod find_duplicates;
pub(crate) mod find_fuzzy_duplicates;
pub(crate) mod git_compare;
pub(crate) mod impute;
pub(crate) mod join;
pub(crate) mod multi_sort;
pub(crate) mod outliers;
pub(crate) mod parse_in_new_tab;
pub(crate) mod partition;
pub(crate) mod pii;
pub(crate) mod pivot;
pub(crate) mod random_sample;
pub(crate) mod raw_perf_prompt;
pub(crate) mod readonly_notice;
pub(crate) mod reload_confirm;
pub(crate) mod rename_columns;
pub(crate) mod repair_file;
pub(crate) mod round_save_prompt;
pub(crate) mod schema_change_save;
pub(crate) mod schema_export;
pub(crate) mod settings;
pub(crate) mod sheet_picker;
pub(crate) mod sql_snippet;
pub(crate) mod sql_snippets_window;
pub(crate) mod sql_write_back;
pub(crate) mod tab_rename;
pub(crate) mod table_picker;
pub(crate) mod tidy_up;
pub(crate) mod time_calc;
pub(crate) mod transform;
pub(crate) mod union;
pub(crate) mod unsaved_changes;
pub(crate) mod update_dialog;
pub(crate) mod validation;
pub(crate) mod value_frequency;
pub(crate) mod value_frequency_picker;

impl OctaApp {
    /// Render every modal dialog in the order the old `update()` body
    /// rendered them. Each dialog early-returns if its visibility flag is
    /// false, so calling all of them every frame is cheap.
    pub(crate) fn render_dialogs(&mut self, ctx: &egui::Context) {
        add_column::render_add_column_dialog(self, ctx);
        column_filter::render_column_filter_dialog(self, ctx);
        column_format::render_column_format_dialog(self, ctx);
        delete_columns::render_delete_columns_dialog(self, ctx);
        unsaved_changes::render_close_confirm_dialog(self, ctx);
        unsaved_changes::render_open_confirm_dialog(self, ctx);
        table_picker::render_table_picker(self, ctx);
        sheet_picker::render_sheet_picker_dialog(self, ctx);
        raw_perf_prompt::render_raw_perf_prompt_dialog(self, ctx);
        repair_file::render_repair_file_dialog(self, ctx);
        readonly_notice::render_readonly_notice_dialog(self, ctx);
        date_ambiguity::render_date_ambiguity_dialog(self, ctx);
        settings::render_settings_dialog(self, ctx);
        documentation::render_documentation_dialog(self, ctx);
        round_save_prompt::render_round_save_prompt_dialog(self, ctx);
        schema_change_save::render_schema_change_save_dialog(self, ctx);
        reload_confirm::render_unalign_confirm_dialog(self, ctx);
        reload_confirm::render_reload_confirm_dialog(self, ctx);
        about::render_about_dialog(self, ctx);
        update_dialog::render_update_dialog(self, ctx);
        parse_in_new_tab::render_parse_in_new_tab_dialog(self, ctx);
        value_frequency_picker::render_value_frequency_picker_dialog(self, ctx);
        value_frequency::render_value_frequency_dialog(self, ctx);
        find_duplicates::render_find_duplicates_dialog(self, ctx);
        find_fuzzy_duplicates::render_find_fuzzy_duplicates_dialog(self, ctx);
        dedupe::render_dedupe_dialog(self, ctx);
        impute::render_impute_dialog(self, ctx);
        conditional_format::render_conditional_format_dialog(self, ctx);
        validation::render_validation_dialog(self, ctx);
        pivot::render_pivot_dialog(self, ctx);
        multi_sort::render_multi_sort_dialog(self, ctx);
        git_compare::render_git_compare_dialog(self, ctx);
        correlation::render_correlation_dialog(self, ctx);
        transform::render_transform_dialog(self, ctx);
        conditional_column::render_conditional_column_dialog(self, ctx);
        anonymize::render_anonymize_dialog(self, ctx);
        partition::render_partition_dialog(self, ctx);
        union::render_union_dialog(self, ctx);
        join::render_join_dialog(self, ctx);
        outliers::render_outliers_dialog(self, ctx);
        pii::render_pii_dialog(self, ctx);
        sql_snippet::render_sql_snippet_dialog(self, ctx);
        sql_snippets_window::render_sql_snippets_window(self, ctx);
        chat_prompt::render_chat_prompt_dialog(self, ctx);
        time_calc::render_time_calc_dialog(self, ctx);
        bookmark::render_bookmark_dialog(self, ctx);
        tab_rename::render_tab_rename_dialog(self, ctx);
        rename_columns::render_rename_columns_dialog(self, ctx);
        random_sample::render_random_sample_dialog(self, ctx);
        tidy_up::render_tidy_up_dialog(self, ctx);
        schema_export::render_schema_export_dialog(self, ctx);
        sql_write_back::render_sql_write_back_dialog(self, ctx);
    }
}
