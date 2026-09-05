//! The **Data** menu: sort, filter, validation, bookmarks and the mark filter.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::data::ViewMode;

pub(super) fn data_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        current_view_mode,
        mark_filter_active,
        ..
    } = cx;
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.data")).color(colors.text_primary),
        |ui| {
            if !has_data {
                ui.weak(crate::i18n::t("menu.need_table"));
                return;
            }
            ui.set_min_width(200.0);
            if ui
                .button(crate::i18n::t("toolbar.time_calc"))
                .on_hover_text(crate::i18n::t("toolbar.time_calc_hint"))
                .clicked()
            {
                action.time_calc = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("transform.menu"))
                .on_hover_text(crate::i18n::t("transform.menu_hint"))
                .clicked()
            {
                action.open_transform = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("ccol.menu"))
                .on_hover_text(crate::i18n::t("ccol.menu_hint"))
                .clicked()
            {
                action.open_conditional_column = true;
                ui.close();
            }
            // Filter to marked: label flips to the "clear" variant when
            // the filter is already active on this tab.
            let filter_marked_label = if mark_filter_active {
                crate::i18n::t("edit_menu.filter_to_marked_clear")
            } else {
                crate::i18n::t("edit_menu.filter_to_marked")
            };
            if ui
                .button(filter_marked_label)
                .on_hover_text(crate::i18n::t("edit_menu.filter_to_marked_hint"))
                .clicked()
            {
                action.filter_to_marked = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("bookmarks.add"))
                .on_hover_text(crate::i18n::t("bookmarks.add_hint"))
                .clicked()
            {
                action.add_bookmark = true;
                ui.close();
            }

            ui.separator();

            if ui
                .button(crate::i18n::t("dedupe.menu"))
                .on_hover_text(crate::i18n::t("dedupe.menu_hint"))
                .clicked()
            {
                action.open_dedupe = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("impute.menu"))
                .on_hover_text(crate::i18n::t("impute.menu_hint"))
                .clicked()
            {
                action.open_impute = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("anonymize.menu"))
                .on_hover_text(crate::i18n::t("anonymize.menu_hint"))
                .clicked()
            {
                action.open_anonymize = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("tidyup.menu"))
                .on_hover_text(crate::i18n::t("tidyup.menu_hint"))
                .clicked()
            {
                action.open_tidy_up = true;
                ui.close();
            }

            // Data validation lives here rather than under
            // Columns: it is a statement about the data,
            // like dedupe and impute above it, not about
            // how a column is displayed.
            if ui
                .button(crate::i18n::t("edit_menu.validation"))
                .on_hover_text(crate::i18n::t("edit_menu.validation_hint"))
                .clicked()
            {
                action.open_validation = true;
                ui.close();
            }

            // Multi-table / file-writing data ops act on the active
            // table, so they're shown only on Table-view tabs (same
            // gate they had in the Analyse menu before the reorg).
            let table_actions = current_view_mode == ViewMode::Table;
            if table_actions {
                ui.separator();
                if ui
                    .button(crate::i18n::t("analyse_menu.multi_sort"))
                    .on_hover_text(crate::i18n::t("analyse_menu.multi_sort_hint"))
                    .clicked()
                {
                    action.open_multi_sort = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("union.menu"))
                    .on_hover_text(crate::i18n::t("union.menu_hint"))
                    .clicked()
                {
                    action.open_union = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("join.menu"))
                    .on_hover_text(crate::i18n::t("join.menu_hint"))
                    .clicked()
                {
                    action.open_join = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("fuzzy_join.menu"))
                    .on_hover_text(crate::i18n::t("fuzzy_join.menu_hint"))
                    .clicked()
                {
                    action.open_fuzzy_join = true;
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("partition.menu"))
                    .on_hover_text(crate::i18n::t("partition.menu_hint"))
                    .clicked()
                {
                    action.open_partition = true;
                    ui.close();
                }
            }
        },
    );
}
