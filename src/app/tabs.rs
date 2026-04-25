//! Tab lifecycle: per-tab state initialization, titles, and the top tab bar
//! that lets the user switch or close tabs.

use std::sync::Arc;

use eframe::egui;
use egui::Color32;

use octa::data::{self, DataTable, ViewMode};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, TabState};

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
            pdf_page_images: Vec::new(),
            pdf_textures: Vec::new(),
            pdf_page_texts: Vec::new(),
            raw_view_formatted: false,
            csv_delimiter: b',',
            bg_row_buffer: None,
            bg_loading_done: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            bg_can_load_more: false,
            bg_file_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            commonmark_cache: egui_commonmark::CommonMarkCache::default(),
            markdown_scroll_target: None,
            json_tree_expanded: std::collections::HashSet::new(),
            json_value: None,
            json_expand_depth: 1,
            json_expand_depth_str: "1".to_string(),
            json_edit_path: None,
            json_edit_buffer: String::new(),
            json_edit_width: None,
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
            new_col_formula: String::new(),
            insert_col_at: None,
            show_delete_columns_dialog: false,
            delete_col_selection: Vec::new(),
            sql_query: String::new(),
            sql_result: None,
            sql_error: None,
            sql_panel_open: false,
            sql_ac_selected: 0,
            sql_ac_visible: true,
            first_row_is_header: true,
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.table.is_modified() || self.raw_content_modified
    }

    pub(crate) fn title_display(&self) -> String {
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
    pub(crate) fn close_tab(&mut self, idx: usize) {
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Render the top tab bar (only shown when at least one file is open).
    pub(crate) fn render_tab_bar(&mut self, ctx: &egui::Context) {
        let has_open_file = self.tabs.iter().any(|t| {
            t.table.source_path.is_some() || t.raw_content.is_some() || t.table.col_count() > 0
        });
        if !has_open_file {
            return;
        }
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let tab_frame = egui::Frame::new()
            .fill(colors.bg_secondary)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0, colors.border_subtle));
        egui::TopBottomPanel::top("tab_bar")
            .exact_height(28.0)
            .frame(tab_frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let mut tab_to_close: Option<usize> = None;
                    let mut tab_to_activate: Option<usize> = None;

                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let is_active = idx == self.active_tab;
                        let label = tab.title_display();
                        let hover_path = tab
                            .table
                            .source_path
                            .clone()
                            .unwrap_or_else(|| "Untitled".to_string());

                        let bg = if is_active {
                            colors.accent.gamma_multiply(0.3)
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
                                if tab_label_resp.clicked() {
                                    tab_to_activate = Some(idx);
                                }
                                // Close button
                                let close_resp = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("  \u{00D7}")
                                            .size(14.0)
                                            .color(colors.text_muted),
                                    )
                                    .sense(egui::Sense::click() | egui::Sense::hover()),
                                );
                                if close_resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::Default);
                                    let r = close_resp.rect.shrink2(egui::vec2(2.0, 1.0));
                                    ui.painter().rect_filled(
                                        r,
                                        3.0,
                                        colors.accent.gamma_multiply(0.25),
                                    );
                                    ui.painter().text(
                                        r.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "\u{00D7}",
                                        egui::FontId::proportional(14.0),
                                        colors.error,
                                    );
                                }
                                if close_resp.clicked() {
                                    tab_to_close = Some(idx);
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
                });
            });
    }
}
