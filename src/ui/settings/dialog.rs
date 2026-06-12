//! Settings dialog UI rendering. The full `impl SettingsDialog` lives here;
//! the struct definition + supporting `AppSettings` plus enums stay in
//! [`super`]. Split out purely for navigability - no behaviour change.

use egui;

use super::*;
use crate::data::{BinaryDisplayMode, MapMode, MarkColor, SearchMode, SearchResultMode};
use crate::ui::shortcuts::{KeyCombo, ShortcutAction};
use crate::ui::theme::{BodyFont, ThemeMode};

impl SettingsDialog {
    /// Open the dialog, seeding the draft from current settings.
    pub fn open(&mut self, current: &AppSettings) {
        self.draft = current.clone();
        self.icon_changed = false;
        self.font_changed = false;
        self.theme_changed = false;
        self.sql_row_limit_buf = current.sql_default_row_limit.to_string();
        // Pick the most natural unit for the current bytes value so the
        // user sees "1 MB" rather than "1,048,576 Bytes" when the setting
        // is at the default.
        self.syntax_highlight_size_unit =
            SyntaxSizeUnit::best_fit(current.syntax_highlight_max_bytes);
        // `SyntaxSizeUnit::factor` is always >= 1, so the division is safe.
        let unit_factor = self.syntax_highlight_size_unit.factor();
        self.syntax_highlight_max_bytes_buf =
            crate::ui::status_bar::format_number(current.syntax_highlight_max_bytes / unit_factor);
        self.initial_load_rows_buf =
            crate::ui::status_bar::format_number(current.initial_load_rows);
        self.text_mode_extensions_buf = current.text_mode_extensions.join(", ");
        // MCP buffers seed from the live settings.
        self.mcp_unlimited_rows = current.mcp_default_row_limit.is_none();
        self.mcp_row_limit_buf =
            crate::ui::status_bar::format_number(current.mcp_default_row_limit.unwrap_or(1000));
        self.mcp_cell_bytes_buf =
            crate::ui::status_bar::format_number(current.mcp_default_cell_bytes);
        self.grep_max_file_size_buf =
            crate::ui::status_bar::format_number(current.grep_max_file_size_mb as usize);
        self.chart_max_points_buf = crate::ui::status_bar::format_number(current.chart_max_points);
        self.chart_max_categories_buf =
            crate::ui::status_bar::format_number(current.chart_max_categories);
        self.table_picker_visible_rows_buf =
            crate::ui::status_bar::format_number(current.table_picker_visible_rows);
        self.excel_max_auto_sheets_buf =
            crate::ui::status_bar::format_number(current.excel_max_auto_sheets);
        self.search_history_limit_buf = current.search_history_limit.to_string();
        self.chat_temperature_buf = format!("{:.2}", current.chat_temperature);
        self.chat_max_iterations_buf = current.chat_max_tool_iterations.to_string();
        self.chat_max_tokens_buf = crate::ui::status_bar::format_number(current.chat_max_tokens);
        self.chat_unlimited_tokens = current.chat_max_tokens_unlimited;
        self.chat_audit_warn_mb_buf =
            (current.chat_audit_log_warn_bytes / (1024 * 1024)).to_string();
        self.chat_key_input_buf.clear();
        self.chat_key_status_msg = None;
        self.chat_key_clear_confirm = None;
        // Reset here; the chat panel re-sets it to true right after calling
        // open() so the Chat section starts expanded only when launched there.
        self.focus_chat_section = false;
        self.recording = None;
        self.shortcut_conflict = None;
        self.show_reset_confirm = false;
        self.open = true;
    }

    /// Draw the dialog. Returns `Some(settings)` when the user clicks Apply.
    /// `logo` is an optional texture (the app icon) rendered as a header; passing
    /// `None` omits it and shows just the title.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        logo: Option<&egui::TextureHandle>,
    ) -> Option<AppSettings> {
        if !self.open {
            return None;
        }

        let mut applied: Option<AppSettings> = None;

        // Render the reset-confirm modal first so it sits above the Settings
        // window in the same frame.
        self.draw_reset_confirm(ctx);

        // Custom title bar (egui's is disabled below) - we render Min /
        // Max / Close buttons inline next to the title, like a typical
        // desktop window. Dragging works because the title text is a
        // non-interactive area inside the window's drag region.
        let screen_center = ctx.content_rect().center();
        let default_pos = screen_center - egui::vec2(340.0, 290.0);
        let dialog_id = egui::Id::new("octa_settings_dialog");
        let size = self.size;
        let window = egui::Window::new("Settings")
            .title_bar(false)
            .collapsible(false);
        let window = size_dialog_window(ctx, dialog_id, size, window, |w| {
            w.resizable(true)
                .default_pos(default_pos)
                .min_width(640.0)
                .default_width(680.0)
                .default_height(580.0)
                .min_height(360.0)
        });
        let minimized = size == DialogSize::Minimized;
        let inner = window.show(ctx, |ui| {
            // Labels in Settings are static captions, not selectable text - turn
            // off egui's default label selection so hovering a row label (e.g.
            // "Temperature") shows the normal pointer instead of the text I-beam.
            // Input fields keep their own I-beam.
            ui.style_mut().interaction.selectable_labels = false;
            // Custom title bar: logo + "Octa Settings" + three control
            // buttons. Stays rendered when minimized so the user can
            // restore from there.
            egui::Panel::top("settings_header")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 6)))
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(tex) = logo {
                            let size = egui::vec2(28.0, 28.0);
                            ui.add(egui::Image::new(tex).fit_to_exact_size(size));
                            ui.add_space(8.0);
                        }
                        ui.label(
                            egui::RichText::new(crate::i18n::t("settings.window_title"))
                                .strong()
                                .size(16.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if draw_window_controls(ui, &mut self.size) {
                                self.open = false;
                            }
                        });
                    });
                });

            if minimized {
                return;
            }

            // Pin Apply/Cancel to the bottom so they're always reachable
            // regardless of how much content the scroll area holds.
            egui::Panel::bottom("settings_buttons")
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(0, 8)))
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::t("common.apply")).clicked() {
                            if let Ok(n) = parse_comma_number(&self.sql_row_limit_buf)
                                && n >= 1
                            {
                                self.draft.sql_default_row_limit = n;
                            }
                            if let Ok(n) = parse_comma_number(&self.syntax_highlight_max_bytes_buf)
                            {
                                // 0 is a valid input meaning "disable highlighting"
                                // - anything <= 0 trips the size guard immediately.
                                let unit_factor = self.syntax_highlight_size_unit.factor();
                                self.draft.syntax_highlight_max_bytes =
                                    n.saturating_mul(unit_factor);
                            }
                            if let Ok(n) = parse_comma_number(&self.initial_load_rows_buf)
                                && n >= 1
                            {
                                self.draft.initial_load_rows = n;
                            }
                            self.draft.text_mode_extensions = self
                                .text_mode_extensions_buf
                                .split([',', ' ', '\t', '\n'])
                                .map(|s| s.trim().trim_start_matches('.').to_lowercase())
                                .filter(|s| !s.is_empty())
                                .collect();
                            // MCP row cap: "Unlimited" overrides the text
                            // input, otherwise parse the comma-separated
                            // number. Invalid input falls back to the
                            // existing draft value so the user doesn't
                            // silently lose their previous setting.
                            if self.mcp_unlimited_rows {
                                self.draft.mcp_default_row_limit = None;
                            } else if let Ok(n) = parse_comma_number(&self.mcp_row_limit_buf)
                                && n >= 1
                            {
                                self.draft.mcp_default_row_limit = Some(n);
                            }
                            if let Ok(n) = parse_comma_number(&self.mcp_cell_bytes_buf) {
                                self.draft.mcp_default_cell_bytes = n;
                            }
                            if let Ok(n) = parse_comma_number(&self.grep_max_file_size_buf) {
                                // Multi-search per-file size cap. Stored as u32
                                // because mb >= 4 GB is nonsense for this knob.
                                self.draft.grep_max_file_size_mb = n.min(u32::MAX as usize) as u32;
                            }
                            if let Ok(n) = parse_comma_number(&self.chart_max_points_buf) {
                                self.draft.chart_max_points = n;
                            }
                            if let Ok(n) = parse_comma_number(&self.chart_max_categories_buf) {
                                self.draft.chart_max_categories = n.max(1);
                            }
                            if let Ok(n) = parse_comma_number(&self.table_picker_visible_rows_buf) {
                                self.draft.table_picker_visible_rows = n.max(1);
                            }
                            if let Ok(n) = parse_comma_number(&self.excel_max_auto_sheets_buf) {
                                self.draft.excel_max_auto_sheets = n.max(1);
                            }
                            // Search history size: 0 is valid (disables history).
                            if let Ok(n) = parse_comma_number(&self.search_history_limit_buf) {
                                self.draft.search_history_limit = n;
                            }
                            // Chat temperature / iterations: parse + clamp;
                            // invalid input keeps the existing draft value.
                            if let Ok(t) = self.chat_temperature_buf.trim().parse::<f32>() {
                                self.draft.chat_temperature = t.clamp(0.0, 2.0);
                            }
                            if let Ok(n) = self.chat_max_iterations_buf.trim().parse::<usize>() {
                                self.draft.chat_max_tool_iterations = n.clamp(1, 30);
                            }
                            // Chat response-token cap: "Unlimited" overrides the
                            // text input; otherwise parse the comma-separated
                            // number (invalid input keeps the existing value).
                            self.draft.chat_max_tokens_unlimited = self.chat_unlimited_tokens;
                            if let Ok(n) = parse_comma_number(&self.chat_max_tokens_buf)
                                && n >= 1
                            {
                                self.draft.chat_max_tokens = n;
                            }
                            // Audit-log warning threshold (MB -> bytes).
                            if let Ok(mb) = parse_comma_number(&self.chat_audit_warn_mb_buf) {
                                self.draft.chat_audit_log_warn_bytes = (mb as u64) * 1024 * 1024;
                            }
                            applied = Some(self.draft.clone());
                            self.open = false;
                        }
                        if ui.button(crate::i18n::t("common.cancel")).clicked() {
                            self.open = false;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label =
                                egui::RichText::new(crate::i18n::t("settings.reset_to_defaults"))
                                    .color(ui.visuals().error_fg_color);
                            if ui.button(label).clicked() {
                                self.show_reset_confirm = true;
                            }
                        });
                    });
                });

            egui::CentralPanel::default()
                .frame(egui::Frame::default())
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            self.draw_sections(ui);
                        });
                });
        });

        if let Some(inner) = inner {
            remember_dialog_rect(ctx, dialog_id, size, inner.response.rect);
        }

        applied
    }

    /// Render the "Reset to defaults?" confirmation modal. On confirm, the
    /// draft is replaced with `AppSettings::default()` and the icon/font/theme
    /// changed flags are set so the existing Apply path re-applies them.
    /// Nothing is written to disk and the Settings window stays open - the
    /// user still has to click Apply (or Cancel) to commit / discard.
    fn draw_reset_confirm(&mut self, ctx: &egui::Context) {
        if !self.show_reset_confirm {
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(crate::i18n::t("settings.reset_confirm_title"))
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(crate::i18n::t("settings.reset_confirm_body"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(crate::i18n::t("settings.reset")).clicked() {
                        confirm = true;
                    }
                    if ui.button(crate::i18n::t("common.cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            self.draft = AppSettings::default();
            self.sql_row_limit_buf = self.draft.sql_default_row_limit.to_string();
            self.syntax_highlight_size_unit =
                SyntaxSizeUnit::best_fit(self.draft.syntax_highlight_max_bytes);
            // `SyntaxSizeUnit::factor` is always >= 1, so the division is safe.
            let factor = self.syntax_highlight_size_unit.factor();
            self.syntax_highlight_max_bytes_buf = crate::ui::status_bar::format_number(
                self.draft.syntax_highlight_max_bytes / factor,
            );
            self.initial_load_rows_buf =
                crate::ui::status_bar::format_number(self.draft.initial_load_rows);
            self.text_mode_extensions_buf = self.draft.text_mode_extensions.join(", ");
            self.mcp_unlimited_rows = self.draft.mcp_default_row_limit.is_none();
            self.mcp_row_limit_buf = crate::ui::status_bar::format_number(
                self.draft.mcp_default_row_limit.unwrap_or(1000),
            );
            self.mcp_cell_bytes_buf =
                crate::ui::status_bar::format_number(self.draft.mcp_default_cell_bytes);
            self.chat_temperature_buf = format!("{:.2}", self.draft.chat_temperature);
            self.chat_max_iterations_buf = self.draft.chat_max_tool_iterations.to_string();
            self.chat_max_tokens_buf =
                crate::ui::status_bar::format_number(self.draft.chat_max_tokens);
            self.chat_unlimited_tokens = self.draft.chat_max_tokens_unlimited;
            self.icon_changed = true;
            self.font_changed = true;
            self.theme_changed = true;
            self.show_reset_confirm = false;
        } else if cancel {
            self.show_reset_confirm = false;
        }
    }

    /// Render the collapsible setting groups inside the scroll area.
    fn draw_sections(&mut self, ui: &mut egui::Ui) {
        // ── Appearance ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_appearance"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_appearance")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_appearance")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.language"))
                        .on_hover_text(crate::i18n::t("settings_hint.language"));
                    let current_lang_label = crate::i18n::LANGUAGES
                        .iter()
                        .find(|(c, _)| *c == self.draft.language)
                        .map(|(_, name)| *name)
                        .unwrap_or("English");
                    egui::ComboBox::from_id_salt("settings_language_combo")
                        .selected_text(current_lang_label)
                        .show_ui(ui, |ui| {
                            for (code, name) in crate::i18n::LANGUAGES {
                                ui.selectable_value(
                                    &mut self.draft.language,
                                    (*code).to_string(),
                                    *name,
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.font_size"))
                        .on_hover_text(crate::i18n::t("settings_hint.font_size"));
                    let old_size = self.draft.font_size;
                    let current_pt = self.draft.font_size.round() as i32;
                    egui::ComboBox::from_id_salt("font_size_combo")
                        .selected_text(format!("{} pt", current_pt))
                        .show_ui(ui, |ui| {
                            for sz in 8..=32 {
                                ui.selectable_value(
                                    &mut self.draft.font_size,
                                    sz as f32,
                                    format!("{} pt", sz),
                                );
                            }
                        });
                    if self.draft.font_size != old_size {
                        self.font_changed = true;
                    }
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.default_theme"))
                        .on_hover_text(crate::i18n::t("settings_hint.default_theme"));
                    let old_theme = self.draft.default_theme;
                    egui::ComboBox::from_id_salt("theme_combo")
                        .selected_text(self.draft.default_theme.label())
                        .show_ui(ui, |ui| {
                            for &preset in ThemeMode::ALL {
                                ui.selectable_value(
                                    &mut self.draft.default_theme,
                                    preset,
                                    preset.label(),
                                );
                            }
                        });
                    if self.draft.default_theme != old_theme {
                        self.theme_changed = true;
                    }
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.body_font"))
                        .on_hover_text(crate::i18n::t("settings_hint.body_font"));
                    let old_body_font = self.draft.body_font;
                    egui::ComboBox::from_id_salt("body_font_combo")
                        .selected_text(self.draft.body_font.label_t())
                        .show_ui(ui, |ui| {
                            for &choice in BodyFont::ALL {
                                ui.selectable_value(
                                    &mut self.draft.body_font,
                                    choice,
                                    choice.label_t(),
                                );
                            }
                        });
                    if self.draft.body_font != old_body_font {
                        self.font_changed = true;
                    }
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.custom_font"))
                        .on_hover_text(crate::i18n::t("settings_hint.custom_font"));
                    let old_path = self.draft.custom_font_path.clone();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.custom_font_path)
                                .hint_text(crate::i18n::t("settings_hint.custom_font_placeholder"))
                                .desired_width(220.0),
                        );
                        if ui.button(crate::i18n::t("dialog.swb_browse")).clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .add_filter("Font (.ttf, .otf, .ttc)", &["ttf", "otf", "ttc"])
                                .pick_file()
                        {
                            self.draft.custom_font_path = p.to_string_lossy().into_owned();
                        }
                        if !self.draft.custom_font_path.is_empty()
                            && ui.button(crate::i18n::t("settings.clear")).clicked()
                        {
                            self.draft.custom_font_path.clear();
                        }
                    });
                    if self.draft.custom_font_path != old_path {
                        self.font_changed = true;
                    }
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.icon_color"))
                        .on_hover_text(crate::i18n::t("settings_hint.icon_color"));
                    let old_icon = self.draft.icon_variant;
                    ui.horizontal(|ui| {
                        paint_icon_swatch(ui, self.draft.icon_variant.preview_color());
                        egui::ComboBox::from_id_salt("icon_combo")
                            .selected_text(self.draft.icon_variant.label())
                            .show_ui(ui, |ui| {
                                for &variant in IconVariant::ALL {
                                    ui.horizontal(|ui| {
                                        paint_icon_swatch(ui, variant.preview_color());
                                        ui.selectable_value(
                                            &mut self.draft.icon_variant,
                                            variant,
                                            variant.label(),
                                        );
                                    });
                                }
                            });
                    });
                    if self.draft.icon_variant != old_icon {
                        self.icon_changed = true;
                    }
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.window_controls"))
                        .on_hover_text(crate::i18n::t("settings_hint.window_controls"));
                    ui.checkbox(&mut self.draft.use_custom_title_bar, "");
                    ui.end_row();
                });
        });

        // ── Table View ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_table_view"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_table")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_table")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.show_row_numbers"))
                        .on_hover_text(crate::i18n::t("settings_hint.show_row_numbers"));
                    ui.checkbox(&mut self.draft.show_row_numbers, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.show_sequential_rows"))
                        .on_hover_text(crate::i18n::t("settings_hint.show_sequential_rows"));
                    ui.checkbox(&mut self.draft.show_sequential_row_numbers, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.alternating_rows"))
                        .on_hover_text(crate::i18n::t("settings_hint.alternating_rows"));
                    ui.checkbox(&mut self.draft.alternating_row_colors, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.negative_red"))
                        .on_hover_text(crate::i18n::t("settings_hint.negative_red"));
                    ui.checkbox(&mut self.draft.negative_numbers_red, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.thousand_sep"))
                        .on_hover_text(crate::i18n::t("settings_hint.thousand_sep"));
                    ui.checkbox(&mut self.draft.thousands_separators_in_cells, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.number_style"))
                        .on_hover_text(crate::i18n::t("settings_hint.number_style"));
                    egui::ComboBox::from_id_salt("settings_number_separator_style")
                        .selected_text(self.draft.number_separator_style.label_t())
                        .show_ui(ui, |ui| {
                            for style in
                                crate::data::num_format::SeparatorStyle::ALL.iter().copied()
                            {
                                ui.selectable_value(
                                    &mut self.draft.number_separator_style,
                                    style,
                                    style.label_t(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.highlight_edits"))
                        .on_hover_text(crate::i18n::t("settings_hint.highlight_edits"));
                    ui.checkbox(&mut self.draft.highlight_edits, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.cell_line_breaks"))
                        .on_hover_text(crate::i18n::t("settings_hint.cell_line_breaks"));
                    ui.checkbox(&mut self.draft.cell_line_breaks, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.binary_display"))
                        .on_hover_text(crate::i18n::t("settings_hint.binary_display"));
                    egui::ComboBox::from_id_salt("binary_display_combo")
                        .selected_text(self.draft.binary_display_mode.label_t())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.draft.binary_display_mode,
                                BinaryDisplayMode::Binary,
                                BinaryDisplayMode::Binary.label_t(),
                            );
                            ui.selectable_value(
                                &mut self.draft.binary_display_mode,
                                BinaryDisplayMode::Hex,
                                BinaryDisplayMode::Hex.label_t(),
                            );
                            ui.selectable_value(
                                &mut self.draft.binary_display_mode,
                                BinaryDisplayMode::Text,
                                BinaryDisplayMode::Text.label_t(),
                            );
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.default_mark_color"))
                        .on_hover_text(crate::i18n::t("settings_hint.default_mark_color"));
                    egui::ComboBox::from_id_salt("default_mark_color_combo")
                        .selected_text(self.draft.default_mark_color.label_t())
                        .show_ui(ui, |ui| {
                            for &color in MarkColor::ALL {
                                ui.selectable_value(
                                    &mut self.draft.default_mark_color,
                                    color,
                                    color.label_t(),
                                );
                            }
                        });
                    ui.end_row();
                });
        });

        // ── Summary ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_summary"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_summary")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.summary_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);
            egui::Grid::new("settings_summary")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    use crate::data::summary::SummaryStat;
                    for stat in SummaryStat::all() {
                        ui.label(crate::i18n::t(stat.i18n_key()))
                            .on_hover_text(crate::i18n::t(stat.hint_key()));
                        if stat.is_mandatory() {
                            // Column name + type are always shown: render a
                            // disabled, checked box so the user sees they are on
                            // but can't turn them off.
                            let mut on = true;
                            ui.add_enabled(false, egui::Checkbox::new(&mut on, ""));
                        } else {
                            let mut on = self.draft.summary_stats.contains(&stat);
                            if ui.checkbox(&mut on, "").changed() {
                                if on {
                                    if !self.draft.summary_stats.contains(&stat) {
                                        self.draft.summary_stats.push(stat);
                                    }
                                } else {
                                    self.draft.summary_stats.retain(|s| *s != stat);
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        });

        // ── Search & Editor ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_search_editor"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_search_editor")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_search_editor")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.default_search_mode"))
                        .on_hover_text(crate::i18n::t("settings_hint.default_search_mode"));
                    egui::ComboBox::from_id_salt("search_mode_combo")
                        .selected_text(self.draft.default_search_mode.label_t())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.draft.default_search_mode,
                                SearchMode::Plain,
                                crate::i18n::t("enum.search_plain"),
                            );
                            ui.selectable_value(
                                &mut self.draft.default_search_mode,
                                SearchMode::Wildcard,
                                crate::i18n::t("enum.search_wildcard"),
                            );
                            ui.selectable_value(
                                &mut self.draft.default_search_mode,
                                SearchMode::Regex,
                                crate::i18n::t("enum.search_regex"),
                            );
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.search_result_mode"))
                        .on_hover_text(crate::i18n::t("settings_hint.search_result_mode"));
                    egui::ComboBox::from_id_salt("search_result_mode_combo")
                        .selected_text(self.draft.search_result_mode.label_t())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.draft.search_result_mode,
                                SearchResultMode::Filter,
                                crate::i18n::t("enum.search_result_filter"),
                            );
                            ui.selectable_value(
                                &mut self.draft.search_result_mode,
                                SearchResultMode::Highlight,
                                crate::i18n::t("enum.search_result_highlight"),
                            );
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.search_history_limit"))
                        .on_hover_text(crate::i18n::t("settings_hint.search_history_limit"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search_history_limit_buf)
                            .desired_width(120.0)
                            .hint_text("5"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.tab_size"))
                        .on_hover_text(crate::i18n::t("settings_hint.tab_size"));
                    egui::ComboBox::from_id_salt("tab_size_combo")
                        .selected_text(self.draft.tab_size.to_string())
                        .width(40.0)
                        .show_ui(ui, |ui| {
                            for n in 1..=16 {
                                ui.selectable_value(&mut self.draft.tab_size, n, n.to_string());
                            }
                        });
                    ui.end_row();
                });
        });

        // ── File-Specific ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_file_specific"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_format")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_format")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.color_aligned"))
                        .on_hover_text(crate::i18n::t("settings_hint.color_aligned"));
                    ui.checkbox(&mut self.draft.color_aligned_columns, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.warn_unalign"))
                        .on_hover_text(crate::i18n::t("settings_hint.warn_unalign"));
                    ui.checkbox(&mut self.draft.warn_raw_align_reload, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.warn_date_change"))
                        .on_hover_text(crate::i18n::t("settings_hint.warn_date_change"));
                    ui.checkbox(&mut self.draft.warn_on_date_format_change, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.trim_whitespace"))
                        .on_hover_text(crate::i18n::t("settings_hint.trim_whitespace"));
                    ui.checkbox(&mut self.draft.trim_whitespace_on_load, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.warn_trim"))
                        .on_hover_text(crate::i18n::t("settings_hint.warn_trim"));
                    ui.checkbox(&mut self.draft.warn_on_whitespace_trim, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.offer_repair"))
                        .on_hover_text(crate::i18n::t("settings_hint.offer_repair"));
                    ui.checkbox(&mut self.draft.offer_repair_on_malformed, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.readonly_notice"))
                        .on_hover_text(crate::i18n::t("settings_hint.readonly_notice"));
                    ui.checkbox(&mut self.draft.show_readonly_notice, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.notebook_output"))
                        .on_hover_text(crate::i18n::t("settings_hint.notebook_output"));
                    egui::ComboBox::from_id_salt("notebook_layout_combo")
                        .selected_text(self.draft.notebook_output_layout.label_t())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.draft.notebook_output_layout,
                                NotebookOutputLayout::Beside,
                                crate::i18n::t("enum.nb_beside"),
                            );
                            ui.selectable_value(
                                &mut self.draft.notebook_output_layout,
                                NotebookOutputLayout::Beneath,
                                crate::i18n::t("enum.nb_beneath"),
                            );
                        });
                    ui.end_row();
                });
        });

        // ── SQL ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_sql"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_sql")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_sql")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.sql_open_default"))
                        .on_hover_text(crate::i18n::t("settings_hint.sql_open_default"));
                    ui.checkbox(&mut self.draft.sql_panel_default_open, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.sql_panel_position"))
                        .on_hover_text(crate::i18n::t("settings_hint.sql_panel_position"));
                    egui::ComboBox::from_id_salt("sql_panel_position_combo")
                        .selected_text(self.draft.sql_panel_position.label_t())
                        .show_ui(ui, |ui| {
                            for &pos in SqlPanelPosition::ALL {
                                ui.selectable_value(
                                    &mut self.draft.sql_panel_position,
                                    pos,
                                    pos.label_t(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.default_row_limit"))
                        .on_hover_text(crate::i18n::t("settings_hint.sql_row_limit"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sql_row_limit_buf)
                            .desired_width(80.0)
                            .hint_text("100"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.autocomplete"))
                        .on_hover_text(crate::i18n::t("settings_hint.autocomplete"));
                    ui.checkbox(&mut self.draft.sql_autocomplete, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.editor_font"))
                        .on_hover_text(crate::i18n::t("settings_hint.editor_font"));
                    egui::ComboBox::from_id_salt("sql_editor_font_combo")
                        .selected_text(self.draft.sql_editor_font.label_t())
                        .show_ui(ui, |ui| {
                            for &font in SqlEditorFont::ALL {
                                ui.selectable_value(
                                    &mut self.draft.sql_editor_font,
                                    font,
                                    font.label_t(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.sql_diff_highlight"))
                        .on_hover_text(crate::i18n::t("settings_hint.sql_diff_highlight"));
                    ui.checkbox(&mut self.draft.sql_row_diff_highlight_enabled, "");
                    ui.end_row();

                    ui.add_enabled_ui(self.draft.sql_row_diff_highlight_enabled, |ui| {
                        ui.label(crate::i18n::t("settings.sql_diff_secs"))
                            .on_hover_text(crate::i18n::t("settings_hint.sql_diff_secs"));
                    });
                    ui.add_enabled_ui(self.draft.sql_row_diff_highlight_enabled, |ui| {
                        egui::ComboBox::from_id_salt("sql_diff_secs_combo")
                            .selected_text(format!("{}", self.draft.sql_row_diff_highlight_secs))
                            .width(56.0)
                            .show_ui(ui, |ui| {
                                for n in [1u32, 2, 3, 4, 5, 8, 10, 15] {
                                    ui.selectable_value(
                                        &mut self.draft.sql_row_diff_highlight_secs,
                                        n,
                                        n.to_string(),
                                    );
                                }
                            });
                    });
                    ui.end_row();
                });
        });

        // ── MCP server ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_mcp"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_mcp")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.mcp_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);
            egui::Grid::new("settings_mcp")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.default_row_limit"))
                        .on_hover_text(crate::i18n::t("settings_hint.mcp_row_limit"));
                    ui.horizontal(|ui| {
                        let edit = egui::TextEdit::singleline(&mut self.mcp_row_limit_buf)
                            .desired_width(100.0)
                            .hint_text("1,000");
                        ui.add_enabled(!self.mcp_unlimited_rows, edit);
                        ui.checkbox(
                            &mut self.mcp_unlimited_rows,
                            crate::i18n::t("settings.unlimited"),
                        );
                    });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.cell_byte_cap"))
                        .on_hover_text(crate::i18n::t("settings_hint.cell_byte_cap"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.mcp_cell_bytes_buf)
                            .desired_width(120.0)
                            .hint_text("65,536"),
                    );
                    ui.end_row();
                });
        });

        // ── Chat / Assistant ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_chat"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_chat")
        // Opens expanded only when launched from the chat panel's Settings
        // button (consumes the one-shot flag).
        .default_open(std::mem::take(&mut self.focus_chat_section))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.chat_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);

            egui::Grid::new("settings_chat")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("chat.provider"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_provider"));
                    egui::ComboBox::from_id_salt("settings_chat_provider")
                        .selected_text(self.draft.chat_provider.label())
                        .show_ui(ui, |ui| {
                            for kind in ChatProviderKind::ALL {
                                ui.selectable_value(
                                    &mut self.draft.chat_provider,
                                    *kind,
                                    kind.label(),
                                );
                            }
                        });
                    ui.end_row();

                    let provider = self.draft.chat_provider;
                    ui.label(crate::i18n::t("chat.default_model"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_model"));
                    self.chat_model_picker(ui, provider);
                    ui.end_row();

                    // The preset model lists come from a hand-editable
                    // models.toml beside settings.toml; let the user reload it
                    // after editing without restarting.
                    ui.label(crate::i18n::t("chat.models_file"));
                    ui.vertical(|ui| {
                        if ui.button(crate::i18n::t("chat.reload_models")).clicked() {
                            crate::ui::settings::chat_models::reload();
                        }
                        ui.label(
                            egui::RichText::new(
                                crate::ui::settings::chat_models::path()
                                    .display()
                                    .to_string(),
                            )
                            .weak()
                            .size(11.0),
                        );
                    });
                    ui.end_row();

                    if provider == ChatProviderKind::OpenAiCompatible {
                        ui.label(crate::i18n::t("chat.base_url"))
                            .on_hover_text(crate::i18n::t("settings_hint.chat_base_url"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.chat_base_url)
                                .desired_width(280.0)
                                .hint_text("https://openrouter.ai/api/v1"),
                        );
                        ui.end_row();
                    }
                    if provider == ChatProviderKind::Ollama {
                        ui.label(crate::i18n::t("chat.ollama_url"))
                            .on_hover_text(crate::i18n::t("settings_hint.chat_ollama_url"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.chat_ollama_url)
                                .desired_width(280.0)
                                .hint_text("http://localhost:11434"),
                        );
                        ui.end_row();
                    }

                    // Text inputs (not Slider/DragValue) so Settings never shows
                    // the drag-resize cursor; parsed + clamped on Apply.
                    ui.label(crate::i18n::t("chat.temperature"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_temperature"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_temperature_buf)
                            .desired_width(100.0)
                            .hint_text("0.0"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("chat.max_iterations"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_max_iterations"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_max_iterations_buf)
                            .desired_width(100.0)
                            .hint_text("12"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("chat.max_tokens"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_max_tokens"));
                    ui.horizontal(|ui| {
                        let edit = egui::TextEdit::singleline(&mut self.chat_max_tokens_buf)
                            .desired_width(100.0)
                            .hint_text("16,384");
                        ui.add_enabled(!self.chat_unlimited_tokens, edit);
                        ui.checkbox(
                            &mut self.chat_unlimited_tokens,
                            crate::i18n::t("settings.unlimited"),
                        );
                    });
                    ui.end_row();

                    ui.label(crate::i18n::t("chat.position"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_position"));
                    egui::ComboBox::from_id_salt("settings_chat_position")
                        .selected_text(self.draft.chat_panel_position.label_t())
                        .show_ui(ui, |ui| {
                            for option in ChatPanelPosition::ALL {
                                ui.selectable_value(
                                    &mut self.draft.chat_panel_position,
                                    *option,
                                    option.label_t(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("chat.export_dir"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_export_dir"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.draft.chat_export_dir)
                                .desired_width(280.0),
                        );
                        if ui.button(crate::i18n::t("chat.browse")).clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            self.draft.chat_export_dir = dir.to_string_lossy().into_owned();
                        }
                    });
                    ui.end_row();

                    ui.label(crate::i18n::t("chat.audit_log"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_audit_log"));
                    ui.checkbox(&mut self.draft.chat_audit_log_enabled, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("chat.audit_warn"))
                        .on_hover_text(crate::i18n::t("settings_hint.chat_audit_warn"));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.draft.chat_audit_log_warn_enabled, "");
                        ui.add_enabled_ui(self.draft.chat_audit_log_warn_enabled, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.chat_audit_warn_mb_buf)
                                    .desired_width(56.0),
                            );
                            ui.label(crate::i18n::t("chat.audit_warn_mb"));
                        });
                    });
                    ui.end_row();
                });

            // API-key management for the active provider; keyless providers
            // (Ollama) just show a note. Keyring writes are immediate; the
            // plaintext fallback commits with the rest of the draft on Apply.
            let provider = self.draft.chat_provider;
            if provider.needs_api_key() {
                ui.separator();
                ui.label(crate::i18n::t("chat.api_key"))
                    .on_hover_text(crate::i18n::t("settings_hint.chat_api_key"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chat_key_input_buf)
                            .password(true)
                            .hint_text(crate::i18n::t("chat.api_key_hint"))
                            .desired_width(220.0),
                    );
                    if ui.button(crate::i18n::t("chat.save_key")).clicked()
                        && !self.chat_key_input_buf.trim().is_empty()
                    {
                        let key = self.chat_key_input_buf.trim().to_string();
                        match secrets::set_api_key(provider, &key, &mut self.draft) {
                            Ok(true) => {
                                self.chat_key_status_msg =
                                    Some(crate::i18n::t("chat.key_stored_keyring"));
                            }
                            Ok(false) => {
                                self.chat_key_status_msg = Some(format!(
                                    "{} {}",
                                    crate::i18n::t("chat.key_stored_plaintext"),
                                    secrets::plaintext_path().display()
                                ));
                            }
                            Err(e) => self.chat_key_status_msg = Some(e),
                        }
                        self.chat_key_input_buf.clear();
                    }
                    if ui.button(crate::i18n::t("chat.clear_key")).clicked() {
                        // Don't wipe the key on a single click - arm a
                        // confirmation row instead (rendered just below).
                        self.chat_key_clear_confirm = Some(provider);
                        self.chat_key_status_msg = None;
                    }
                });
                // Confirmation row: only an explicit second click deletes the
                // saved key. Cancel disarms it.
                if self.chat_key_clear_confirm == Some(provider) {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(crate::i18n::t("chat.clear_key_confirm"))
                                .color(egui::Color32::from_rgb(0xd9, 0x53, 0x4f)),
                        );
                        if ui.button(crate::i18n::t("chat.clear_key_yes")).clicked() {
                            secrets::delete_api_key(provider, &mut self.draft);
                            self.chat_key_status_msg = Some(crate::i18n::t("chat.key_cleared"));
                            self.chat_key_clear_confirm = None;
                        }
                        if ui.button(crate::i18n::t("chat.clear_key_cancel")).clicked() {
                            self.chat_key_clear_confirm = None;
                        }
                    });
                }
                let where_msg = match secrets::storage_location(provider, &self.draft) {
                    secrets::KeyStorage::Env(var) => {
                        format!("{} {var}", crate::i18n::t("chat.key_source_env"))
                    }
                    secrets::KeyStorage::Keyring => crate::i18n::t("chat.key_source_keyring"),
                    secrets::KeyStorage::Plaintext(path) => format!(
                        "{} {}",
                        crate::i18n::t("chat.key_source_plaintext"),
                        path.display()
                    ),
                    secrets::KeyStorage::None => crate::i18n::t("chat.key_source_none"),
                };
                ui.label(format!("{} {where_msg}", crate::i18n::t("chat.key_source")));
                if let Some(msg) = &self.chat_key_status_msg {
                    ui.colored_label(egui::Color32::from_rgb(0x30, 0x80, 0x30), msg);
                }
                // Keys stored in the keyring take effect at once, but the
                // plaintext fallback only commits with the rest of the dialog -
                // tell the user to Apply.
                ui.label(
                    egui::RichText::new(crate::i18n::t("chat.key_apply_hint"))
                        .weak()
                        .size(11.0),
                );
            } else {
                ui.separator();
                ui.label(crate::i18n::t("chat.ollama_no_key"));
            }

            // Per-provider key overview, so the user can see at a glance which
            // providers are configured (not just the selected one).
            ui.separator();
            ui.label(egui::RichText::new(crate::i18n::t("chat.key_status_title")).strong());
            egui::Grid::new("settings_chat_keystatus")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    for kind in ChatProviderKind::ALL {
                        ui.label(kind.label());
                        if !kind.needs_api_key() {
                            ui.weak(crate::i18n::t("chat.ollama_no_key_short"));
                            ui.end_row();
                            continue;
                        }
                        let (text, set) = match secrets::storage_location(*kind, &self.draft) {
                            secrets::KeyStorage::Env(var) => (
                                format!("{} ({var})", crate::i18n::t("chat.key_source_env")),
                                true,
                            ),
                            secrets::KeyStorage::Keyring => {
                                (crate::i18n::t("chat.key_source_keyring"), true)
                            }
                            secrets::KeyStorage::Plaintext(_) => {
                                (crate::i18n::t("chat.key_source_plaintext_short"), true)
                            }
                            secrets::KeyStorage::None => {
                                (crate::i18n::t("chat.key_source_none"), false)
                            }
                        };
                        if set {
                            ui.colored_label(egui::Color32::from_rgb(0x30, 0x80, 0x30), text);
                        } else {
                            ui.weak(text);
                        }
                        ui.end_row();
                    }
                });
        });

        // ── Map ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_map"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_map")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.map_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);
            egui::Grid::new("settings_map")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.map_default_mode"))
                        .on_hover_text(crate::i18n::t("settings_hint.map_default_mode"));
                    egui::ComboBox::from_id_salt("map_default_mode_combo")
                        .selected_text(self.draft.map_default_mode.label_t())
                        .show_ui(ui, |ui| {
                            for &m in MapMode::ALL {
                                ui.selectable_value(
                                    &mut self.draft.map_default_mode,
                                    m,
                                    m.label_t(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.map_fallback"))
                        .on_hover_text(crate::i18n::t("settings_hint.map_fallback"));
                    ui.checkbox(&mut self.draft.map_fallback_to_geometry, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.tile_url"))
                        .on_hover_text(crate::i18n::t("settings_hint.tile_url"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.draft.map_tile_url_template)
                            .desired_width(380.0)
                            .hint_text("https://tile.openstreetmap.org/{z}/{x}/{y}.png"),
                    );
                    ui.end_row();
                });
        });

        // ── Directory Tree ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_directory_tree"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_directory_tree")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_directory_tree")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.sidebar_position"))
                        .on_hover_text(crate::i18n::t("settings_hint.sidebar_position"));
                    egui::ComboBox::from_id_salt("directory_tree_position_combo")
                        .selected_text(self.draft.directory_tree_position.label_t())
                        .show_ui(ui, |ui| {
                            for &pos in DirectoryTreePosition::ALL {
                                ui.selectable_value(
                                    &mut self.draft.directory_tree_position,
                                    pos,
                                    pos.label_t(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.directory_tree_filter"))
                        .on_hover_text(crate::i18n::t("settings_hint.directory_tree_filter"));
                    ui.checkbox(&mut self.draft.directory_tree_filter_enabled, "");
                    ui.end_row();
                });
        });

        // ── Shortcuts ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_shortcuts"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_shortcuts")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::t("settings_hint.shortcuts_intro"))
                    .weak()
                    .size(11.0),
            );
            ui.add_space(6.0);
            self.draw_shortcuts_grid(ui);
        });

        // ── Performance ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_performance"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_performance")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_performance")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.initial_load_cap"))
                        .on_hover_text(crate::i18n::t("settings_hint.initial_load_cap"));
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            !self.draft.initial_load_rows_unlimited,
                            egui::TextEdit::singleline(&mut self.initial_load_rows_buf)
                                .desired_width(120.0)
                                .hint_text("5,000,000"),
                        );
                        ui.checkbox(
                            &mut self.draft.initial_load_rows_unlimited,
                            crate::i18n::t("settings.unlimited"),
                        )
                        .on_hover_text(crate::i18n::t("settings_hint.initial_load_unlimited"));
                    });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.syntax_size_cap"))
                        .on_hover_text(crate::i18n::t("settings_hint.syntax_size_cap"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.syntax_highlight_max_bytes_buf)
                                .desired_width(100.0)
                                .hint_text("1"),
                        );
                        egui::ComboBox::from_id_salt("syntax_size_unit_combo")
                            .selected_text(self.syntax_highlight_size_unit.label_t())
                            .width(70.0)
                            .show_ui(ui, |ui| {
                                for &unit in SyntaxSizeUnit::ALL {
                                    ui.selectable_value(
                                        &mut self.syntax_highlight_size_unit,
                                        unit,
                                        unit.label_t(),
                                    );
                                }
                            });
                    });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.multi_search_cap"))
                        .on_hover_text(crate::i18n::t("settings_hint.multi_search_cap"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.grep_max_file_size_buf)
                            .desired_width(120.0)
                            .hint_text("50"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.chart_max_points"))
                        .on_hover_text(crate::i18n::t("settings_hint.chart_max_points"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chart_max_points_buf)
                            .desired_width(120.0)
                            .hint_text("100,000"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.chart_max_categories"))
                        .on_hover_text(crate::i18n::t("settings_hint.chart_max_categories"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.chart_max_categories_buf)
                            .desired_width(120.0)
                            .hint_text("200"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.tables_in_picker"))
                        .on_hover_text(crate::i18n::t("settings_hint.tables_in_picker"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.table_picker_visible_rows_buf)
                            .desired_width(120.0)
                            .hint_text("10"),
                    );
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.excel_auto_open"))
                        .on_hover_text(crate::i18n::t("settings_hint.excel_auto_open"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.excel_max_auto_sheets_buf)
                            .desired_width(120.0)
                            .hint_text("5"),
                    );
                    ui.end_row();
                });
        });

        // ── Files ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_files"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_files")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_files")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.max_recent"))
                        .on_hover_text(crate::i18n::t("settings_hint.max_recent"));
                    egui::ComboBox::from_id_salt("max_recent_combo")
                        .selected_text(self.draft.max_recent_files.to_string())
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for n in 1..=30 {
                                ui.selectable_value(
                                    &mut self.draft.max_recent_files,
                                    n,
                                    n.to_string(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.open_as_text"))
                        .on_hover_text(crate::i18n::t("settings_hint.open_as_text"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.text_mode_extensions_buf)
                            .desired_width(280.0)
                            .hint_text("log4j, myproj, rawdata"),
                    );
                    ui.end_row();
                });
        });

        // ── Window ──
        egui::CollapsingHeader::new(
            egui::RichText::new(crate::i18n::t("settings.sec_window"))
                .strong()
                .size(13.0),
        )
        .id_salt("settings_section_window")
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("settings_window")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label(crate::i18n::t("settings.start_maximised"))
                        .on_hover_text(crate::i18n::t("settings_hint.start_maximised"));
                    ui.checkbox(&mut self.draft.start_maximized, "");
                    ui.end_row();

                    ui.label(crate::i18n::t("settings.initial_window_size"))
                        .on_hover_text(crate::i18n::t("settings_hint.initial_window_size"));
                    ui.add_enabled_ui(!self.draft.start_maximized, |ui| {
                        egui::ComboBox::from_id_salt("window_size_combo")
                            .selected_text(self.draft.window_size.label())
                            .show_ui(ui, |ui| {
                                for &size in WindowSize::ALL {
                                    ui.selectable_value(
                                        &mut self.draft.window_size,
                                        size,
                                        size.label(),
                                    );
                                }
                            });
                    });
                    ui.end_row();
                });
        });
    }

    /// The model configured for `provider` in the draft, falling back to the
    /// provider's built-in default.
    fn chat_current_model(&self, provider: ChatProviderKind) -> String {
        self.draft
            .chat_models
            .get(provider.id())
            .filter(|m| !m.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| crate::ui::settings::chat_models::default_model(provider))
    }

    /// Default-model picker for the Chat section: a preset dropdown (when the
    /// provider has presets) stacked above a roomy free-text field, mirroring the
    /// panel's quick-switch but writing into `draft.chat_models`.
    fn chat_model_picker(&mut self, ui: &mut egui::Ui, provider: ChatProviderKind) {
        let presets = crate::ui::settings::chat_models::preset_models(provider);
        let mut model = self.chat_current_model(provider);
        let mut changed = false;
        const W: f32 = 320.0;

        // Vertical so the free-text field gets its own line at a comfortable
        // width instead of being squeezed next to the dropdown.
        ui.vertical(|ui| {
            if !presets.is_empty() {
                egui::ComboBox::from_id_salt(("settings_chat_model_preset", provider.id()))
                    .selected_text(if model.is_empty() {
                        "(model)".to_string()
                    } else {
                        model.clone()
                    })
                    .width(W)
                    .show_ui(ui, |ui| {
                        for m in &presets {
                            if ui.selectable_label(&model == m, m.as_str()).clicked() {
                                model = m.clone();
                                changed = true;
                            }
                        }
                    });
            }

            let resp = ui.add(
                egui::TextEdit::singleline(&mut model)
                    .desired_width(W)
                    .hint_text(crate::ui::settings::chat_models::default_model(provider)),
            );
            if resp.changed() {
                changed = true;
            }
        });

        if changed {
            self.draft
                .chat_models
                .insert(provider.id().to_string(), model);
        }
    }

    /// One grid row per [`ShortcutAction`]: name, current combo, Record/Clear/Reset.
    fn draw_shortcuts_grid(&mut self, ui: &mut egui::Ui) {
        use strum::IntoEnumIterator;
        // If the user is recording a binding, capture the next real key press.
        if let Some(action) = self.recording {
            let captured = ui.input(capture_combo);
            if let Some(CaptureResult::Cancel) = captured {
                self.recording = None;
            } else if let Some(CaptureResult::Combo(combo)) = captured {
                // Reject combos already bound to another action so two
                // functions can never share a shortcut.
                let conflict = self
                    .draft
                    .shortcuts
                    .bindings
                    .iter()
                    .find(|(other, existing)| **other != action && **existing == combo)
                    .map(|(other, _)| *other);
                if let Some(other) = conflict {
                    self.shortcut_conflict = Some(format!(
                        "{} is already bound to \"{}\". Clear that binding first or pick a different key.",
                        combo.label(),
                        other.label(),
                    ));
                } else {
                    self.draft.shortcuts.set(action, combo);
                    self.shortcut_conflict = None;
                }
                self.recording = None;
            }
        }

        if let Some(msg) = &self.shortcut_conflict {
            ui.colored_label(egui::Color32::from_rgb(0xd9, 0x53, 0x4f), msg);
            ui.add_space(4.0);
        }

        // One section per group, in `ShortcutGroup::ALL` order. Rows use fixed
        // column widths (`add_sized`) so the action / combo columns line up
        // across *every* group, and each group gets a full-width highlighted
        // header bar so the sections are easy to scan.
        const LABEL_W: f32 = 250.0;
        const COMBO_W: f32 = 160.0;
        for group in crate::ui::shortcuts::ShortcutGroup::ALL {
            let actions: Vec<ShortcutAction> = ShortcutAction::iter()
                .filter(|a| a.group() == *group)
                .collect();
            if actions.is_empty() {
                continue;
            }
            ui.add_space(8.0);
            // Highlighted header bar spanning the panel width.
            egui::Frame::NONE
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(egui::Margin::symmetric(6, 3))
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(crate::i18n::t(group.i18n_key()))
                            .strong()
                            .size(14.0)
                            .color(ui.visuals().strong_text_color()),
                    );
                });
            ui.add_space(2.0);
            let row_h = ui.spacing().interact_size.y;
            for action in actions {
                ui.horizontal(|ui| {
                    // Fixed-width columns kept for cross-row alignment, but the
                    // action and combo text are left-aligned within them (a bare
                    // `add_sized` centres its content).
                    ui.allocate_ui_with_layout(
                        egui::vec2(LABEL_W, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(LABEL_W);
                            ui.add(
                                egui::Label::new(action.label())
                                    .wrap_mode(egui::TextWrapMode::Truncate),
                            );
                        },
                    );
                    let combo = self.draft.shortcuts.combo(action);
                    let label_text = if self.recording == Some(action) {
                        egui::RichText::new("Press any key...").italics()
                    } else {
                        egui::RichText::new(combo.label()).monospace()
                    };
                    ui.allocate_ui_with_layout(
                        egui::vec2(COMBO_W, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(COMBO_W);
                            ui.add(egui::Label::new(label_text));
                        },
                    );
                    if self.recording == Some(action) {
                        if ui.button(crate::i18n::t("settings.sc_stop")).clicked() {
                            self.recording = None;
                        }
                    } else if ui.button(crate::i18n::t("settings.sc_record")).clicked() {
                        self.recording = Some(action);
                    }
                    if ui.button(crate::i18n::t("settings.clear")).clicked() {
                        self.draft.shortcuts.set(action, KeyCombo::UNBOUND);
                    }
                    if ui.button(crate::i18n::t("settings.reset")).clicked() {
                        self.draft.shortcuts.reset(action);
                    }
                });
            }
        }
    }
}
/// Result of a single-frame shortcut capture.
enum CaptureResult {
    Cancel,
    Combo(KeyCombo),
}

/// While recording, watch for a non-modifier key press and return it with the
/// current modifier state. Esc cancels.
fn capture_combo(input: &egui::InputState) -> Option<CaptureResult> {
    if input.key_pressed(egui::Key::Escape) {
        return Some(CaptureResult::Cancel);
    }
    let mods = input.modifiers;
    for ev in &input.events {
        if let egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            ..
        } = ev
        {
            if matches!(key, egui::Key::Escape) {
                return Some(CaptureResult::Cancel);
            }
            return Some(CaptureResult::Combo(KeyCombo {
                key: Some(*key),
                ctrl: mods.command,
                shift: mods.shift,
                alt: mods.alt,
            }));
        }
    }
    None
}
