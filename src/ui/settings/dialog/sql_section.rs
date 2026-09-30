//! SQL section: panel position, row limit, autocomplete, editor font and
//! query history.
//!
//! Split out of `dialog/mod.rs`, which rendered fifteen sections inline while
//! chat, cloud and databases already had a file each. The body is unchanged:
//! `draw_sections` keeps the `CollapsingHeader` and calls this.

use egui;

use super::super::*;

/// The query the formatting preview lays out: short, but with a list, a join
/// and a second statement, so every setting visibly changes something.
const SQL_FORMAT_SAMPLE: &str = "select c.name, count(*) as orders, coalesce(sum(o.total), 0) as spent \
from customers c join orders o on o.customer_id = c.id \
where o.placed >= '2026-01-01' and c.id in (select id from vip) group by c.name; select 1";

impl SettingsDialog {
    pub(super) fn sql_section_body(&mut self, ui: &mut egui::Ui) {
        // Grouped by what a setting affects. Closed here, unlike the other
        // sections' groups: SQL has six, and the point of grouping them was
        // to stop showing all of them at once.
        Self::sub_section_open(
            ui,
            "settings.sub_sql_panel",
            "settings_sql_sub_panel",
            false,
            |ui| self.sql_panel_settings(ui),
        );
        Self::sub_section_open(
            ui,
            "settings.sub_sql_editor",
            "settings_sql_sub_editor",
            false,
            |ui| self.sql_editor_settings(ui),
        );
        Self::sub_section_open(
            ui,
            "settings.sub_sql_results",
            "settings_sql_sub_results",
            false,
            |ui| self.sql_result_settings(ui),
        );
        Self::sub_section_open(
            ui,
            "settings.sub_sql_other_tabs",
            "settings_sql_sub_other_tabs",
            false,
            |ui| self.sql_other_tabs_settings(ui),
        );
        Self::sub_section_open(
            ui,
            "db.history_title",
            "settings_sql_sub_history",
            false,
            |ui| self.sql_history_settings(ui),
        );
        Self::sub_section_open(
            ui,
            "settings.sql_format_title",
            "settings_sql_sub_format",
            false,
            |ui| self.sql_format_settings(ui),
        );
    }

    fn sql_panel_settings(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::control_grid(ui, "settings_sql_panel", |ui| {
            ui.label(crate::i18n::t("settings.sql_open_default"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_open_default"));
            ui.checkbox(&mut self.draft.sql_panel_default_open, "")
                .on_hover_text(crate::i18n::t("settings_hint.sql_open_default"));
            ui.end_row();

            ui.label(crate::i18n::t("settings.sql_keep_open"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_keep_open"));
            ui.checkbox(&mut self.draft.sql_keep_open, "")
                .on_hover_text(crate::i18n::t("settings_hint.sql_keep_open"));
            ui.end_row();

            ui.label(crate::i18n::t("settings.sql_panel_position"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_panel_position"));
            egui::ComboBox::from_id_salt("sql_panel_position_combo")
                .selected_text(self.draft.sql_panel_position.label_t())
                .show_ui(ui, |ui| {
                    for &pos in SqlPanelPosition::ALL {
                        ui.selectable_value(&mut self.draft.sql_panel_position, pos, pos.label_t());
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("settings_hint.sql_panel_position"));
            ui.end_row();
        });
    }

    fn sql_editor_settings(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::control_grid(ui, "settings_sql_editor", |ui| {
            ui.label(crate::i18n::t("settings.autocomplete"))
                .on_hover_text(crate::i18n::t("settings_hint.autocomplete"));
            ui.checkbox(&mut self.draft.sql_autocomplete, "")
                .on_hover_text(crate::i18n::t("settings_hint.autocomplete"));
            ui.end_row();

            ui.label(crate::i18n::t("settings.editor_font"))
                .on_hover_text(crate::i18n::t("settings_hint.editor_font"));
            egui::ComboBox::from_id_salt("sql_editor_font_combo")
                .selected_text(self.draft.sql_editor_font.label_t())
                .show_ui(ui, |ui| {
                    for &font in SqlEditorFont::ALL {
                        ui.selectable_value(&mut self.draft.sql_editor_font, font, font.label_t());
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("settings_hint.editor_font"));
            ui.end_row();

            ui.label(crate::i18n::t("settings.default_row_limit"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_row_limit"));
            ui.add(
                egui::TextEdit::singleline(&mut self.sql_row_limit_buf)
                    .desired_width(80.0)
                    .hint_text("100"),
            )
            .on_hover_text(crate::i18n::t("settings_hint.sql_row_limit"));
            ui.end_row();
        });
    }

    fn sql_result_settings(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::control_grid(ui, "settings_sql_results", |ui| {
            ui.label(crate::i18n::t("settings.sql_page_rows"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_page_rows"));
            ui.add(
                egui::TextEdit::singleline(&mut self.sql_result_page_rows_buf)
                    .desired_width(80.0)
                    .hint_text("1000"),
            )
            .on_hover_text(crate::i18n::t("settings_hint.sql_page_rows"));
            ui.end_row();

            ui.label(crate::i18n::t("settings.sql_diff_highlight"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_diff_highlight"));
            ui.checkbox(&mut self.draft.sql_row_diff_highlight_enabled, "")
                .on_hover_text(crate::i18n::t("settings_hint.sql_diff_highlight"));
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
                    })
                    .response
                    .on_hover_text(crate::i18n::t("settings_hint.sql_diff_secs"));
            });
            ui.end_row();
        });
    }

    fn sql_other_tabs_settings(&mut self, ui: &mut egui::Ui) {
        crate::ui::control_row::control_grid(ui, "settings_sql_other_tabs", |ui| {
            ui.label(crate::i18n::t("settings.sql_auto_register"))
                .on_hover_text(crate::i18n::t("settings_hint.sql_auto_register"));
            ui.checkbox(&mut self.draft.sql_auto_register_open_tabs, "")
                .on_hover_text(crate::i18n::t("settings_hint.sql_auto_register"));
            ui.end_row();

            let auto_on = self.draft.sql_auto_register_open_tabs;
            let max_hint = if auto_on {
                crate::i18n::t("settings_hint.sql_auto_register_max")
            } else {
                crate::i18n::t("settings_hint.sql_auto_register_max_off")
            };
            ui.add_enabled_ui(auto_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_auto_register_max"))
                    .on_hover_text(&max_hint)
                    .on_disabled_hover_text(&max_hint);
            });
            ui.add_enabled_ui(auto_on, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.sql_auto_register_max_buf)
                        .desired_width(80.0)
                        .hint_text("200000"),
                )
                .on_hover_text(&max_hint)
                .on_disabled_hover_text(&max_hint);
            });
            ui.end_row();
        });
    }

    /// Style for the SQL panel's **Format** button, with a live preview of a
    /// sample query so each choice shows what it does.
    fn sql_format_settings(&mut self, ui: &mut egui::Ui) {
        use crate::sql::format::{CommaStyle, IndentStyle, KeywordCase};
        ui.label(egui::RichText::new(crate::i18n::t("settings_hint.sql_format_title")).weak());
        let f = &mut self.draft.sql_format;
        // What one-line mode leaves nothing for: there are no line breaks
        // to place, so the layout controls say why they are off.
        let layout_on = !f.one_line;
        let off_hint = crate::i18n::t("settings_hint.sql_format_off_one_line");
        crate::ui::control_row::control_grid(ui, "settings_sql_format", |ui| {
            let hint = crate::i18n::t("settings_hint.sql_format_keywords");
            ui.label(crate::i18n::t("settings.sql_format_keywords"))
                .on_hover_text(&hint);
            egui::ComboBox::from_id_salt("sql_format_keywords")
                .selected_text(f.keyword_case.label_t())
                .show_ui(ui, |ui| {
                    for &k in KeywordCase::ALL {
                        ui.selectable_value(&mut f.keyword_case, k, k.label_t());
                    }
                })
                .response
                .on_hover_text(&hint);
            ui.end_row();

            let hint = crate::i18n::t("settings_hint.sql_format_one_line");
            ui.label(crate::i18n::t("settings.sql_format_one_line"))
                .on_hover_text(&hint);
            ui.checkbox(&mut f.one_line, "").on_hover_text(&hint);
            ui.end_row();

            let hint = crate::i18n::t("settings_hint.sql_format_indent");
            ui.add_enabled_ui(layout_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_format_indent"))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.add_enabled_ui(layout_on, |ui| {
                egui::ComboBox::from_id_salt("sql_format_indent")
                    .selected_text(f.indent.label_t())
                    .show_ui(ui, |ui| {
                        for &k in IndentStyle::ALL {
                            ui.selectable_value(&mut f.indent, k, k.label_t());
                        }
                    })
                    .response
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.end_row();

            let hint = crate::i18n::t("settings_hint.sql_format_commas");
            ui.add_enabled_ui(layout_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_format_commas"))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.add_enabled_ui(layout_on, |ui| {
                egui::ComboBox::from_id_salt("sql_format_commas")
                    .selected_text(f.commas.label_t())
                    .show_ui(ui, |ui| {
                        for &k in CommaStyle::ALL {
                            ui.selectable_value(&mut f.commas, k, k.label_t());
                        }
                    })
                    .response
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.end_row();

            let hint = crate::i18n::t("settings_hint.sql_format_joins");
            ui.add_enabled_ui(layout_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_format_joins"))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.add_enabled_ui(layout_on, |ui| {
                ui.checkbox(&mut f.joins_top_level, "")
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.end_row();

            let [list_buf, clause_buf, bracket_buf, blank_buf] = &mut self.sql_fmt_bufs;

            let hint = crate::i18n::t("settings_hint.sql_format_inline");
            ui.add_enabled_ui(layout_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_format_inline"))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.add_enabled_ui(layout_on, |ui| {
                if number_field(ui, list_buf, "0", &hint, &off_hint)
                    && let Ok(n) = parse_comma_number(list_buf)
                {
                    f.inline_width = n.min(MAX_WIDTH);
                }
            });
            ui.end_row();
            explanation(ui, "settings.sql_format_inline_explain");

            let hint = crate::i18n::t("settings_hint.sql_format_clause_width");
            let clause_on = layout_on && f.inline_width > 0;
            let clause_off = if layout_on {
                crate::i18n::t("settings_hint.sql_format_clause_needs_list")
            } else {
                off_hint.clone()
            };
            ui.add_enabled_ui(clause_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_format_clause_width"))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&clause_off);
            });
            ui.add_enabled_ui(clause_on, |ui| {
                // Empty follows the list width, which the faded text shows.
                let follows = f.inline_width.to_string();
                if number_field(ui, clause_buf, &follows, &hint, &clause_off) {
                    f.clause_width = if clause_buf.trim().is_empty() {
                        None
                    } else {
                        parse_comma_number(clause_buf)
                            .ok()
                            .map(|n| n.min(MAX_WIDTH))
                            .or(f.clause_width)
                    };
                }
            });
            ui.end_row();
            explanation(ui, "settings.sql_format_clause_explain");

            let hint = crate::i18n::t("settings_hint.sql_format_bracket_width");
            ui.add_enabled_ui(layout_on, |ui| {
                ui.label(crate::i18n::t("settings.sql_format_bracket_width"))
                    .on_hover_text(&hint)
                    .on_disabled_hover_text(&off_hint);
            });
            ui.add_enabled_ui(layout_on, |ui| {
                if number_field(ui, bracket_buf, "50", &hint, &off_hint)
                    && let Ok(n) = parse_comma_number(bracket_buf)
                {
                    f.bracket_width = n.min(MAX_WIDTH);
                }
            });
            ui.end_row();
            explanation(ui, "settings.sql_format_bracket_explain");

            let hint = crate::i18n::t("settings_hint.sql_format_blank_lines");
            ui.label(crate::i18n::t("settings.sql_format_blank_lines"))
                .on_hover_text(&hint);
            if number_field(ui, blank_buf, "1", &hint, &hint)
                && let Ok(n) = parse_comma_number(blank_buf)
            {
                f.blank_lines_between = n.min(3) as u8;
            }
            ui.end_row();

            let hint = crate::i18n::t("settings_hint.sql_format_semicolon");
            ui.label(crate::i18n::t("settings.sql_format_semicolon"))
                .on_hover_text(&hint);
            ui.checkbox(&mut f.final_semicolon, "").on_hover_text(&hint);
            ui.end_row();
        });
        let preview = crate::sql::format::format_sql(
            SQL_FORMAT_SAMPLE,
            &self.draft.sql_format,
            crate::sql::format::FormatDialect::Generic,
        );
        ui.label(crate::i18n::t("settings.sql_format_preview"))
            .on_hover_text(crate::i18n::t("settings_hint.sql_format_preview"));
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new(preview).monospace());
        });
    }

    /// Query-history settings: whether to keep the queries you run, and how
    /// many. The history is one list for every SQL editor, so it lives with
    /// the rest of the SQL settings. The i18n keys keep their `db.` names
    /// from when it sat under Databases.
    fn sql_history_settings(&mut self, ui: &mut egui::Ui) {
        let was_on = self.draft.sql_history_enabled;
        ui.checkbox(
            &mut self.draft.sql_history_enabled,
            crate::i18n::t("db.history_enabled"),
        )
        .on_hover_text(crate::i18n::t("db.history_enabled_hint"));
        if was_on && !self.draft.sql_history_enabled {
            // Switching it off means "do not keep my queries", not merely
            // "stop adding to the pile". Queries can carry literals out of the
            // data, so leaving the old file behind would be a nasty surprise.
            crate::sql::history::forget_everything();
        }
        ui.add_enabled_ui(self.draft.sql_history_enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("db.history_limit"))
                    .on_hover_text(crate::i18n::t("db.history_limit_hint"));
                let mut buf = self.draft.sql_history_limit.to_string();
                if ui
                    .add(egui::TextEdit::singleline(&mut buf).desired_width(60.0))
                    .on_hover_text(crate::i18n::t("db.history_limit_hint"))
                    .changed()
                    && let Ok(v) = buf.trim().parse::<usize>()
                {
                    self.draft.sql_history_limit = v;
                }
            });
        });
    }
}

/// Upper bound for the Format SQL widths: past it nothing changes, a line
/// that long is already off the screen.
const MAX_WIDTH: usize = 500;

/// A short number field in the settings grid. Returns whether the text
/// changed this frame, for the caller to parse.
fn number_field(
    ui: &mut egui::Ui,
    buf: &mut String,
    placeholder: &str,
    hint: &str,
    disabled_hint: &str,
) -> bool {
    ui.add(
        egui::TextEdit::singleline(buf)
            .desired_width(60.0)
            .hint_text(placeholder),
    )
    .on_hover_text(hint)
    .on_disabled_hover_text(disabled_hint)
    .changed()
}

/// What a "keep short" width means, with an example, under its field. A
/// hover alone was too easy to miss for three numbers that read alike.
fn explanation(ui: &mut egui::Ui, key: &str) {
    ui.label("");
    ui.scope(|ui| {
        ui.set_max_width(380.0);
        ui.add(egui::Label::new(egui::RichText::new(crate::i18n::t(key)).small().weak()).wrap());
    });
    ui.end_row();
}
