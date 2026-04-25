//! Render the directory tree sidebar when open, and dispatch tree actions
//! (close sidebar, open file) back to `OctaApp`.

use eframe::egui;

use octa::ui;

use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn render_sidebar(&mut self, ctx: &egui::Context) {
        if self.directory_tree.is_none() {
            return;
        }
        let tree_action = {
            let position = self.settings.directory_tree_position;
            let state = self.directory_tree.as_mut().unwrap();
            let mut action = ui::directory_tree::TreeAction::default();
            // Default to a 50/50 split the first time the sidebar is shown;
            // subsequent frames honor whatever width the user has dragged the
            // separator to.
            let screen_w = ctx.screen_rect().width();
            let default_w = (screen_w * 0.5).clamp(160.0, screen_w - 160.0);
            let max_w = (screen_w - 80.0).max(160.0);
            match position {
                ui::settings::DirectoryTreePosition::Left => {
                    egui::SidePanel::left("directory_tree_panel")
                        .resizable(true)
                        .default_width(default_w)
                        .width_range(80.0..=max_w)
                        .show(ctx, |ui| {
                            action = ui::directory_tree::render_directory_tree(ui, state);
                        });
                }
                ui::settings::DirectoryTreePosition::Right => {
                    egui::SidePanel::right("directory_tree_panel")
                        .resizable(true)
                        .default_width(default_w)
                        .width_range(80.0..=max_w)
                        .show(ctx, |ui| {
                            action = ui::directory_tree::render_directory_tree(ui, state);
                        });
                }
            }
            action
        };
        if tree_action.close {
            self.directory_tree = None;
        } else if let Some(path) = tree_action.open_file {
            self.load_file(path);
        }
    }
}
