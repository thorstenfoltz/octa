//! Render the sidebar (cloud connections browser and/or directory tree) when
//! open, and dispatch their actions back to `OctaApp`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use eframe::egui;

use octa::cloud::{CloudConnection, CloudKind};
use octa::ui;

use super::cloud_browser::{CloudSelection, ConnPrefix, ListState, SignInState};
use super::cloud_tree::{self, CloudTreeAction, TreeCtx};
use super::db_browser::{ConnSchema, DbListState};
use super::db_tree::{self, DbTreeAction};
use super::state::OctaApp;

impl OctaApp {
    pub(crate) fn render_sidebar(&mut self, parent_ui: &mut egui::Ui) {
        let cloud_visible = self.cloud_browser.visible;
        let db_visible = self.db_browser.visible;
        let dir_open = self.directory_tree.is_some();
        if !cloud_visible && !db_visible && !dir_open {
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
        let cloud_sort = self.cloud_browser.sort;
        let cloud_selected: HashSet<CloudSelection> = self.cloud_browser.selected.clone();
        let expanded: HashSet<ConnPrefix> = self.cloud_browser.expanded.clone();
        let listings_arc: Arc<Mutex<HashMap<ConnPrefix, ListState>>> =
            self.cloud_browser.listings.clone();
        let signin_arc: Arc<Mutex<HashMap<String, SignInState>>> =
            self.cloud_browser.sign_in_status.clone();

        // Database-browser data the body closure reads (same borrow dance).
        let db_connections: Vec<octa::db::DbConnection> = self.settings.db_connections.clone();
        let db_expanded: HashSet<ConnSchema> = self.db_browser.expanded.clone();
        let db_listings_arc: Arc<Mutex<HashMap<ConnSchema, DbListState>>> =
            self.db_browser.listings.clone();

        let position = self.settings.directory_tree_position;
        let allowed_ref = allowed_exts.as_ref();
        let mut dir_state = self.directory_tree.as_mut();

        let content_rect = parent_ui.ctx().content_rect();
        let screen_w = content_rect.width();
        let screen_h = content_rect.height();
        // Left/right dock: resize by width. Top/bottom dock: resize by height.
        let default_w = (screen_w * 0.5).clamp(160.0, screen_w - 160.0);
        let max_w = (screen_w - 80.0).max(160.0);
        let default_h = (screen_h * 0.35).clamp(120.0, (screen_h - 120.0).max(120.0));
        let max_h = (screen_h - 80.0).max(120.0);

        let mut cloud_action = CloudTreeAction::default();
        let mut db_action = DbTreeAction::default();
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
                        sort: cloud_sort,
                        selected: &cloud_selected,
                    };
                    cloud_action = cloud_tree::render_cloud_tree(
                        ui,
                        &connections,
                        &tree_ctx,
                        dir_open || db_visible,
                    );
                }
                if dir_open || db_visible {
                    ui.separator();
                }
            }
            if db_visible {
                if let Ok(listings) = db_listings_arc.lock() {
                    db_action = db_tree::render_db_tree(
                        ui,
                        &db_connections,
                        &listings,
                        &db_expanded,
                        dir_open,
                    );
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
            ui::settings::DirectoryTreePosition::Top => {
                egui::Panel::top("directory_tree_panel")
                    .resizable(true)
                    .default_size(default_h)
                    .size_range(80.0..=max_h)
                    .show_inside(parent_ui, &mut body);
            }
            ui::settings::DirectoryTreePosition::Bottom => {
                egui::Panel::bottom("directory_tree_panel")
                    .resizable(true)
                    .default_size(default_h)
                    .size_range(80.0..=max_h)
                    .show_inside(parent_ui, &mut body);
            }
        }

        // Dispatch cloud actions.
        if cloud_action.close {
            self.cloud_browser.visible = false;
        }
        if cloud_action.add_connection {
            // `open` already clears the cloud form, so the Cloud section comes
            // up expanded with a blank connection ready to fill in.
            self.settings_dialog.open(&self.settings);
            self.settings_dialog.focus_cloud_section = true;
            self.settings_dialog.focus_cloud_form = true;
        }
        if let Some(sel) = cloud_action.toggle_select
            && !self.cloud_browser.selected.remove(&sel)
        {
            self.cloud_browser.selected.insert(sel);
        }
        if cloud_action.clear_selection {
            self.cloud_browser.selected.clear();
        }
        if cloud_action.union_selected {
            self.union_cloud_selection(&ctx);
        }
        if let Some(sort) = cloud_action.set_sort {
            self.cloud_browser.sort = sort;
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
        if let Some((conn_id, prefix)) = cloud_action.inventory {
            self.cloud_inventory(&ctx, conn_id, prefix);
        }

        // Dispatch database-tree actions.
        if db_action.close {
            self.db_browser.visible = false;
        }
        if db_action.add_connection {
            self.settings_dialog.open(&self.settings);
            self.settings_dialog.focus_db_section = true;
        }
        if let Some((conn_id, schema)) = db_action.toggle {
            self.toggle_db_node(&ctx, conn_id, schema);
        }
        if let Some((conn_id, catalog, schema, table)) = db_action.open {
            self.open_db_table(&ctx, conn_id, catalog, schema, table);
        }
        if let Some((conn_id, catalog, schema, table)) = db_action.copy {
            self.open_db_copy_dialog(conn_id, catalog, schema, table);
        }
        if let Some((conn_id, catalog, schema, table)) = db_action.metadata {
            self.open_db_metadata(&ctx, conn_id, catalog, schema, table);
        }
        if let Some(conn_id) = db_action.refresh {
            self.refresh_db_conn(&ctx, conn_id);
        }

        // Dispatch directory-tree actions.
        if tree_action.close {
            self.directory_tree = None;
        } else if let Some(files) = tree_action.union_files {
            self.open_union_for_files(files);
        } else if let Some(dir) = tree_action.open_dataset {
            // Routes through load_file's `is_dir()` branch -> dataset /
            // lakehouse detection.
            self.load_file_in_new_tab(dir);
        } else if let Some(path) = tree_action.open_file {
            self.load_file(path);
        }
    }
}
