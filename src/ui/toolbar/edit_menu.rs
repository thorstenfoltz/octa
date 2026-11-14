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
use crate::i18n::t;
use crate::ui::menu_entry::{disabled_tip, menu_entry};

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
        first_row_header_blocked,
        hide_rows_block,
        has_hidden_rows,
        can_undo,
        can_redo,
        can_reopen_tab,
        table,
        edits_blocked,
        ..
    } = cx;
    // Was a local in `draw_toolbar`; recomputed here so the moved body
    // reads exactly as it did before.
    let has_selected_cell = selected_cell.is_some();
    top_menu_button(
        ui,
        RichText::new(crate::i18n::t("menu.edit")).color(colors.text_primary),
        |ui| {
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
            // Edit menu entries deliberately omit shortcut suffixes -
            // bindings are discoverable via Settings -> Shortcuts; cramming
            // them into the menu was visually noisy.
            let why_undo = base
                .clone()
                .or_else(|| (!can_undo).then(|| t("greyed.nothing_to_undo")));
            if entry(ui, "edit_menu.undo", "edit_menu.undo_hint", why_undo) {
                action.undo = true;
                ui.close();
            }
            let why_redo = base
                .clone()
                .or_else(|| (!can_redo).then(|| t("greyed.nothing_to_redo")));
            if entry(ui, "edit_menu.redo", "edit_menu.redo_hint", why_redo) {
                action.redo = true;
                ui.close();
            }
            // Works with no tab open, which is when it is most useful.
            let why_reopen = (!can_reopen_tab).then(|| t("greyed.nothing_to_reopen"));
            if entry(
                ui,
                "edit_menu.reopen_tab",
                "edit_menu.reopen_tab_hint",
                why_reopen,
            ) {
                action.reopen_last_closed_tab = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.fit_all_columns",
                "edit_menu.fit_all_columns_hint",
                no_table.clone(),
            ) {
                action.fit_all_columns = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.fit_all_rows",
                "edit_menu.fit_all_rows_hint",
                no_table.clone(),
            ) {
                action.fit_all_rows = true;
                ui.close();
            }
            if let Some(f) = crate::ui::copy_as::copy_as_menu(ui, no_table.as_deref()) {
                action.copy_as = Some(f);
            }
            let why_fill = base.clone().or_else(|| {
                (selected_cols.is_empty() && selected_cells.len() < 2)
                    .then(|| t("greyed.need_two_cells"))
            });
            if entry(
                ui,
                "edit_menu.fill_series",
                "edit_menu.fill_series_hint",
                why_fill,
            ) {
                action.open_fill_series = true;
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
            if entry(
                ui,
                "edit_menu.insert_row",
                "edit_menu.insert_row_hint",
                base.clone(),
            ) {
                action.add_row = true;
                ui.close();
            }
            let why_del = base.clone().or_else(need_cell);
            if entry(
                ui,
                "edit_menu.delete_row",
                "edit_menu.delete_row_hint",
                why_del,
            ) {
                action.delete_row = true;
                ui.close();
            }
            // Hiding changes nothing in the data, so read-only mode allows it.
            if entry(
                ui,
                "edit_menu.hide_rows",
                "edit_menu.hide_rows_hint",
                hide_rows_block.map(str::to_string),
            ) {
                action.hide_selected_rows = true;
                ui.close();
            }
            let why_show_rows = no_table
                .clone()
                .or_else(|| (!has_hidden_rows).then(|| t("greyed.need_hidden_rows")));
            if entry(
                ui,
                "edit_menu.show_hidden_rows",
                "edit_menu.show_hidden_rows_hint",
                why_show_rows,
            ) {
                action.show_hidden_rows = true;
                ui.close();
            }

            let why_up = base.clone().or_else(need_cell).or_else(|| {
                selected_cell
                    .is_some_and(|(r, _)| r == 0)
                    .then(|| t("greyed.row_first"))
            });
            if entry(
                ui,
                "edit_menu.move_row_up",
                "edit_menu.move_row_up_hint",
                why_up,
            ) {
                action.move_row_up = true;
                ui.close();
            }
            let why_down = base.clone().or_else(need_cell).or_else(|| {
                selected_cell
                    .is_some_and(|(r, _)| r + 1 >= row_count)
                    .then(|| t("greyed.row_last"))
            });
            if entry(
                ui,
                "edit_menu.move_row_down",
                "edit_menu.move_row_down_hint",
                why_down,
            ) {
                action.move_row_down = true;
                ui.close();
            }

            ui.separator();

            // "Parse in new tab" submenu - opens a modal that
            // parses the chosen scope (cell / row / column / whole
            // table) as a user-picked format and opens the result
            // in a new tab. Cell / Row / Column require a selected
            // cell so we know which row+col to target; Whole table
            // needs only a table.
            let parse_hint = t("edit_menu.parse_in_new_tab_hint");
            let parse_resp = ui
                .add_enabled_ui(has_data, |ui| {
                    ui.menu_button(crate::i18n::t("edit_menu.parse_in_new_tab"), |ui| {
                        let why_cell = need_cell();
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
                        if let Some(why) = &why_cell {
                            cell_btn.on_disabled_hover_text(why);
                            row_btn.on_disabled_hover_text(why);
                            col_btn.on_disabled_hover_text(why);
                        }
                        if ui.button(crate::i18n::t("edit_menu.scope_table")).clicked() {
                            action.parse_in_new_tab = Some(ParseScope::Table);
                            ui.close();
                        }
                    })
                    .response
                })
                .inner;
            match &no_table {
                None => parse_resp.on_hover_text(parse_hint),
                Some(why) => parse_resp.on_disabled_hover_text(disabled_tip(&parse_hint, why)),
            };

            ui.separator();
            ui.label(
                RichText::new(crate::i18n::t("edit_menu.section_sort_rows"))
                    .strong()
                    .size(11.0)
                    .color(colors.text_muted),
            );
            let why_sort = no_table.clone().or_else(need_cell);
            if entry(
                ui,
                "edit_menu.sort_asc",
                "edit_menu.sort_asc_hint",
                why_sort.clone(),
            ) {
                if let Some((_, col)) = selected_cell {
                    action.sort_rows_asc_by = Some(col);
                }
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.sort_desc",
                "edit_menu.sort_desc_hint",
                why_sort,
            ) {
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
            let why_mark = no_table
                .clone()
                .or_else(|| (!menu_enabled).then(|| t("greyed.need_selection")));
            let mark_hint = t("edit_menu.mark_hint");
            let mark_resp = ui
                .add_enabled_ui(menu_enabled, |ui| {
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
                        });
                        ui.separator();
                        let clear = ui
                            .add_enabled(
                                any_currently_marked,
                                egui::Button::new(crate::i18n::t("edit_menu.clear")),
                            )
                            .on_disabled_hover_text(t("greyed.need_marks"));
                        if clear.clicked() {
                            for k in &mark_keys {
                                action.clear_marks.push(k.clone());
                            }
                            ui.close();
                        }
                        let clear_all = ui
                            .add_enabled(
                                table_has_any_marks,
                                egui::Button::new(crate::i18n::t("edit_menu.clear_all_marks")),
                            )
                            .on_disabled_hover_text(t("greyed.need_any_marks"));
                        if clear_all.clicked() {
                            action.clear_all_marks = true;
                            ui.close();
                        }
                    })
                    .response
                })
                .inner;
            match &why_mark {
                None => mark_resp.on_hover_text(mark_hint),
                Some(why) => mark_resp.on_disabled_hover_text(disabled_tip(&mark_hint, why)),
            };

            ui.separator();
            let mut header_flag = first_row_is_header;
            let header_why = no_table
                .clone()
                .or_else(|| first_row_header_blocked.map(str::to_string));
            let header_box = ui.add_enabled(
                header_why.is_none(),
                egui::Checkbox::new(
                    &mut header_flag,
                    crate::i18n::t("edit_menu.first_row_is_header"),
                ),
            );
            if header_box
                .on_hover_text(crate::i18n::t("edit_menu.first_row_is_header_hint"))
                .on_disabled_hover_text(header_why.unwrap_or_default())
                .changed()
            {
                action.toggle_first_row_header = true;
                ui.close();
            }

            ui.separator();
            if entry(
                ui,
                "edit_menu.edit_audit",
                "edit_menu.edit_audit_hint",
                no_table.clone(),
            ) {
                action.toggle_edit_audit = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.recipe_panel",
                "edit_menu.recipe_panel_hint",
                no_table.clone(),
            ) {
                action.toggle_recipe_panel = true;
                ui.close();
            }
            if entry(
                ui,
                "edit_menu.apply_recipe",
                "edit_menu.apply_recipe_hint",
                no_table.clone(),
            ) {
                action.open_apply_recipe = true;
                ui.close();
            }

            ui.separator();
            let why_discard = base
                .clone()
                .or_else(|| (!has_edits).then(|| t("greyed.need_edits")));
            if entry(
                ui,
                "edit_menu.discard_all_edits",
                "edit_menu.discard_all_edits_hint",
                why_discard,
            ) {
                action.discard_edits = true;
                ui.close();
            }
        },
    );
}
