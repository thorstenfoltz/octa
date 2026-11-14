//! The **Columns** menu: add, rename, delete, hide, reorder and format columns.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};
use crate::i18n::t;
use crate::ui::menu_entry::menu_entry;

pub(super) fn columns_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        selected_cell,
        col_count,
        has_hidden_columns,
        hide_columns_block,
        source_has_more,
        hash_on_server,
        jev_off,
        edits_blocked,
        current_view_mode,
        ..
    } = cx;
    // Was a local in `draw_toolbar`; recomputed here so the moved body
    // reads exactly as it did before.
    let has_selected_cell = selected_cell.is_some();
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.columns")).color(colors.text_primary),
        |ui| {
            ui.set_min_width(200.0);
            if edits_blocked {
                ui.weak(crate::i18n::t("context_menu.header_row_readonly"));
                ui.separator();
            }
            // Reasons, in order: no table, read-only, the entry's own.
            let no_table = (!has_data).then(|| t("greyed.need_table"));
            let base = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("context_menu.header_row_readonly")));
            let need_cell = || (!has_selected_cell).then(|| t("greyed.need_selection"));
            let entry = |ui: &mut Ui, label: &str, hint: &str, why: Option<String>| {
                menu_entry(ui, &t(label), &t(hint), why.as_deref())
            };
            if entry(
                ui,
                "toolbar.insert_column",
                "toolbar.insert_column_hint",
                base.clone(),
            ) {
                action.add_column = true;
                ui.close();
            }
            if entry(ui, "ccol.menu", "ccol.menu_hint", base.clone()) {
                action.open_conditional_column = true;
                ui.close();
            }
            // A partly loaded tab would get a hash for the loaded rows only,
            // unless the database computes it.
            let why_hash = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("hashcols.readonly_hint")))
                .or_else(|| {
                    (source_has_more && !hash_on_server).then(|| t("hashcols.partial_hint"))
                });
            if entry(ui, "hashcols.menu", "hashcols.menu_hint", why_hash) {
                action.open_hash_columns = true;
                ui.close();
            }
            let why_calc = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("calc.readonly_hint")));
            if entry(ui, "calc.menu", "calc.menu_hint", why_calc) {
                action.open_calculated_column = true;
                ui.close();
            }
            let why_bands = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("calc.readonly_hint")));
            if entry(ui, "bands.menu", "bands.menu_hint", why_bands) {
                action.open_bands = true;
                ui.close();
            }
            let why_classify = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("classify.readonly_hint")))
                .or_else(|| jev_off.then(|| t("greyed.jev_off")));
            if entry(ui, "classify.menu", "classify.menu_hint", why_classify) {
                action.open_classify = true;
                ui.close();
            }
            let why_del = base.clone().or_else(need_cell);
            if entry(
                ui,
                "header.delete_columns",
                "toolbar.delete_column_hint",
                why_del,
            ) {
                action.delete_column = true;
                ui.close();
            }
            if entry(
                ui,
                "retype.menu_entry",
                "retype.menu_entry_hint",
                base.clone(),
            ) {
                action.open_retype = true;
                ui.close();
            }
            let why_dates = no_table
                .clone()
                .or_else(|| edits_blocked.then(|| t("detect_dates.blocked_hint")));
            if entry(ui, "detect_dates.menu", "detect_dates.menu_hint", why_dates) {
                action.detect_dates = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.rename_columns",
                "edit_menu.rename_columns_hint",
                base.clone(),
            ) {
                action.open_rename_columns = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.fix_duplicate_cols",
                "edit_menu.fix_duplicate_cols_hint",
                base.clone(),
            ) {
                action.fix_duplicate_columns = true;
                ui.close();
            }

            let why_left = base.clone().or_else(need_cell).or_else(|| {
                selected_cell
                    .is_some_and(|(_, c)| c == 0)
                    .then(|| t("greyed.col_first"))
            });
            if entry(
                ui,
                "edit_menu.move_col_left",
                "edit_menu.move_col_left_hint",
                why_left,
            ) {
                action.move_col_left = true;
                ui.close();
            }
            let why_right = base.clone().or_else(need_cell).or_else(|| {
                selected_cell
                    .is_some_and(|(_, c)| c + 1 >= col_count)
                    .then(|| t("greyed.col_last"))
            });
            if entry(
                ui,
                "edit_menu.move_col_right",
                "edit_menu.move_col_right_hint",
                why_right,
            ) {
                action.move_col_right = true;
                ui.close();
            }

            let why_sort = base
                .clone()
                .or_else(|| (col_count <= 1).then(|| t("greyed.need_two_cols")));
            if entry(
                ui,
                "edit_menu.sort_cols_asc",
                "edit_menu.sort_cols_asc_hint",
                why_sort.clone(),
            ) {
                action.sort_columns_asc = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.sort_cols_desc",
                "edit_menu.sort_cols_desc_hint",
                why_sort,
            ) {
                action.sort_columns_desc = true;
                ui.close();
            }
            // Sorts the rows, not the columns, but by columns you pick, so it
            // sits with the other two sorts (the user's call, 2026-11-12).
            let why_view = no_table.clone().or_else(|| {
                (current_view_mode != crate::data::ViewMode::Table)
                    .then(|| t("greyed.need_table_view"))
            });
            if entry(
                ui,
                "analyse_menu.multi_sort",
                "analyse_menu.multi_sort_hint",
                why_view,
            ) {
                action.open_multi_sort = true;
                ui.close();
            }

            ui.separator();

            // The Column Filter window: any column, a find field, values or
            // shapes. The header funnel (and its right-click entry) is the
            // same filter for one column. Deliberately *not* suffixed with
            // the shortcut combo, same convention as the F8 read-only entry.
            if entry(
                ui,
                "columns_menu.filter",
                "columns_menu.filter_hint",
                no_table.clone(),
            ) {
                action.show_column_filter = Some(None);
                ui.close();
            }

            ui.separator();

            let why_fmt = no_table.clone().or_else(need_cell);
            if entry(
                ui,
                "edit_menu.number_format",
                "edit_menu.number_format_hint",
                why_fmt,
            ) {
                action.open_column_format = true;
                ui.close();
            }

            if entry(
                ui,
                "edit_menu.conditional_format",
                "edit_menu.conditional_format_hint",
                no_table.clone(),
            ) {
                action.open_conditional_format = true;
                ui.close();
            }

            ui.separator();

            if entry(
                ui,
                "edit_menu.hide_columns",
                "edit_menu.hide_columns_hint",
                hide_columns_block.map(str::to_string),
            ) {
                action.hide_selected_columns = true;
                ui.close();
            }
            let why_show = no_table
                .clone()
                .or_else(|| (!has_hidden_columns).then(|| t("greyed.need_hidden_cols")));
            if entry(
                ui,
                "edit_menu.show_hidden_columns",
                "edit_menu.show_hidden_columns_hint",
                why_show,
            ) {
                action.show_all_columns = true;
                ui.close();
            }
            if entry(
                ui,
                "view_menu.column_navigator",
                "view_menu.column_navigator_hint",
                no_table.clone(),
            ) {
                action.toggle_column_navigator = true;
                ui.close();
            }
        },
    );
}
