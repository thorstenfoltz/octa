//! Dispatch for the Help menu's [`ToolbarAction`] fields.
//!
//! One file per menu, mirroring `ui::toolbar::help_menu`, so a menu entry and
//! the code it runs sit in matching files.

use eframe::egui;

use octa::ui;

use crate::app::state::OctaApp;

impl OctaApp {
    /// Run every Help-menu action set on `action`.
    pub(super) fn dispatch_help_menu(
        &mut self,
        ctx: &egui::Context,
        action: &ui::toolbar::ToolbarAction,
    ) {
        if action.show_documentation {
            self.show_documentation_dialog = true;
        }
        if action.show_settings {
            self.settings_dialog.open(&self.settings);
        }
        if action.show_about {
            self.show_about_dialog = true;
        }
        if action.show_ai_report {
            self.show_ai_report_dialog = true;
        }
        if action.check_for_updates {
            self.show_update_dialog = true;
            self.check_for_updates(ctx);
        }
        if action.export_debug_report {
            self.export_debug_report_now();
        }
    }
}
