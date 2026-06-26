//! Render the sidebar (cloud connections browser and/or directory tree) when
//! open, and dispatch their actions back to `OctaApp`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::cloud::{CloudConnection, CloudKind};
use octa::ui;

use super::cloud_browser::{ConnPrefix, ListState, SignInState};
use super::cloud_tree::{self, CloudTreeAction, TreeCtx};
use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn render_sidebar(&mut self, parent_ui: &mut egui::Ui) {
        let cloud_visible = self.cloud_browser.visible;
        let dir_open = self.directory_tree.is_some();
        if !cloud_visible && !dir_open {
            return;
        }
        let ctx = parent_ui.ctx().clone();

        // Build the openable-extension allowlist once per frame when the
        // filter is on. Includes the registry's extensions plus any the user
        // forced into text mode, all lowercased. `None` lists every file.
        let allowed_exts: Option<HashSet<String>> = if self.settings.directory_tree_filter_enabled {
            let mut set: HashSet<String> = self
                .registry
                .all_extensions()
                .into_iter()
                .map(|e| e.to_ascii_lowercase())
                .collect();
            for e in &self.settings.text_mode_extensions {
                set.insert(e.trim_start_matches('.').to_ascii_lowercase());
            }
            Some(set)
        } else {
            None
        };

        // Cloud data the body closure reads. Cloned/Arc-cloned so the closure
        // borrows locals, not `self` (mirrors how the directory tree extracts
        // `state` before the panel).
        let connections: Vec<CloudConnection> = self.settings.cloud_connections.clone();
        // Memoise the two per-connection lookups that touch the OS (CLI on
        // PATH; secret in keyring) so they run once, not every repaint. Copies
        // are passed into the renderer, which therefore never does IO.
        if cloud_visible {
            for conn in &connections {
                self.cloud_browser
                    .cli_cache
                    .entry(conn.kind)
                    .or_insert_with(|| octa::cloud::cli_available(conn.kind));
                let id = conn.id.clone();
                if !self.cloud_browser.secret_cache.contains_key(&id) {
                    let present = !matches!(
                        octa::ui::settings::cloud_secrets::cloud_secret_storage(
                            &id,
                            &self.settings
                        ),
                        octa::ui::settings::secrets::KeyStorage::None
                    );
                    self.cloud_browser.secret_cache.insert(id, present);
                }
            }
        }
        let cli_cache: HashMap<CloudKind, bool> = self.cloud_browser.cli_cache.clone();
        let secret_cache: HashMap<String, bool> = self.cloud_browser.secret_cache.clone();
        let sign_out_confirm: Option<String> = self.cloud_browser.sign_out_confirm.clone();
        let expanded: HashSet<ConnPrefix> = self.cloud_browser.expanded.clone();
        let listings_arc: Arc<Mutex<HashMap<ConnPrefix, ListState>>> =
            self.cloud_browser.listings.clone();
        let signin_arc: Arc<Mutex<HashMap<String, SignInState>>> =
            self.cloud_browser.sign_in_status.clone();

        let position = self.settings.directory_tree_position;
        let allowed_ref = allowed_exts.as_ref();
        let mut dir_state = self.directory_tree.as_mut();

        let screen_w = parent_ui.ctx().content_rect().width();
        let default_w = (screen_w * 0.5).clamp(160.0, screen_w - 160.0);
        let max_w = (screen_w - 80.0).max(160.0);

        let mut cloud_action = CloudTreeAction::default();
        let mut tree_action = ui::directory_tree::TreeAction::default();

        let mut body = |ui: &mut egui::Ui| {
            if cloud_visible {
                if let (Ok(listings), Ok(signin)) = (listings_arc.lock(), signin_arc.lock()) {
                    let tree_ctx = TreeCtx {
                        listings: &listings,
                        expanded: &expanded,
                        sign_in: &signin,
                        cli_avail: &cli_cache,
                        secret_present: &secret_cache,
                        sign_out_confirm: sign_out_confirm.as_deref(),
                    };
                    cloud_action =
                        cloud_tree::render_cloud_tree(ui, &connections, &tree_ctx, dir_open);
                }
                if dir_open {
                    ui.separator();
                }
            }
            if let Some(state) = dir_state.as_deref_mut() {
                tree_action = ui::directory_tree::render_directory_tree(ui, state, allowed_ref);
            }
        };

        match position {
            ui::settings::DirectoryTreePosition::Left => {
                egui::Panel::left("directory_tree_panel")
                    .resizable(true)
                    .default_size(default_w)
                    .size_range(80.0..=max_w)
                    .show_inside(parent_ui, &mut body);
            }
            ui::settings::DirectoryTreePosition::Right => {
                egui::Panel::right("directory_tree_panel")
                    .resizable(true)
                    .default_size(default_w)
                    .size_range(80.0..=max_w)
                    .show_inside(parent_ui, &mut body);
            }
        }

        // Dispatch cloud actions.
        if cloud_action.close {
            self.cloud_browser.visible = false;
        }
        if let Some((conn_id, prefix)) = cloud_action.toggle {
            self.toggle_cloud_node(&ctx, conn_id, prefix);
        }
        if let Some((conn_id, key, name)) = cloud_action.open {
            self.open_cloud_object(&ctx, conn_id, key, name);
        }
        if let Some(conn_id) = cloud_action.sign_in {
            self.cloud_sign_in(&ctx, conn_id);
        }
        if let Some(conn_id) = cloud_action.sign_out_arm {
            self.arm_cloud_sign_out(Some(conn_id));
        }
        if let Some(conn_id) = cloud_action.sign_out_yes {
            self.cloud_sign_out(conn_id);
        }
        if cloud_action.sign_out_cancel {
            self.arm_cloud_sign_out(None);
        }
        if let Some(conn_id) = cloud_action.refresh {
            self.refresh_cloud_conn(&ctx, conn_id);
        }

        // Dispatch directory-tree actions.
        if tree_action.close {
            self.directory_tree = None;
        } else if let Some(path) = tree_action.open_file {
            self.load_file(path);
        }
    }
}
