//! The top tab bar: one label per tab, its right-click menu, the close
//! button and the "+" for a new scratch tab.

use eframe::egui;
use egui::Color32;

use octa::data::ViewMode;
use octa::ui;

use crate::app::state::{OctaApp, TabState};

/// Thickness of the tab bar's horizontal scrollbar.
const SCROLL_BAR_WIDTH: f32 = 6.0;
/// Gap between the tab row and the scrollbar under it.
const SCROLL_BAR_INNER_MARGIN: f32 = 2.0;
/// Height the solid scrollbar claims for itself, added to the tab bar panel so
/// the bar sits cleanly beneath the tabs rather than across them.
const SCROLL_BAR_STRIP: f32 = SCROLL_BAR_WIDTH + SCROLL_BAR_INNER_MARGIN;

/// What the user asked for this frame, applied after the tab loop so the loop
/// never mutates the tab list it iterates.
#[derive(Default)]
struct TabBarAction {
    close: Option<usize>,
    activate: Option<usize>,
    /// "Compare with active tab" from a tab's right-click menu.
    compare_with: Option<usize>,
    /// "Pin tab" / "Unpin tab".
    toggle_pin: Option<usize>,
    rename: Option<usize>,
    export_pdf: Option<usize>,
    refresh: Option<usize>,
}

/// What the right-click menu needs to know about the tab it belongs to.
struct TabMenuContext {
    idx: usize,
    is_active: bool,
    pinned: bool,
    has_source: bool,
    has_data: bool,
    /// The tab has a file, table, object or endpoint to read again.
    can_refresh: bool,
}

impl OctaApp {
    /// Render the top tab bar (only shown when at least one file is open).
    pub(crate) fn render_tab_bar(&mut self, parent_ui: &mut egui::Ui) {
        let has_open_file = self.tabs.iter().any(|t| {
            t.table.source_path.is_some() || t.raw_content.is_some() || t.table.col_count() > 0
        });
        if !has_open_file {
            return;
        }
        let ctx = parent_ui.ctx().clone();
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let tab_frame = egui::Frame::new()
            .fill(colors.bg_secondary)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0_f32, colors.border_subtle));
        egui::Panel::top("tab_bar")
            // Tall enough for a tab row (28) plus the scrollbar strip beneath
            // it (`SCROLL_BAR_STRIP`), so the bar never sits on top of the tab
            // labels.
            .exact_size(28.0 + SCROLL_BAR_STRIP)
            .frame(tab_frame)
            .show(parent_ui, |ui| {
                // Scroll the row sideways once the tabs outrun the window,
                // instead of clipping the overflow out of reach. egui only
                // routes a plain (vertical) mouse wheel to a horizontal-only
                // scroll area when `always_scroll_the_only_direction` is set;
                // without it the wheel would need Shift held down here.
                ui.style_mut().always_scroll_the_only_direction = true;
                // egui's default scrollbars *float*: they are drawn on top of
                // the content, which left the bar lying across the tab labels.
                // A solid bar claims its own strip below them instead.
                let mut scroll_style = egui::style::ScrollStyle::solid();
                scroll_style.bar_inner_margin = SCROLL_BAR_INNER_MARGIN;
                scroll_style.bar_outer_margin = 0.0;
                scroll_style.bar_width = SCROLL_BAR_WIDTH;
                ui.style_mut().spacing.scroll = scroll_style;
                egui::ScrollArea::horizontal()
                    .id_salt("tab_bar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let mut action = TabBarAction::default();
                            for idx in 0..self.tabs.len() {
                                self.draw_tab(ui, &ctx, idx, &colors, &mut action);
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
                                action.activate = Some(self.tabs.len() - 1);
                            }

                            self.apply_tab_bar_action(action, &ctx);
                        });
                    });
            });
    }

    /// One tab: its label (with hover and right-click menu) and close button.
    fn draw_tab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        idx: usize,
        colors: &ui::theme::ThemeColors,
        action: &mut TabBarAction,
    ) {
        let tab = &self.tabs[idx];
        let is_active = idx == self.active_tab;
        let is_multi_selected = self.tab_multi_selection.contains(&idx);
        let full_label = tab.title_display();
        // 📌 prefix marks pinned tabs at a glance. U+1F4CC, supplementary
        // plane - covered by the bundled NotoEmoji font.
        let label = if tab.pinned {
            format!("\u{1f4cc} {}", full_label)
        } else {
            full_label.clone()
        };
        let menu = TabMenuContext {
            idx,
            is_active,
            pinned: tab.pinned,
            has_source: tab.table.source_path.is_some(),
            has_data: tab.table.col_count() > 0,
            can_refresh: tab.refresh_source().is_some(),
        };
        // Hover shows the full path when file-backed, else the full
        // (untruncated) tab title, so a shortened title is always
        // recoverable on hover.
        let hover_path = tab.tab_hint.clone().unwrap_or_else(|| {
            tab.table
                .source_path
                .clone()
                .unwrap_or_else(|| full_label.clone())
        });

        // Distinct visual states: active uses the accent at 30% alpha;
        // Ctrl-click-selected (but not active) uses the accent at 15% so
        // users see which tabs they staged for compare without confusing
        // them with the active one.
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
                    .on_hover_ui(|ui| {
                        ui.label(&hover_path);
                        // The same figure View -> Tab memory shows, under
                        // the path, so "which tab is holding the memory"
                        // needs no dialog. Inside the hover closure because
                        // the estimate walks every cell and only one tab is
                        // ever hovered.
                        let bytes = octa::data::tab_memory::estimate_bytes(&tab.table);
                        ui.weak(
                            octa::i18n::t("tabmem.hover")
                                .replace("{size}", &octa::data::tab_memory::format_estimate(bytes))
                                .replace("{rows}", &tab.table.row_count().to_string()),
                        );
                    });
                if tab_label_resp.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Default);
                }
                tab_label_resp.context_menu(|ui| tab_context_menu(ui, &menu, action));
                if tab_label_resp.clicked() {
                    // Ctrl-click toggles multi-selection without changing the
                    // active tab. Plain click activates and clears the staged
                    // selection.
                    let cmd_held = ctx.input(|i| i.modifiers.command);
                    if cmd_held && !is_active {
                        if is_multi_selected {
                            self.tab_multi_selection.remove(&idx);
                        } else {
                            self.tab_multi_selection.insert(idx);
                        }
                    } else {
                        action.activate = Some(idx);
                        self.tab_multi_selection.clear();
                    }
                }
                // Close button (hidden on pinned tabs - the user has to unpin
                // first via the right-click context menu). The leading spacing
                // lives outside the label so the response rect tightly hugs
                // the × glyph - that way the hover overlay (painted at
                // rect.center) sits exactly where egui drew the original glyph
                // and no horizontal shift is visible on hover.
                if !menu.pinned {
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
                        ui.painter()
                            .rect_filled(r, 3.0, colors.accent.gamma_multiply(0.25));
                        ui.painter().text(
                            close_resp.rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "\u{00D7}",
                            egui::FontId::proportional(14.0),
                            colors.error,
                        );
                    }
                    if close_resp.clicked() {
                        action.close = Some(idx);
                    }
                }
            });
        });
    }

    fn apply_tab_bar_action(&mut self, action: TabBarAction, ctx: &egui::Context) {
        if let Some(idx) = action.activate {
            self.active_tab = idx;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                self.tabs[self.active_tab].title_display(),
            ));
        }
        if let Some(idx) = action.close {
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
        if let Some(idx) = action.compare_with {
            self.begin_compare_with_tab(idx);
        }
        if let Some(idx) = action.toggle_pin {
            self.toggle_tab_pinned(idx);
        }
        if let Some(idx) = action.rename {
            self.begin_rename_tab(idx);
        }
        if let Some(idx) = action.refresh {
            self.request_refresh(idx, ctx);
        }
        if let Some(idx) = action.export_pdf {
            // The dialog prints the active tab, so make it the one that was
            // right-clicked.
            self.active_tab = idx;
            self.pdf_export_dialog = Some(crate::app::state::PdfExportState::default());
        }
    }
}

/// Right-click menu of one tab. "Compare with active tab" only makes sense on
/// a non-active tab; "Pin tab" / "Unpin tab" applies to any tab but only
/// file-backed ones (scratch tabs have nowhere to persist).
fn tab_context_menu(ui: &mut egui::Ui, menu: &TabMenuContext, action: &mut TabBarAction) {
    let idx = menu.idx;
    if !menu.is_active
        && ui
            .button(octa::i18n::t("context_menu.compare_active"))
            .clicked()
    {
        action.compare_with = Some(idx);
        ui.close();
    }
    if ui
        .add_enabled(
            menu.can_refresh,
            egui::Button::new(octa::i18n::t("refresh.menu")),
        )
        .on_hover_text(octa::i18n::t("refresh.menu_hint"))
        .on_disabled_hover_text(octa::i18n::t("refresh.menu_disabled_hint"))
        .clicked()
    {
        action.refresh = Some(idx);
        ui.close();
    }
    let pin_label = if menu.pinned { "Unpin tab" } else { "Pin tab" };
    // Size the button to the wider of the two labels (plus padding) so the
    // context-menu entry stays the same width whether the tab is pinned or
    // not. Without this the button shrink-wraps "Pin tab" and looks cramped
    // when the user right-clicks a pinned tab.
    let pin_btn = ui.add_enabled(
        menu.has_source,
        egui::Button::new(pin_label).min_size(egui::vec2(140.0, 0.0)),
    );
    let pin_btn = if !menu.has_source {
        pin_btn.on_disabled_hover_text("Pinning is for file-backed tabs; save the tab first.")
    } else {
        pin_btn
    };
    if pin_btn.clicked() {
        action.toggle_pin = Some(idx);
        ui.close();
    }
    if ui
        .button(octa::i18n::t("context_menu.rename_tab"))
        .clicked()
    {
        action.rename = Some(idx);
        ui.close();
    }
    // Every result tab (Summary, the Quality report, a compare) is a table,
    // so the PDF export belongs on all of them, not just the file-backed ones.
    if ui
        .add_enabled(
            menu.has_data,
            egui::Button::new(octa::i18n::t("context_menu.export_pdf")),
        )
        .on_hover_text(octa::i18n::t("file_menu.export_pdf_hint"))
        .on_disabled_hover_text(octa::i18n::t("file_menu.export_pdf_hint"))
        .clicked()
    {
        action.export_pdf = Some(idx);
        ui.close();
    }
}
