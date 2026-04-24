//! Thin wrapper around the shared `ui::table_picker` widget; dispatches its
//! [`TablePickerAction`] back to [`OctaApp::load_table`].

use eframe::egui;

use octa::ui;

use super::super::state::OctaApp;

pub(crate) fn render_table_picker(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(state) = app.pending_table_picker.as_mut() else {
        return;
    };
    let action = ui::table_picker::render_table_picker(ctx, state);
    match action {
        ui::table_picker::TablePickerAction::None => {}
        ui::table_picker::TablePickerAction::Cancel => {
            app.pending_table_picker = None;
        }
        ui::table_picker::TablePickerAction::Open(path, table_name) => {
            app.pending_table_picker = None;
            app.load_table(path, table_name);
        }
    }
}
