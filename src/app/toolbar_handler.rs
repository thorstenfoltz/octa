//! Render the top toolbar (via `ui::toolbar::draw_toolbar`) and dispatch its
//! [`ToolbarAction`] back to the corresponding method or state mutation.

use eframe::egui;

use octa::data::ViewMode;
use octa::ui;

use super::state::{ColumnInspectorSort, OctaApp, TabState};

impl OctaApp {
    pub(crate) fn render_toolbar(&mut self, parent_ui: &mut egui::Ui) {
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        let header_colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let toolbar_frame = egui::Frame::new()
            .fill(header_colors.bg_header)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .stroke(egui::Stroke::new(1.0, header_colors.border_subtle));
        egui::Panel::top("toolbar")
            .exact_size(40.0)
            .frame(toolbar_frame)
            .show_inside(parent_ui, |ui| {
                self.ensure_logo_textures(ctx);

                let tab = &mut self.tabs[self.active_tab];
                let action = ui::toolbar::draw_toolbar(
                    ui,
                    self.theme_mode,
                    &mut tab.search_text,
                    &mut tab.search_mode,
                    self.search_focus_requested,
                    tab.show_replace_bar,
                    &mut tab.replace_text,
                    tab.table.col_count() > 0,
                    tab.table.is_modified(),
                    tab.table.source_path.is_some(),
                    tab.table_state.selected_cell,
                    &tab.table_state.selected_rows,
                    &tab.table_state.selected_cols,
                    &tab.table_state.selected_cells,
                    tab.table.row_count(),
                    tab.table.col_count(),
                    tab.view_mode,
                    tab.raw_content.is_some(),
                    tab.table.format_name.as_deref() == Some("Markdown"),
                    tab.table.format_name.as_deref() == Some("Jupyter Notebook"),
                    !tab.epub_chapters_md.is_empty(),
                    tab.table.format_name.as_deref() == Some("GeoJSON"),
                    tab.json_value.is_some(),
                    tab.yaml_value.is_some(),
                    self.readonly_mode,
                    tab.sql_panel_open,
                    self.zoom_percent,
                    self.logo_texture.as_ref(),
                    &self.recent_files,
                    self.directory_tree.is_some(),
                    tab.first_row_is_header,
                    !tab.hidden_columns.is_empty(),
                    !tab.table.undo_stack.is_empty(),
                    !tab.table.redo_stack.is_empty(),
                    !self.recently_closed_tabs.is_empty(),
                    &self.settings.shortcuts,
                    &tab.table,
                    self.settings.use_custom_title_bar,
                );
                self.search_focus_requested = false;

                self.dispatch_toolbar_action(ctx, action);
            });
    }

    /// Lazily build the two logo textures the first frame they're needed (or
    /// after the icon variant changes). The toolbar needs the small one; the
    /// welcome screen needs the high-resolution one.
    fn ensure_logo_textures(&mut self, ctx: &egui::Context) {
        if self.logo_texture.is_some() && self.welcome_logo_texture.is_some() {
            return;
        }
        let opt = resvg::usvg::Options::default();
        let svg_src = self.resolved_icon.svg_source();
        let Ok(tree) = resvg::usvg::Tree::from_str(svg_src, &opt) else {
            return;
        };
        if self.logo_texture.is_none() {
            let size = tree.size();
            let (w, h) = (size.width() as u32, size.height() as u32);
            if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) {
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                );
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    pixmap.data(),
                );
                self.logo_texture =
                    Some(ctx.load_texture("octa_logo", image, egui::TextureOptions::LINEAR));
            }
        }
        if self.welcome_logo_texture.is_none() {
            let render_size = 512u32;
            let size = tree.size();
            let sx = render_size as f32 / size.width();
            let sy = render_size as f32 / size.height();
            if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(render_size, render_size) {
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::from_scale(sx, sy),
                    &mut pixmap.as_mut(),
                );
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [render_size as usize, render_size as usize],
                    pixmap.data(),
                );
                self.welcome_logo_texture = Some(ctx.load_texture(
                    "octa_welcome_logo",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
    }

    fn dispatch_toolbar_action(&mut self, ctx: &egui::Context, action: ui::toolbar::ToolbarAction) {
        if action.new_file {
            let mut new_tab = TabState::new(self.settings.default_search_mode);
            new_tab.view_mode = ViewMode::Raw;
            new_tab.raw_content = Some(String::new());
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }
        if action.open_file {
            self.open_file();
        }
        if action.open_directory
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.directory_tree = Some(ui::directory_tree::DirectoryTreeState::new(path));
        }
        if action.close_directory {
            self.directory_tree = None;
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
        if action.exit {
            if self.tabs[self.active_tab].is_modified() && !self.confirmed_close {
                self.show_close_confirm = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if action.toggle_theme {
            self.theme_mode = self.theme_mode.toggle();
            self.apply_zoom(ctx);
        }
        if action.zoom_in {
            self.zoom_percent = (self.zoom_percent + 5).min(500);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action.zoom_out {
            self.zoom_percent = self.zoom_percent.saturating_sub(5).max(25);
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action.zoom_reset {
            self.zoom_percent = 100;
            self.apply_zoom(ctx);
            self.tabs[self.active_tab]
                .table_state
                .invalidate_row_heights();
        }
        if action.search_changed {
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.toggle_replace_bar {
            self.tabs[self.active_tab].show_replace_bar =
                !self.tabs[self.active_tab].show_replace_bar;
        }
        if action.replace_next {
            self.replace_next_match();
        }
        if action.replace_all {
            self.replace_all_matches();
        }

        if let Some(new_mode) = action.view_mode_changed {
            self.tabs[self.active_tab].view_mode = new_mode;
        }

        if action.toggle_sql_panel {
            let tab = &mut self.tabs[self.active_tab];
            tab.sql_panel_open = !tab.sql_panel_open;
        }

        if action.toggle_readonly {
            self.toggle_readonly();
        }

        if action.search_focus {
            self.search_focus_requested = true;
        }

        if action.show_documentation {
            self.show_documentation_dialog = true;
        }
        if action.show_settings {
            self.settings_dialog.open(&self.settings);
        }
        if action.show_about {
            self.show_about_dialog = true;
        }
        if action.check_for_updates {
            self.show_update_dialog = true;
            self.check_for_updates(ctx);
        }

        if let Some(scope) = action.parse_in_new_tab {
            let tab = &self.tabs[self.active_tab];
            self.pending_parse_modal =
                super::dialogs::parse_in_new_tab::build_modal_state(tab, scope);
        }

        if action.add_row {
            let insert_at = match self.tabs[self.active_tab].table_state.selected_cell {
                Some((row, _)) => row + 1,
                None => self.tabs[self.active_tab].table.row_count(),
            };
            self.tabs[self.active_tab].table.insert_row(insert_at);
            let sel_col = self.tabs[self.active_tab]
                .table_state
                .selected_cell
                .map(|(_, c)| c)
                .unwrap_or(0);
            self.tabs[self.active_tab].table_state.selected_cell = Some((insert_at, sel_col));
            self.tabs[self.active_tab].table_state.editing_cell = None;
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.delete_row
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
        {
            self.tabs[self.active_tab].table.delete_row(row);
            self.tabs[self.active_tab].table_state.editing_cell = None;
            if self.tabs[self.active_tab].table.row_count() == 0 {
                self.tabs[self.active_tab].table_state.selected_cell = None;
            } else {
                let new_row = row.min(self.tabs[self.active_tab].table.row_count() - 1);
                self.tabs[self.active_tab].table_state.selected_cell = Some((new_row, col));
            }
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.move_row_up
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && row > 0
        {
            self.tabs[self.active_tab].table.move_row(row, row - 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row - 1, col));
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.move_row_down
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && row + 1 < self.tabs[self.active_tab].table.row_count()
        {
            self.tabs[self.active_tab].table.move_row(row, row + 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row + 1, col));
            self.tabs[self.active_tab].filter_dirty = true;
        }

        if action.add_column {
            self.tabs[self.active_tab].show_add_column_dialog = true;
            self.tabs[self.active_tab].new_col_name.clear();
            self.tabs[self.active_tab].new_col_type = "String".to_string();
            self.tabs[self.active_tab].new_col_formula.clear();
            self.tabs[self.active_tab].insert_col_at = self.tabs[self.active_tab]
                .table_state
                .selected_cell
                .map(|(_, c)| c + 1);
        }
        if action.delete_column && self.tabs[self.active_tab].table.col_count() > 0 {
            self.open_delete_columns_dialog();
        }
        if action.move_col_left
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && col > 0
        {
            self.tabs[self.active_tab].table.move_column(col, col - 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row, col - 1));
            self.tabs[self.active_tab].table_state.widths_initialized = false;
        }
        if action.move_col_right
            && let Some((row, col)) = self.tabs[self.active_tab].table_state.selected_cell
            && col + 1 < self.tabs[self.active_tab].table.col_count()
        {
            self.tabs[self.active_tab].table.move_column(col, col + 1);
            self.tabs[self.active_tab].table_state.selected_cell = Some((row, col + 1));
            self.tabs[self.active_tab].table_state.widths_initialized = false;
        }
        if let Some(col_idx) = action.sort_rows_asc_by {
            self.tabs[self.active_tab]
                .table
                .sort_rows_by_column(col_idx, true);
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if let Some(col_idx) = action.sort_rows_desc_by {
            self.tabs[self.active_tab]
                .table
                .sort_rows_by_column(col_idx, false);
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.sort_columns_asc {
            self.sort_columns_alphabetically(true);
        }
        if action.sort_columns_desc {
            self.sort_columns_alphabetically(false);
        }
        if action.show_column_inspector {
            let tab = &mut self.tabs[self.active_tab];
            tab.show_column_inspector = true;
            tab.column_inspector_sort = ColumnInspectorSort::Default;
        }
        if action.show_all_columns {
            self.tabs[self.active_tab].hidden_columns.clear();
        }
        if let Some(preselect) = action.show_column_filter {
            self.open_column_filter_dialog(preselect);
        }
        if action.logo_clicked {
            self.register_logo_click(ctx);
        }

        if action.discard_edits {
            self.tabs[self.active_tab].table.discard_edits();
        }

        if action.toggle_first_row_header {
            let tab = &mut self.tabs[self.active_tab];
            if tab.first_row_is_header {
                tab.table.promote_headers_to_row();
                tab.first_row_is_header = false;
            } else {
                tab.table.promote_first_row_to_headers();
                tab.first_row_is_header = true;
            }
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
            tab.table_state.editing_cell = None;
            tab.table_state.selected_rows.clear();
            tab.table_state.selected_cols.clear();
            if tab.table.row_count() > 0 && tab.table.col_count() > 0 {
                tab.table_state.selected_cell = Some((0, 0));
            } else {
                tab.table_state.selected_cell = None;
            }
        }

        for (key, color) in action.set_marks {
            self.tabs[self.active_tab].table.set_mark(key, color);
        }
        for key in action.clear_marks {
            self.tabs[self.active_tab].table.clear_mark(key);
        }
        if action.clear_all_marks {
            self.tabs[self.active_tab].table.clear_all_marks();
        }

        if action.undo {
            self.do_undo();
        }
        if action.redo {
            self.do_redo();
        }
        if action.reopen_last_closed_tab {
            self.reopen_last_closed_tab(ctx);
        }
        if action.fit_all_columns {
            self.tabs[self.active_tab]
                .table_state
                .fit_all_columns_requested = true;
        }
        if action.compare_with {
            self.begin_compare_with();
        }
        if action.show_schema_export {
            super::dialogs::schema_export::open(self);
        }
        if action.toggle_multi_search {
            self.toggle_multi_search();
        }
        if action.open_chart_tab {
            self.open_chart_tab();
        }
        if action.show_find_duplicates {
            let tab = &mut self.tabs[self.active_tab];
            if tab.table.col_count() > 0 {
                // Seed the key with the currently selected column (or the
                // selected cell's column) so common workflows don't need
                // an extra click. Otherwise leave empty and let the user
                // tick boxes.
                tab.find_duplicates_key_cols.clear();
                if !tab.table_state.selected_cols.is_empty() {
                    for &c in &tab.table_state.selected_cols {
                        if c < tab.table.col_count() {
                            tab.find_duplicates_key_cols.insert(c);
                        }
                    }
                } else if let Some((_, c)) = tab.table_state.selected_cell
                    && c < tab.table.col_count()
                {
                    tab.find_duplicates_key_cols.insert(c);
                }
                tab.show_find_duplicates = true;
            }
        }
    }
}
