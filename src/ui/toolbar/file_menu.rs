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
use crate::i18n::t;
use crate::ui::menu_entry::menu_entry;

pub(super) fn file_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        has_raw_content,
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
            let entry = |ui: &mut Ui, label: &str, hint: &str, why: Option<String>| {
                menu_entry(ui, &t(label), &t(hint), why.as_deref())
            };
            if entry(ui, "file_menu.new_file", "file_menu.new_file_hint", None) {
                action.new_file = true;
                ui.close();
            }
            if entry(ui, "file_menu.new_table", "file_menu.new_table_hint", None) {
                action.new_table = true;
                ui.close();
            }
            if entry(ui, "common.open", "file_menu.open_hint", None) {
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
            if entry(
                ui,
                "file_menu.open_directory",
                "file_menu.open_directory_hint",
                None,
            ) {
                action.open_directory = true;
                ui.close();
            }
            let why_folder = (!directory_tree_open).then(|| t("greyed.no_folder_open"));
            if entry(
                ui,
                "file_menu.close_directory",
                "file_menu.close_directory_hint",
                why_folder,
            ) {
                action.close_directory = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.open_table_folder",
                "file_menu.open_table_folder_hint",
                None,
            ) {
                action.open_table_folder = true;
                ui.close();
            }
            // Belongs with the other ways IN, not with the
            // save/export block below: it was gated on
            // `has_data`, so the one moment you need it - an
            // empty window and a URL to open - was the one
            // moment it was hidden.
            if entry(ui, "file_menu.open_url", "file_menu.open_url_hint", None) {
                action.open_url = true;
                ui.close();
            }
            if entry(ui, "file_menu.open_api", "file_menu.open_api_hint", None) {
                action.open_api = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.batch_convert",
                "file_menu.batch_convert_hint",
                None,
            ) {
                action.open_batch_convert = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.schema_drift",
                "file_menu.schema_drift_hint",
                None,
            ) {
                action.open_schema_drift = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.merge_versions",
                "file_menu.merge_versions_hint",
                None,
            ) {
                action.open_merge_versions = true;
                ui.close();
            }
            if entry(ui, "file_menu.harmonise", "file_menu.harmonise_hint", None) {
                action.open_harmonise = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.cloud_connections",
                "file_menu.cloud_connections_hint",
                None,
            ) {
                action.toggle_cloud_browser = true;
                ui.close();
            }
            if entry(ui, "file_menu.databases", "file_menu.databases_hint", None) {
                action.toggle_db_browser = true;
                ui.close();
            }
            // A text document (File -> New file, a raw view) has no columns
            // but saves all the same; gating this on `has_data` left it
            // saveable by shortcut only.
            let why_save = (!(has_data || has_raw_content)).then(|| t("greyed.need_table"));
            let why_table = (!has_data).then(|| t("greyed.need_table"));
            ui.separator();
            // Every way of SAVING the open table, then a
            // separator, then every way of EXPORTING
            // something derived from it. Export workbook
            // used to sit between Save and Save as.
            let why_in_place = why_save
                .clone()
                .or_else(|| (!can_save_in_place).then(|| t("greyed.need_file")));
            if entry(ui, "common.save", "file_menu.save_hint", why_in_place) {
                action.save_file = true;
                ui.close();
            }
            if entry(
                ui,
                "common.save_as",
                "file_menu.save_as_hint",
                why_save.clone(),
            ) {
                action.save_file_as = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.save_password",
                "file_menu.save_password_hint",
                why_save.clone(),
            ) {
                action.save_with_password = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.save_cloud",
                "file_menu.save_cloud_hint",
                why_save.clone(),
            ) {
                action.save_to_cloud = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.save_to_db",
                "file_menu.save_to_db_hint",
                why_table.clone(),
            ) {
                action.open_table_to_db = true;
                ui.close();
            }
            let why_sql = (!is_db_tab).then(|| t("greyed.need_db_tab"));
            if entry(ui, "file_menu.save_sql", "file_menu.save_sql_hint", why_sql) {
                action.save_db_sql = true;
                ui.close();
            }
            // A file on disk only: a database tab has no earlier
            // versions of its own here.
            let why_versions = why_save
                .clone()
                .or_else(|| (!can_save_in_place || is_db_tab).then(|| t("greyed.need_file")));
            if entry(
                ui,
                "file_menu.versions",
                "file_menu.versions_hint",
                why_versions,
            ) {
                action.open_file_versions = true;
                ui.close();
            }
            ui.separator();

            if entry(
                ui,
                "file_menu.export_workbook",
                "file_menu.export_workbook_hint",
                why_table.clone(),
            ) {
                action.export_workbook = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.export_schema",
                "file_menu.export_schema_hint",
                why_table.clone(),
            ) {
                action.show_schema_export = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.report",
                "file_menu.report_hint",
                why_table.clone(),
            ) {
                action.open_report = true;
                ui.close();
            }
            if entry(
                ui,
                "file_menu.export_pdf",
                "file_menu.export_pdf_hint",
                why_table.clone(),
            ) {
                action.export_pdf = true;
                ui.close();
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
            if entry(ui, "file_menu.exit", "file_menu.exit_hint", None) {
                action.exit = true;
                ui.close();
            }
        },
    );
}
