use std::collections::HashSet;

use egui::{Align2, Color32, CursorIcon, RichText, Sense, Ui, Vec2};

use super::theme::{ThemeColors, ThemeMode};
use crate::data::DataTable;

/// State for the table view (selection, editing).
#[derive(Default)]
pub struct TableViewState {
    /// Currently selected cell (row, col). None means no selection.
    pub selected_cell: Option<(usize, usize)>,
    /// Cell currently being edited, with its buffer.
    pub editing_cell: Option<(usize, usize, String)>,
    /// Whether the edit widget needs initial focus (set true when editing starts).
    edit_needs_focus: bool,
    /// Column widths (auto-sized initially, user can resize later).
    pub col_widths: Vec<f32>,
    /// Whether col_widths have been initialized.
    pub widths_initialized: bool,
    /// Column currently being resized (index), if any.
    resizing_col: Option<usize>,
    /// Vertical scroll offset in pixels (persisted across frames).
    scroll_y: f32,
    /// Horizontal scroll offset in pixels.
    scroll_x: f32,
    /// Column drag-and-drop state
    pub dragging_col: Option<usize>,
    pub drag_drop_target: Option<usize>,
    /// Multi-selection: selected rows (by actual row index).
    pub selected_rows: HashSet<usize>,
    /// Multi-selection: selected columns (by column index).
    pub selected_cols: HashSet<usize>,
    /// Clipboard content (tab-separated values, rows separated by newlines).
    pub clipboard: Option<String>,
    /// Whether the OS clipboard currently has text content (set each frame by the app).
    pub os_clipboard_has_text: bool,
}

const ROW_HEIGHT: f32 = 26.0;
const MIN_COL_WIDTH: f32 = 60.0;
const MAX_COL_WIDTH: f32 = 800.0;
const DEFAULT_COL_WIDTH: f32 = 120.0;
const ROW_NUMBER_WIDTH: f32 = 60.0;
const HEADER_HEIGHT: f32 = 44.0; // taller to fit column index number
const RESIZE_HANDLE_WIDTH: f32 = 6.0;
const SORT_ARROW_SIZE: f32 = 14.0;
const COL_INDEX_HEIGHT: f32 = 12.0; // space for the column index number at top

impl TableViewState {
    /// Ensure column widths are initialized for the given table.
    pub fn ensure_widths(&mut self, table: &DataTable) {
        if !self.widths_initialized || self.col_widths.len() != table.col_count() {
            self.col_widths = vec![DEFAULT_COL_WIDTH; table.col_count()];
            for (i, col) in table.columns.iter().enumerate() {
                let name_width = col.name.len() as f32 * 8.0 + 32.0 + SORT_ARROW_SIZE * 2.0 + 8.0;
                let type_width = col.data_type.len() as f32 * 6.5 + 32.0;
                let mut max_width = name_width.max(type_width);

                let sample_count = table.row_count().min(50);
                for row in 0..sample_count {
                    if let Some(val) = table.get(row, i) {
                        let text = val.to_string();
                        let text_width = text.len() as f32 * 7.5 + 20.0;
                        max_width = max_width.max(text_width);
                    }
                }

                self.col_widths[i] = max_width.clamp(MIN_COL_WIDTH, MAX_COL_WIDTH);
            }
            self.widths_initialized = true;
        }
    }
}

/// Signals from the table back to the app.
pub struct TableInteraction {
    /// Column header was clicked (for setting insert position).
    pub header_col_clicked: Option<usize>,
    /// A drag-and-drop move completed: (from_col, to_col).
    pub col_drag_move: Option<(usize, usize)>,
    /// Sort rows ascending by this column index.
    pub sort_rows_asc_by: Option<usize>,
    /// Sort rows descending by this column index.
    pub sort_rows_desc_by: Option<usize>,
    /// Right-click context menu actions
    pub ctx_insert_row: bool,
    pub ctx_delete_row: bool,
    pub ctx_insert_column: bool,
    pub ctx_delete_column: bool,
    pub ctx_move_row_up: bool,
    pub ctx_move_row_down: bool,
    pub ctx_move_col_left: bool,
    pub ctx_move_col_right: bool,
    /// Copy/Paste signals
    pub ctx_copy: bool,
    pub ctx_paste: bool,
    /// Text received from OS clipboard via Ctrl+V / Paste event
    pub paste_text: Option<String>,
}

impl Default for TableInteraction {
    fn default() -> Self {
        Self {
            header_col_clicked: None,
            col_drag_move: None,
            sort_rows_asc_by: None,
            sort_rows_desc_by: None,
            ctx_insert_row: false,
            ctx_delete_row: false,
            ctx_insert_column: false,
            ctx_delete_column: false,
            ctx_move_row_up: false,
            ctx_move_row_down: false,
            ctx_move_col_left: false,
            ctx_move_col_right: false,
            ctx_copy: false,
            ctx_paste: false,
            paste_text: None,
        }
    }
}

/// Draw the data table with true row virtualization.
pub fn draw_table(
    ui: &mut Ui,
    table: &mut DataTable,
    state: &mut TableViewState,
    theme_mode: ThemeMode,
    filtered_rows: &[usize],
    os_clipboard_has_content: bool,
) -> TableInteraction {
    let colors = ThemeColors::for_mode(theme_mode);
    state.ensure_widths(table);
    state.os_clipboard_has_text = os_clipboard_has_content;

    let mut interaction = TableInteraction::default();

    if table.col_count() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Open a file to get started")
                    .size(18.0)
                    .color(colors.text_muted),
            );
        });
        return interaction;
    }

    let total_col_width: f32 = ROW_NUMBER_WIDTH + state.col_widths.iter().sum::<f32>();
    let row_count = filtered_rows.len();
    let total_data_height = row_count as f32 * ROW_HEIGHT;
    let total_content_height = HEADER_HEIGHT + 1.0 + total_data_height + 8.0;

    let available_rect = ui.available_rect_before_wrap();
    let view_width = available_rect.width();
    let view_height = available_rect.height();

    // Handle scroll input and keyboard shortcuts
    ui.input(|input| {
        let scroll_delta = input.smooth_scroll_delta;
        state.scroll_y = (state.scroll_y - scroll_delta.y)
            .clamp(0.0, (total_content_height - view_height).max(0.0));
        state.scroll_x =
            (state.scroll_x - scroll_delta.x).clamp(0.0, (total_col_width - view_width).max(0.0));
    });

    // Ctrl+C / Ctrl+V
    let ctrl_held = ui.input(|i| i.modifiers.command);
    if ctrl_held && ui.input(|i| i.key_pressed(egui::Key::C)) {
        // Don't copy if we're currently editing a cell (let the text edit handle it)
        if state.editing_cell.is_none() {
            interaction.ctx_copy = true;
        }
    }
    // Detect paste from OS clipboard via egui's Paste event
    let paste_from_event: Option<String> = ui.input(|i| {
        i.events.iter().find_map(|e| {
            if let egui::Event::Paste(text) = e {
                Some(text.clone())
            } else {
                None
            }
        })
    });
    if let Some(text) = paste_from_event {
        if state.editing_cell.is_none() {
            interaction.ctx_paste = true;
            interaction.paste_text = Some(text);
        }
    }

    let (panel_rect, _) =
        ui.allocate_exact_size(Vec2::new(view_width, view_height), Sense::hover());

    let painter = ui.painter_at(panel_rect);

    // --- Draw header ---
    let header_y = panel_rect.top();
    draw_header_direct(
        ui,
        &painter,
        table,
        state,
        &colors,
        panel_rect.left(),
        header_y,
        panel_rect,
        &mut interaction,
    );

    // Header bottom border
    let header_bottom = header_y + HEADER_HEIGHT;
    painter.line_segment(
        [
            egui::pos2(panel_rect.left(), header_bottom),
            egui::pos2(panel_rect.right(), header_bottom),
        ],
        egui::Stroke::new(1.0, colors.border),
    );

    // --- Visible row range ---
    let data_area_top = header_bottom + 1.0;
    let data_area_height = panel_rect.bottom() - data_area_top;

    let data_clip_rect =
        egui::Rect::from_min_max(egui::pos2(panel_rect.left(), data_area_top), panel_rect.max);
    let data_painter = painter.with_clip_rect(data_clip_rect);

    let first_visible = (state.scroll_y / ROW_HEIGHT).floor() as usize;
    let visible_count = (data_area_height / ROW_HEIGHT).ceil() as usize + 2;
    let last_visible = (first_visible + visible_count).min(row_count);

    for display_idx in first_visible..last_visible {
        let actual_row = filtered_rows[display_idx];
        let row_y = data_area_top + (display_idx as f32 * ROW_HEIGHT) - state.scroll_y;

        if row_y + ROW_HEIGHT < data_area_top || row_y > panel_rect.bottom() {
            continue;
        }

        draw_data_row_direct(
            ui,
            &data_painter,
            table,
            state,
            &colors,
            actual_row,
            display_idx,
            panel_rect.left(),
            row_y,
            panel_rect,
            &mut interaction,
        );
    }

    // --- Vertical scrollbar ---
    if total_content_height > view_height {
        let scrollbar_width = 10.0;
        let scrollbar_x = panel_rect.right() - scrollbar_width - 1.0;
        let scrollbar_track_top = panel_rect.top();
        let scrollbar_track_height = view_height;

        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, scrollbar_track_top),
            Vec2::new(scrollbar_width, scrollbar_track_height),
        );
        painter.rect_filled(track_rect, scrollbar_width / 2.0, colors.scrollbar_track);

        let thumb_fraction = view_height / total_content_height;
        let thumb_height = (thumb_fraction * scrollbar_track_height).max(24.0);
        let max_scroll = total_content_height - view_height;
        let thumb_offset = if max_scroll > 0.0 {
            (state.scroll_y / max_scroll) * (scrollbar_track_height - thumb_height)
        } else {
            0.0
        };

        let thumb_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, scrollbar_track_top + thumb_offset),
            Vec2::new(scrollbar_width, thumb_height),
        );

        let sb_response = ui.interact(thumb_rect, ui.id().with("vscroll_thumb"), Sense::drag());
        let thumb_color = if sb_response.dragged() || sb_response.hovered() {
            colors.scrollbar_thumb_hover
        } else {
            colors.scrollbar_thumb
        };
        painter.rect_filled(thumb_rect, scrollbar_width / 2.0, thumb_color);

        if sb_response.dragged() {
            let delta_y = sb_response.drag_delta().y;
            let scroll_per_pixel = max_scroll / (scrollbar_track_height - thumb_height);
            state.scroll_y = (state.scroll_y + delta_y * scroll_per_pixel).clamp(0.0, max_scroll);
        }

        let track_response = ui.interact(track_rect, ui.id().with("vscroll_track"), Sense::click());
        if track_response.clicked() {
            if let Some(pos) = track_response.interact_pointer_pos() {
                let click_fraction = (pos.y - scrollbar_track_top) / scrollbar_track_height;
                state.scroll_y = (click_fraction * total_content_height - view_height / 2.0)
                    .clamp(0.0, max_scroll);
            }
        }
    }

    // --- Horizontal scrollbar ---
    if total_col_width > view_width {
        let scrollbar_height = 10.0;
        let scrollbar_y = panel_rect.bottom() - scrollbar_height - 1.0;
        let scrollbar_track_left = panel_rect.left();
        let scrollbar_track_width = view_width;

        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_track_left, scrollbar_y),
            Vec2::new(scrollbar_track_width, scrollbar_height),
        );
        painter.rect_filled(track_rect, scrollbar_height / 2.0, colors.scrollbar_track);

        let thumb_fraction = view_width / total_col_width;
        let thumb_width = (thumb_fraction * scrollbar_track_width).max(24.0);
        let max_scroll = total_col_width - view_width;
        let thumb_offset = if max_scroll > 0.0 {
            (state.scroll_x / max_scroll) * (scrollbar_track_width - thumb_width)
        } else {
            0.0
        };

        let thumb_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_track_left + thumb_offset, scrollbar_y),
            Vec2::new(thumb_width, scrollbar_height),
        );

        let sb_response = ui.interact(thumb_rect, ui.id().with("hscroll_thumb"), Sense::drag());
        let thumb_color = if sb_response.dragged() || sb_response.hovered() {
            colors.scrollbar_thumb_hover
        } else {
            colors.scrollbar_thumb
        };
        painter.rect_filled(thumb_rect, scrollbar_height / 2.0, thumb_color);

        if sb_response.dragged() {
            let delta_x = sb_response.drag_delta().x;
            let scroll_per_pixel = max_scroll / (scrollbar_track_width - thumb_width);
            state.scroll_x = (state.scroll_x + delta_x * scroll_per_pixel).clamp(0.0, max_scroll);
        }

        let track_response = ui.interact(track_rect, ui.id().with("hscroll_track"), Sense::click());
        if track_response.clicked() {
            if let Some(pos) = track_response.interact_pointer_pos() {
                let click_fraction = (pos.x - scrollbar_track_left) / scrollbar_track_width;
                state.scroll_x =
                    (click_fraction * total_col_width - view_width / 2.0).clamp(0.0, max_scroll);
            }
        }
    }

    interaction
}

#[allow(clippy::too_many_arguments)]
fn draw_header_direct(
    ui: &mut Ui,
    painter: &egui::Painter,
    table: &DataTable,
    state: &mut TableViewState,
    colors: &ThemeColors,
    left_x: f32,
    top_y: f32,
    panel_rect: egui::Rect,
    interaction: &mut TableInteraction,
) {
    let rn_rect = egui::Rect::from_min_size(
        egui::pos2(left_x, top_y),
        Vec2::new(ROW_NUMBER_WIDTH, HEADER_HEIGHT),
    );
    let header_clip = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left(), top_y),
        egui::pos2(panel_rect.right(), top_y + HEADER_HEIGHT),
    );

    let mut x = left_x + ROW_NUMBER_WIDTH - state.scroll_x;
    let col_clip = egui::Rect::from_min_max(
        egui::pos2(left_x + ROW_NUMBER_WIDTH, top_y),
        egui::pos2(panel_rect.right(), top_y + HEADER_HEIGHT),
    );
    let col_painter = painter.with_clip_rect(col_clip);

    let mut col_starts: Vec<f32> = Vec::with_capacity(table.col_count());

    for (col_idx, col) in table.columns.iter().enumerate() {
        let w = state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);

        col_starts.push(x);

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
        } else if is_drop_target {
            colors.bg_selected
        } else if is_col_selected {
            colors.bg_selected
        } else {
            colors.bg_header
        };

        col_painter.rect_filled(rect, 0.0, header_bg);

        // --- Column index number at top ---
        let index_text = format!("{}", col_idx + 1);
        let index_galley = painter.layout_no_wrap(
            index_text,
            egui::FontId::new(9.0, egui::FontFamily::Monospace),
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

        let name_galley = painter.layout_no_wrap(
            col.name.clone(),
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
            colors.text_header,
        );
        painter.with_clip_rect(cell_clip).galley(
            egui::pos2(rect.left() + 6.0, content_top + 1.0),
            name_galley,
            Color32::TRANSPARENT,
        );

        // Data type subtitle
        let type_galley = painter.layout_no_wrap(
            col.data_type.clone(),
            egui::FontId::new(9.0, egui::FontFamily::Monospace),
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
                ui.label(RichText::new("Clipboard").strong().size(11.0));
                if ui.button("Copy Column").clicked() {
                    // Select this column for copy
                    if !state.selected_cols.contains(&col_idx) {
                        state.selected_cols.clear();
                        state.selected_cols.insert(col_idx);
                    }
                    interaction.ctx_copy = true;
                    ui.close_menu();
                }
                if state.clipboard.is_some() || state.os_clipboard_has_text {
                    if ui.button("Paste").clicked() {
                        interaction.ctx_paste = true;
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Sort A-Z").clicked() {
                    interaction.sort_rows_asc_by = Some(col_idx);
                    ui.close_menu();
                }
                if ui.button("Sort Z-A").clicked() {
                    interaction.sort_rows_desc_by = Some(col_idx);
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Insert Column...").clicked() {
                    interaction.header_col_clicked = Some(col_idx);
                    interaction.ctx_insert_column = true;
                    ui.close_menu();
                }
                if ui.button("Delete Columns...").clicked() {
                    interaction.ctx_delete_column = true;
                    ui.close_menu();
                }
                ui.separator();
                if col_idx > 0 {
                    if ui.button("Move Left").clicked() {
                        state.selected_cell = state
                            .selected_cell
                            .map(|(r, _)| (r, col_idx))
                            .or(Some((0, col_idx)));
                        interaction.ctx_move_col_left = true;
                        ui.close_menu();
                    }
                }
                if col_idx + 1 < table.col_count() {
                    if ui.button("Move Right").clicked() {
                        state.selected_cell = state
                            .selected_cell
                            .map(|(r, _)| (r, col_idx))
                            .or(Some((0, col_idx)));
                        interaction.ctx_move_col_right = true;
                        ui.close_menu();
                    }
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
                    let pointer_x = pointer_pos.x + state.scroll_x - (left_x + ROW_NUMBER_WIDTH);
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
                if let (Some(from), Some(to)) = (state.dragging_col, state.drag_drop_target) {
                    if from != to {
                        interaction.col_drag_move = Some((from, to));
                    }
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
                Sense::drag(),
            );

            if resize_response.hovered() || resize_response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            }

            if resize_response.drag_started() {
                state.resizing_col = Some(col_idx);
            }

            if let Some(resizing) = state.resizing_col {
                if resizing == col_idx && resize_response.dragged() {
                    let delta = resize_response.drag_delta().x;
                    if let Some(width) = state.col_widths.get_mut(col_idx) {
                        *width = (*width + delta).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH);
                    }
                }
            }

            if resize_response.drag_stopped() {
                state.resizing_col = None;
            }
        }

        x += w;
    }

    // Drop indicator line
    if let (Some(from), Some(to)) = (state.dragging_col, state.drag_drop_target) {
        if from != to {
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
            let indicator_x = indicator_x.clamp(left_x + ROW_NUMBER_WIDTH, panel_rect.right());
            col_painter.line_segment(
                [
                    egui::pos2(indicator_x, top_y),
                    egui::pos2(indicator_x, top_y + HEADER_HEIGHT),
                ],
                egui::Stroke::new(3.0, colors.accent),
            );
        }
    }

    // Row number header (pinned)
    painter
        .with_clip_rect(header_clip)
        .rect_filled(rn_rect, 0.0, colors.bg_header);
    painter.with_clip_rect(header_clip).text(
        rn_rect.center(),
        Align2::CENTER_CENTER,
        "#",
        egui::FontId::new(12.0, egui::FontFamily::Monospace),
        colors.text_muted,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_data_row_direct(
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
) {
    let is_selected_row = state
        .selected_cell
        .map(|(r, _)| r == actual_row)
        .unwrap_or(false);
    let is_multi_selected_row = state.selected_rows.contains(&actual_row);

    let row_bg = if is_selected_row || is_multi_selected_row {
        colors.bg_selected
    } else if display_idx % 2 == 0 {
        colors.row_even
    } else {
        colors.row_odd
    };

    let rn_rect = egui::Rect::from_min_size(
        egui::pos2(left_x, row_y),
        Vec2::new(ROW_NUMBER_WIDTH, ROW_HEIGHT),
    );

    let data_area_left = left_x + ROW_NUMBER_WIDTH;
    let col_clip = egui::Rect::from_min_max(
        egui::pos2(data_area_left, panel_rect.top() + HEADER_HEIGHT + 1.0),
        panel_rect.max,
    );
    let col_painter = painter.with_clip_rect(col_clip);

    let mut x = data_area_left - state.scroll_x;
    let row_count = table.row_count();
    let col_count = table.col_count();

    for col_idx in 0..col_count {
        let w = state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);

        let rect = egui::Rect::from_min_size(egui::pos2(x, row_y), Vec2::new(w, ROW_HEIGHT));

        if rect.right() >= data_area_left && rect.left() <= panel_rect.right() {
            let is_editing = state
                .editing_cell
                .as_ref()
                .map(|(r, c, _)| *r == actual_row && *c == col_idx)
                .unwrap_or(false);
            let is_selected = state
                .selected_cell
                .map(|(r, c)| r == actual_row && c == col_idx)
                .unwrap_or(false);
            let is_edited = table.is_edited(actual_row, col_idx);
            let is_col_selected = state.selected_cols.contains(&col_idx);

            let cell_bg = if is_selected {
                colors.bg_selected
            } else if is_multi_selected_row || is_col_selected {
                colors.bg_selected
            } else if is_edited {
                colors.bg_edited
            } else {
                row_bg
            };
            col_painter.rect_filled(rect, 0.0, cell_bg);

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
                            .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                            .frame(false)
                            .desired_width(text_rect.width());
                        let edit_response = ui.put(text_rect.intersect(panel_rect), edit);

                        if state.edit_needs_focus {
                            edit_response.request_focus();
                            state.edit_needs_focus = false;
                        }

                        if edit_response.lost_focus() {
                            commit_text = Some(buf.clone());
                        }
                    }
                }
                if let Some(new_text) = commit_text {
                    if let Some(old_val) = table.get(actual_row, col_idx) {
                        let new_val = crate::data::CellValue::parse_like(old_val, &new_text);
                        table.set(actual_row, col_idx, new_val);
                    }
                    state.editing_cell = None;
                }
            } else {
                if let Some(value) = table.get(actual_row, col_idx) {
                    let display_text = value.to_string();
                    let text_color = match value {
                        crate::data::CellValue::Null => colors.text_muted,
                        crate::data::CellValue::Int(_) | crate::data::CellValue::Float(_) => {
                            colors.accent
                        }
                        crate::data::CellValue::Bool(_) => colors.warning,
                        crate::data::CellValue::Nested(_) => colors.text_secondary,
                        _ => colors.text_primary,
                    };

                    let text_rect = rect.shrink2(Vec2::new(6.0, 0.0));
                    let cell_clip = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 2.0, rect.top()),
                        egui::pos2(rect.right() - 2.0, rect.bottom()),
                    )
                    .intersect(col_clip);

                    let galley = painter.layout_no_wrap(
                        display_text,
                        egui::FontId::new(12.0, egui::FontFamily::Monospace),
                        text_color,
                    );
                    painter.with_clip_rect(cell_clip).galley(
                        egui::pos2(
                            text_rect.left(),
                            text_rect.center().y - galley.size().y / 2.0,
                        ),
                        galley,
                        Color32::TRANSPARENT,
                    );
                }
            }

            // Cell interactions (left click + right click)
            if rect.intersects(panel_rect) {
                let interact_rect = rect.intersect(col_clip);
                let response = ui.interact(
                    interact_rect,
                    ui.id().with(("cell", actual_row, col_idx)),
                    Sense::click(),
                );

                if response.clicked() {
                    state.selected_cell = Some((actual_row, col_idx));
                    state.editing_cell = None;
                    // Clear multi-selections on regular cell click (unless Ctrl held)
                    let modifiers = ui.input(|i| i.modifiers);
                    if !modifiers.command {
                        state.selected_rows.clear();
                        state.selected_cols.clear();
                    }
                }
                if response.double_clicked() {
                    state.selected_cell = Some((actual_row, col_idx));
                    let current_text = table
                        .get(actual_row, col_idx)
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    state.editing_cell = Some((actual_row, col_idx, current_text));
                    state.edit_needs_focus = true;
                }

                // Right-click context menu on cell
                response.context_menu(|ui| {
                    // Ensure this cell is selected for operations
                    state.selected_cell = Some((actual_row, col_idx));

                    // --- Copy / Paste ---
                    ui.label(RichText::new("Clipboard").strong().size(11.0));
                    if ui.button("Copy").clicked() {
                        interaction.ctx_copy = true;
                        ui.close_menu();
                    }
                    if state.clipboard.is_some() || state.os_clipboard_has_text {
                        if ui.button("Paste").clicked() {
                            interaction.ctx_paste = true;
                            ui.close_menu();
                        }
                    }
                    ui.separator();

                    ui.label(RichText::new("Row").strong().size(11.0));
                    if ui.button("Insert Row").clicked() {
                        interaction.ctx_insert_row = true;
                        ui.close_menu();
                    }
                    if ui.button("Delete Row").clicked() {
                        interaction.ctx_delete_row = true;
                        ui.close_menu();
                    }
                    if actual_row > 0 {
                        if ui.button("Move Row Up").clicked() {
                            interaction.ctx_move_row_up = true;
                            ui.close_menu();
                        }
                    }
                    if actual_row + 1 < row_count {
                        if ui.button("Move Row Down").clicked() {
                            interaction.ctx_move_row_down = true;
                            ui.close_menu();
                        }
                    }

                    ui.separator();
                    ui.label(RichText::new("Column").strong().size(11.0));
                    if ui.button("Insert Column...").clicked() {
                        interaction.header_col_clicked = Some(col_idx);
                        interaction.ctx_insert_column = true;
                        ui.close_menu();
                    }
                    if ui.button("Delete Columns...").clicked() {
                        interaction.ctx_delete_column = true;
                        ui.close_menu();
                    }
                    if col_idx > 0 {
                        if ui.button("Move Column Left").clicked() {
                            interaction.ctx_move_col_left = true;
                            ui.close_menu();
                        }
                    }
                    if col_idx + 1 < col_count {
                        if ui.button("Move Column Right").clicked() {
                            interaction.ctx_move_col_right = true;
                            ui.close_menu();
                        }
                    }

                    ui.separator();
                    ui.label(RichText::new("Sort").strong().size(11.0));
                    if ui.button("Sort A-Z").clicked() {
                        interaction.sort_rows_asc_by = Some(col_idx);
                        ui.close_menu();
                    }
                    if ui.button("Sort Z-A").clicked() {
                        interaction.sort_rows_desc_by = Some(col_idx);
                        ui.close_menu();
                    }
                });
            }
        }

        x += w;
    }

    // Row number (pinned) - clickable for row selection
    let rn_bg = if is_multi_selected_row {
        colors.bg_selected
    } else {
        colors.row_number_bg
    };
    painter.rect_filled(rn_rect, 0.0, rn_bg);
    painter.text(
        rn_rect.center(),
        Align2::CENTER_CENTER,
        format!("{}", actual_row + 1),
        egui::FontId::new(11.0, egui::FontFamily::Monospace),
        colors.row_number_text,
    );

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

            ui.label(RichText::new("Clipboard").strong().size(11.0));
            if ui.button("Copy Row").clicked() {
                interaction.ctx_copy = true;
                ui.close_menu();
            }
            if state.clipboard.is_some() || state.os_clipboard_has_text {
                if ui.button("Paste").clicked() {
                    interaction.ctx_paste = true;
                    ui.close_menu();
                }
            }
            ui.separator();

            ui.label(RichText::new("Row").strong().size(11.0));
            if ui.button("Insert Row").clicked() {
                interaction.ctx_insert_row = true;
                ui.close_menu();
            }
            if ui.button("Delete Row").clicked() {
                interaction.ctx_delete_row = true;
                ui.close_menu();
            }
            if actual_row > 0 {
                if ui.button("Move Row Up").clicked() {
                    interaction.ctx_move_row_up = true;
                    ui.close_menu();
                }
            }
            if actual_row + 1 < row_count {
                if ui.button("Move Row Down").clicked() {
                    interaction.ctx_move_row_down = true;
                    ui.close_menu();
                }
            }
        });
    }
}
