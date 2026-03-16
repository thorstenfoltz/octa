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
}

const ROW_HEIGHT: f32 = 26.0;
const MIN_COL_WIDTH: f32 = 60.0;
const MAX_COL_WIDTH: f32 = 800.0;
const DEFAULT_COL_WIDTH: f32 = 120.0;
const ROW_NUMBER_WIDTH: f32 = 60.0;
const HEADER_HEIGHT: f32 = 36.0;
const RESIZE_HANDLE_WIDTH: f32 = 6.0;

impl TableViewState {
    /// Ensure column widths are initialized for the given table.
    /// Auto-sizes based on column name length and samples cell content.
    pub fn ensure_widths(&mut self, table: &DataTable) {
        if !self.widths_initialized || self.col_widths.len() != table.col_count() {
            self.col_widths = vec![DEFAULT_COL_WIDTH; table.col_count()];
            for (i, col) in table.columns.iter().enumerate() {
                // Start with column name width
                let name_width = col.name.len() as f32 * 8.0 + 32.0;
                let type_width = col.data_type.len() as f32 * 6.5 + 32.0;
                let mut max_width = name_width.max(type_width);

                // Sample up to 50 rows to estimate content width
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

/// Draw the data table with true row virtualization.
/// Only the visible rows are rendered; off-screen rows are replaced by blank space.
pub fn draw_table(
    ui: &mut Ui,
    table: &mut DataTable,
    state: &mut TableViewState,
    theme_mode: ThemeMode,
    filtered_rows: &[usize],
) {
    let colors = ThemeColors::for_mode(theme_mode);
    state.ensure_widths(table);

    if table.col_count() == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Open a file to get started")
                    .size(18.0)
                    .color(colors.text_muted),
            );
        });
        return;
    }

    let total_col_width: f32 = ROW_NUMBER_WIDTH + state.col_widths.iter().sum::<f32>();
    let row_count = filtered_rows.len();
    let total_data_height = row_count as f32 * ROW_HEIGHT;
    // Total content height = header + 1px gap + data rows + 8px bottom padding
    let total_content_height = HEADER_HEIGHT + 1.0 + total_data_height + 8.0;

    let available_rect = ui.available_rect_before_wrap();
    let view_width = available_rect.width();
    let view_height = available_rect.height();

    // Handle scroll input
    ui.input(|input| {
        let scroll_delta = input.smooth_scroll_delta;
        state.scroll_y = (state.scroll_y - scroll_delta.y)
            .clamp(0.0, (total_content_height - view_height).max(0.0));
        state.scroll_x =
            (state.scroll_x - scroll_delta.x).clamp(0.0, (total_col_width - view_width).max(0.0));
    });

    // Allocate the full panel area (hover only -- cells handle their own clicks)
    let (panel_rect, _) =
        ui.allocate_exact_size(Vec2::new(view_width, view_height), Sense::hover());

    // We paint everything relative to panel_rect, offset by scroll
    let painter = ui.painter_at(panel_rect);

    // --- Draw header (always visible, pinned at top) ---
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

    // --- Determine visible row range ---
    let data_area_top = header_bottom + 1.0;
    let data_area_height = panel_rect.bottom() - data_area_top;

    // Clip data painting to area below header
    let data_clip_rect =
        egui::Rect::from_min_max(egui::pos2(panel_rect.left(), data_area_top), panel_rect.max);
    let data_painter = painter.with_clip_rect(data_clip_rect);

    // Which rows are visible?
    let first_visible = (state.scroll_y / ROW_HEIGHT).floor() as usize;
    let visible_count = (data_area_height / ROW_HEIGHT).ceil() as usize + 2; // +2 for partial rows
    let last_visible = (first_visible + visible_count).min(row_count);

    // Draw only visible rows
    for display_idx in first_visible..last_visible {
        let actual_row = filtered_rows[display_idx];
        let row_y = data_area_top + (display_idx as f32 * ROW_HEIGHT) - state.scroll_y;

        // Skip if completely off-screen
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
        );
    }

    // Draw vertical scrollbar
    if total_content_height > view_height {
        let scrollbar_width = 10.0;
        let scrollbar_x = panel_rect.right() - scrollbar_width - 1.0;
        let scrollbar_track_top = panel_rect.top();
        let scrollbar_track_height = view_height;

        // Track
        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_x, scrollbar_track_top),
            Vec2::new(scrollbar_width, scrollbar_track_height),
        );
        painter.rect_filled(track_rect, scrollbar_width / 2.0, colors.scrollbar_track);

        // Thumb
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

        // Interactive scrollbar dragging
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

        // Click on track to jump
        let track_response = ui.interact(track_rect, ui.id().with("vscroll_track"), Sense::click());
        if track_response.clicked() {
            if let Some(pos) = track_response.interact_pointer_pos() {
                let click_fraction = (pos.y - scrollbar_track_top) / scrollbar_track_height;
                state.scroll_y = (click_fraction * total_content_height - view_height / 2.0)
                    .clamp(0.0, max_scroll);
            }
        }
    }

    // Draw horizontal scrollbar
    if total_col_width > view_width {
        let scrollbar_height = 10.0;
        let scrollbar_y = panel_rect.bottom() - scrollbar_height - 1.0;
        let scrollbar_track_left = panel_rect.left();
        let scrollbar_track_width = view_width;

        // Track
        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_track_left, scrollbar_y),
            Vec2::new(scrollbar_track_width, scrollbar_height),
        );
        painter.rect_filled(track_rect, scrollbar_height / 2.0, colors.scrollbar_track);

        // Thumb
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

        // Interactive scrollbar dragging
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

        // Click on track to jump
        let track_response = ui.interact(track_rect, ui.id().with("hscroll_track"), Sense::click());
        if track_response.clicked() {
            if let Some(pos) = track_response.interact_pointer_pos() {
                let click_fraction = (pos.x - scrollbar_track_left) / scrollbar_track_width;
                state.scroll_x =
                    (click_fraction * total_col_width - view_width / 2.0).clamp(0.0, max_scroll);
            }
        }
    }
}

fn draw_header_direct(
    ui: &mut Ui,
    painter: &egui::Painter,
    table: &DataTable,
    state: &mut TableViewState,
    colors: &ThemeColors,
    left_x: f32,
    top_y: f32,
    panel_rect: egui::Rect,
) {
    // Row number header
    let rn_rect = egui::Rect::from_min_size(
        egui::pos2(left_x, top_y), // row number column doesn't scroll horizontally
        Vec2::new(ROW_NUMBER_WIDTH, HEADER_HEIGHT),
    );
    // Paint row number bg on top of everything
    let header_clip = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left(), top_y),
        egui::pos2(panel_rect.right(), top_y + HEADER_HEIGHT),
    );

    // First draw all column headers (they scroll)
    let mut x = left_x + ROW_NUMBER_WIDTH - state.scroll_x;
    let col_clip = egui::Rect::from_min_max(
        egui::pos2(left_x + ROW_NUMBER_WIDTH, top_y),
        egui::pos2(panel_rect.right(), top_y + HEADER_HEIGHT),
    );
    let col_painter = painter.with_clip_rect(col_clip);

    for (col_idx, col) in table.columns.iter().enumerate() {
        let w = state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);

        let rect = egui::Rect::from_min_size(egui::pos2(x, top_y), Vec2::new(w, HEADER_HEIGHT));

        col_painter.rect_filled(rect, 0.0, colors.bg_header);

        // Column name (clipped to cell bounds)
        let text_rect = rect.shrink2(Vec2::new(6.0, 2.0));
        let cell_clip = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, rect.top()),
            egui::pos2(rect.right() - 4.0, rect.bottom()),
        )
        .intersect(col_clip);

        let name_galley = painter.layout_no_wrap(
            col.name.clone(),
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
            colors.text_header,
        );
        painter.with_clip_rect(cell_clip).galley(
            egui::pos2(text_rect.left(), text_rect.top()),
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
            egui::pos2(text_rect.left(), rect.bottom() - type_galley.size().y - 2.0),
            type_galley,
            Color32::TRANSPARENT,
        );

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
        // Only interactive if visible
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

    // Draw row number header on top (it's pinned, doesn't scroll horizontally)
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
) {
    let is_selected_row = state
        .selected_cell
        .map(|(r, _)| r == actual_row)
        .unwrap_or(false);

    let row_bg = if is_selected_row {
        colors.bg_selected
    } else if display_idx % 2 == 0 {
        colors.row_even
    } else {
        colors.row_odd
    };

    // Row number (pinned, doesn't scroll horizontally)
    let rn_rect = egui::Rect::from_min_size(
        egui::pos2(left_x, row_y),
        Vec2::new(ROW_NUMBER_WIDTH, ROW_HEIGHT),
    );

    // Data cells (scroll horizontally)
    let data_area_left = left_x + ROW_NUMBER_WIDTH;
    let col_clip = egui::Rect::from_min_max(
        egui::pos2(data_area_left, panel_rect.top() + HEADER_HEIGHT + 1.0),
        panel_rect.max,
    );
    let col_painter = painter.with_clip_rect(col_clip);

    let mut x = data_area_left - state.scroll_x;

    for col_idx in 0..table.col_count() {
        let w = state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);

        let rect = egui::Rect::from_min_size(egui::pos2(x, row_y), Vec2::new(w, ROW_HEIGHT));

        // Skip cells that are completely off-screen horizontally
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

            // Cell background
            let cell_bg = if is_selected {
                colors.bg_selected
            } else if is_edited {
                colors.bg_edited
            } else {
                row_bg
            };
            col_painter.rect_filled(rect, 0.0, cell_bg);

            // Right border
            col_painter.line_segment(
                [rect.right_top(), rect.right_bottom()],
                egui::Stroke::new(0.5, colors.border_subtle),
            );

            if is_editing {
                // For inline editing, we use ui.put() which needs to be within the panel
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

                        // Request focus on first frame only
                        if state.edit_needs_focus {
                            edit_response.request_focus();
                            state.edit_needs_focus = false;
                        }

                        if edit_response.lost_focus() {
                            commit_text = Some(buf.clone());
                        }
                    }
                }
                // Commit outside the borrow of editing_cell
                if let Some(new_text) = commit_text {
                    if let Some(old_val) = table.get(actual_row, col_idx) {
                        let new_val = crate::data::CellValue::parse_like(old_val, &new_text);
                        table.set(actual_row, col_idx, new_val);
                    }
                    state.editing_cell = None;
                }
            } else {
                // Display cell value (clipped)
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

            // Handle click: select, double-click: edit
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
            }
        }

        x += w;
    }

    // Draw row number on top (pinned, doesn't scroll)
    painter.rect_filled(rn_rect, 0.0, colors.row_number_bg);
    painter.text(
        rn_rect.center(),
        Align2::CENTER_CENTER,
        format!("{}", actual_row + 1),
        egui::FontId::new(11.0, egui::FontFamily::Monospace),
        colors.row_number_text,
    );
}
