//! The **Analyse** group: SQL, Chart, value frequency and the Assistant.
//!
//! Always-visible dropdown. SQL / Chart / Value frequency act on a table, so
//! they are greyed (with the reason in the tooltip) without one or off the
//! Table view; the **Assistant** entry is always enabled (it works on any open
//! tab, including non-tabular ones).
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::data::ViewMode;
use crate::ui::menu_entry::menu_entry;

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
            let why_sql = (current_view_mode != ViewMode::Table)
                .then(|| crate::i18n::t("greyed.need_table_view"));
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.sql"),
                &crate::i18n::t("analyse_menu.sql_hint"),
                why_sql.as_deref(),
            ) {
                action.toggle_sql_panel = true;
                ui.close();
            }
            let why = if !has_data {
                Some(crate::i18n::t("greyed.need_table"))
            } else if current_view_mode != ViewMode::Table {
                Some(crate::i18n::t("greyed.need_table_view"))
            } else {
                None
            };
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.chart"),
                &crate::i18n::t("analyse_menu.chart_hint"),
                why.as_deref(),
            ) {
                action.open_chart_tab = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.value_frequency"),
                &crate::i18n::t("analyse_menu.value_frequency_hint"),
                why.as_deref(),
            ) {
                action.open_value_frequency = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.random_sample"),
                &crate::i18n::t("analyse_menu.random_sample_hint"),
                why.as_deref(),
            ) {
                action.open_random_sample = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.describe"),
                &crate::i18n::t("analyse_menu.describe_hint"),
                why.as_deref(),
            ) {
                action.open_describe_tab = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.quality"),
                &crate::i18n::t("analyse_menu.quality_hint"),
                why.as_deref(),
            ) {
                action.open_quality = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.join_keys"),
                &crate::i18n::t("analyse_menu.join_keys_hint"),
                why.as_deref(),
            ) {
                action.open_join_keys = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.join_diag"),
                &crate::i18n::t("analyse_menu.join_diag_hint"),
                why.as_deref(),
            ) {
                action.open_join_diag = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.db_compare"),
                &crate::i18n::t("analyse_menu.db_compare_hint"),
                why.as_deref(),
            ) {
                action.open_db_compare = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.folder_compare"),
                &crate::i18n::t("analyse_menu.folder_compare_hint"),
                why.as_deref(),
            ) {
                action.open_folder_compare = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("datadrift.menu"),
                &crate::i18n::t("datadrift.menu_hint"),
                why.as_deref(),
            ) {
                action.open_drift = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("relmap.menu"),
                &crate::i18n::t("relmap.menu_hint"),
                why.as_deref(),
            ) {
                action.open_rel_map = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.file_internals"),
                &crate::i18n::t("analyse_menu.file_internals_hint"),
                why.as_deref(),
            ) {
                action.open_file_internals = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.pivot"),
                &crate::i18n::t("analyse_menu.pivot_hint"),
                why.as_deref(),
            ) {
                action.open_pivot = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.timeseries"),
                &crate::i18n::t("analyse_menu.timeseries_hint"),
                why.as_deref(),
            ) {
                action.open_timeseries = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.cohorts"),
                &crate::i18n::t("analyse_menu.cohorts_hint"),
                why.as_deref(),
            ) {
                action.open_cohorts = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.funnel"),
                &crate::i18n::t("analyse_menu.funnel_hint"),
                why.as_deref(),
            ) {
                action.open_funnel = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.rfm"),
                &crate::i18n::t("analyse_menu.rfm_hint"),
                why.as_deref(),
            ) {
                action.open_rfm = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.cleanup"),
                &crate::i18n::t("analyse_menu.cleanup_hint"),
                why.as_deref(),
            ) {
                action.open_cleanup_panel = true;
                ui.close();
            }
            // Explaining a file is a chat turn, so it needs a model profile.
            let why_explain = why.clone().or_else(|| {
                (!chat_profile_available).then(|| crate::i18n::t("greyed.need_chat_profile"))
            });
            if menu_entry(
                ui,
                &crate::i18n::t("chat.explain"),
                &crate::i18n::t("chat.explain_hint"),
                why_explain.as_deref(),
            ) {
                action.explain_file = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.transpose"),
                &crate::i18n::t("analyse_menu.transpose_hint"),
                why.as_deref(),
            ) {
                action.open_transpose = true;
                ui.close();
            }
            // Needs two rows, selected or marked: one row differs from
            // nothing. The table is only read once one is open.
            let why_rows = why.clone().or_else(|| {
                let comparable =
                    crate::data::row_compare::rows_to_compare(cx.table, cx.selected_rows).len();
                (comparable < 2).then(|| crate::i18n::t("greyed.need_two_rows"))
            });
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.row_compare"),
                &crate::i18n::t("analyse_menu.row_compare_hint"),
                why_rows.as_deref(),
            ) {
                action.open_row_compare = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.correlation"),
                &crate::i18n::t("analyse_menu.correlation_hint"),
                why.as_deref(),
            ) {
                action.open_correlation = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.lookups"),
                &crate::i18n::t("analyse_menu.lookups_hint"),
                why.as_deref(),
            ) {
                action.open_lookups = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("distcmp.menu"),
                &crate::i18n::t("distcmp.menu_hint"),
                why.as_deref(),
            ) {
                action.open_dist_compare = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("refint.menu"),
                &crate::i18n::t("refint.menu_hint"),
                why.as_deref(),
            ) {
                action.open_referential = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("outliers.menu"),
                &crate::i18n::t("outliers.menu_hint"),
                why.as_deref(),
            ) {
                action.open_outliers = true;
                ui.close();
            }
            if menu_entry(
                ui,
                &crate::i18n::t("pii.menu"),
                &crate::i18n::t("pii.menu_hint"),
                why.as_deref(),
            ) {
                action.open_pii = true;
                ui.close();
            }
            ui.separator();
            if menu_entry(
                ui,
                &crate::i18n::t("analyse_menu.assistant"),
                &crate::i18n::t("analyse_menu.assistant_hint"),
                None,
            ) {
                action.toggle_chat_panel = true;
                ui.close();
            }
        },
    );
}
