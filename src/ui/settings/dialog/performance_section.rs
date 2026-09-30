//! Performance section: large-file threshold, caps and background limits.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

impl SettingsDialog {
    pub(super) fn performance_section_body(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("settings_performance")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(crate::i18n::t("settings.initial_load_cap"))
                    .on_hover_text(crate::i18n::t("settings_hint.initial_load_cap"));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !self.draft.initial_load_rows_unlimited,
                        egui::TextEdit::singleline(&mut self.initial_load_rows_buf)
                            .desired_width(120.0)
                            .hint_text("2,000,000"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.initial_load_cap"))
                    .on_disabled_hover_text(crate::i18n::t("settings_hint.initial_load_cap"));
                    ui.checkbox(
                        &mut self.draft.initial_load_rows_unlimited,
                        crate::i18n::t("settings.unlimited"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.initial_load_unlimited"));
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.db_page_rows"))
                    .on_hover_text(crate::i18n::t("settings_hint.db_page_rows"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.db_page_rows_buf)
                        .desired_width(120.0)
                        .hint_text("100,000"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.db_page_rows"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.raw_view_cap"))
                    .on_hover_text(crate::i18n::t("settings_hint.raw_view_cap"));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !self.draft.raw_view_max_bytes_unlimited,
                        egui::TextEdit::singleline(&mut self.raw_view_max_mb_buf)
                            .desired_width(120.0)
                            .hint_text("500"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.raw_view_cap"))
                    .on_disabled_hover_text(crate::i18n::t("settings_hint.raw_view_cap"));
                    ui.checkbox(
                        &mut self.draft.raw_view_max_bytes_unlimited,
                        crate::i18n::t("settings.unlimited"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.raw_view_unlimited"));
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.decompress_cap"))
                    .on_hover_text(crate::i18n::t("settings_hint.decompress_cap"));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !self.draft.max_decompressed_unlimited,
                        egui::TextEdit::singleline(&mut self.max_decompressed_mb_buf)
                            .desired_width(120.0)
                            .hint_text("4,295"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.decompress_cap"))
                    .on_disabled_hover_text(crate::i18n::t("settings_hint.decompress_cap"));
                    ui.checkbox(
                        &mut self.draft.max_decompressed_unlimited,
                        crate::i18n::t("settings.unlimited"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.decompress_unlimited"));
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.syntax_size_cap"))
                    .on_hover_text(crate::i18n::t("settings_hint.syntax_size_cap"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.syntax_highlight_max_bytes_buf)
                            .desired_width(100.0)
                            .hint_text("1"),
                    );
                    egui::ComboBox::from_id_salt("syntax_size_unit_combo")
                        .selected_text(self.syntax_highlight_size_unit.label_t())
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for &unit in SizeUnit::ALL {
                                ui.selectable_value(
                                    &mut self.syntax_highlight_size_unit,
                                    unit,
                                    unit.label_t(),
                                );
                            }
                        })
                        .response
                        .on_hover_text(crate::i18n::t("settings_hint.syntax_size_cap"));
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.large_file_min_bytes"))
                    .on_hover_text(crate::i18n::t("settings_hint.large_file_min_bytes"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.large_file_min_bytes_buf)
                            .desired_width(100.0)
                            .hint_text("10"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.large_file_min_bytes"));
                    egui::ComboBox::from_id_salt("large_file_size_unit_combo")
                        .selected_text(self.large_file_size_unit.label_t())
                        .width(70.0)
                        .show_ui(ui, |ui| {
                            for &unit in SizeUnit::ALL {
                                ui.selectable_value(
                                    &mut self.large_file_size_unit,
                                    unit,
                                    unit.label_t(),
                                );
                            }
                        });
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.show_large_file_notice"))
                    .on_hover_text(crate::i18n::t("settings_hint.show_large_file_notice"));
                ui.checkbox(&mut self.draft.show_large_file_notice, "")
                    .on_hover_text(crate::i18n::t("settings_hint.show_large_file_notice"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.multi_search_cap"))
                    .on_hover_text(crate::i18n::t("settings_hint.multi_search_cap"));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !self.draft.grep_max_file_size_unlimited,
                        egui::TextEdit::singleline(&mut self.grep_max_file_size_buf)
                            .desired_width(120.0)
                            .hint_text("50"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.multi_search_cap"))
                    .on_disabled_hover_text(crate::i18n::t("settings_hint.multi_search_cap"));
                    ui.checkbox(
                        &mut self.draft.grep_max_file_size_unlimited,
                        crate::i18n::t("settings.unlimited"),
                    )
                    .on_hover_text(crate::i18n::t("settings_hint.multi_search_unlimited"));
                });
                ui.end_row();

                ui.label(crate::i18n::t("settings.chart_max_points"))
                    .on_hover_text(crate::i18n::t("settings_hint.chart_max_points"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chart_max_points_buf)
                        .desired_width(120.0)
                        .hint_text("25,000"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.chart_max_points"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.chart_max_categories"))
                    .on_hover_text(crate::i18n::t("settings_hint.chart_max_categories"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.chart_max_categories_buf)
                        .desired_width(120.0)
                        .hint_text("200"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.chart_max_categories"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.tables_in_picker"))
                    .on_hover_text(crate::i18n::t("settings_hint.tables_in_picker"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.table_picker_visible_rows_buf)
                        .desired_width(120.0)
                        .hint_text("10"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.tables_in_picker"));
                ui.end_row();

                ui.label(crate::i18n::t("settings.excel_auto_open"))
                    .on_hover_text(crate::i18n::t("settings_hint.excel_auto_open"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.excel_max_auto_sheets_buf)
                        .desired_width(120.0)
                        .hint_text("5"),
                )
                .on_hover_text(crate::i18n::t("settings_hint.excel_auto_open"));
                ui.end_row();
            });

        Self::sub_section(
            ui,
            "settings.sub_cloud_union",
            "settings_perf_sub_cloud_union",
            |ui| {
                egui::Grid::new("settings_performance_cloud_union")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(crate::i18n::t("settings.folder_union_cap"))
                            .on_hover_text(crate::i18n::t("settings_hint.folder_union_cap"));
                        ui.horizontal(|ui| {
                            ui.add_enabled(
                                !self.draft.folder_union_max_files_unlimited,
                                egui::TextEdit::singleline(&mut self.folder_union_max_files_buf)
                                    .desired_width(120.0)
                                    .hint_text("500"),
                            )
                            .on_hover_text(crate::i18n::t("settings_hint.folder_union_cap"))
                            .on_disabled_hover_text(crate::i18n::t(
                                "settings_hint.folder_union_cap",
                            ));
                            ui.checkbox(
                                &mut self.draft.folder_union_max_files_unlimited,
                                crate::i18n::t("settings.unlimited"),
                            )
                            .on_hover_text(crate::i18n::t("settings_hint.folder_union_unlimited"));
                        });
                        ui.end_row();

                        // No Unlimited companion here on purpose: "as many at
                        // once as there are files" is not a thing anyone
                        // wants, it is a thread per object and a rate limit.
                        ui.label(crate::i18n::t("settings.cloud_download_concurrency"))
                            .on_hover_text(crate::i18n::t(
                                "settings_hint.cloud_download_concurrency",
                            ));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cloud_download_concurrency_buf)
                                .desired_width(120.0)
                                .hint_text("8"),
                        )
                        .on_hover_text(crate::i18n::t("settings_hint.cloud_download_concurrency"));
                        ui.end_row();
                    });
            },
        );

        Self::sub_section(
            ui,
            "settings.sub_change_type",
            "settings_perf_sub_change_type",
            |ui| {
                egui::Grid::new("settings_performance_change_type")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        // The hover spells out the trade-off in both
                        // directions, because neither is obvious: the cost is
                        // seven date parses per sampled value on the UI
                        // thread, and the benefit only appears on a column
                        // whose first values are unrepresentative.
                        let hint = crate::i18n::t("settings_hint.retype_layout_sample");
                        ui.label(crate::i18n::t("settings.retype_layout_sample"))
                            .on_hover_text(&hint);
                        ui.horizontal(|ui| {
                            ui.add_enabled(
                                !self.draft.retype_layout_sample_unlimited,
                                egui::TextEdit::singleline(&mut self.retype_layout_sample_buf)
                                    .desired_width(120.0)
                                    .hint_text("10,000"),
                            )
                            .on_hover_text(&hint)
                            .on_disabled_hover_text(crate::i18n::t(
                                "settings_hint.retype_layout_sample_off",
                            ));
                            ui.checkbox(
                                &mut self.draft.retype_layout_sample_unlimited,
                                crate::i18n::t("settings.unlimited"),
                            )
                            .on_hover_text(crate::i18n::t(
                                "settings_hint.retype_layout_sample_unlimited",
                            ));
                        });
                        ui.end_row();
                    });
            },
        );
    }
}
