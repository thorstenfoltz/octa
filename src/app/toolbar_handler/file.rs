//! Dispatch for the File menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::file_menu`, so a menu entry and
//! the code it runs sit in matching files.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every File-menu action set on `action`.
    pub(super) fn dispatch_file_menu(
        &mut self,
        ctx: &egui::Context,
        action: &ui::toolbar::ToolbarAction,
    ) {
        if action.new_file {
            self.new_file();
        }
        if action.open_file {
            self.open_file();
        }
        if action.open_table_folder
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            // load_file detects Delta/Iceberg table directories; a plain
            // folder surfaces a clear status message.
            self.load_file_in_new_tab(path);
        }
        if action.open_batch_convert
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            crate::app::dialogs::batch_convert::open_for_folder(self, &path);
        }
        if action.open_directory
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.directory_tree = Some(ui::directory_tree::DirectoryTreeState::new(path));
        }
        if action.close_directory {
            self.directory_tree = None;
        }
        if action.toggle_cloud_browser {
            self.toggle_cloud_browser();
        }
        if action.toggle_db_browser {
            self.toggle_db_browser();
        }
        if let Some(ref path) = action.open_recent {
            let path_buf = std::path::PathBuf::from(path);
            if path_buf.exists() {
                self.load_file(path_buf);
            } else {
                self.recent_files.retain(|p| p != path);
                self.save_recent_files();
                self.status_message =
                    Some((format!("File not found: {path}"), std::time::Instant::now()));
            }
        }
        if let Some(ref path) = action.remove_recent {
            self.recent_files.retain(|p| p != path);
            self.save_recent_files();
        }
        if action.clear_recent {
            self.recent_files.clear();
            self.save_recent_files();
        }
        if action.save_file {
            self.save_file();
        }
        if action.save_file_as {
            self.save_file_as();
        }
        if action.save_db_sql {
            self.save_db_sql(self.active_tab);
        }
        if action.export_workbook {
            self.open_workbook_dialog();
        }
        if action.open_url {
            self.open_url_dialog();
        }
        if action.exit {
            if self.tabs[self.active_tab].is_modified() && !self.confirmed_close {
                self.show_close_confirm = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Some(reader_name) = action.open_as_files {
            self.open_files_as(reader_name);
        }
        if action.show_schema_export {
            crate::app::dialogs::schema_export::open(self);
        }
        if action.open_schema_drift {
            self.schema_drift_dialog =
                Some(crate::app::state::SchemaDriftState::new(String::new()));
        }
        if action.open_harmonise {
            self.harmonise_dialog = Some(crate::app::state::HarmoniseState::new(String::new()));
        }
        if action.open_report && self.tabs[self.active_tab].table.col_count() > 0 {
            crate::app::dialogs::report::open_report_dialog(self);
        }
        if action.export_pdf && self.tabs[self.active_tab].table.col_count() > 0 {
            self.pdf_export_dialog = Some(crate::app::state::PdfExportState::default());
        }
        if action.open_table_to_db {
            self.open_table_to_db_dialog();
        }
    }
}
