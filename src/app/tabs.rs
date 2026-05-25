//! Tab lifecycle: per-tab state initialization, titles, and the top tab bar
//! that lets the user switch or close tabs.

use std::sync::Arc;

use eframe::egui;
use egui::Color32;

use octa::data::{self, DataTable, ViewMode};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{ColumnInspectorSort, OctaApp, RawCsvEscape, RawCsvQuote, TabState};

impl TabState {
    pub(crate) fn new(search_mode: data::SearchMode) -> Self {
        Self {
            table: DataTable::empty(),
            table_state: TableViewState::default(),
            search_text: String::new(),
            search_mode,
            show_replace_bar: false,
            replace_text: String::new(),
            filtered_rows: Vec::new(),
            filter_dirty: true,
            view_mode: ViewMode::Table,
            raw_content: None,
            raw_content_modified: false,
            raw_content_original: None,
            raw_color_enabled: true,
            raw_file_size: None,
            raw_perf_prompt_resolved: false,
            raw_view_formatted: false,
            csv_delimiter: b',',
            raw_csv_quote: RawCsvQuote::default(),
            raw_csv_escape: RawCsvEscape::default(),
            bg_row_buffer: None,
            bg_loading_done: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            bg_can_load_more: false,
            bg_file_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            markdown_scroll_target: None,
            markdown_layout: data::MarkdownLayout::default(),
            markdown_render_cache: None,
            json_tree_expanded: std::collections::HashSet::new(),
            json_value: None,
            yaml_value: None,
            json_expand_depth: 1,
            json_expand_depth_str: "1".to_string(),
            json_file_max_depth: 0,
            json_edit_path: None,
            json_edit_buffer: String::new(),
            json_edit_width: None,
            tree_key_edit_path: None,
            tree_key_edit_buffer: String::new(),
            tree_add_key_path: None,
            tree_add_key_buffer: String::new(),
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
            new_col_formula: String::new(),
            insert_col_at: None,
            insert_col_at_text: String::new(),
            show_delete_columns_dialog: false,
            delete_col_selection: Vec::new(),
            sql_query: String::new(),
            sql_result: None,
            sql_error: None,
            sql_panel_open: false,
            sql_ac_selected: 0,
            sql_ac_visible: true,
            first_row_is_header: true,
            show_column_inspector: false,
            column_inspector_sort: ColumnInspectorSort::Default,
            column_inspector_size: octa::ui::settings::DialogSize::default(),
            column_inspector_selected: std::collections::HashSet::new(),
            column_inspector_anchor: None,
            value_frequency_col: None,
            value_frequency_top_n: Some(50),
            value_frequency_bin_numeric: true,
            value_frequency_size: octa::ui::settings::DialogSize::default(),
            show_find_duplicates: false,
            find_duplicates_key_cols: std::collections::HashSet::new(),
            find_duplicates_mode: super::state::FindDuplicatesMode::default(),
            hidden_columns: std::collections::HashSet::new(),
            pinned: false,
            is_chart_tab: false,
            chart_tab_label: None,
            column_filters: std::collections::HashMap::new(),
            show_column_filter: false,
            column_filter_size: octa::ui::settings::DialogSize::default(),
            column_filter_picker_col: None,
            column_filter_value_search: String::new(),
            column_filter_draft_allowed: std::collections::HashSet::new(),
            column_filter_needs_seed: false,
            empty_file_placeholder: false,
            parse_error_banner: None,
            compare_right_path: None,
            compare_right_raw: None,
            compare_right_table: None,
            compare_mode: data::CompareMode::default(),
            compare_columns_left: Vec::new(),
            compare_columns_right: Vec::new(),
            compare_error: None,
            epub_chapters_md: Vec::new(),
            epub_chapter_titles: Vec::new(),
            epub_image_bytes: std::collections::HashMap::new(),
            epub_image_textures: std::collections::HashMap::new(),
            epub_active_chapter: 0,
            epub_title: None,
            geojson_features: Vec::new(),
            map_mode: data::MapMode::default(),
            map_tiles: None,
            map_memory: None,
            chart_config: data::chart::ChartConfig::default(),
            chart_buffers: super::state::ChartInputBuffers::default(),
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.table.is_modified() || self.raw_content_modified
    }

    /// Ordered list of view modes that make sense for this tab — same order
    /// as the View menu radio buttons. Used by the toolbar (gating which
    /// options are clickable) and by the `CycleViewMode` shortcut handler
    /// (advancing to the next available mode).
    pub(crate) fn available_view_modes(&self) -> Vec<ViewMode> {
        // Chart tabs are single-mode: the tab IS the chart, switching to
        // Table / Raw / anything else here would just confuse the user
        // (the tab has no source path, no readers, etc).
        if self.is_chart_tab {
            return vec![ViewMode::Chart];
        }
        let mut modes = Vec::new();
        let has_notebook = self.table.format_name.as_deref() == Some("Jupyter Notebook");
        let has_markdown = self.table.format_name.as_deref() == Some("Markdown");
        let has_epub = !self.epub_chapters_md.is_empty();
        let has_map = self.table.format_name.as_deref() == Some("GeoJSON");
        let has_json = self.json_value.is_some();
        let has_yaml = self.yaml_value.is_some();
        let has_raw = self.raw_content.is_some();

        if !has_notebook && !has_epub {
            modes.push(ViewMode::Table);
        } else if has_epub {
            // EPUBs still expose the flat paragraph table for searching /
            // exporting, just not as the default.
            modes.push(ViewMode::Table);
        }
        if has_raw {
            modes.push(ViewMode::Raw);
        }
        if has_markdown {
            modes.push(ViewMode::Markdown);
        }
        if has_notebook {
            modes.push(ViewMode::Notebook);
        }
        if has_epub {
            modes.push(ViewMode::EpubReader);
        }
        if has_map {
            modes.push(ViewMode::Map);
        }
        // Chart is **not** in the View menu — it opens via the Analyse →
        // Chart toolbar button as its own dedicated tab. Adding it here
        // would let the user mode-switch a data tab into a chart, which
        // breaks the tab-identity expectation (no source_path, no readers,
        // single view mode).
        if has_json {
            modes.push(ViewMode::JsonTree);
        }
        if has_yaml {
            modes.push(ViewMode::YamlTree);
        }
        if self.compare_right_path.is_some() {
            modes.push(ViewMode::Compare);
        }
        modes
    }

    pub(crate) fn title_display(&self) -> String {
        if self.is_chart_tab {
            return self
                .chart_tab_label
                .clone()
                .unwrap_or_else(|| "Chart".to_string());
        }
        let name = if let Some(ref path) = self.table.source_path {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            "Untitled".to_string()
        };
        if self.is_modified() {
            format!("{} *", name)
        } else {
            name
        }
    }
}

impl OctaApp {
    /// Open a new chart tab seeded from the active tab's table.
    ///
    /// The new tab gets a deep clone of the table (so subsequent edits in
    /// the source don't drift the chart), an empty `ChartConfig`, and a
    /// title derived from the source filename. Triggered by the
    /// **Analyse → Chart** toolbar button or the `OpenChart` shortcut.
    ///
    /// No-ops with a status message when there's no active table or the
    /// table has no numeric columns — charting either is useless and the
    /// rfd dialog cost would be wasted.
    pub(crate) fn open_chart_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            self.status_message = Some((
                "Open a file with columns before charting.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        if !octa::data::chart::has_numeric_column(&source.table) {
            self.status_message = Some((
                "Chart needs at least one numeric column.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());
        let chart_label = format!("Chart \u{2014} {source_label}");

        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = source.table.clone();
        // Detach from disk: a chart tab can't be saved back over its
        // source — that would silently overwrite the user's data file.
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.table.structural_changes = false;
        new_tab.table.edits.clear();
        new_tab.filtered_rows = source.filtered_rows.clone();
        new_tab.is_chart_tab = true;
        new_tab.chart_tab_label = Some(chart_label);
        new_tab.view_mode = octa::data::ViewMode::Chart;

        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    pub(crate) fn close_tab(&mut self, idx: usize) {
        // Pinned tabs refuse to close. The user has to unpin them from the
        // tab right-click context menu first; the status bar tells them.
        if self.tabs.get(idx).is_some_and(|t| t.pinned) {
            self.status_message = Some((
                "Tab is pinned; unpin from the tab right-click menu first.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        // Take a snapshot before removal so Ctrl+Shift+T can restore it.
        // Skip wholly empty tabs (no source path, no raw content, no
        // columns) — those would just be re-created empty.
        if let Some(tab) = self.tabs.get(idx) {
            let snapshot = if let Some(ref p) = tab.table.source_path {
                Some(super::state::ClosedTabSnapshot::Path(
                    std::path::PathBuf::from(p),
                ))
            } else if let Some(ref content) = tab.raw_content {
                if !content.is_empty() || tab.table.col_count() > 0 {
                    Some(super::state::ClosedTabSnapshot::Scratch {
                        raw_content: content.clone(),
                        view_mode: tab.view_mode,
                        format_name: tab.table.format_name.clone(),
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(snap) = snapshot {
                if self.recently_closed_tabs.len() >= super::state::MAX_CLOSED_TAB_HISTORY {
                    self.recently_closed_tabs.pop_front();
                }
                self.recently_closed_tabs.push_back(snap);
            }
        }

        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Toggle the pinned state for the tab at `idx`. Pinning a file-backed
    /// tab adds its absolute path to `AppSettings.pinned_tabs` so the file
    /// re-opens on next launch; unpinning removes it. Settings are saved
    /// immediately so the change survives a crash.
    ///
    /// Pinning a scratch tab (no `source_path`) is a no-op — the UI already
    /// greys out the menu entry for those.
    pub(crate) fn toggle_tab_pinned(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let Some(path) = tab.table.source_path.clone() else {
            return;
        };
        tab.pinned = !tab.pinned;
        let now_pinned = tab.pinned;
        let pinned_list = &mut self.settings.pinned_tabs;
        if now_pinned {
            if !pinned_list.contains(&path) {
                pinned_list.push(path);
            }
        } else {
            pinned_list.retain(|p| p != &path);
        }
        self.settings.save();
    }

    /// Restore the most-recently-closed tab (Ctrl+Shift+T). Path-backed tabs
    /// reload through the standard `load_file` pipeline; scratch tabs
    /// recreate from the stored raw_content. No-op when the close stack
    /// is empty.
    pub(crate) fn reopen_last_closed_tab(&mut self, ctx: &egui::Context) {
        let Some(snap) = self.recently_closed_tabs.pop_back() else {
            return;
        };
        match snap {
            super::state::ClosedTabSnapshot::Path(path) => {
                self.load_file(path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
            super::state::ClosedTabSnapshot::Scratch {
                raw_content,
                view_mode,
                format_name,
            } => {
                let mut tab = TabState::new(self.settings.default_search_mode);
                tab.raw_content = Some(raw_content);
                tab.raw_content_original = tab.raw_content.clone();
                tab.view_mode = view_mode;
                tab.table.format_name = format_name;
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
        }
    }

    /// Render the top tab bar (only shown when at least one file is open).
    pub(crate) fn render_tab_bar(&mut self, parent_ui: &mut egui::Ui) {
        let has_open_file = self.tabs.iter().any(|t| {
            t.table.source_path.is_some() || t.raw_content.is_some() || t.table.col_count() > 0
        });
        if !has_open_file {
            return;
        }
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let tab_frame = egui::Frame::new()
            .fill(colors.bg_secondary)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0, colors.border_subtle));
        egui::Panel::top("tab_bar")
            .exact_size(28.0)
            .frame(tab_frame)
            .show_inside(parent_ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let mut tab_to_close: Option<usize> = None;
                    let mut tab_to_activate: Option<usize> = None;
                    // Set when the user picks "Compare with active tab" from
                    // a tab's right-click context menu.
                    let mut tab_to_compare_with: Option<usize> = None;
                    // Set when the user picks "Pin tab" / "Unpin tab".
                    let mut tab_to_toggle_pin: Option<usize> = None;

                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let is_active = idx == self.active_tab;
                        let is_multi_selected = self.tab_multi_selection.contains(&idx);
                        let raw_label = tab.title_display();
                        // 📌 prefix marks pinned tabs at a glance. U+1F4CC,
                        // supplementary plane — covered by the bundled
                        // NotoEmoji font.
                        let label = if tab.pinned {
                            format!("\u{1f4cc} {}", raw_label)
                        } else {
                            raw_label
                        };
                        let pinned = tab.pinned;
                        let has_source = tab.table.source_path.is_some();
                        let hover_path = tab
                            .table
                            .source_path
                            .clone()
                            .unwrap_or_else(|| "Untitled".to_string());

                        // Distinct visual states: active uses the accent at
                        // 30% alpha; Ctrl-click-selected (but not active) uses
                        // the accent at 15% so users see which tabs they
                        // staged for compare without confusing them with the
                        // active one.
                        let bg = if is_active {
                            colors.accent.gamma_multiply(0.3)
                        } else if is_multi_selected {
                            colors.accent.gamma_multiply(0.15)
                        } else {
                            Color32::TRANSPARENT
                        };

                        let frame = egui::Frame::new()
                            .fill(bg)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .corner_radius(4.0);

                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let text = if is_active {
                                    egui::RichText::new(&label)
                                        .strong()
                                        .color(colors.text_primary)
                                } else {
                                    egui::RichText::new(&label).color(colors.text_secondary)
                                };
                                let tab_label_resp = ui
                                    .add(egui::Label::new(text).sense(egui::Sense::click()))
                                    .on_hover_text(&hover_path);
                                if tab_label_resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::Default);
                                }
                                // Right-click context menu — "Compare with
                                // active tab" only makes sense on a non-active
                                // tab; "Pin tab" / "Unpin tab" applies to any
                                // tab (active or not) but only file-backed
                                // ones (scratch tabs have nowhere to persist).
                                tab_label_resp.context_menu(|ui| {
                                    if !is_active && ui.button("Compare with active tab").clicked()
                                    {
                                        tab_to_compare_with = Some(idx);
                                        ui.close();
                                    }
                                    let pin_label = if pinned { "Unpin tab" } else { "Pin tab" };
                                    let pin_btn =
                                        ui.add_enabled(has_source, egui::Button::new(pin_label));
                                    let pin_btn = if !has_source {
                                        pin_btn.on_disabled_hover_text(
                                            "Pinning is for file-backed tabs; save the tab first.",
                                        )
                                    } else {
                                        pin_btn
                                    };
                                    if pin_btn.clicked() {
                                        tab_to_toggle_pin = Some(idx);
                                        ui.close();
                                    }
                                });
                                if tab_label_resp.clicked() {
                                    // Ctrl-click toggles multi-selection
                                    // without changing the active tab. Plain
                                    // click activates and clears the staged
                                    // selection.
                                    let cmd_held = ctx.input(|i| i.modifiers.command);
                                    if cmd_held && !is_active {
                                        if is_multi_selected {
                                            self.tab_multi_selection.remove(&idx);
                                        } else {
                                            self.tab_multi_selection.insert(idx);
                                        }
                                    } else {
                                        tab_to_activate = Some(idx);
                                        self.tab_multi_selection.clear();
                                    }
                                }
                                // Close button (hidden on pinned tabs — the
                                // user has to unpin first via the right-click
                                // context menu). The leading spacing lives
                                // outside the label so the response rect tightly
                                // hugs the × glyph — that way the hover overlay
                                // (painted at rect.center) sits exactly where
                                // egui drew the original glyph and no horizontal
                                // shift is visible on hover.
                                if !pinned {
                                    ui.add_space(6.0);
                                    let close_resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new("\u{00D7}")
                                                .size(14.0)
                                                .color(colors.text_muted),
                                        )
                                        .sense(egui::Sense::click() | egui::Sense::hover()),
                                    );
                                    if close_resp.hovered() {
                                        ctx.set_cursor_icon(egui::CursorIcon::Default);
                                        let r = close_resp.rect.expand2(egui::vec2(3.0, 1.0));
                                        ui.painter().rect_filled(
                                            r,
                                            3.0,
                                            colors.accent.gamma_multiply(0.25),
                                        );
                                        ui.painter().text(
                                            close_resp.rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "\u{00D7}",
                                            egui::FontId::proportional(14.0),
                                            colors.error,
                                        );
                                    }
                                    if close_resp.clicked() {
                                        tab_to_close = Some(idx);
                                    }
                                }
                            });
                        });
                    }

                    // "+" button to add new empty tab (opens editor)
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new("+").size(14.0).color(colors.text_muted),
                        ))
                        .clicked()
                    {
                        let mut new_tab = TabState::new(self.settings.default_search_mode);
                        new_tab.view_mode = ViewMode::Raw;
                        new_tab.raw_content = Some(String::new());
                        self.tabs.push(new_tab);
                        tab_to_activate = Some(self.tabs.len() - 1);
                    }

                    // Process tab actions
                    if let Some(idx) = tab_to_activate {
                        self.active_tab = idx;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                            self.tabs[self.active_tab].title_display(),
                        ));
                    }
                    if let Some(idx) = tab_to_close {
                        if self.tabs[idx].is_modified() {
                            self.pending_close_tab = Some(idx);
                            self.show_close_confirm = true;
                        } else {
                            self.close_tab(idx);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                self.tabs[self.active_tab].title_display(),
                            ));
                        }
                    }
                    if let Some(idx) = tab_to_compare_with {
                        self.begin_compare_with_tab(idx);
                    }
                    if let Some(idx) = tab_to_toggle_pin {
                        self.toggle_tab_pinned(idx);
                    }
                });
            });
    }
}
