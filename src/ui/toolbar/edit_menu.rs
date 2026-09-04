//! The **Edit** menu: undo/redo, clipboard, row and column edits, marks,
//! parse-in-new-tab.
//!
//! Split out of `toolbar/mod.rs`, where every menu lived inside one ~2,000-line
//! `draw_toolbar`. The body below is unchanged; it reads its inputs from
//! [`ToolbarCtx`] and reports the user's choice by writing to
//! [`ToolbarAction`], exactly as it did as an inline block.

use egui::{RichText, Ui};

use crate::data::{MarkColor, MarkKey};
use crate::ui::theme::ThemeColors;

use super::menu_button::top_menu_button;
use super::types::ParseScope;
use super::types::{ToolbarAction, ToolbarCtx};

pub(super) fn edit_menu(ui: &mut Ui, cx: ToolbarCtx<'_>, action: &mut ToolbarAction) {
    // Destructured rather than used as `cx.field` so the moved body needs no
    // rewriting: the locals keep the names the code already used.
    let ToolbarCtx {
        colors,
        has_data,
        has_edits,
        selected_cell,
        selected_rows,
        selected_cols,
        selected_cells,
        row_count,
        first_row_is_header,
        can_undo,
        can_redo,
        can_reopen_tab,
        table,
        ..
    } = cx;
    // Was a local in `draw_toolbar`; recomputed here so the moved body
    // reads exactly as it did before.
    let has_selected_cell = selected_cell.is_some();
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.edit")).color(colors.text_primary),
        |ui| {
            if !has_data {
                ui.weak(crate::i18n::t("menu.need_table"));
                return;
            }
            // Edit menu entries deliberately omit shortcut suffixes -
            // bindings are discoverable via Settings -> Shortcuts; cramming
            // them into the menu was visually noisy.
            if ui
                .add_enabled(
                    can_undo,
                    egui::Button::new(crate::i18n::t("edit_menu.undo")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.undo_hint"))
                .clicked()
            {
                action.undo = true;
                ui.close();
            }
            if ui
                .add_enabled(
                    can_redo,
                    egui::Button::new(crate::i18n::t("edit_menu.redo")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.redo_hint"))
                .clicked()
            {
                action.redo = true;
                ui.close();
            }
            if ui
                .add_enabled(
                    can_reopen_tab,
                    egui::Button::new(crate::i18n::t("edit_menu.reopen_tab")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.reopen_tab_hint"))
                .clicked()
            {
                action.reopen_last_closed_tab = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("edit_menu.fit_all_columns"))
                .on_hover_text(crate::i18n::t("edit_menu.fit_all_columns_hint"))
                .clicked()
            {
                action.fit_all_columns = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("edit_menu.copy_markdown"))
                .on_hover_text(crate::i18n::t("edit_menu.copy_markdown_hint"))
                .clicked()
            {
                action.copy_as_markdown = true;
                ui.close();
            }
            ui.separator();

            // Row operations
            ui.label(
                RichText::new(crate::i18n::t("edit_menu.section_rows"))
                    .strong()
                    .size(11.0)
                    .color(colors.text_muted),
            );
            if ui
                .button(crate::i18n::t("edit_menu.insert_row"))
                .on_hover_text(crate::i18n::t("edit_menu.insert_row_hint"))
                .clicked()
            {
                action.add_row = true;
                ui.close();
            }
            let del_row = ui
                .add_enabled(
                    has_selected_cell,
                    egui::Button::new(crate::i18n::t("edit_menu.delete_row")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.delete_row_hint"));
            if del_row.clicked() {
                action.delete_row = true;
                ui.close();
            }

            let can_move_up = selected_cell.is_some_and(|(r, _)| r > 0);
            let can_move_down = selected_cell.is_some_and(|(r, _)| r + 1 < row_count);

            let up_btn = ui
                .add_enabled(
                    can_move_up,
                    egui::Button::new(crate::i18n::t("edit_menu.move_row_up")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.move_row_up_hint"));
            if up_btn.clicked() {
                action.move_row_up = true;
                ui.close();
            }
            let down_btn = ui
                .add_enabled(
                    can_move_down,
                    egui::Button::new(crate::i18n::t("edit_menu.move_row_down")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.move_row_down_hint"));
            if down_btn.clicked() {
                action.move_row_down = true;
                ui.close();
            }

            ui.separator();

            // "Parse in new tab" submenu - opens a modal that
            // parses the chosen scope (cell / row / column / whole
            // table) as a user-picked format and opens the result
            // in a new tab. Cell / Row / Column require a selected
            // cell so we know which row+col to target; Whole table
            // is always available.
            ui.menu_button(crate::i18n::t("edit_menu.parse_in_new_tab"), |ui| {
                let cell_btn = ui.add_enabled(
                    has_selected_cell,
                    egui::Button::new(crate::i18n::t("edit_menu.scope_cell")),
                );
                if cell_btn.clicked()
                    && let Some((row, col)) = selected_cell
                {
                    action.parse_in_new_tab = Some(ParseScope::Cell { row, col });
                    ui.close();
                }
                let row_btn = ui.add_enabled(
                    has_selected_cell,
                    egui::Button::new(crate::i18n::t("edit_menu.scope_row")),
                );
                if row_btn.clicked()
                    && let Some((row, _)) = selected_cell
                {
                    action.parse_in_new_tab = Some(ParseScope::Row { row });
                    ui.close();
                }
                let col_btn = ui.add_enabled(
                    has_selected_cell,
                    egui::Button::new(crate::i18n::t("edit_menu.scope_column")),
                );
                if col_btn.clicked()
                    && let Some((_, col)) = selected_cell
                {
                    action.parse_in_new_tab = Some(ParseScope::Column { col });
                    ui.close();
                }
                if ui.button(crate::i18n::t("edit_menu.scope_table")).clicked() {
                    action.parse_in_new_tab = Some(ParseScope::Table);
                    ui.close();
                }
            })
            .response
            .on_hover_text(crate::i18n::t("edit_menu.parse_in_new_tab_hint"));

            ui.separator();
            ui.label(
                RichText::new(crate::i18n::t("edit_menu.section_sort_rows"))
                    .strong()
                    .size(11.0)
                    .color(colors.text_muted),
            );
            let can_sort = selected_cell.is_some();
            let sort_asc = ui
                .add_enabled(
                    can_sort,
                    egui::Button::new(crate::i18n::t("edit_menu.sort_asc")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.sort_asc_hint"));
            if sort_asc.clicked() {
                if let Some((_, col)) = selected_cell {
                    action.sort_rows_asc_by = Some(col);
                }
                ui.close();
            }
            let sort_desc = ui
                .add_enabled(
                    can_sort,
                    egui::Button::new(crate::i18n::t("edit_menu.sort_desc")),
                )
                .on_hover_text(crate::i18n::t("edit_menu.sort_desc_hint"));
            if sort_desc.clicked() {
                if let Some((_, col)) = selected_cell {
                    action.sort_rows_desc_by = Some(col);
                }
                ui.close();
            }

            ui.separator();

            // Mark submenu - surfaces the same colors as the right-click
            // context menu, scoped to the current selection.
            let mark_keys: Vec<MarkKey> = if !selected_rows.is_empty() {
                let mut rs: Vec<usize> = selected_rows.iter().copied().collect();
                rs.sort();
                rs.into_iter().map(MarkKey::Row).collect()
            } else if !selected_cols.is_empty() {
                let mut cs: Vec<usize> = selected_cols.iter().copied().collect();
                cs.sort();
                cs.into_iter().map(MarkKey::Column).collect()
            } else if !selected_cells.is_empty() {
                let mut cs: Vec<(usize, usize)> = selected_cells.iter().copied().collect();
                cs.sort();
                cs.into_iter().map(|(r, c)| MarkKey::Cell(r, c)).collect()
            } else if let Some((r, c)) = selected_cell {
                vec![MarkKey::Cell(r, c)]
            } else {
                Vec::new()
            };
            let has_marks_keys = !mark_keys.is_empty();
            let any_currently_marked = mark_keys.iter().any(|k| table.marks.contains_key(k));
            let table_has_any_marks = !table.marks.is_empty();
            // The submenu opens whenever a clear path is available -
            // either the selection has marks to color/clear, or the
            // table has marks somewhere (so "Clear all marks" applies).
            let menu_enabled = has_marks_keys || table_has_any_marks;
            ui.add_enabled_ui(menu_enabled, |ui| {
                ui.menu_button(crate::i18n::t("edit_menu.mark"), |ui| {
                    // Color buttons + scoped Clear act on the current
                    // selection; greyed when there is none so the user
                    // can still reach the always-available "Clear all
                    // marks" entry below.
                    ui.add_enabled_ui(has_marks_keys, |ui| {
                        for &color in MarkColor::ALL {
                            let swatch = ThemeColors::mark_swatch(color);
                            let label = color.label_t();
                            let btn = egui::Button::new(RichText::new(label).color(swatch));
                            if ui.add(btn).clicked() {
                                for k in &mark_keys {
                                    action.set_marks.push((k.clone(), color));
                                }
                                ui.close();
                            }
                        }
                        if any_currently_marked {
                            ui.separator();
                            if ui.button(crate::i18n::t("edit_menu.clear")).clicked() {
                                for k in &mark_keys {
                                    action.clear_marks.push(k.clone());
                                }
                                ui.close();
                            }
                        }
                    });
                    if table_has_any_marks {
                        ui.separator();
                        if ui
                            .button(crate::i18n::t("edit_menu.clear_all_marks"))
                            .clicked()
                        {
                            action.clear_all_marks = true;
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(crate::i18n::t("edit_menu.mark_hint"));
            });

            ui.separator();
            let mut header_flag = first_row_is_header;
            if ui
                .checkbox(
                    &mut header_flag,
                    crate::i18n::t("edit_menu.first_row_is_header"),
                )
                .on_hover_text(crate::i18n::t("edit_menu.first_row_is_header_hint"))
                .changed()
            {
                action.toggle_first_row_header = true;
                ui.close();
            }

            if has_edits {
                ui.separator();
                if ui
                    .button(crate::i18n::t("edit_menu.discard_all_edits"))
                    .on_hover_text(crate::i18n::t("edit_menu.discard_all_edits_hint"))
                    .clicked()
                {
                    action.discard_edits = true;
                    ui.close();
                }
            }
        },
    );
}
