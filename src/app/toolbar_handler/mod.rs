//! Render the top toolbar (via `ui::toolbar::draw_toolbar`) and dispatch its
//! [`ToolbarAction`] back to the corresponding method or state mutation.

use eframe::egui;

use octa::ui;

use super::state::OctaApp;

// One dispatch file per menu, mirroring `ui::toolbar`'s eight menu files.
mod analyse;
mod columns;
mod data;
mod edit;
mod file;
mod help;
mod search;
mod view;

/// Height of the toolbar's widget row plus its frame margins.
const TOOLBAR_ROW_H: f32 = 40.0;
/// Id of the single foreground layer every window-resize grab strip lives in.
const HANDLE_LAYER: &str = "octa_window_resize_handles";
/// Total height of the toolbar panel: the widget row plus the strip its
/// horizontal scrollbar lives in. One definition, used both for the panel itself
/// and to keep the window resize-grab strips clear of it.
const TOOLBAR_H: f32 = TOOLBAR_ROW_H + ui::toolbar::SCROLL_BAR_STRIP;

impl OctaApp {
    /// Paint invisible resize-grab strips along the window edges and corners
    /// when running with a custom title bar.
    ///
    /// A borderless window (`with_decorations(false)`) loses the WM's resize
    /// frame on most compositors, so without this the window can't be resized
    /// at all. Each strip is a drag-sensing rect in one shared foreground layer
    /// that hands control to the windowing system via
    /// [`egui::ViewportCommand::BeginResize`] (winit's native
    /// `drag_resize_window`), so the actual resize is done by the OS.
    ///
    /// Two details make it feel like a native border rather than a clunky
    /// widget:
    /// - We fire on the *press* (`is_pointer_button_down_on`), not on a drag
    ///   threshold. `Sense::drag()` only reports `drag_started()` after the
    ///   cursor has moved ~6px, so a press in a small corner did nothing until
    ///   it had already slid off - the OS border engages instantly, so we
    ///   match that.
    /// - The strips sense **drag only**, never clicks: a foreground click-sense
    ///   widget would *steal* clicks from whatever sits under it, which is
    ///   exactly what killed the min/max/close buttons. Drag-only lets a plain
    ///   click pass straight through (egui splits the click/drag hit-test), and
    ///   we additionally keep the side strips **below the toolbar row** so they
    ///   never overlap the menus or the window-control buttons at all. Note the
    ///   click/drag split only saves widgets a strip *partially* covers: a
    ///   foreground hit that fully covers the pointer's search radius discards
    ///   the layers beneath it whatever it senses, which is why the strips
    ///   staying their intended size is a correctness requirement, not a
    ///   cosmetic one.
    ///
    /// Geometry: bottom + side targets are generous, bottom corners are large
    /// diagonal zones (a borderless window has no forgiving margin *outside*
    /// the glass, so the easy grab area lives inside). The top stays a thin
    /// sliver plus small corners, all above the toolbar's interactive row.
    /// Strips are mutually exclusive, so every pixel belongs to one handle.
    /// Skipped while maximised (the WM owns that geometry).
    pub(crate) fn render_window_resize_handles(&self, ctx: &egui::Context) {
        if !self.settings.use_custom_title_bar {
            return;
        }
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }

        use egui::{CursorIcon, ResizeDirection as Dir, ViewportCommand};

        // Grab thicknesses in logical points.
        const EDGE: f32 = 8.0; // left / right / bottom side strips
        const TOP_EDGE: f32 = 4.0; // top: thin sliver above the toolbar
        const CORNER: f32 = 20.0; // bottom corners: big and easy to hit
        const TOP_CORNER: f32 = 8.0; // top corners: small, above the toolbar
        // Keep the side strips clear of the toolbar's interactive row (menus +
        // window-control buttons live here); `TOOLBAR_H` is the same const
        // `render_toolbar` sizes the panel with.
        let rect = ctx.viewport_rect();
        let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
        let side_top = t + TOOLBAR_H;

        // (id, grab rect, resize direction, hover cursor). Non-overlapping.
        let handles: [(&str, egui::Rect, Dir, CursorIcon); 8] = [
            (
                "resize_w",
                egui::Rect::from_min_max(egui::pos2(l, side_top), egui::pos2(l + EDGE, b - CORNER)),
                Dir::West,
                CursorIcon::ResizeWest,
            ),
            (
                "resize_e",
                egui::Rect::from_min_max(egui::pos2(r - EDGE, side_top), egui::pos2(r, b - CORNER)),
                Dir::East,
                CursorIcon::ResizeEast,
            ),
            (
                "resize_n",
                egui::Rect::from_min_max(
                    egui::pos2(l + TOP_CORNER, t),
                    egui::pos2(r - TOP_CORNER, t + TOP_EDGE),
                ),
                Dir::North,
                CursorIcon::ResizeNorth,
            ),
            (
                "resize_s",
                egui::Rect::from_min_max(
                    egui::pos2(l + CORNER, b - EDGE),
                    egui::pos2(r - CORNER, b),
                ),
                Dir::South,
                CursorIcon::ResizeSouth,
            ),
            (
                "resize_nw",
                egui::Rect::from_min_max(
                    egui::pos2(l, t),
                    egui::pos2(l + TOP_CORNER, t + TOP_CORNER),
                ),
                Dir::NorthWest,
                CursorIcon::ResizeNorthWest,
            ),
            (
                "resize_ne",
                egui::Rect::from_min_max(
                    egui::pos2(r - TOP_CORNER, t),
                    egui::pos2(r, t + TOP_CORNER),
                ),
                Dir::NorthEast,
                CursorIcon::ResizeNorthEast,
            ),
            (
                "resize_sw",
                egui::Rect::from_min_max(egui::pos2(l, b - CORNER), egui::pos2(l + CORNER, b)),
                Dir::SouthWest,
                CursorIcon::ResizeSouthWest,
            ),
            (
                "resize_se",
                egui::Rect::from_min_max(egui::pos2(r - CORNER, b - CORNER), egui::pos2(r, b)),
                Dir::SouthEast,
                CursorIcon::ResizeSouthEast,
            ),
        ];

        // All eight strips live in ONE foreground layer, each registered at an
        // absolute rect.
        //
        // Deliberately NOT `egui::Area`, which is what this was: an Area decides
        // its own rect by laying out its contents, and it can settle somewhere
        // other than the rect handed to `allocate_rect` inside it. On Linux Mint
        // (Cinnamon) the bottom strip ended up covering [[20 640] - [1900 1040]]
        // instead of its 8-point band, i.e. the lower 40% of the window. The
        // layer is Foreground, and egui's hit test stops at the first hit that
        // covers the search area and discards every layer beneath it
        // (`hit_test.rs`), so every widget in that band went dead - no hover, no
        // click - while dragging and scrolling the same dialog still worked.
        // `Ui::interact` registers exactly the rect it is given, so the strips
        // cannot wander no matter what the window manager reports.
        let ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::new(HANDLE_LAYER),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new(HANDLE_LAYER),
                ))
                // Also the clip rect `Ui::interact` intersects each strip with,
                // so it has to span the whole window.
                .max_rect(rect),
        );

        for (id, grab, dir, cursor) in handles {
            let resp = ui.interact(grab, egui::Id::new(id), egui::Sense::drag());
            // The invariant the Area silently broke. Free in release builds.
            debug_assert_eq!(
                resp.interact_rect, grab,
                "resize strip {id} must be hit-tested at exactly the rect it was given"
            );
            if resp.contains_pointer() {
                ctx.set_cursor_icon(cursor);
            }
            // Engage on the press itself (no drag threshold) so the grab feels
            // like a native sizing border.
            if resp.is_pointer_button_down_on() {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        }
    }

    pub(crate) fn render_toolbar(&mut self, parent_ui: &mut egui::Ui) {
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        let header_colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let toolbar_frame = egui::Frame::new()
            .fill(header_colors.bg_header)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .stroke(egui::Stroke::new(1.0_f32, header_colors.border_subtle));
        egui::Panel::top("toolbar")
            .exact_size(TOOLBAR_H)
            .frame(toolbar_frame)
            .show(parent_ui, |ui| {
                self.ensure_logo_textures(ctx);

                let tab = &mut self.tabs[self.active_tab];
                let highlight_active =
                    super::state::effective_highlight(tab.view_mode, self.search_result_mode);
                let match_count = tab.search_nav.match_count;
                let match_current = if match_count == 0 {
                    0
                } else {
                    tab.search_nav.current.min(match_count - 1) + 1
                };
                let search_col_names: Vec<String> =
                    tab.table.columns.iter().map(|c| c.name.clone()).collect();
                // Library-safe shape for the toolbar's Bookmarks dropdown
                // (the `Bookmark` type lives in the binary-side `app` module).
                let bookmark_tuples: Vec<(String, usize, Option<usize>)> = tab
                    .bookmarks
                    .iter()
                    .map(|b| (b.name.clone(), b.row, b.col))
                    .collect();
                // Ask controls: which profiles exist, and which one answers.
                // Seeded from the active profile the first time this renders.
                let ask_profiles: Vec<(String, String)> = self
                    .settings
                    .chat_profiles
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone()))
                    .collect();
                if tab.search_ask_profile.is_empty() {
                    tab.search_ask_profile = self.settings.chat_active_profile.clone();
                }
                // Computed before the call: the argument list borrows several
                // fields of `tab` mutably, and a &self method on the whole
                // struct cannot coexist with those disjoint field borrows.
                let can_save_in_place = tab.saves_in_place();
                let is_db_tab = tab.db_origin.is_some();
                let ask_controls = ui::toolbar::AskControls {
                    enabled: !ask_profiles.is_empty(),
                    profiles: &ask_profiles,
                    mode: &mut tab.search_ask_mode,
                    profile_id: &mut tab.search_ask_profile,
                };
                let cx = ui::toolbar::ToolbarCtx {
                    colors: header_colors,
                    has_data: tab.table.col_count() > 0,
                    has_edits: tab.table.is_modified(),
                    has_source_path: tab.table.source_path.is_some(),
                    can_save_in_place,
                    is_db_tab,
                    selected_cell: tab.table_state.selected_cell,
                    selected_rows: &tab.table_state.selected_rows,
                    selected_cols: &tab.table_state.selected_cols,
                    selected_cells: &tab.table_state.selected_cells,
                    row_count: tab.table.row_count(),
                    col_count: tab.table.col_count(),
                    current_view_mode: tab.view_mode,
                    has_raw_content: tab.raw_content.is_some(),
                    has_markdown: tab.table.format_name.as_deref() == Some("Markdown"),
                    has_notebook: tab.table.format_name.as_deref() == Some("Jupyter Notebook"),
                    has_epub: !tab.epub_chapters_md.is_empty(),
                    has_map: tab.table.format_name.as_deref() == Some("GeoJSON"),
                    has_record: tab.table.col_count() > 0 && !tab.is_chart_tab,
                    has_json: tab.json_value.is_some(),
                    has_yaml: tab.yaml_value.is_some(),
                    chat_profile_available: !self.settings.chat_profiles.is_empty(),
                    readonly_mode: self.readonly_mode,
                    split_view: tab.table_state.is_split(),
                    split_side_by_side: tab.table_state.split_side_by_side,
                    split_panes: tab.table_state.split_panes(),
                    mark_filter_active: tab.mark_filter_active,
                    zoom_percent: self.zoom_percent,
                    recent_files: &self.recent_files,
                    directory_tree_open: self.directory_tree.is_some(),
                    first_row_is_header: tab.first_row_is_header,
                    has_hidden_columns: !tab.hidden_columns.is_empty(),
                    can_undo: !tab.table.undo_stack.is_empty(),
                    can_redo: !tab.table.redo_stack.is_empty(),
                    can_reopen_tab: !self.recently_closed_tabs.is_empty(),
                    table: &tab.table,
                    logo_texture: self.logo_texture.as_ref(),
                    show_window_controls: self.settings.use_custom_title_bar,
                };
                let search = ui::toolbar::SearchControls {
                    text: &mut tab.search_text,
                    mode: &mut tab.search_mode,
                    case_sensitive: &mut tab.search_case_sensitive,
                    whole_word: &mut tab.search_whole_word,
                    scope_col: &mut tab.search_scope_col,
                    column_names: &search_col_names,
                    ask: ask_controls,
                    history: &self.search_history,
                    result_mode: &mut self.search_result_mode,
                    highlight_active,
                    match_count,
                    match_current,
                    focus_requested: self.search_focus_requested,
                    show_replace_bar: tab.show_replace_bar,
                    replace_text: &mut tab.replace_text,
                    bookmarks: &bookmark_tuples,
                };
                let action = ui::toolbar::draw_toolbar(ui, cx, search);
                self.search_focus_requested = false;

                self.dispatch_toolbar_action(ctx, action);
            });
    }

    /// Lazily build the two logo textures the first frame they're needed (or
    /// after the icon variant changes). The toolbar needs the small one; the
    /// welcome screen needs the high-resolution one.
    ///
    /// When the hidden Rainbow easter-egg theme is active we render the
    /// dedicated rainbow rosette (`assets/octa-random.svg`) instead of the
    /// user's normal `resolved_icon` SVG, so the logo visually matches the
    /// cycling rainbow palette. Leaving Rainbow invalidates these textures
    /// elsewhere so the user's icon comes back on the next rebuild.
    fn ensure_logo_textures(&mut self, ctx: &egui::Context) {
        if self.logo_texture.is_some() && self.welcome_logo_texture.is_some() {
            return;
        }
        let opt = resvg::usvg::Options::default();
        let svg_src = if self.theme_mode.is_rainbow() {
            include_str!("../../../assets/octa-random.svg")
        } else {
            self.resolved_icon.svg_source()
        };
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
        self.dispatch_file_menu(ctx, &action);
        if action.toggle_theme {
            let was_rainbow = self.theme_mode.is_rainbow();
            self.theme_mode = self.theme_mode.toggle();
            if was_rainbow && !self.theme_mode.is_rainbow() {
                self.rainbow_active = false;
                self.logo_texture = None;
                self.welcome_logo_texture = None;
            }
            self.apply_zoom(ctx);
        }
        self.dispatch_view_menu(ctx, &action);
        if action.search_changed {
            self.tabs[self.active_tab].search_nav.reset();
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.ask_submitted {
            self.start_ask_filter(ctx);
        }
        if action.commit_search_history {
            let query = self.tabs[self.active_tab].search_text.clone();
            super::search_history::record(
                &mut self.search_history,
                &query,
                self.settings.search_history_limit,
            );
        }
        if action.search_result_mode_changed {
            // Switching Filter<->Highlight changes whether rows are hidden, so
            // the table filter must be recomputed; reset match navigation too.
            self.tabs[self.active_tab].search_nav.reset();
            self.tabs[self.active_tab].filter_dirty = true;
        }
        if action.find_next {
            self.tabs[self.active_tab].search_nav.pending_jump = Some(super::state::NavDir::Next);
        }
        if action.find_prev {
            self.tabs[self.active_tab].search_nav.pending_jump = Some(super::state::NavDir::Prev);
        }
        self.dispatch_search_menu(&action);
        if action.replace_next {
            self.replace_next_match();
        }
        if action.replace_all {
            self.replace_all_matches();
        }
        self.dispatch_analyse_menu(ctx, &action);
        self.dispatch_help_menu(ctx, &action);
        self.dispatch_edit_menu(ctx, &action);
        self.dispatch_columns_menu(&action);
        self.dispatch_data_menu(&action);
        if action.logo_clicked {
            self.register_logo_click(ctx);
        }
        if let Some(i) = action.jump_bookmark {
            self.jump_to_bookmark(i);
        }
        if let Some(i) = action.delete_bookmark {
            let tab = &mut self.tabs[self.active_tab];
            if i < tab.bookmarks.len() {
                tab.bookmarks.remove(i);
            }
        }
    }

    /// Build a redacted debug report, then reveal it in the OS file manager.
    pub(crate) fn export_debug_report_now(&mut self) {
        match octa::diagnostics::report::export_debug_report(&self.settings) {
            Ok(path) => {
                reveal_in_file_manager(&path);
                self.status_message = Some((
                    octa::i18n::t("diagnostics.report_saved"),
                    std::time::Instant::now(),
                ));
            }
            Err(e) => {
                self.status_message = Some((
                    format!("{}: {e}", octa::i18n::t("diagnostics.report_failed")),
                    std::time::Instant::now(),
                ));
            }
        }
    }
}

/// Open the OS file manager with the given file selected (or its folder).
fn reveal_in_file_manager(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    {
        let dir = path.parent().unwrap_or(path);
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
}
