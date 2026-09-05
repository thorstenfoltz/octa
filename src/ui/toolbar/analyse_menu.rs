//! The **Analyse** group: SQL, Chart, value frequency and the Assistant.
//!
//! Always-visible dropdown. SQL / Chart / Value frequency act on a table, so
//! they are shown only on Table-view tabs; the **Assistant** entry is always
//! present (it works on any open tab, including non-tabular ones) so the panel
//! stays discoverable everywhere, not just via the shortcut.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::data::ViewMode;

pub(super) fn analyse_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        current_view_mode,
        chat_profile_available,
        ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.analyse")).color(colors.text_primary),
        |ui| {
            ui.set_min_width(120.0);
            // SQL works even with no table open: attach a saved
            // database connection in the panel and query the servers
            // directly.
            if current_view_mode == ViewMode::Table
                && ui
                    .button(crate::i18n::t("analyse_menu.sql"))
                    .on_hover_text(crate::i18n::t("analyse_menu.sql_hint"))
                    .clicked()
            {
                action.toggle_sql_panel = true;
                ui.close();
            }
            let table_actions = current_view_mode == ViewMode::Table && has_data;
            if table_actions {
                if ui
                    .button(crate::i18n::t("analyse_menu.chart"))
                    .on_hover_text(crate::i18n::t("analyse_menu.chart_hint"))
                    .clicked()
                {
                    action.open_chart_tab = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.value_frequency"))
                    .on_hover_text(crate::i18n::t("analyse_menu.value_frequency_hint"))
                    .clicked()
                {
                    action.open_value_frequency = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.random_sample"))
                    .on_hover_text(crate::i18n::t("analyse_menu.random_sample_hint"))
                    .clicked()
                {
                    action.open_random_sample = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.describe"))
                    .on_hover_text(crate::i18n::t("analyse_menu.describe_hint"))
                    .clicked()
                {
                    action.open_describe_tab = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.quality"))
                    .on_hover_text(crate::i18n::t("analyse_menu.quality_hint"))
                    .clicked()
                {
                    action.open_quality = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.join_keys"))
                    .on_hover_text(crate::i18n::t("analyse_menu.join_keys_hint"))
                    .clicked()
                {
                    action.open_join_keys = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.join_diag"))
                    .on_hover_text(crate::i18n::t("analyse_menu.join_diag_hint"))
                    .clicked()
                {
                    action.open_join_diag = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.db_compare"))
                    .on_hover_text(crate::i18n::t("analyse_menu.db_compare_hint"))
                    .clicked()
                {
                    action.open_db_compare = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("datadrift.menu"))
                    .on_hover_text(crate::i18n::t("datadrift.menu_hint"))
                    .clicked()
                {
                    action.open_drift = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("relmap.menu"))
                    .on_hover_text(crate::i18n::t("relmap.menu_hint"))
                    .clicked()
                {
                    action.open_rel_map = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.file_internals"))
                    .on_hover_text(crate::i18n::t("analyse_menu.file_internals_hint"))
                    .clicked()
                {
                    action.open_file_internals = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.pivot"))
                    .on_hover_text(crate::i18n::t("analyse_menu.pivot_hint"))
                    .clicked()
                {
                    action.open_pivot = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.timeseries"))
                    .on_hover_text(crate::i18n::t("analyse_menu.timeseries_hint"))
                    .clicked()
                {
                    action.open_timeseries = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.cleanup"))
                    .on_hover_text(crate::i18n::t("analyse_menu.cleanup_hint"))
                    .clicked()
                {
                    action.open_cleanup_panel = true;
                    ui.close();
                }
                // Explaining a file is a chat turn, so it needs a model
                // profile; the hint says so on the disabled variant too.
                if ui
                    .add_enabled(
                        chat_profile_available,
                        egui::Button::new(crate::i18n::t("chat.explain")),
                    )
                    .on_hover_text(crate::i18n::t("chat.explain_hint"))
                    .on_disabled_hover_text(crate::i18n::t("chat.explain_hint"))
                    .clicked()
                {
                    action.explain_file = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.transpose"))
                    .on_hover_text(crate::i18n::t("analyse_menu.transpose_hint"))
                    .clicked()
                {
                    action.open_transpose = true;
                    ui.close();
                }
                // Needs two rows, selected or marked: one row differs from
                // nothing, and the disabled hint is the only place that says so.
                let comparable =
                    crate::data::row_compare::rows_to_compare(cx.table, cx.selected_rows).len();
                if ui
                    .add_enabled(
                        comparable >= 2,
                        egui::Button::new(crate::i18n::t("analyse_menu.row_compare")),
                    )
                    .on_hover_text(crate::i18n::t("analyse_menu.row_compare_hint"))
                    .on_disabled_hover_text(crate::i18n::t("analyse_menu.row_compare_hint"))
                    .clicked()
                {
                    action.open_row_compare = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.correlation"))
                    .on_hover_text(crate::i18n::t("analyse_menu.correlation_hint"))
                    .clicked()
                {
                    action.open_correlation = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("distcmp.menu"))
                    .on_hover_text(crate::i18n::t("distcmp.menu_hint"))
                    .clicked()
                {
                    action.open_dist_compare = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("refint.menu"))
                    .on_hover_text(crate::i18n::t("refint.menu_hint"))
                    .clicked()
                {
                    action.open_referential = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("outliers.menu"))
                    .on_hover_text(crate::i18n::t("outliers.menu_hint"))
                    .clicked()
                {
                    action.open_outliers = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("pii.menu"))
                    .on_hover_text(crate::i18n::t("pii.menu_hint"))
                    .clicked()
                {
                    action.open_pii = true;
                    ui.close();
                }
                ui.separator();
            }
            if ui
                .button(crate::i18n::t("analyse_menu.assistant"))
                .on_hover_text(crate::i18n::t("analyse_menu.assistant_hint"))
                .clicked()
            {
                action.toggle_chat_panel = true;
                ui.close();
            }
        },
    );
}
