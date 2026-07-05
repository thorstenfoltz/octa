//! Data-row rendering: cell paint, hover/selection highlight, inline-edit
//! TextEdit, the per-row right-click context menu, and the formula display.
//! Split out of [`super`] (the 2,300-line table_view.rs) for navigability;
//! no behaviour change.

use std::collections::HashSet;

use egui::{Align2, Color32, CursorIcon, RichText, Sense, Ui, Vec2};

use crate::data::{BinaryDisplayMode, CellValue, DataTable, MarkKey, is_numeric_data_type};
use crate::ui::status_bar::format_number;
use crate::ui::theme::ThemeColors;
use crate::ui::toolbar;

use super::{DEFAULT_COL_WIDTH, HEADER_HEIGHT, TableInteraction, TableViewState, mark_submenu};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_data_row_direct(
    ui: &mut Ui,
    painter: &egui::Painter,
    table: &mut DataTable,
    state: &mut TableViewState,
    colors: &ThemeColors,
    actual_row: usize,
    display_idx: usize,
    left_x: f32,
    row_y: f32,
    panel_rect: egui::Rect,
    interaction: &mut TableInteraction,
    show_row_numbers: bool,
    alternating_row_colors: bool,
    negative_numbers_red: bool,
    highlight_edits: bool,
    font_size: f32,
    cell_line_breaks: bool,
    clickable_links: bool,
    binary_display_mode: BinaryDisplayMode,
    row_height: f32,
    readonly: bool,
    hidden_columns: &HashSet<usize>,
    is_rainbow_theme: bool,
    thousands_separators: bool,
    separator_style: crate::data::num_format::SeparatorStyle,
    column_number_formats: &std::collections::HashMap<usize, crate::data::num_format::NumberFormat>,
    frozen_cols: usize,
    frozen_width: f32,
    search_matches: &HashSet<(usize, usize)>,
    current_match: Option<(usize, usize)>,
    conditional_format_rules: &[crate::data::conditional_format::CondRule],
    validation_violations: &HashSet<(usize, usize)>,
    outlier_cells: &HashSet<(usize, usize)>,
) {
    let is_multi_selected_row = state.selected_rows.contains(&actual_row);
    // Highlight-search backgrounds derived from the theme (translucent so the
    // cell text / row tint stay legible). Only consulted when there are matches.
    let (search_bg, search_bg_active) = crate::ui::search_highlight::highlight_colors(colors);

    let row_bg = if is_multi_selected_row {
        colors.bg_selected
    } else if alternating_row_colors && display_idx.is_multiple_of(2) {
        colors.row_even
    } else {
        colors.row_odd
    };

    let rn_rect = egui::Rect::from_min_size(
        egui::pos2(left_x, row_y),
        Vec2::new(state.row_number_width, row_height),
    );

    let data_area_left = left_x + state.row_number_width;
    let frozen_right = data_area_left + frozen_width;
    // Frozen columns paint inside the fixed band at the left edge of the
    // data area; scrolled columns are clipped to start after it so they
    // slide underneath. With no frozen band both clips collapse to the
    // original single clip.
    let frozen_clip = egui::Rect::from_min_max(
        egui::pos2(data_area_left, panel_rect.top() + HEADER_HEIGHT + 1.0),
        egui::pos2(frozen_right, panel_rect.bottom()),
    );
    let scrolled_clip = egui::Rect::from_min_max(
        egui::pos2(frozen_right, panel_rect.top() + HEADER_HEIGHT + 1.0),
        panel_rect.max,
    );

    let mut x_frozen = data_area_left;
    let mut x_scrolled = data_area_left + frozen_width - state.scroll_x;
    let row_count = table.row_count();
    let col_count = table.col_count();

    for col_idx in 0..col_count {
        if hidden_columns.contains(&col_idx) {
            // Hidden column: zero width, no paint, no x advance.
            continue;
        }
        let is_frozen = col_idx < frozen_cols;
        let x = if is_frozen { x_frozen } else { x_scrolled };
        let col_clip = if is_frozen {
            frozen_clip
        } else {
            scrolled_clip
        };
        let col_painter = painter.with_clip_rect(col_clip);
        let region_left = if is_frozen {
            data_area_left
        } else {
            frozen_right
        };
        let w = state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);

        let rect = egui::Rect::from_min_size(egui::pos2(x, row_y), Vec2::new(w, row_height));

        if rect.right() >= region_left && rect.left() <= col_clip.right() {
            let is_editing = state
                .editing_cell
                .as_ref()
                .map(|(r, c, _)| *r == actual_row && *c == col_idx)
                .unwrap_or(false);
            // Detect a web link in this cell (non-numeric columns only) so it
            // can be styled as a hyperlink and opened on Ctrl+click. Computed
            // here so both the draw and click blocks can see it. Cheap: skipped
            // entirely when the setting is off.
            let cell_link: Option<String> = if clickable_links
                && !table
                    .columns
                    .get(col_idx)
                    .is_some_and(|c| is_numeric_data_type(&c.data_type))
            {
                table.get(actual_row, col_idx).and_then(|v| {
                    let text = v.display_with_binary_mode(binary_display_mode);
                    crate::data::links::detect_url(&text).map(|u| u.to_string())
                })
            } else {
                None
            };
            let is_selected = state
                .selected_cell
                .map(|(r, c)| r == actual_row && c == col_idx)
                .unwrap_or(false);
            let is_edited = table.is_edited(actual_row, col_idx);
            let is_col_selected = state.selected_cols.contains(&col_idx);
            let is_multi_cell = state.selected_cells.contains(&(actual_row, col_idx));

            // Explicit manual marks win; otherwise a matching conditional-
            // formatting rule colours the cell. Both feed the same downstream
            // background / text-contrast logic, so a conditional colour looks
            // and behaves like a manual mark.
            let mark_color = table
                .get_mark_color(actual_row, col_idx)
                .or_else(|| {
                    if conditional_format_rules.is_empty() {
                        None
                    } else {
                        table.get(actual_row, col_idx).and_then(|v| {
                            crate::data::conditional_format::match_color(
                                conditional_format_rules,
                                col_idx,
                                &v.to_string(),
                            )
                        })
                    }
                })
                // A failed data-validation rule paints the cell red. Explicit
                // marks and conditional colours take precedence (they are the
                // user's deliberate choices).
                .or_else(|| {
                    if !validation_violations.is_empty()
                        && validation_violations.contains(&(actual_row, col_idx))
                    {
                        Some(crate::data::MarkColor::Red)
                    } else {
                        None
                    }
                })
                // A detected numeric outlier paints the cell orange. Lowest
                // precedence: manual marks, conditional colours and validation
                // failures all win.
                .or_else(|| {
                    if !outlier_cells.is_empty() && outlier_cells.contains(&(actual_row, col_idx)) {
                        Some(crate::data::MarkColor::Orange)
                    } else {
                        None
                    }
                });
            let is_any_selected =
                is_selected || is_multi_selected_row || is_col_selected || is_multi_cell;
            let is_search_match =
                !search_matches.is_empty() && search_matches.contains(&(actual_row, col_idx));
            let is_current_match = current_match == Some((actual_row, col_idx));
            let cell_bg = if is_editing {
                colors.bg_primary
            } else if is_any_selected {
                colors.bg_selected
            } else if let Some(mc) = mark_color {
                colors.mark_color(mc)
            } else if highlight_edits && is_edited {
                colors.bg_edited
            } else {
                row_bg
            };
            col_painter.rect_filled(rect, 0.0, cell_bg);
            // Highlight-search overlay: translucent so the underlying row tint,
            // mark or edit colour still shows through. Suppressed under an
            // active selection (the selection colour already marks the cell,
            // e.g. the current match becomes selected after a jump).
            if is_search_match && !is_editing && !is_any_selected {
                let overlay = if is_current_match {
                    search_bg_active
                } else {
                    search_bg
                };
                col_painter.rect_filled(rect, 0.0, overlay);
            }

            col_painter.line_segment(
                [rect.right_top(), rect.right_bottom()],
                egui::Stroke::new(0.5, colors.border_subtle),
            );

            if is_editing {
                let mut commit_text: Option<String> = None;
                if let Some((_, _, ref mut buf)) = state.editing_cell {
                    let text_rect = rect.shrink2(Vec2::new(4.0, 2.0));
                    if text_rect.intersects(panel_rect) {
                        let edit_id = ui.id().with(("cell_edit", actual_row, col_idx));
                        let edit = egui::TextEdit::singleline(buf)
                            .id(edit_id)
                            .font(egui::FontId::new(font_size, egui::FontFamily::Monospace))
                            .frame(egui::Frame::NONE)
                            .desired_width(text_rect.width());
                        let edit_response = ui.put(text_rect.intersect(panel_rect), edit);

                        if state.edit_needs_focus {
                            edit_response.request_focus();
                            // Select all text so user can immediately type to replace
                            if let Some(mut te_state) =
                                egui::TextEdit::load_state(ui.ctx(), edit_id)
                            {
                                let ccursor_range = egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(buf.len()),
                                );
                                te_state.cursor.set_char_range(Some(ccursor_range));
                                te_state.store(ui.ctx(), edit_id);
                            }
                            state.edit_needs_focus = false;
                        }

                        if edit_response.lost_focus() {
                            commit_text = Some(buf.clone());
                        }
                    }
                }
                if let Some(new_text) = commit_text {
                    if let Some(old_val) = table.get(actual_row, col_idx) {
                        let new_val = if let Some(formula) = new_text.strip_prefix('=') {
                            // Formula: evaluate and store result
                            match crate::data::evaluate_formula(formula, table) {
                                Some(result) => {
                                    // Keep result as Int if it's a whole number, otherwise Float
                                    if result.fract() == 0.0 && result.abs() < i64::MAX as f64 {
                                        crate::data::CellValue::Int(result as i64)
                                    } else {
                                        crate::data::CellValue::Float(result)
                                    }
                                }
                                None => {
                                    // Invalid formula - store as string
                                    crate::data::CellValue::String(new_text)
                                }
                            }
                        } else if matches!(old_val, CellValue::Binary(_)) {
                            CellValue::parse_binary(&new_text, binary_display_mode)
                        } else {
                            CellValue::parse_like(old_val, &new_text)
                        };
                        if new_val != *old_val {
                            table.set(actual_row, col_idx, new_val);
                            state.invalidate_row_heights();
                        }
                    }
                    state.editing_cell = None;
                }
            } else {
                if let Some(value) = table.get(actual_row, col_idx) {
                    // Numeric columns honour the global thousand-separator
                    // setting and any per-column rounding format (display
                    // only). A number stored in a non-numeric column reads as
                    // text and is left unformatted, matching the alignment /
                    // colour logic below.
                    let col_numeric = table
                        .columns
                        .get(col_idx)
                        .is_some_and(|c| is_numeric_data_type(&c.data_type));
                    let display_text = if col_numeric {
                        crate::data::num_format::format_cell_number(
                            value,
                            column_number_formats.get(&col_idx).copied(),
                            thousands_separators,
                            separator_style,
                        )
                        .unwrap_or_else(|| value.display_with_binary_mode(binary_display_mode))
                    } else {
                        value.display_with_binary_mode(binary_display_mode)
                    };
                    let is_negative = match value {
                        crate::data::CellValue::Int(n) => *n < 0,
                        crate::data::CellValue::Float(f) => *f < 0.0,
                        _ => false,
                    };
                    // Color picks both the cell variant *and* the column type: a
                    // numeric value sitting in a string column is conceptually
                    // text - render it the same way as a real string so the
                    // column reads uniformly. The variant alone isn't enough.
                    let col_numeric_for_color = table
                        .columns
                        .get(col_idx)
                        .is_some_and(|c| is_numeric_data_type(&c.data_type));
                    let text_color = if is_any_selected || mark_color.is_some() {
                        // Selection backgrounds in most themes are a translucent
                        // tint of the same hue family as `accent`, so a numeric
                        // cell painted with the accent color disappears the
                        // moment it gets selected. Mark backgrounds are saturated
                        // tints that swallow accent-colored numeric text the
                        // same way. Fall back to a high-contrast text color
                        // whenever the cell has any colored background.
                        //
                        // Rainbow easter-egg theme: `colors.text_primary`
                        // cycles through HSV hues every frame and can collide
                        // with the mark fill at unpredictable moments. Pin
                        // the text colour to white (black on pale-yellow
                        // marks) so the cell stays readable.
                        if is_rainbow_theme {
                            if mark_color.map(|c| c.needs_dark_text()).unwrap_or(false) {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            }
                        } else {
                            colors.text_primary
                        }
                    } else {
                        match value {
                            crate::data::CellValue::Null => colors.text_muted,
                            crate::data::CellValue::Int(_) | crate::data::CellValue::Float(_)
                                if col_numeric_for_color =>
                            {
                                if negative_numbers_red && is_negative {
                                    Color32::from_rgb(0xef, 0x44, 0x44)
                                } else {
                                    colors.accent
                                }
                            }
                            crate::data::CellValue::Bool(_) => colors.warning,
                            crate::data::CellValue::Nested(_) => colors.text_secondary,
                            _ => colors.text_primary,
                        }
                    };
                    // Link cells read in the theme's hyperlink colour, unless a
                    // selection / mark background needs the high-contrast colour
                    // already chosen above.
                    let text_color =
                        if cell_link.is_some() && !(is_any_selected || mark_color.is_some()) {
                            ui.visuals().hyperlink_color
                        } else {
                            text_color
                        };

                    let text_rect = rect.shrink2(Vec2::new(6.0, 0.0));
                    let cell_clip = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 2.0, rect.top()),
                        egui::pos2(rect.right() - 2.0, rect.bottom()),
                    )
                    .intersect(col_clip);

                    let galley = if cell_link.is_some() {
                        // Underline link cells so the hyperlink affordance is
                        // visible. Built as a LayoutJob because the plain
                        // painter.layout helpers cannot carry an underline.
                        let mut job = egui::text::LayoutJob::default();
                        job.wrap.max_width = if cell_line_breaks {
                            text_rect.width()
                        } else {
                            f32::INFINITY
                        };
                        job.append(
                            &display_text,
                            0.0,
                            egui::TextFormat {
                                font_id: egui::FontId::new(font_size, egui::FontFamily::Monospace),
                                color: text_color,
                                underline: egui::Stroke::new(1.0, text_color),
                                ..Default::default()
                            },
                        );
                        painter.layout_job(job)
                    } else if cell_line_breaks {
                        painter.layout(
                            display_text,
                            egui::FontId::new(font_size, egui::FontFamily::Monospace),
                            text_color,
                            text_rect.width(),
                        )
                    } else {
                        painter.layout_no_wrap(
                            display_text,
                            egui::FontId::new(font_size, egui::FontFamily::Monospace),
                            text_color,
                        )
                    };
                    let col_is_numeric = table
                        .columns
                        .get(col_idx)
                        .is_some_and(|c| is_numeric_data_type(&c.data_type));
                    let x = if col_is_numeric {
                        text_rect.right() - galley.size().x
                    } else {
                        text_rect.left()
                    };
                    painter.with_clip_rect(cell_clip).galley(
                        egui::pos2(x, text_rect.center().y - galley.size().y / 2.0),
                        galley,
                        Color32::TRANSPARENT,
                    );
                }
            }

            // Cell interactions (left click + right click). Skipped for the cell
            // currently being edited so the inline TextEdit owns clicks - without
            // this, clicking to reposition the caret hits the cell's own
            // `response.clicked()` arm, which sets `editing_cell = None` and exits
            // edit mode. Editing still commits on real focus loss (Enter / Esc /
            // clicking a different cell, whose own interaction stays active).
            if !is_editing && rect.intersects(panel_rect) {
                let interact_rect = rect.intersect(col_clip);
                let response = ui.interact(
                    interact_rect,
                    ui.id().with(("cell", actual_row, col_idx)),
                    Sense::click(),
                );

                if response.clicked() {
                    let modifiers = ui.input(|i| i.modifiers);
                    state.editing_cell = None;
                    if modifiers.command && cell_link.is_some() {
                        // Ctrl/Cmd+click on a link cell opens the URL and takes
                        // precedence over the disjoint multi-select toggle for
                        // that cell. Selection is left as it was.
                        if let Some(url) = &cell_link {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    } else if modifiers.command {
                        // Ctrl/Cmd+click toggles the clicked cell in the
                        // disjoint multi-cell selection. Promote the prior
                        // single `selected_cell` into the set on the first
                        // toggle so the original anchor isn't lost.
                        if let Some(prev) = state.selected_cell
                            && state.selected_cells.is_empty()
                        {
                            state.selected_cells.insert(prev);
                        }
                        let target = (actual_row, col_idx);
                        if state.selected_cells.contains(&target) {
                            state.selected_cells.remove(&target);
                        } else {
                            state.selected_cells.insert(target);
                        }
                        // Keyboard navigation should continue from the most
                        // recent click even when we just removed it.
                        state.selected_cell = Some(target);
                        // Disjoint cell selection lives separately from row /
                        // column selection - drop those so the precedence is
                        // unambiguous.
                        state.selected_rows.clear();
                        state.selected_cols.clear();
                        state.selection_anchor_display = None;
                    } else {
                        state.selected_cell = Some((actual_row, col_idx));
                        // Plain click resets to a single-cell selection.
                        state.selected_rows.clear();
                        state.selected_cols.clear();
                        state.selected_cells.clear();
                        state.selection_anchor_display = None;
                    }
                }
                if response.double_clicked() && !readonly {
                    state.selected_cell = Some((actual_row, col_idx));
                    let current_text = table
                        .get(actual_row, col_idx)
                        .map(|v| v.display_with_binary_mode(binary_display_mode))
                        .unwrap_or_default();
                    state.editing_cell = Some((actual_row, col_idx, current_text));
                    state.edit_needs_focus = true;
                }

                // Right-click context menu on cell
                response.context_menu(|ui| {
                    state.selected_cell = Some((actual_row, col_idx));

                    // --- Copy / Cut / Paste ---
                    ui.label(
                        RichText::new(crate::i18n::t("context_menu.sec_clipboard"))
                            .strong()
                            .size(11.0),
                    );
                    if ui.button(crate::i18n::t("header.copy")).clicked() {
                        interaction.ctx_copy = true;
                        ui.close();
                    }
                    if ui
                        .button(crate::i18n::t("context_menu.copy_markdown"))
                        .clicked()
                    {
                        interaction.ctx_copy_markdown = true;
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("header.cut")).clicked() {
                        interaction.ctx_cut = true;
                        ui.close();
                    }
                    if (state.clipboard.is_some() || state.os_clipboard_has_text)
                        && ui.button(crate::i18n::t("header.paste")).clicked()
                    {
                        interaction.ctx_paste = true;
                        ui.close();
                    }
                    ui.separator();

                    // --- Mark ---
                    // Honour the current multi-selection: if the right-clicked
                    // cell is part of the active selection, colour the whole
                    // selection (rows > columns > free cells > single cell -
                    // same precedence Ctrl+M uses). Outside the selection,
                    // mark only the clicked cell.
                    let cell_anchor = MarkKey::Cell(actual_row, col_idx);
                    let inside_cells = state.selected_cells.contains(&(actual_row, col_idx));
                    let inside_rows = state.selected_rows.contains(&actual_row);
                    let inside_cols = state.selected_cols.contains(&col_idx);
                    let mark_keys: Vec<MarkKey> = if inside_rows && !state.selected_rows.is_empty()
                    {
                        let mut rs: Vec<usize> = state.selected_rows.iter().copied().collect();
                        rs.sort_unstable();
                        rs.into_iter().map(MarkKey::Row).collect()
                    } else if inside_cols && !state.selected_cols.is_empty() {
                        let mut cs: Vec<usize> = state.selected_cols.iter().copied().collect();
                        cs.sort_unstable();
                        cs.into_iter().map(MarkKey::Column).collect()
                    } else if inside_cells && !state.selected_cells.is_empty() {
                        let mut cs: Vec<(usize, usize)> =
                            state.selected_cells.iter().copied().collect();
                        cs.sort_unstable();
                        cs.into_iter().map(|(r, c)| MarkKey::Cell(r, c)).collect()
                    } else {
                        vec![cell_anchor.clone()]
                    };
                    mark_submenu(ui, mark_keys, &cell_anchor, table, interaction);
                    ui.separator();

                    ui.label(
                        RichText::new(crate::i18n::t("context_menu.sec_row"))
                            .strong()
                            .size(11.0),
                    );
                    if ui.button(crate::i18n::t("edit_menu.insert_row")).clicked() {
                        interaction.ctx_insert_row = true;
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("edit_menu.delete_row")).clicked() {
                        interaction.ctx_delete_row = true;
                        ui.close();
                    }
                    if actual_row > 0
                        && ui.button(crate::i18n::t("edit_menu.move_row_up")).clicked()
                    {
                        interaction.ctx_move_row_up = true;
                        ui.close();
                    }
                    if actual_row + 1 < row_count
                        && ui
                            .button(crate::i18n::t("edit_menu.move_row_down"))
                            .clicked()
                    {
                        interaction.ctx_move_row_down = true;
                        ui.close();
                    }

                    ui.separator();
                    ui.label(
                        RichText::new(crate::i18n::t("context_menu.sec_column"))
                            .strong()
                            .size(11.0),
                    );
                    if ui
                        .button(crate::i18n::t("context_menu.rename_column"))
                        .clicked()
                    {
                        state.editing_col_name =
                            Some((col_idx, table.columns[col_idx].name.clone()));
                        state.edit_col_needs_focus = true;
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("toolbar.insert_column")).clicked() {
                        interaction.header_col_clicked = Some(col_idx);
                        interaction.ctx_insert_column = true;
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("header.delete_columns")).clicked() {
                        interaction.ctx_delete_column = true;
                        ui.close();
                    }
                    if col_idx > 0
                        && ui
                            .button(crate::i18n::t("edit_menu.move_col_left"))
                            .clicked()
                    {
                        interaction.ctx_move_col_left = true;
                        ui.close();
                    }
                    if col_idx + 1 < col_count
                        && ui
                            .button(crate::i18n::t("edit_menu.move_col_right"))
                            .clicked()
                    {
                        interaction.ctx_move_col_right = true;
                        ui.close();
                    }

                    ui.separator();
                    ui.label(
                        RichText::new(crate::i18n::t("context_menu.sec_sort"))
                            .strong()
                            .size(11.0),
                    );
                    if ui.button(crate::i18n::t("header.sort_az")).clicked() {
                        interaction.sort_rows_asc_by = Some(col_idx);
                        ui.close();
                    }
                    if ui.button(crate::i18n::t("header.sort_za")).clicked() {
                        interaction.sort_rows_desc_by = Some(col_idx);
                        ui.close();
                    }

                    ui.separator();
                    if ui
                        .button(crate::i18n::t("bookmarks.add"))
                        .on_hover_text(crate::i18n::t("bookmarks.add_hint"))
                        .clicked()
                    {
                        interaction.ctx_add_bookmark = true;
                        ui.close();
                    }

                    ui.separator();
                    // Parse-in-new-tab submenu - mirrors the Edit menu
                    // entry so the user can launch the modal from
                    // wherever the cell they care about lives.
                    ui.menu_button(crate::i18n::t("edit_menu.parse_in_new_tab"), |ui| {
                        if ui.button(crate::i18n::t("edit_menu.scope_cell")).clicked() {
                            interaction.ctx_parse_in_new_tab = Some(toolbar::ParseScope::Cell {
                                row: actual_row,
                                col: col_idx,
                            });
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("edit_menu.scope_row")).clicked() {
                            interaction.ctx_parse_in_new_tab =
                                Some(toolbar::ParseScope::Row { row: actual_row });
                            ui.close();
                        }
                        if ui
                            .button(crate::i18n::t("edit_menu.scope_column"))
                            .clicked()
                        {
                            interaction.ctx_parse_in_new_tab =
                                Some(toolbar::ParseScope::Column { col: col_idx });
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("edit_menu.scope_table")).clicked() {
                            interaction.ctx_parse_in_new_tab = Some(toolbar::ParseScope::Table);
                            ui.close();
                        }
                    });
                });
            }
        }

        if is_frozen {
            x_frozen += w;
        } else {
            x_scrolled += w;
        }
    }

    // Row number (pinned) - clickable for row selection
    if !show_row_numbers {
        return;
    }
    let rn_bg = if is_multi_selected_row {
        colors.bg_selected
    } else {
        colors.row_number_bg
    };
    painter.rect_filled(rn_rect, 0.0, rn_bg);
    let rn_font = egui::FontId::new((font_size * 0.85).round(), egui::FontFamily::Monospace);
    let seq_w = state.seq_number_width;
    if seq_w > 0.0 {
        // Two sub-columns: original row number (left) | sequential 1..N (right).
        let orig_w = (rn_rect.width() - seq_w).max(0.0);
        let orig_rect = egui::Rect::from_min_size(rn_rect.min, Vec2::new(orig_w, rn_rect.height()));
        let seq_rect = egui::Rect::from_min_size(
            egui::pos2(rn_rect.left() + orig_w, rn_rect.top()),
            Vec2::new(seq_w, rn_rect.height()),
        );
        painter.text(
            orig_rect.center(),
            Align2::CENTER_CENTER,
            format_number(actual_row + 1 + table.row_offset),
            rn_font.clone(),
            colors.row_number_text,
        );
        // Thin separator between the two numbers.
        painter.line_segment(
            [
                egui::pos2(seq_rect.left(), rn_rect.top() + 2.0),
                egui::pos2(seq_rect.left(), rn_rect.bottom() - 2.0),
            ],
            egui::Stroke::new(1.0, colors.border),
        );
        painter.text(
            seq_rect.center(),
            Align2::CENTER_CENTER,
            format_number(display_idx + 1),
            rn_font,
            colors.text_muted,
        );
    } else {
        painter.text(
            rn_rect.center(),
            Align2::CENTER_CENTER,
            format_number(actual_row + 1 + table.row_offset),
            rn_font,
            colors.row_number_text,
        );
    }

    // Row number click interaction
    if rn_rect.intersects(panel_rect) {
        let rn_interact_rect = rn_rect.intersect(panel_rect);
        let rn_response = ui.interact(
            rn_interact_rect,
            ui.id().with(("row_num", actual_row)),
            Sense::click(),
        );

        if rn_response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        if rn_response.clicked() {
            let modifiers = ui.input(|i| i.modifiers);
            if modifiers.command {
                // Toggle this row in the multi-selection
                if state.selected_rows.contains(&actual_row) {
                    state.selected_rows.remove(&actual_row);
                } else {
                    state.selected_rows.insert(actual_row);
                }
            } else if modifiers.shift && !state.selected_rows.is_empty() {
                // Range select
                let min_row = *state.selected_rows.iter().min().unwrap();
                let max_row = *state.selected_rows.iter().max().unwrap();
                let range_start = min_row.min(actual_row);
                let range_end = max_row.max(actual_row);
                state.selected_rows.clear();
                for r in range_start..=range_end {
                    state.selected_rows.insert(r);
                }
            } else {
                // Exclusive row selection
                state.selected_rows.clear();
                state.selected_rows.insert(actual_row);
                state.selected_cols.clear();
                state.selected_cells.clear();
            }
            state.selected_cell =
                Some((actual_row, state.selected_cell.map(|(_, c)| c).unwrap_or(0)));
            state.editing_cell = None;
        }

        // Right-click context menu on row number
        rn_response.context_menu(|ui| {
            state.selected_cell =
                Some((actual_row, state.selected_cell.map(|(_, c)| c).unwrap_or(0)));
            if !state.selected_rows.contains(&actual_row) {
                state.selected_rows.clear();
                state.selected_rows.insert(actual_row);
            }

            ui.label(
                RichText::new(crate::i18n::t("context_menu.sec_clipboard"))
                    .strong()
                    .size(11.0),
            );
            if ui.button(crate::i18n::t("header.copy")).clicked() {
                interaction.ctx_copy = true;
                ui.close();
            }
            if ui
                .button(crate::i18n::t("context_menu.copy_markdown"))
                .clicked()
            {
                interaction.ctx_copy_markdown = true;
                ui.close();
            }
            if ui.button(crate::i18n::t("header.cut")).clicked() {
                interaction.ctx_cut = true;
                ui.close();
            }
            if (state.clipboard.is_some() || state.os_clipboard_has_text)
                && ui.button(crate::i18n::t("header.paste")).clicked()
            {
                interaction.ctx_paste = true;
                ui.close();
            }
            ui.separator();

            // Right-click on a row number: if the row is part of a multi-row
            // selection, colour every selected row. Otherwise just this one.
            // The selected_rows preservation logic above already keeps
            // multi-row selection intact when the click lands inside it.
            let row_anchor = MarkKey::Row(actual_row);
            let row_keys: Vec<MarkKey> =
                if state.selected_rows.contains(&actual_row) && state.selected_rows.len() > 1 {
                    let mut rs: Vec<usize> = state.selected_rows.iter().copied().collect();
                    rs.sort_unstable();
                    rs.into_iter().map(MarkKey::Row).collect()
                } else {
                    vec![row_anchor.clone()]
                };
            mark_submenu(ui, row_keys, &row_anchor, table, interaction);
            ui.separator();

            ui.label(
                RichText::new(crate::i18n::t("context_menu.sec_row"))
                    .strong()
                    .size(11.0),
            );
            if ui.button(crate::i18n::t("edit_menu.insert_row")).clicked() {
                interaction.ctx_insert_row = true;
                ui.close();
            }
            if ui.button(crate::i18n::t("edit_menu.delete_row")).clicked() {
                interaction.ctx_delete_row = true;
                ui.close();
            }
            if actual_row > 0 && ui.button(crate::i18n::t("edit_menu.move_row_up")).clicked() {
                interaction.ctx_move_row_up = true;
                ui.close();
            }
            if actual_row + 1 < row_count
                && ui
                    .button(crate::i18n::t("edit_menu.move_row_down"))
                    .clicked()
            {
                interaction.ctx_move_row_down = true;
                ui.close();
            }
        });
    }
}
