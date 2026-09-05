//! The **Columns** menu: add, rename, delete, hide, reorder and format columns.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use super::menu_button::top_menu_button;
use super::types::{ToolbarAction, ToolbarCtx};

pub(super) fn columns_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        selected_cell,
        col_count,
        has_hidden_columns,
        ..
    } = cx;
    // Was a local in `draw_toolbar`; recomputed here so the moved body
    // reads exactly as it did before.
    let has_selected_cell = selected_cell.is_some();
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.columns")).color(colors.text_primary),
        |ui| {
            if !has_data {
                ui.weak(crate::i18n::t("menu.need_table"));
                return;
            }
            ui.set_min_width(200.0);
            if ui
                .button(crate::i18n::t("toolbar.insert_column"))
                .on_hover_text(crate::i18n::t("toolbar.insert_column_hint"))
                .clicked()
            {
                action.add_column = true;
                ui.close();
            }
            let del_col = ui
                .add_enabled(
                    has_selected_cell,
                    egui::Button::new(crate::i18n::t("toolbar.delete_column")),
                )
                .on_hover_text(crate::i18n::t("toolbar.delete_column_hint"));
            if del_col.clicked() {
                action.delete_column = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("edit_menu.rename_columns"))
                .on_hover_text(crate::i18n::t("edit_menu.rename_columns_hint"))
                .clicked()
            {
                action.open_rename_columns = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("edit_menu.fix_duplicate_cols"))
                .on_hover_text(crate::i18n::t("edit_menu.fix_duplicate_cols_hint"))
                .clicked()
            {
                action.fix_duplicate_columns = true;
                ui.close();
            }

            let can_move_left = selected_cell.is_some_and(|(_, c)| c > 0);
            let can_move_right = selected_cell.is_some_and(|(_, c)| c + 1 < col_count);

            let left_btn = ui
                .add_enabled(
                    can_move_left,
                    egui::Button::new(crate::i18n::t("edit_menu.move_col_left")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.move_col_left_hint"));
            if left_btn.clicked() {
                action.move_col_left = true;
                ui.close();
            }
            let right_btn = ui
                .add_enabled(
                    can_move_right,
                    egui::Button::new(crate::i18n::t("edit_menu.move_col_right")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.move_col_right_hint"));
            if right_btn.clicked() {
                action.move_col_right = true;
                ui.close();
            }

            let can_sort_cols = col_count > 1;
            let sort_cols_asc = ui
                .add_enabled(
                    can_sort_cols,
                    egui::Button::new(crate::i18n::t("edit_menu.sort_cols_asc")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.sort_cols_asc_hint"));
            if sort_cols_asc.clicked() {
                action.sort_columns_asc = true;
                ui.close();
            }
            let sort_cols_desc = ui
                .add_enabled(
                    can_sort_cols,
                    egui::Button::new(crate::i18n::t("edit_menu.sort_cols_desc")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.sort_cols_desc_hint"));
            if sort_cols_desc.clicked() {
                action.sort_columns_desc = true;
                ui.close();
            }

            ui.separator();

            let num_fmt_btn = ui.add_enabled(
                has_selected_cell,
                egui::Button::new(crate::i18n::t("edit_menu.number_format")),
            );
            if num_fmt_btn
                .on_hover_text(crate::i18n::t("edit_menu.number_format_hint"))
                .clicked()
            {
                action.open_column_format = true;
                ui.close();
            }

            if ui
                .button(crate::i18n::t("edit_menu.conditional_format"))
                .on_hover_text(crate::i18n::t("edit_menu.conditional_format_hint"))
                .clicked()
            {
                action.open_conditional_format = true;
                ui.close();
            }

            ui.separator();

            let show_all_btn = ui.add_enabled(
                has_hidden_columns,
                egui::Button::new(crate::i18n::t("edit_menu.show_hidden_columns")),
            );
            let show_all_btn = if !has_hidden_columns {
                show_all_btn
                    .on_disabled_hover_text(crate::i18n::t("edit_menu.show_hidden_columns_hint"))
            } else {
                show_all_btn
            };
            if show_all_btn.clicked() {
                action.show_all_columns = true;
                ui.close();
            }
        },
    );
}
