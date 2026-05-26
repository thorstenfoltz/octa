mod header;
mod rows;
mod state;

use std::collections::HashSet;

use egui::{Color32, RichText, Sense, Ui, Vec2};

use super::shortcuts::{ShortcutAction, Shortcuts};
use super::status_bar::format_number;
use super::theme::{ThemeColors, ThemeMode};
use crate::data::{BinaryDisplayMode, DataTable, MarkColor, MarkKey};

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
    /// Display-index anchor for Shift+Arrow row-range selection.
    /// Seeded on first Shift+Arrow press and cleared on any non-Shift move.
    pub selection_anchor_display: Option<usize>,
    /// Multi-selection: selected columns (by column index).
    pub selected_cols: HashSet<usize>,
    /// Multi-cell selection (a free set of (row, col) cells). Populated by
    /// Ctrl+Arrow extension starting from a single cell. Cleared on plain
    /// click or plain-arrow navigation.
    pub selected_cells: HashSet<(usize, usize)>,
    /// Clipboard content (tab-separated values, rows separated by newlines).
    pub clipboard: Option<String>,
    /// Whether the OS clipboard currently has text content (set each frame by the app).
    pub os_clipboard_has_text: bool,
    /// Column header being renamed: (col_idx, current_buffer).
    pub editing_col_name: Option<(usize, String)>,
    /// Whether the column name edit widget needs initial focus.
    pub edit_col_needs_focus: bool,
    /// Dynamic row number column width (computed from total row count).
    pub row_number_width: f32,
    /// Cached prefix sums of row heights when cell_line_breaks is on.
    /// `[i]` = Y offset of display row i; `[row_count]` = total data height.
    row_y_offsets: Vec<f32>,
    /// Generation counter — bumped on any change that could affect row heights.
    row_heights_generation: u64,
    /// Generation at which the cache was last built.
    row_heights_cached_generation: u64,
    /// Pending request from the `FitAllColumns` shortcut. Drained on the next
    /// `draw_table` call, where a `Ui` is available for font measurement.
    pub fit_all_columns_requested: bool,
}

const DEFAULT_ROW_HEIGHT: f32 = 26.0;
const MIN_COL_WIDTH: f32 = 60.0;
const DEFAULT_COL_WIDTH: f32 = 120.0;
const MIN_ROW_NUMBER_WIDTH: f32 = 60.0;
const HEADER_HEIGHT: f32 = 44.0; // taller to fit column index number
const RESIZE_HANDLE_WIDTH: f32 = 6.0;
/// Empty pad after the last column so its tail characters never sit under
/// the vertical scrollbar. Reachable via horizontal scroll, not painted over.
const TRAILING_GAP: f32 = 12.0;

/// Scroll vertically so the given display-row index stays visible.
fn scroll_row_into_view(
    state: &mut TableViewState,
    display_idx: usize,
    row_height: f32,
    data_area_height: f32,
    max_scroll_y: f32,
) {
    let (row_top, row_bottom) = if state.row_y_offsets.len() > display_idx {
        let top = state.row_y_offsets[display_idx];
        let bottom = if display_idx + 1 < state.row_y_offsets.len() {
            state.row_y_offsets[display_idx + 1]
        } else {
            top + row_height
        };
        (top, bottom)
    } else {
        let top = display_idx as f32 * row_height;
        (top, top + row_height)
    };
    if row_top < state.scroll_y {
        state.scroll_y = row_top;
    } else if row_bottom > state.scroll_y + data_area_height {
        state.scroll_y = row_bottom - data_area_height;
    }
    state.scroll_y = state.scroll_y.clamp(0.0, max_scroll_y);
}

/// Scroll horizontally so the given column index stays visible.
fn scroll_col_into_view(
    state: &mut TableViewState,
    col_idx: usize,
    view_width: f32,
    max_scroll_x: f32,
) {
    let col_left: f32 = state.col_widths[..col_idx].iter().sum();
    let col_right = col_left
        + state
            .col_widths
            .get(col_idx)
            .copied()
            .unwrap_or(DEFAULT_COL_WIDTH);
    if col_left < state.scroll_x {
        state.scroll_x = col_left;
    } else if col_right > state.scroll_x + (view_width - state.row_number_width) {
        state.scroll_x = col_right - (view_width - state.row_number_width);
    }
    state.scroll_x = state.scroll_x.clamp(0.0, max_scroll_x);
}

/// Binary search in prefix-sum array to find the row containing a given scroll offset.
fn row_at_offset(offsets: &[f32], scroll_y: f32) -> usize {
    let mut lo = 0usize;
    let mut hi = offsets.len().saturating_sub(2);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if offsets[mid] <= scroll_y {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Rebuild the prefix-sum of row heights if the cache is stale.
fn ensure_row_y_offsets(
    ui: &Ui,
    state: &mut TableViewState,
    table: &DataTable,
    filtered_rows: &[usize],
    font_size: f32,
    base_row_height: f32,
    binary_display_mode: BinaryDisplayMode,
) {
    if state.row_heights_cached_generation == state.row_heights_generation
        && state.row_y_offsets.len() == filtered_rows.len() + 1
    {
        return;
    }
    let col_widths = state.col_widths.clone();
    let mut offsets = Vec::with_capacity(filtered_rows.len() + 1);
    offsets.push(0.0);
    let mut cumulative = 0.0f32;
    for &actual_row in filtered_rows {
        let h = compute_row_height(
            ui,
            table,
            actual_row,
            &col_widths,
            font_size,
            base_row_height,
            binary_display_mode,
        );
        cumulative += h;
        offsets.push(cumulative);
    }
    state.row_y_offsets = offsets;
    state.row_heights_cached_generation = state.row_heights_generation;
}

const SORT_ARROW_SIZE: f32 = 14.0;
const COL_INDEX_HEIGHT: f32 = 12.0; // space for the column index letter at top

/// Cap on how many rows to sample when computing the best-fit column width on
/// double-click. The cell renderer truncates beyond the visible area anyway,
/// and walking 11 M rows for a single double-click would freeze the UI.
const AUTOFIT_MAX_ROWS: usize = 5_000;

/// Padding added to the longest measured cell or header text so the column
/// doesn't end with the last glyph kissing the right border.
const AUTOFIT_PADDING: f32 = 16.0;

/// Compute the "best fit" width for a column by measuring header + content
/// with the actual font, then padding. Sample is capped at [`AUTOFIT_MAX_ROWS`]
/// rows from the filtered set.
fn compute_optimal_col_width(
    ui: &Ui,
    table: &DataTable,
    filtered_rows: &[usize],
    col_idx: usize,
    font_size: f32,
    binary_display_mode: BinaryDisplayMode,
) -> f32 {
    let mono = egui::FontId::new(font_size, egui::FontFamily::Monospace);
    let mut max_w: f32 = 0.0;

    if let Some(col) = table.columns.get(col_idx) {
        let header_w = ui.fonts_mut(|f| {
            f.layout_no_wrap(col.name.clone(), mono.clone(), egui::Color32::WHITE)
                .size()
                .x
        });
        // Header row also fits a sort-arrow icon and the column-index letter,
        // so reserve some extra headroom.
        max_w = max_w.max(header_w + SORT_ARROW_SIZE * 2.0 + 16.0);

        let type_w = ui.fonts_mut(|f| {
            f.layout_no_wrap(col.data_type.clone(), mono.clone(), egui::Color32::WHITE)
                .size()
                .x
        });
        max_w = max_w.max(type_w + 16.0);
    }

    let sample_count = filtered_rows.len().min(AUTOFIT_MAX_ROWS);
    for row_idx in &filtered_rows[..sample_count] {
        if let Some(value) = table.get(*row_idx, col_idx) {
            let text = value.display_with_binary_mode(binary_display_mode);
            if text.is_empty() {
                continue;
            }
            let w = ui.fonts_mut(|f| {
                f.layout_no_wrap(text, mono.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            });
            if w > max_w {
                max_w = w;
            }
        }
    }

    (max_w + AUTOFIT_PADDING).max(MIN_COL_WIDTH)
}

/// Convert a 0-based column index to an Excel-style letter label (A, B, ..., Z, AA, AB, ...).
fn col_index_letter(idx: usize) -> String {
    let mut result = String::new();
    let mut n = idx;
    loop {
        result.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    result
}

/// Signals from the table back to the app.
#[derive(Default)]
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
    /// Copy/Cut/Paste signals
    pub ctx_copy: bool,
    pub ctx_cut: bool,
    pub ctx_paste: bool,
    /// Text received from OS clipboard via Ctrl+V / Paste event
    pub paste_text: Option<String>,
    /// Column rename: (col_idx, new_name).
    pub rename_column: Option<(usize, String)>,
    /// Change column data type: (col_idx, new_type).
    pub change_col_type: Option<(usize, String)>,
    /// Copy just the selected cell's value (not row/column selection).
    pub ctx_copy_cell: bool,
    /// Signal that more rows should be loaded (scroll near bottom with truncated data).
    pub needs_more_rows: bool,
    /// Set a color mark on one or more keys. The list lets the right-click
    /// "Mark" submenu honour the current multi-selection (cells / rows /
    /// columns) instead of always colouring just the clicked target — the
    /// same precedence Ctrl+M follows via `mark_selection_default`.
    pub set_mark: Option<(Vec<MarkKey>, MarkColor)>,
    /// Clear a color mark from one or more keys.
    pub clear_mark: Option<Vec<MarkKey>>,
    /// Open the "Parse in new tab" modal for the selected scope.
    pub ctx_parse_in_new_tab: Option<super::toolbar::ParseScope>,
    /// Open the Column Filter dialog pre-selected on this column index.
    /// Fired by the column-header right-click menu's "Filter values..." entry.
    pub ctx_filter_column: Option<usize>,
    /// Hide a column from the table view. The data is preserved on disk
    /// (Save / Save As writes hidden columns too); only the renderer omits
    /// them. Cleared via Edit → Show hidden columns.
    pub ctx_hide_column: Option<usize>,
    /// Open the Value Frequency dialog for this column. Fired by the
    /// column-header right-click menu's "Value frequency…" entry; the
    /// `ColumnValueFrequency` keyboard shortcut goes through
    /// `shortcuts_dispatch` instead.
    pub ctx_value_frequency: Option<usize>,
    /// The big logo on the welcome screen (rendered when the active tab has
    /// no columns) was just clicked. Counted by the snow easter egg —
    /// three within 1.5s triggers a 5-second snowfall.
    pub welcome_logo_clicked: bool,
    /// Screen-space rect the welcome-screen logo image was painted into, if
    /// the welcome screen rendered this frame. Used by the Christmas easter
    /// egg to overlay a Santa hat at a position the binary side can compute
    /// without re-deriving the centred-image math.
    pub welcome_logo_rect: Option<egui::Rect>,
}

/// Draw the data table with true row virtualization.
#[allow(clippy::too_many_arguments)]
pub fn draw_table(
    ui: &mut Ui,
    table: &mut DataTable,
    state: &mut TableViewState,
    theme_mode: ThemeMode,
    filtered_rows: &[usize],
    os_clipboard_has_content: bool,
    show_row_numbers: bool,
    alternating_row_colors: bool,
    negative_numbers_red: bool,
    highlight_edits: bool,
    font_size: f32,
    cell_line_breaks: bool,
    binary_display_mode: BinaryDisplayMode,
    welcome_logo_texture: Option<&egui::TextureHandle>,
    shortcuts: &Shortcuts,
    readonly: bool,
    // Column indices that currently have an active per-column filter. Used
    // only to paint the header dot marker; the actual row filtering is
    // already applied in `filtered_rows`.
    filtered_columns: &HashSet<usize>,
    // Column indices the user has hidden via right-click → "Hide column".
    // Hidden columns render with width 0 and skip paint entirely. Data
    // stays in the table (Save / Save As writes them).
    hidden_columns: &HashSet<usize>,
) -> TableInteraction {
    let colors = ThemeColors::for_mode(theme_mode);
    let row_height = (font_size * 2.0).max(DEFAULT_ROW_HEIGHT);
    state.ensure_widths(table);
    state.os_clipboard_has_text = os_clipboard_has_content;

    // Fulfil a pending FitAllColumns shortcut request now that we have a Ui
    // for font measurement.
    if state.fit_all_columns_requested {
        state.fit_all_columns_requested = false;
        state.fit_all_columns(ui, table, filtered_rows, font_size, binary_display_mode);
    }

    // Compute row number column width based on the largest row number
    if show_row_numbers {
        let max_row_num = table.row_offset + filtered_rows.len();
        let formatted_len = format_number(max_row_num).len() as f32;
        state.row_number_width = (formatted_len * 8.0 + 16.0).max(MIN_ROW_NUMBER_WIDTH);
    } else {
        state.row_number_width = 0.0;
    }

    let mut interaction = TableInteraction::default();

    if table.col_count() == 0 {
        // Rainbow easter-egg: paint a large, faded copy of the (now-random)
        // welcome icon as a background watermark behind the normal centred
        // icon. Only on the welcome screen — data views stay clean per the
        // user choice. The watermark uses the same texture as the centre
        // logo so it follows the random variant rolled at activation time.
        if theme_mode.is_rainbow()
            && let Some(tex) = welcome_logo_texture
        {
            let panel = ui.available_rect_before_wrap();
            let side = (panel.width().min(panel.height()) * 0.95).clamp(160.0, 1024.0);
            let bg_rect = egui::Rect::from_center_size(panel.center(), Vec2::new(side, side));
            // Low-alpha white tint so the watermark sits softly behind the
            // crisp centred logo. Same image, just enlarged + dimmed.
            let tint = Color32::from_white_alpha(36);
            ui.painter().image(
                tex.id(),
                bg_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
        }
        ui.vertical_centered(|ui| {
            let avail = ui.available_size();
            let logo_size = (avail.x.min(avail.y) * 0.55).clamp(128.0, 512.0);
            ui.add_space((avail.y - logo_size - 40.0).max(0.0) / 2.0);
            if let Some(tex) = welcome_logo_texture {
                let resp = ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(
                        tex.id(),
                        [logo_size, logo_size],
                    ))
                    .sense(egui::Sense::click()),
                );
                if resp.clicked() {
                    interaction.welcome_logo_clicked = true;
                }
                interaction.welcome_logo_rect = Some(resp.rect);
            }
            ui.add_space(16.0);
            ui.label(RichText::new("Octa").size(28.0).color(colors.text_muted));
        });
        return interaction;
    }

    let total_col_width: f32 = state.row_number_width
        + state.col_widths.iter().sum::<f32>()
        + RESIZE_HANDLE_WIDTH
        + TRAILING_GAP;
    let row_count = filtered_rows.len();

    let available_rect = ui.available_rect_before_wrap();
    let view_width = available_rect.width();
    let view_height = available_rect.height();

    let total_data_height = if cell_line_breaks {
        ensure_row_y_offsets(
            ui,
            state,
            table,
            filtered_rows,
            font_size,
            row_height,
            binary_display_mode,
        );
        state.row_y_offsets[row_count]
    } else {
        state.row_y_offsets.clear();
        row_count as f32 * row_height
    };

    // When the vertical scrollbar is visible it sits at the right edge of the
    // panel and occludes the last ~12 px of column data. Account for its width
    // so that the user can scroll far enough right to reveal the last column.
    let vscroll_visible = total_data_height + HEADER_HEIGHT + 1.0 > view_height;
    let vscroll_width = if vscroll_visible { 12.0 } else { 0.0 };

    // When the horizontal scrollbar is visible it sits at the bottom of the
    // panel and would otherwise paint over the last data row. Reserve its
    // footprint here so the data area shrinks accordingly and `max_scroll_y`
    // lands the last row exactly above the scrollbar — no slack, no clipping.
    let horizontal_scrollbar_visible = total_col_width > view_width - vscroll_width;
    let horizontal_scrollbar_height = if horizontal_scrollbar_visible {
        11.0
    } else {
        0.0
    };
    let total_content_height =
        HEADER_HEIGHT + 1.0 + total_data_height + horizontal_scrollbar_height;

    // Handle scroll input and keyboard shortcuts
    ui.input(|input| {
        let scroll_delta = input.smooth_scroll_delta;
        state.scroll_y = (state.scroll_y - scroll_delta.y)
            .clamp(0.0, (total_content_height - view_height).max(0.0));
        state.scroll_x = (state.scroll_x - scroll_delta.x)
            .clamp(0.0, (total_col_width + vscroll_width - view_width).max(0.0));
    });

    // Arrow key navigation: move selected cell and auto-scroll into view.
    //
    // Key layout (defaults; all remappable via Settings → Shortcuts):
    //   Arrow           — move the selection by one cell
    //   Shift+Arrow     — extend the row range from the anchor
    //   Ctrl+Shift+↑/↓  — jump to first/last row
    //   Ctrl+Shift+←/→  — jump to first/last column
    //   Ctrl+↑/↓        — when whole row(s) are selected, grow the row
    //                     selection by one above/below
    //   Ctrl+←/→        — when whole column(s) are selected, grow the column
    //                     selection by one to the left/right
    let any_text_edit_focused = ui
        .ctx()
        .memory(|m| m.focused())
        .and_then(|id| egui::TextEdit::load_state(ui.ctx(), id).map(|_| ()))
        .is_some();
    if state.editing_cell.is_none() && !any_text_edit_focused {
        let max_scroll_y = (total_content_height - view_height).max(0.0);
        let max_scroll_x = (total_col_width + vscroll_width - view_width).max(0.0);
        let data_area_height =
            (view_height - HEADER_HEIGHT - 1.0 - horizontal_scrollbar_height).max(0.0);

        let triggered = |a: ShortcutAction| ui.input(|i| shortcuts.triggered(a, i));
        let jump_first_row = triggered(ShortcutAction::JumpFirstRow);
        let jump_last_row = triggered(ShortcutAction::JumpLastRow);
        let jump_first_col = triggered(ShortcutAction::JumpFirstCol);
        let jump_last_col = triggered(ShortcutAction::JumpLastCol);
        let ext_up = triggered(ShortcutAction::ExtendSelectionUp);
        let ext_down = triggered(ShortcutAction::ExtendSelectionDown);
        let ext_left = triggered(ShortcutAction::ExtendSelectionLeft);
        let ext_right = triggered(ShortcutAction::ExtendSelectionRight);
        let page_up = triggered(ShortcutAction::ScrollPageUp);
        let page_down = triggered(ShortcutAction::ScrollPageDown);

        // Page scrolling: advance the selection by the number of rows
        // currently visible and let `scroll_row_into_view` follow the
        // selection so the new top (PageDown) / bottom (PageUp) row of
        // the viewport is the now-selected one. Run before the per-cell
        // nav block so the plain-arrow handler doesn't also fire.
        if (page_up || page_down) && !filtered_rows.is_empty() {
            let row_count = filtered_rows.len();
            let (cur_row, cur_col) = state.selected_cell.unwrap_or((0, 0));
            let cur_display = filtered_rows
                .iter()
                .position(|&r| r == cur_row)
                .unwrap_or(0);
            // Estimate rows per visible page from the average row height.
            // `row_height` is the default-cell height; when cell line
            // breaks are on, individual rows are taller than this, but
            // approximating still gives a useful page step.
            let rows_per_page = if row_height > 0.0 {
                ((data_area_height / row_height).floor() as usize).max(1)
            } else {
                1
            };
            let new_display = if page_down {
                (cur_display + rows_per_page).min(row_count.saturating_sub(1))
            } else {
                cur_display.saturating_sub(rows_per_page)
            };
            if let Some(&new_row) = filtered_rows.get(new_display) {
                state.selected_cell = Some((new_row, cur_col));
                state.selected_cells.clear();
                state.selected_rows.clear();
                state.selected_cols.clear();
                state.selection_anchor_display = None;
                scroll_row_into_view(
                    state,
                    new_display,
                    row_height,
                    data_area_height,
                    max_scroll_y,
                );
            }
        }

        // Handle "extend row/column selection by one" first: applies when
        // a whole row/column block is selected, or when only a single cell
        // is selected (in which case the cell anchors a new row/column run).
        // Returns true if consumed so the plain-arrow handler below doesn't
        // also fire.
        let row_block_selected = !state.selected_rows.is_empty() && state.selected_cols.is_empty();
        let col_block_selected = !state.selected_cols.is_empty() && state.selected_rows.is_empty();
        // Cell-extension mode: Ctrl+Arrow extends a free multi-cell selection
        // anchored at the current selected_cell. Triggered from a single-cell
        // selection or while a previous cell-extension run is active.
        let cell_extend_mode = state.selected_cell.is_some()
            && state.selected_rows.is_empty()
            && state.selected_cols.is_empty();

        let mut handled = false;

        if cell_extend_mode
            && (ext_up || ext_down)
            && let Some((cur_row, cur_col)) = state.selected_cell
        {
            let cur_display = filtered_rows
                .iter()
                .position(|&r| r == cur_row)
                .unwrap_or(0);
            let new_display = if ext_up {
                cur_display.saturating_sub(1)
            } else {
                (cur_display + 1).min(filtered_rows.len().saturating_sub(1))
            };
            if let Some(&new_row) = filtered_rows.get(new_display) {
                state.selected_cells.insert((cur_row, cur_col));
                state.selected_cells.insert((new_row, cur_col));
                state.selected_cell = Some((new_row, cur_col));
                scroll_row_into_view(
                    state,
                    new_display,
                    row_height,
                    data_area_height,
                    max_scroll_y,
                );
            }
            handled = true;
        }

        if !handled
            && cell_extend_mode
            && (ext_left || ext_right)
            && let Some((cur_row, cur_col)) = state.selected_cell
        {
            let col_count = table.col_count();
            let new_col = if ext_left {
                cur_col.saturating_sub(1)
            } else {
                (cur_col + 1).min(col_count.saturating_sub(1))
            };
            state.selected_cells.insert((cur_row, cur_col));
            state.selected_cells.insert((cur_row, new_col));
            state.selected_cell = Some((cur_row, new_col));
            scroll_col_into_view(state, new_col, view_width, max_scroll_x);
            handled = true;
        }

        if row_block_selected && (ext_up || ext_down) {
            let displays: Vec<usize> = filtered_rows
                .iter()
                .enumerate()
                .filter_map(|(d, r)| {
                    if state.selected_rows.contains(r) {
                        Some(d)
                    } else {
                        None
                    }
                })
                .collect();
            if !displays.is_empty() {
                let new_display = if ext_up {
                    displays.iter().copied().min().unwrap().saturating_sub(1)
                } else {
                    (displays.iter().copied().max().unwrap() + 1).min(filtered_rows.len() - 1)
                };
                if let Some(&new_row) = filtered_rows.get(new_display) {
                    state.selected_rows.insert(new_row);
                    let col = state.selected_cell.map(|(_, c)| c).unwrap_or(0);
                    state.selected_cell = Some((new_row, col));
                    scroll_row_into_view(
                        state,
                        new_display,
                        row_height,
                        data_area_height,
                        max_scroll_y,
                    );
                }
                handled = true;
            }
        }

        if col_block_selected && (ext_left || ext_right) {
            let cols: Vec<usize> = state.selected_cols.iter().copied().collect();
            if !cols.is_empty() {
                let col_count = table.col_count();
                let new_col = if ext_left {
                    cols.iter().copied().min().unwrap().saturating_sub(1)
                } else {
                    (cols.iter().copied().max().unwrap() + 1).min(col_count.saturating_sub(1))
                };
                state.selected_cols.insert(new_col);
                let row = state.selected_cell.map(|(r, _)| r).unwrap_or(0);
                state.selected_cell = Some((row, new_col));
                scroll_col_into_view(state, new_col, view_width, max_scroll_x);
                handled = true;
            }
        }

        if !handled {
            let shift = ui.input(|i| i.modifiers.shift);
            // Raw arrow keys (no modifiers, or Shift for row-range extension).
            // Guard against Ctrl — Ctrl+Arrow is handled via the extend-row /
            // extend-column shortcuts above; plain arrows must not also fire
            // when Ctrl is held, otherwise we'd both grow and move the cell.
            let no_ctrl = ui.input(|i| !(i.modifiers.ctrl || i.modifiers.mac_cmd));
            let arrow_up = no_ctrl && ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let arrow_down = no_ctrl && ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
            let arrow_left = no_ctrl && ui.input(|i| i.key_pressed(egui::Key::ArrowLeft));
            let arrow_right = no_ctrl && ui.input(|i| i.key_pressed(egui::Key::ArrowRight));

            if arrow_up
                || arrow_down
                || arrow_left
                || arrow_right
                || jump_first_row
                || jump_last_row
                || jump_first_col
                || jump_last_col
            {
                let row_count = filtered_rows.len();
                let col_count = table.col_count();
                let (cur_row, cur_col) = state.selected_cell.unwrap_or((0, 0));

                let cur_display = filtered_rows
                    .iter()
                    .position(|&r| r == cur_row)
                    .unwrap_or(0);

                let mut new_display = cur_display;
                let mut new_col = cur_col;

                if jump_first_row {
                    new_display = 0;
                } else if jump_last_row {
                    new_display = row_count.saturating_sub(1);
                } else if arrow_up && cur_display > 0 {
                    new_display = cur_display - 1;
                } else if arrow_down && cur_display + 1 < row_count {
                    new_display = cur_display + 1;
                }
                if jump_first_col {
                    new_col = 0;
                } else if jump_last_col {
                    new_col = col_count.saturating_sub(1);
                } else if arrow_left && cur_col > 0 {
                    new_col = cur_col - 1;
                } else if arrow_right && cur_col + 1 < col_count {
                    new_col = cur_col + 1;
                }

                if let Some(&new_row) = filtered_rows.get(new_display) {
                    state.selected_cell = Some((new_row, new_col));
                    state.selected_cells.clear();

                    let extending_rows = shift && (arrow_up || arrow_down);
                    if extending_rows {
                        let anchor = *state.selection_anchor_display.get_or_insert(cur_display);
                        let (lo, hi) = if anchor <= new_display {
                            (anchor, new_display)
                        } else {
                            (new_display, anchor)
                        };
                        state.selected_rows.clear();
                        for d in lo..=hi {
                            if let Some(&r) = filtered_rows.get(d) {
                                state.selected_rows.insert(r);
                            }
                        }
                        state.selected_cols.clear();
                    } else {
                        state.selection_anchor_display = None;
                        state.selected_rows.clear();
                        state.selected_cols.clear();
                    }

                    scroll_row_into_view(
                        state,
                        new_display,
                        row_height,
                        data_area_height,
                        max_scroll_y,
                    );
                    scroll_col_into_view(state, new_col, view_width, max_scroll_x);
                }
            }
        }
    }

    // Ctrl+Z / Ctrl+Y are dispatched by `handle_shortcuts` via
    // `ShortcutAction::Undo`/`Redo`, which honors user-rebound combos.
    // Also detect paste from egui's Paste event (carries clipboard text directly)
    let paste_from_event: Option<String> = ui.input(|i| {
        i.events.iter().find_map(|e| {
            if let egui::Event::Paste(text) = e {
                Some(text.clone())
            } else {
                None
            }
        })
    });
    if let Some(text) = paste_from_event
        && state.editing_cell.is_none()
    {
        interaction.ctx_paste = true;
        interaction.paste_text = Some(text);
    }

    let (panel_rect, _) =
        ui.allocate_exact_size(Vec2::new(view_width, view_height), Sense::hover());

    let painter = ui.painter_at(panel_rect);

    // --- Draw header ---
    let header_y = panel_rect.top();
    header::draw_header_direct(
        ui,
        &painter,
        table,
        state,
        &colors,
        panel_rect.left(),
        header_y,
        panel_rect,
        &mut interaction,
        font_size,
        filtered_rows,
        binary_display_mode,
        filtered_columns,
        hidden_columns,
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
    let data_area_height =
        (panel_rect.bottom() - data_area_top - horizontal_scrollbar_height).max(0.0);
    let data_area_bottom = data_area_top + data_area_height;

    let data_clip_rect = egui::Rect::from_min_max(
        egui::pos2(panel_rect.left(), data_area_top),
        egui::pos2(panel_rect.right(), data_area_bottom),
    );
    let data_painter = painter.with_clip_rect(data_clip_rect);

    let (first_visible, first_visible_offset) =
        if cell_line_breaks && !state.row_y_offsets.is_empty() {
            let idx = row_at_offset(&state.row_y_offsets, state.scroll_y);
            (idx, state.row_y_offsets[idx])
        } else {
            let idx = (state.scroll_y / row_height).floor() as usize;
            (idx, idx as f32 * row_height)
        };
    let visible_count = (data_area_height / row_height).ceil() as usize + 2;
    let last_visible = (first_visible + visible_count).min(row_count);

    let mut current_y = data_area_top + first_visible_offset - state.scroll_y;

    #[allow(clippy::needless_range_loop)]
    for display_idx in first_visible..last_visible {
        let actual_row = filtered_rows[display_idx];

        let actual_row_height = if cell_line_breaks && display_idx + 1 < state.row_y_offsets.len() {
            state.row_y_offsets[display_idx + 1] - state.row_y_offsets[display_idx]
        } else {
            row_height
        };

        if current_y + actual_row_height >= data_area_top && current_y <= data_area_bottom {
            rows::draw_data_row_direct(
                ui,
                &data_painter,
                table,
                state,
                &colors,
                actual_row,
                display_idx,
                panel_rect.left(),
                current_y,
                panel_rect,
                &mut interaction,
                show_row_numbers,
                alternating_row_colors,
                negative_numbers_red,
                highlight_edits,
                font_size,
                cell_line_breaks,
                binary_display_mode,
                actual_row_height,
                readonly,
                hidden_columns,
                theme_mode.is_rainbow(),
            );
        }

        current_y += actual_row_height;
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
        if track_response.clicked()
            && let Some(pos) = track_response.interact_pointer_pos()
        {
            let click_fraction = (pos.y - scrollbar_track_top) / scrollbar_track_height;
            state.scroll_y =
                (click_fraction * total_content_height - view_height / 2.0).clamp(0.0, max_scroll);
        }
    }

    // --- Horizontal scrollbar ---
    if horizontal_scrollbar_visible {
        let scrollbar_height = 10.0;
        let scrollbar_y = panel_rect.bottom() - scrollbar_height - 1.0;
        let scrollbar_track_left = panel_rect.left();
        let scrollbar_track_width = view_width;

        let track_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_track_left, scrollbar_y),
            Vec2::new(scrollbar_track_width, scrollbar_height),
        );
        painter.rect_filled(track_rect, scrollbar_height / 2.0, colors.scrollbar_track);

        let effective_width = view_width - vscroll_width;
        let thumb_fraction = effective_width / total_col_width;
        let thumb_width = (thumb_fraction * scrollbar_track_width).max(24.0);
        let max_scroll = (total_col_width + vscroll_width - view_width).max(0.0);
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
        if track_response.clicked()
            && let Some(pos) = track_response.interact_pointer_pos()
        {
            let click_fraction = (pos.x - scrollbar_track_left) / scrollbar_track_width;
            state.scroll_x =
                (click_fraction * total_col_width - view_width / 2.0).clamp(0.0, max_scroll);
        }
    }

    // Signal that more rows should be loaded when scrolled near the bottom
    if table.total_rows.is_some()
        && state.scroll_y + view_height >= total_content_height - row_height * 100.0
    {
        interaction.needs_more_rows = true;
    }

    interaction
}

/// Compute the height of a row by measuring wrapped text in each cell.
fn compute_row_height(
    ui: &Ui,
    table: &DataTable,
    actual_row: usize,
    col_widths: &[f32],
    font_size: f32,
    base_row_height: f32,
    binary_display_mode: BinaryDisplayMode,
) -> f32 {
    let mut max_height = base_row_height;
    let font_id = egui::FontId::new(font_size, egui::FontFamily::Monospace);
    for col_idx in 0..table.col_count() {
        if let Some(value) = table.get(actual_row, col_idx) {
            let text = value.display_with_binary_mode(binary_display_mode);
            let col_width = col_widths
                .get(col_idx)
                .copied()
                .unwrap_or(DEFAULT_COL_WIDTH);
            let wrap_width = (col_width - 12.0).max(20.0); // account for cell padding
            let galley =
                ui.fonts_mut(|f| f.layout(text, font_id.clone(), egui::Color32::WHITE, wrap_width));
            let text_height = galley.size().y + 4.0; // small vertical padding
            max_height = max_height.max(text_height);
        }
    }
    max_height
}

/// Render the right-click "Mark" submenu.
///
/// `keys` is the full list of marks to apply when the user picks a colour —
/// caller-built so a right-click on a cell that's part of a multi-cell
/// selection colours every selected cell (mirrors Ctrl+M). `current` is the
/// mark on the *anchor* (the right-clicked target) used to show
/// "(current)" / surface a Clear entry when the anchor is already marked.
fn mark_submenu(
    ui: &mut Ui,
    keys: Vec<MarkKey>,
    anchor: &MarkKey,
    table: &DataTable,
    interaction: &mut TableInteraction,
) {
    let current_mark = table.marks.get(anchor).copied();
    ui.menu_button("Mark", |ui| {
        for &color in MarkColor::ALL {
            let swatch = ThemeColors::mark_swatch(color);
            let label = if current_mark == Some(color) {
                format!("{} (current)", color.label())
            } else {
                color.label().to_string()
            };
            let btn = egui::Button::new(RichText::new(label).color(swatch));
            if ui.add(btn).clicked() {
                interaction.set_mark = Some((keys.clone(), color));
                ui.close();
            }
        }
        if current_mark.is_some() {
            ui.separator();
            if ui.button("Clear").clicked() {
                interaction.clear_mark = Some(keys.clone());
                ui.close();
            }
        }
    });
}
