//! Header rendering: column boxes, sort arrows, drag-reorder hooks, resize
//! handles, and the right-click context menu. Split out of [`super`] (the
//! 2,300-line table_view.rs) for navigability; no behaviour change.

use std::collections::HashSet;

use egui::{Align2, Color32, CursorIcon, RichText, Sense, Ui, Vec2};

use crate::data::{BinaryDisplayMode, DataTable, MarkKey};
use crate::ui::theme::ThemeColors;

use super::{
    COL_INDEX_HEIGHT, DEFAULT_COL_WIDTH, HEADER_HEIGHT, MIN_COL_WIDTH, NumFmtCtx,
    RESIZE_HANDLE_WIDTH, SORT_ARROW_SIZE, TableInteraction, TableViewState, col_index_letter,
    compute_optimal_col_width, mark_submenu,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_header_direct(
    ui: &mut Ui,
    painter: &egui::Painter,
    table: &DataTable,
    state: &mut TableViewState,
    colors: &ThemeColors,
    left_x: f32,
    top_y: f32,
    panel_rect: egui::Rect,
    interaction: &mut TableInteraction,
    font_size: f32,
    filtered_rows: &[usize],
    binary_display_mode: BinaryDisplayMode,
    filtered_columns: &HashSet<usize>,
    hidden_columns: &HashSet<usize>,
    num_fmt: NumFmtCtx<'_>,
) {
    let rn_rect = egui::Rect::from_min_size(
        egui::pos2(left_x, top_y),
        Vec2::new(state.row_number_width, HEADER_HEIGHT),
    );
    let header_clip = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left(), top_y),
        egui::pos2(panel_rect.right(), top_y + HEADER_HEIGHT),
    );

    let mut x = left_x + state.row_number_width - state.scroll_x;
    let col_clip = egui::Rect::from_min_max(
        egui::pos2(left_x + state.row_number_width, top_y),
        egui::pos2(panel_rect.right(), top_y + HEADER_HEIGHT),
    );
    let col_painter = painter.with_clip_rect(col_clip);

    let mut col_starts: Vec<f32> = Vec::with_capacity(table.col_count());

    for (col_idx, col) in table.columns.iter().enumerate() {
        // Hidden columns collapse to zero width and skip every paint /
        // interaction inside this loop. col_idx arithmetic is otherwise
        // unchanged so col_widths, marks, edits, sort arrows, selected_cols
        // - everything keyed by col_idx - stays correct.
        let hidden = hidden_columns.contains(&col_idx);
        let w = if hidden {
            0.0
        } else {
            state
                .col_widths
                .get(col_idx)
                .copied()
                .unwrap_or(DEFAULT_COL_WIDTH)
        };

        col_starts.push(x);

        if hidden {
            // No header paint, no resize handle, no x advance.
            continue;
        }

        let rect = egui::Rect::from_min_size(egui::pos2(x, top_y), Vec2::new(w, HEADER_HEIGHT));

        let is_dragging = state.dragging_col == Some(col_idx);
        let is_drop_target = state.drag_drop_target == Some(col_idx);
        let is_col_selected = state.selected_cols.contains(&col_idx);

        let header_bg = if is_dragging {
            Color32::from_rgba_unmultiplied(
                colors.bg_header.r(),
                colors.bg_header.g(),
                colors.bg_header.b(),
                120,
            )
        } else if is_drop_target || is_col_selected {
            colors.bg_selected
        } else {
            colors.bg_header
        };

        col_painter.rect_filled(rect, 0.0, header_bg);

        // --- Column index letter + number at top ---
        let index_text = format!("{} ({})", col_index_letter(col_idx), col_idx + 1);
        let index_galley = painter.layout_no_wrap(
            index_text,
            egui::FontId::new((font_size * 0.7).round(), egui::FontFamily::Monospace),
            colors.text_muted,
        );
        let index_clip = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, rect.top()),
            egui::pos2(rect.right() - 4.0, rect.top() + COL_INDEX_HEIGHT),
        )
        .intersect(col_clip);
        painter.with_clip_rect(index_clip).galley(
            egui::pos2(rect.left() + 6.0, rect.top() + 1.0),
            index_galley,
            Color32::TRANSPARENT,
        );

        // --- Sort arrows (right side, vertically centered below index) ---
        let arrows_total_w = SORT_ARROW_SIZE * 2.0 + 2.0;
        let arrows_x = rect.right() - RESIZE_HANDLE_WIDTH - arrows_total_w - 2.0;
        let content_top = rect.top() + COL_INDEX_HEIGHT;
        let content_center_y = (content_top + rect.bottom()) / 2.0;

        // --- Column name (below index, before sort arrows) ---
        let name_clip_right = arrows_x - 2.0;
        let cell_clip = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, content_top),
            egui::pos2(name_clip_right.max(rect.left() + 4.0), rect.bottom()),
        )
        .intersect(col_clip);

        // --- Column name: inline edit or label ---
        let is_editing_name = state
            .editing_col_name
            .as_ref()
            .map(|(idx, _)| *idx == col_idx)
            .unwrap_or(false);

        if is_editing_name {
            let name_edit_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 4.0, content_top),
                egui::pos2(name_clip_right.max(rect.left() + 40.0), content_top + 18.0),
            )
            .intersect(col_clip);
            if name_edit_rect.width() > 10.0 {
                let edit_id = ui.id().with(("col_name_edit", col_idx));
                if let Some((_, ref mut buf)) = state.editing_col_name {
                    let edit = egui::TextEdit::singleline(buf)
                        .id(edit_id)
                        .font(egui::FontId::new(font_size, egui::FontFamily::Proportional))
                        .frame(egui::Frame::NONE)
                        .desired_width(name_edit_rect.width());
                    let edit_response = ui.put(name_edit_rect, edit);
                    if state.edit_col_needs_focus {
                        edit_response.request_focus();
                        state.edit_col_needs_focus = false;
                    }
                    if edit_response.lost_focus() {
                        interaction.rename_column = Some((col_idx, buf.clone()));
                        state.editing_col_name = None;
                    }
                }
            }
        } else {
            let name_galley = painter.layout_no_wrap(
                col.name.clone(),
                egui::FontId::new(font_size, egui::FontFamily::Proportional),
                colors.text_header,
            );
            let name_size = name_galley.size();
            painter.with_clip_rect(cell_clip).galley(
                egui::pos2(rect.left() + 6.0, content_top + 1.0),
                name_galley,
                Color32::TRANSPARENT,
            );
            // Active column-filter marker. A small accent-filled disc beside
            // the column name (not a triangle - the sort indicator already
            // owns the ▼/▲ glyphs to the right). Painted only when this
            // column has an active filter so unfiltered headers look
            // unchanged.
            if filtered_columns.contains(&col_idx) {
                let dot_center = egui::pos2(
                    rect.left() + 6.0 + name_size.x + 8.0,
                    content_top + 1.0 + name_size.y / 2.0,
                );
                painter
                    .with_clip_rect(cell_clip)
                    .circle_filled(dot_center, 3.5, colors.accent);
            }
        }

        // Data type subtitle
        let type_galley = painter.layout_no_wrap(
            col.data_type.clone(),
            egui::FontId::new((font_size * 0.7).round(), egui::FontFamily::Monospace),
            colors.text_muted,
        );
        painter.with_clip_rect(cell_clip).galley(
            egui::pos2(
                rect.left() + 6.0,
                rect.bottom() - type_galley.size().y - 2.0,
            ),
            type_galley,
            Color32::TRANSPARENT,
        );

        // --- Sort arrows: up triangle (asc) and down triangle (desc) ---
        let asc_rect = egui::Rect::from_center_size(
            egui::pos2(arrows_x + SORT_ARROW_SIZE / 2.0, content_center_y),
            Vec2::new(SORT_ARROW_SIZE, SORT_ARROW_SIZE),
        );
        let desc_rect = egui::Rect::from_center_size(
            egui::pos2(
                arrows_x + SORT_ARROW_SIZE + 2.0 + SORT_ARROW_SIZE / 2.0,
                content_center_y,
            ),
            Vec2::new(SORT_ARROW_SIZE, SORT_ARROW_SIZE),
        );

        // Up arrow (sort ascending)
        if asc_rect.intersects(col_clip) {
            let asc_response = ui.interact(
                asc_rect.intersect(col_clip),
                ui.id().with(("sort_asc", col_idx)),
                Sense::click(),
            );
            let arrow_color = if asc_response.hovered() {
                colors.accent
            } else {
                colors.text_muted
            };
            let cx = asc_rect.center().x;
            let cy = asc_rect.center().y;
            let s = 4.0;
            col_painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(cx, cy - s),
                    egui::pos2(cx + s, cy + s),
                    egui::pos2(cx - s, cy + s),
                ],
                arrow_color,
                egui::Stroke::NONE,
            ));
            if asc_response.clicked() {
                interaction.sort_rows_asc_by = Some(col_idx);
            }
        }

        // Down arrow (sort descending)
        if desc_rect.intersects(col_clip) {
            let desc_response = ui.interact(
                desc_rect.intersect(col_clip),
                ui.id().with(("sort_desc", col_idx)),
                Sense::click(),
            );
            let arrow_color = if desc_response.hovered() {
                colors.accent
            } else {
                colors.text_muted
            };
            let cx = desc_rect.center().x;
            let cy = desc_rect.center().y;
            let s = 4.0;
            col_painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(cx, cy + s),
                    egui::pos2(cx + s, cy - s),
                    egui::pos2(cx - s, cy - s),
                ],
                arrow_color,
                egui::Stroke::NONE,
            ));
            if desc_response.clicked() {
                interaction.sort_rows_desc_by = Some(col_idx);
            }
        }

        // Right border
        col_painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            egui::Stroke::new(0.5, colors.border_subtle),
        );

        // Resize handle
        let resize_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - RESIZE_HANDLE_WIDTH / 2.0, rect.top()),
            Vec2::new(RESIZE_HANDLE_WIDTH, HEADER_HEIGHT),
        );

        // Header click + drag (area excluding resize handle and sort arrows)
        let header_interact_rect = egui::Rect::from_min_size(
            egui::pos2(x, top_y),
            Vec2::new((arrows_x - x).max(0.0), HEADER_HEIGHT),
        );

        if header_interact_rect.width() > 0.0 && header_interact_rect.intersects(panel_rect) {
            let visible_interact = header_interact_rect.intersect(panel_rect);
            let header_response = ui.interact(
                visible_interact,
                ui.id().with(("col_header", col_idx)),
                Sense::click_and_drag(),
            );

            if header_response.hovered() && state.dragging_col.is_none() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }

            if header_response.double_clicked() && state.dragging_col.is_none() {
                state.editing_col_name = Some((col_idx, col.name.clone()));
                state.edit_col_needs_focus = true;
            }

            if header_response.clicked() && state.dragging_col.is_none() {
                interaction.header_col_clicked = Some(col_idx);
                state.selected_cell = state.selected_cell.map(|(r, _)| (r, col_idx));

                // Multi-select columns with Ctrl, toggle; without Ctrl, exclusive select
                let modifiers = ui.input(|i| i.modifiers);
                if modifiers.command {
                    // Toggle this column in the selection
                    if state.selected_cols.contains(&col_idx) {
                        state.selected_cols.remove(&col_idx);
                    } else {
                        state.selected_cols.insert(col_idx);
                    }
                } else if modifiers.shift && !state.selected_cols.is_empty() {
                    // Range select: from min/max of current selection to this column
                    let min_col = *state.selected_cols.iter().min().unwrap();
                    let max_col = *state.selected_cols.iter().max().unwrap();
                    let range_start = min_col.min(col_idx);
                    let range_end = max_col.max(col_idx);
                    state.selected_cols.clear();
                    for c in range_start..=range_end {
                        state.selected_cols.insert(c);
                    }
                } else {
                    // Exclusive selection
                    state.selected_cols.clear();
                    state.selected_cols.insert(col_idx);
                    state.selected_rows.clear();
                    state.selected_cells.clear();
                }
            }

            // Right-click on header: context menu
            header_response.context_menu(|ui| {
                ui.label(
                    RichText::new(format!("Column: {}", col.name))
                        .strong()
                        .size(11.0),
                );
                ui.separator();
                if ui.button(crate::i18n::t("header.rename")).clicked() {
                    state.editing_col_name = Some((col_idx, col.name.clone()));
                    state.edit_col_needs_focus = true;
                    ui.close();
                }
                ui.separator();
                ui.label(RichText::new("Clipboard").strong().size(11.0));
                if ui.button(crate::i18n::t("header.copy")).clicked() {
                    if !state.selected_cols.contains(&col_idx) {
                        state.selected_cols.clear();
                        state.selected_cols.insert(col_idx);
                    }
                    interaction.ctx_copy = true;
                    ui.close();
                }
                if ui.button(crate::i18n::t("header.cut")).clicked() {
                    if !state.selected_cols.contains(&col_idx) {
                        state.selected_cols.clear();
                        state.selected_cols.insert(col_idx);
                    }
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
                // Multi-column when the right-clicked column is part of the
                // existing column selection - mirrors the Copy / Cut handling
                // above so the user can mark every selected column in one go.
                let col_anchor = MarkKey::Column(col_idx);
                let col_keys: Vec<MarkKey> =
                    if state.selected_cols.contains(&col_idx) && state.selected_cols.len() > 1 {
                        let mut cs: Vec<usize> = state.selected_cols.iter().copied().collect();
                        cs.sort_unstable();
                        cs.into_iter().map(MarkKey::Column).collect()
                    } else {
                        vec![col_anchor.clone()]
                    };
                mark_submenu(ui, col_keys, &col_anchor, table, interaction);
                ui.separator();
                if ui.button(crate::i18n::t("header.sort_az")).clicked() {
                    interaction.sort_rows_asc_by = Some(col_idx);
                    ui.close();
                }
                if ui.button(crate::i18n::t("header.sort_za")).clicked() {
                    interaction.sort_rows_desc_by = Some(col_idx);
                    ui.close();
                }
                if ui.button(crate::i18n::t("header.filter_values")).clicked() {
                    interaction.ctx_filter_column = Some(col_idx);
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("analyse_menu.value_frequency"))
                    .clicked()
                {
                    interaction.ctx_value_frequency = Some(col_idx);
                    ui.close();
                }
                if crate::data::is_numeric_data_type(&col.data_type)
                    && ui
                        .button(crate::i18n::t("edit_menu.number_format"))
                        .clicked()
                {
                    interaction.ctx_column_format = Some(col_idx);
                    ui.close();
                }
                if ui.button(crate::i18n::t("header.hide_column")).clicked() {
                    interaction.ctx_hide_column = Some(col_idx);
                    ui.close();
                }
                if ui
                    .button(crate::i18n::t("header.copy_column_names"))
                    .clicked()
                {
                    // Multi-column when the right-clicked column is part of
                    // an existing column selection; otherwise just this one.
                    let names: Vec<String> = if state.selected_cols.contains(&col_idx)
                        && state.selected_cols.len() > 1
                    {
                        let mut ordered: Vec<usize> = state.selected_cols.iter().copied().collect();
                        ordered.sort_unstable();
                        ordered
                            .into_iter()
                            .filter_map(|i| table.columns.get(i).map(|c| c.name.clone()))
                            .collect()
                    } else {
                        vec![col.name.clone()]
                    };
                    ui.ctx().copy_text(names.join("\n"));
                    ui.close();
                }
                ui.separator();
                if ui.button(crate::i18n::t("toolbar.insert_column")).clicked() {
                    interaction.header_col_clicked = Some(col_idx);
                    interaction.ctx_insert_column = true;
                    ui.close();
                }
                if ui.button(crate::i18n::t("header.delete_columns")).clicked() {
                    interaction.ctx_delete_column = true;
                    ui.close();
                }
                ui.menu_button("Change Type", |ui| {
                    let types = &[
                        "String",
                        "Int64",
                        "Float64",
                        "Boolean",
                        "Date32",
                        "Timestamp(Microsecond, None)",
                    ];
                    for &t in types {
                        let is_current = col.data_type == t;
                        let can_convert = is_current || table.can_convert_column(col_idx, t);
                        let label = if is_current {
                            format!("{} (current)", t)
                        } else {
                            t.to_string()
                        };
                        let btn =
                            ui.add_enabled(!is_current && can_convert, egui::Button::new(label));
                        let btn = if !can_convert && !is_current {
                            btn.on_disabled_hover_text(
                                "Not all values can be converted to this type",
                            )
                        } else {
                            btn
                        };
                        if btn.clicked() {
                            interaction.change_col_type = Some((col_idx, t.to_string()));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if col_idx > 0 && ui.button(crate::i18n::t("header.move_left")).clicked() {
                    state.selected_cell = state
                        .selected_cell
                        .map(|(r, _)| (r, col_idx))
                        .or(Some((0, col_idx)));
                    interaction.ctx_move_col_left = true;
                    ui.close();
                }
                if col_idx + 1 < table.col_count()
                    && ui.button(crate::i18n::t("header.move_right")).clicked()
                {
                    state.selected_cell = state
                        .selected_cell
                        .map(|(r, _)| (r, col_idx))
                        .or(Some((0, col_idx)));
                    interaction.ctx_move_col_right = true;
                    ui.close();
                }
            });

            // Drag start
            if header_response.drag_started() {
                state.dragging_col = Some(col_idx);
                state.drag_drop_target = Some(col_idx);
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            }

            if header_response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                if let Some(pointer_pos) = header_response.interact_pointer_pos() {
                    let pointer_x =
                        pointer_pos.x + state.scroll_x - (left_x + state.row_number_width);
                    let mut acc = 0.0f32;
                    let mut target = table.col_count().saturating_sub(1);
                    for (i, &cw) in state.col_widths.iter().enumerate() {
                        if pointer_x < acc + cw / 2.0 {
                            target = i;
                            break;
                        }
                        acc += cw;
                        target = i;
                    }
                    state.drag_drop_target = Some(target);
                }
            }

            if header_response.drag_stopped() {
                if let (Some(from), Some(to)) = (state.dragging_col, state.drag_drop_target)
                    && from != to
                {
                    interaction.col_drag_move = Some((from, to));
                }
                state.dragging_col = None;
                state.drag_drop_target = None;
            }
        }

        // Resize handle interaction
        if resize_rect.intersects(panel_rect) {
            let resize_response = ui.interact(
                resize_rect.intersect(panel_rect),
                ui.id().with(("col_resize", col_idx)),
                Sense::click_and_drag(),
            );

            if resize_response.hovered()
                || resize_response.dragged()
                || resize_response.is_pointer_button_down_on()
            {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }

            if resize_response.drag_started() {
                state.resizing_col = Some(col_idx);
            }

            if let Some(resizing) = state.resizing_col
                && resizing == col_idx
                && resize_response.dragged()
            {
                let delta = resize_response.drag_delta().x;
                if let Some(width) = state.col_widths.get_mut(col_idx) {
                    *width = (*width + delta).max(MIN_COL_WIDTH);
                }
            }

            if resize_response.drag_stopped() {
                state.resizing_col = None;
                state.invalidate_row_heights();
            }

            // Double-click on the seam -> fit-to-content (best fit) for the
            // column to the LEFT of the seam.
            if resize_response.double_clicked() {
                let optimal = compute_optimal_col_width(
                    ui,
                    table,
                    filtered_rows,
                    col_idx,
                    font_size,
                    binary_display_mode,
                    num_fmt,
                );
                if let Some(w) = state.col_widths.get_mut(col_idx) {
                    *w = optimal;
                }
                state.invalidate_row_heights();
            }
        }

        x += w;
    }

    // Drop indicator line
    if let (Some(from), Some(to)) = (state.dragging_col, state.drag_drop_target)
        && from != to
    {
        let target_x = col_starts.get(to).copied().unwrap_or(x);
        let indicator_x = if to > from {
            target_x
                + state
                    .col_widths
                    .get(to)
                    .copied()
                    .unwrap_or(DEFAULT_COL_WIDTH)
        } else {
            target_x
        };
        let indicator_x = indicator_x.clamp(left_x + state.row_number_width, panel_rect.right());
        col_painter.line_segment(
            [
                egui::pos2(indicator_x, top_y),
                egui::pos2(indicator_x, top_y + HEADER_HEIGHT),
            ],
            egui::Stroke::new(3.0, colors.accent),
        );
    }

    // Row number header (pinned)
    painter
        .with_clip_rect(header_clip)
        .rect_filled(rn_rect, 0.0, colors.bg_header);
    painter.with_clip_rect(header_clip).text(
        rn_rect.center(),
        Align2::CENTER_CENTER,
        "#",
        egui::FontId::new(font_size, egui::FontFamily::Monospace),
        colors.text_muted,
    );
}
