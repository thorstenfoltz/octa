//! Keyboard input for the table: arrow / page / jump navigation, the
//! extend-selection shortcuts, and paste events.

use egui::Ui;

use super::{TableInteraction, TableViewState, scroll_col_into_view, scroll_row_into_view};
use crate::data::DataTable;
use crate::ui::shortcuts::{ShortcutAction, Shortcuts};

/// The viewport numbers the navigation needs to scroll a moved selection
/// back into view.
pub(super) struct NavGeometry {
    pub row_height: f32,
    pub data_area_height: f32,
    pub max_scroll_y: f32,
    pub max_scroll_x: f32,
    pub view_width: f32,
    pub frozen_cols: usize,
    pub frozen_width: f32,
}

/// Arrow key navigation: move selected cell and auto-scroll into view.
///
/// Key layout (defaults; all remappable via Settings -> Shortcuts):
///   Arrow           - move the selection by one cell
///   Shift+Arrow     - extend the row range from the anchor
///   Ctrl+Shift+Up/Down    - jump to first/last row
///   Ctrl+Shift+Left/Right - jump to first/last column
///   Ctrl+Up/Down    - when whole row(s) are selected, grow the row
///                     selection by one above/below
///   Ctrl+Left/Right - when whole column(s) are selected, grow the column
///                     selection by one to the left/right
pub(super) fn handle_keyboard_nav(
    ui: &Ui,
    state: &mut TableViewState,
    table: &DataTable,
    filtered_rows: &[usize],
    shortcuts: &Shortcuts,
    geometry: NavGeometry,
    interaction: &mut TableInteraction,
) {
    let NavGeometry {
        row_height,
        data_area_height,
        max_scroll_y,
        max_scroll_x,
        view_width,
        frozen_cols,
        frozen_width,
    } = geometry;
    let triggered = |a: ShortcutAction| ui.input(|i| shortcuts.triggered(a, i));
    let mut jump_first_row = triggered(ShortcutAction::JumpFirstRow);
    let mut jump_last_row = triggered(ShortcutAction::JumpLastRow);
    // In large-file mode "first"/"last" row means the file's, not the
    // loaded page's. Handled here rather than below because the in-page
    // move would scroll to a page edge, which is exactly what re-arms the
    // paging triggers - the jump would be undone in the same frame.
    if let Some((_, file_rows)) = state.virtual_rows.filter(|&(_, n)| n > filtered_rows.len()) {
        if jump_first_row {
            interaction.jump_to_row = Some(0);
        } else if jump_last_row {
            interaction.jump_to_row = Some(file_rows.saturating_sub(1));
        }
        jump_first_row = false;
        jump_last_row = false;
    }
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
        scroll_col_into_view(
            state,
            new_col,
            view_width,
            max_scroll_x,
            frozen_cols,
            frozen_width,
        );
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
            scroll_col_into_view(
                state,
                new_col,
                view_width,
                max_scroll_x,
                frozen_cols,
                frozen_width,
            );
            handled = true;
        }
    }

    if !handled {
        let shift = ui.input(|i| i.modifiers.shift);
        // Raw arrow keys (no modifiers, or Shift for row-range extension).
        // Guard against Ctrl - Ctrl+Arrow is handled via the extend-row /
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
                scroll_col_into_view(
                    state,
                    new_col,
                    view_width,
                    max_scroll_x,
                    frozen_cols,
                    frozen_width,
                );
            }
        }
    }
}

/// Ctrl+Z / Ctrl+Y are dispatched by `handle_shortcuts` via
/// `ShortcutAction::Undo`/`Redo`, which honors user-rebound combos. This only
/// detects paste from egui's Paste event (carries clipboard text directly).
pub(super) fn take_paste_event(
    ui: &Ui,
    state: &TableViewState,
    handles_input: bool,
    interaction: &mut TableInteraction,
) {
    let paste_from_event: Option<String> = if handles_input {
        ui.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Paste(text) = e {
                    Some(text.clone())
                } else {
                    None
                }
            })
        })
    } else {
        None
    };
    if let Some(text) = paste_from_event
        && state.editing_cell.is_none()
    {
        interaction.ctx_paste = true;
        interaction.paste_text = Some(text);
    }
}
