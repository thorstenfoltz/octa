//! The **File** menu: new / open / recent, open-as, save and export, exit.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::OPEN_AS_FORMATS;
use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};

pub(super) fn file_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        can_save_in_place,
        is_db_tab,
        recent_files,
        directory_tree_open,
        ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.file")).color(colors.text_primary),
        |ui| {
            ui.set_min_width(180.0);
            if ui
                .button(crate::i18n::t("file_menu.new_file"))
                .on_hover_text(crate::i18n::t("file_menu.new_file_hint"))
                .clicked()
            {
                action.new_file = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("common.open"))
                .on_hover_text(crate::i18n::t("file_menu.open_hint"))
                .clicked()
            {
                action.open_file = true;
                ui.close();
            }
            // Open as... - pick the reader first, then the files. Opening the
            // picker straight from the chosen format keeps this to one step
            // and lets the picker stay unfiltered, which is the point: the
            // files worth opening this way are exactly the ones whose
            // extension Octa would otherwise route somewhere unhelpful.
            ui.menu_button(crate::i18n::t("file_menu.open_as"), |ui| {
                for (key, reader) in OPEN_AS_FORMATS {
                    if ui.button(crate::i18n::t(key)).clicked() {
                        action.open_as_files = Some(reader);
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(crate::i18n::t("file_menu.open_as_hint"));
            if ui
                .button(crate::i18n::t("file_menu.open_table_folder"))
                .on_hover_text(crate::i18n::t("file_menu.open_table_folder_hint"))
                .clicked()
            {
                action.open_table_folder = true;
                ui.close();
            }
            // Belongs with the other ways IN, not with the
            // save/export block below: it was gated on
            // `has_data`, so the one moment you need it - an
            // empty window and a URL to open - was the one
            // moment it was hidden.
            if ui
                .button(crate::i18n::t("file_menu.open_url"))
                .on_hover_text(crate::i18n::t("file_menu.open_url_hint"))
                .clicked()
            {
                action.open_url = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("file_menu.batch_convert"))
                .on_hover_text(crate::i18n::t("file_menu.batch_convert_hint"))
                .clicked()
            {
                action.open_batch_convert = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("file_menu.schema_drift"))
                .on_hover_text(crate::i18n::t("file_menu.schema_drift_hint"))
                .clicked()
            {
                action.open_schema_drift = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("file_menu.harmonise"))
                .on_hover_text(crate::i18n::t("file_menu.harmonise_hint"))
                .clicked()
            {
                action.open_harmonise = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("file_menu.open_directory"))
                .on_hover_text(crate::i18n::t("file_menu.open_directory_hint"))
                .clicked()
            {
                action.open_directory = true;
                ui.close();
            }
            if directory_tree_open
                && ui
                    .button(crate::i18n::t("file_menu.close_directory"))
                    .on_hover_text(crate::i18n::t("file_menu.close_directory_hint"))
                    .clicked()
            {
                action.close_directory = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("file_menu.cloud_connections"))
                .on_hover_text(crate::i18n::t("file_menu.cloud_connections_hint"))
                .clicked()
            {
                action.toggle_cloud_browser = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("file_menu.databases"))
                .on_hover_text(crate::i18n::t("file_menu.databases_hint"))
                .clicked()
            {
                action.toggle_db_browser = true;
                ui.close();
            }
            if has_data {
                ui.separator();
                // Every way of SAVING the open table, then a
                // separator, then every way of EXPORTING
                // something derived from it. Export workbook
                // used to sit between Save and Save as.
                if can_save_in_place
                    && ui
                        .button(crate::i18n::t("common.save"))
                        .on_hover_text(crate::i18n::t("file_menu.save_hint"))
                        .clicked()
                {
                    action.save_file = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("common.save_as"))
                    .on_hover_text(crate::i18n::t("file_menu.save_as_hint"))
                    .clicked()
                {
                    action.save_file_as = true;
                    ui.close();
                }
                if is_db_tab
                    && ui
                        .button(crate::i18n::t("file_menu.save_sql"))
                        .on_hover_text(crate::i18n::t("file_menu.save_sql_hint"))
                        .clicked()
                {
                    action.save_db_sql = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("file_menu.save_to_db"))
                    .on_hover_text(crate::i18n::t("file_menu.save_to_db_hint"))
                    .clicked()
                {
                    action.open_table_to_db = true;
                    ui.close();
                }

                ui.separator();

                if ui
                    .button(crate::i18n::t("file_menu.export_workbook"))
                    .on_hover_text(crate::i18n::t("file_menu.export_workbook_hint"))
                    .clicked()
                {
                    action.export_workbook = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("file_menu.export_schema"))
                    .on_hover_text(crate::i18n::t("file_menu.export_schema_hint"))
                    .clicked()
                {
                    action.show_schema_export = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("file_menu.report"))
                    .on_hover_text(crate::i18n::t("file_menu.report_hint"))
                    .clicked()
                {
                    action.open_report = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("file_menu.export_pdf"))
                    .on_hover_text(crate::i18n::t("file_menu.export_pdf_hint"))
                    .clicked()
                {
                    action.export_pdf = true;
                    ui.close();
                }
            }
            ui.separator();
            ui.menu_button(crate::i18n::t("menu.recent_files"), |ui| {
                ui.set_min_width(250.0);
                if recent_files.is_empty() {
                    ui.add_enabled(
                        false,
                        egui::Button::new(crate::i18n::t("file_menu.recent_none")),
                    );
                } else {
                    for path in recent_files {
                        let filename = std::path::Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.clone());
                        let resp = ui.button(&filename).on_hover_text(path);
                        if resp.clicked() {
                            action.open_recent = Some(path.clone());
                            ui.close();
                        }
                        resp.context_menu(|ui| {
                            if ui
                                .button(crate::i18n::t("file_menu.remove_from_list"))
                                .clicked()
                            {
                                action.remove_recent = Some(path.clone());
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(crate::i18n::t("file_menu.clear_all")).clicked() {
                                action.clear_recent = true;
                                ui.close();
                            }
                        });
                    }
                }
            });
            ui.separator();
            if ui
                .button(crate::i18n::t("file_menu.exit"))
                .on_hover_text(crate::i18n::t("file_menu.exit_hint"))
                .clicked()
            {
                action.exit = true;
                ui.close();
            }
        },
    );
}
