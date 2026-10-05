mod header;
mod input;
mod layout;
mod rows;
mod scrollbars;
mod split;
mod state;

use std::collections::{HashMap, HashSet};

use egui::{Color32, RichText, Sense, Ui, Vec2};

use super::shortcuts::Shortcuts;
use super::status_bar::format_number;
use super::theme::{ThemeColors, ThemeMode};
use crate::data::{BinaryDisplayMode, DataTable, MarkColor, MarkKey};

use layout::*;
pub use split::draw_table_split;

/// State for the table view (selection, editing).
#[derive(Default)]
pub struct TableViewState {
    /// Currently selected cell (row, col). None means no selection.
    pub selected_cell: Option<(usize, usize)>,
    /// Cell currently being edited, with its buffer.
    pub editing_cell: Option<(usize, usize, String)>,
    /// Whether the edit widget needs initial focus (set true when editing
    /// starts). Public because the Record view runs the same begin-edit /
    /// focus-once / commit-on-lost-focus cycle outside this module.
    pub edit_needs_focus: bool,
    /// Column widths (auto-sized initially, user can resize later).
    pub col_widths: Vec<f32>,
    /// Whether col_widths have been initialized.
    pub widths_initialized: bool,
    /// Column currently being resized (index), if any.
    resizing_col: Option<usize>,
    /// Vertical scroll offset in pixels (persisted across frames).
    scroll_y: f32,
    /// `(page offset, rows in the whole file)` when the vertical scrollbar
    /// stands for a file bigger than the loaded page (large-file mode).
    /// `None` everywhere else. See `set_virtual_rows`.
    virtual_rows: Option<(usize, usize)>,
    /// Row the virtual thumb is being dragged to, while the button is held.
    /// The jump fires on release: each one is a DuckDB query, and one per
    /// frame of a drag would stall the window it exists to keep responsive.
    virtual_drag_row: Option<f32>,
    /// A display row the viewport should be moved onto, honoured on the next
    /// frame. Callers outside this module cannot turn a row into a pixel
    /// offset - the row height is `(font_size * 2).max(26)` and rows carrying
    /// line breaks are taller still - so they name the row and `draw_table`
    /// does the arithmetic with the real measurements.
    pending_scroll_row: Option<usize>,
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
    /// Column header being renamed: (col_idx, current_buffer).
    pub editing_col_name: Option<(usize, String)>,
    /// Whether the column name edit widget needs initial focus.
    pub edit_col_needs_focus: bool,
    /// Dynamic row number column width (computed from total row count). When
    /// the sequential column is shown this is the *total* gutter width
    /// (original + sequential); `seq_number_width` holds just the sequential
    /// sub-column.
    pub row_number_width: f32,
    /// Width of the sequential (1..N) sub-column inside the row-number gutter,
    /// or 0.0 when it isn't shown. Always <= `row_number_width`.
    pub seq_number_width: f32,
    /// Heights the user dragged, keyed by **actual** row index (not display
    /// index, which moves with every filter and sort). Session-only, like
    /// `col_widths` and `frozen_cols`. Empty is the normal case, and while it
    /// is empty rows keep the uniform height.
    ///
    /// A non-empty map switches the variable-height path on even when word
    /// wrap is off - that path was built for wrapping and does the same job
    /// here.
    pub row_heights: std::collections::HashMap<usize, f32>,
    /// The row whose bottom seam is being dragged right now, by actual index.
    pub resizing_row: Option<usize>,
    /// Height for every row that has no entry in `row_heights`, set by dragging
    /// the seam under the "#" corner. `None` is the font-derived default. One
    /// value rather than an entry per row, so setting the height of an 11 M-row
    /// table stays free and the uniform fast path keeps working.
    pub uniform_row_height: Option<f32>,
    /// Cached prefix sums of row heights when cell_line_breaks is on.
    /// `[i]` = Y offset of display row i; `[row_count]` = total data height.
    row_y_offsets: Vec<f32>,
    /// Generation counter - bumped on any change that could affect row heights.
    row_heights_generation: u64,
    /// Generation at which the cache was last built.
    row_heights_cached_generation: u64,
    /// Pending request from the `FitAllColumns` shortcut. Drained on the next
    /// `draw_table` call, where a `Ui` is available for font measurement.
    pub fit_all_columns_requested: bool,
    /// Number of leading columns pinned to the left edge while scrolling
    /// horizontally (Excel-style freeze). 0 = nothing frozen. Set from the
    /// column-header context menu; session-only, like column widths.
    pub frozen_cols: usize,
    /// Which column's facet popup is open, if any. Session-only, like the
    /// column widths beside it.
    pub facet_col: Option<usize>,
    /// Search box inside the facet popup. Non-empty switches the frequency
    /// pass from "top N" to the whole distinct set, which is why it is the
    /// exception rather than what every open pays for.
    pub facet_search: String,
    /// Which values are ticked in the open popup. Seeded from the column's
    /// existing filter, or from every listed value when it has none.
    pub facet_ticked: std::collections::HashSet<String>,
    /// One-shot: the next render seeds `facet_ticked` from the live filter.
    /// Without it, "Select none" would be undone on the very next frame.
    pub facet_needs_seed: bool,
    /// Cached popup rows, the column's true distinct count, and the
    /// `(column, search)` they describe. The frequency pass walks the whole
    /// column, so redoing it every frame would make the popup unusable on a
    /// big table.
    pub facet_rows: Vec<(String, usize)>,
    pub facet_unique: usize,
    pub facet_cache_key: Option<(usize, String)>,
    /// The facet popup lists shapes (`A-9999`) instead of values. Session only.
    pub facet_shapes_mode: bool,
    /// Shapes of the popup's column: `(shape, count, example)`, most common first.
    pub facet_shape_rows: Vec<(String, usize, String)>,
    /// Which column `facet_shape_rows` belongs to.
    pub facet_shape_cache_col: Option<usize>,
    /// Ticked shapes; turned into a value allow-set on Apply.
    pub facet_shape_ticked: std::collections::HashSet<String>,
    /// The app fills the popup's value list from the server (a partial
    /// database tab): the popup does not count the loaded rows, it asks for
    /// `(column, search)` through `TableInteraction::facet_values_wanted`
    /// and lists what the app writes into `facet_rows`, `facet_unique` and
    /// `facet_cache_key`.
    pub facet_external: bool,
    /// Under the popup's title while `facet_external`: where the counts came
    /// from (`Ok`), or why there are none (`Err`, shown as a copyable error).
    pub facet_external_note: Option<Result<String, String>>,
    /// Optional per-column hover descriptions shown on the column header.
    /// Indexed by column. An empty vec (or an empty/short entry) means no
    /// tooltip. Used by the Summary tab to explain each statistic; empty
    /// everywhere else, so it has no effect on ordinary tables.
    pub header_tooltips: Vec<String>,
    /// Optional per-value hover descriptions shown on the *cells* of a column.
    /// Indexed by column, then keyed by the cell's exact text.
    ///
    /// For a column whose cells are a closed set of labels standing for a
    /// judgement: the quality report's `benford_verdict` and
    /// `calendar_verdict`, where a label short enough for a cell cannot also
    /// say what it means. The header tooltip describes the column; this
    /// describes the answer in front of you.
    ///
    /// Text cells only, which is what these columns hold. A number formats for
    /// display (separators, rounding), so keying a lookup on what is painted
    /// would depend on settings; nothing needs that, so nothing does it.
    /// Empty everywhere else, so ordinary tables are unaffected.
    pub cell_tooltips: Vec<std::collections::HashMap<String, String>>,
    /// How many panes the table view is cut into: 0 or 1 mean no split, up to
    /// [`MAX_SPLIT_PANES`] (View -> Split view / Split side by side, then Add
    /// pane). Everything except the scroll offsets is shared, because every
    /// pane reads this one state: columns, filters, sort, marks, edits and the
    /// selection. Session-only, per tab, like `frozen_cols`.
    split_panes: usize,
    /// Which way the dividers run: `false` = bands stacked one above the
    /// other, `true` = bands side by side.
    pub split_side_by_side: bool,
    /// Divider positions as fractions of the split axis, ascending, one per
    /// divider (`panes - 1`). Empty means evenly spaced, which is where a
    /// fresh split starts and where a changed pane count returns to.
    split_fractions: Vec<f32>,
    /// Scroll offsets of every pane but the first, `(x, y)`, indexed by
    /// `pane - 1`. `draw_table` only ever knows about `scroll_x` / `scroll_y`,
    /// so `split::draw_table_split` swaps a pane's pair in around its call and
    /// back out afterwards. The first pane keeps using `scroll_x` / `scroll_y`
    /// directly, which is what lets everything outside this module (paging,
    /// scroll-into-view) go on reading one offset and mean the pane the user
    /// is working in.
    ///
    /// **Both** axes are per pane. Sharing the cross axis lines rows or
    /// columns up neatly and makes the second pane show the same cells as the
    /// first, which is the one thing a split is for avoiding.
    pane_scroll: Vec<(f32, f32)>,
    /// Which pane the keyboard and the mouse wheel act on: the one the
    /// pointer was last over. Sticky, so moving the pointer onto a menu does
    /// not hand the arrow keys back to the first pane.
    active_pane: usize,
}

/// The most panes one table view can be cut into. Six 80-pixel bands need a
/// 500-pixel-tall panel before the minimum-size clamp starts squeezing them,
/// which a maximised window has and a small one does not; the clamp keeps
/// every band usable either way.
pub const MAX_SPLIT_PANES: usize = 6;

impl TableViewState {
    /// Whether the table view is currently split at all.
    pub fn is_split(&self) -> bool {
        self.split_panes > 1
    }

    /// How many panes are showing. 1 when there is no split.
    pub fn split_panes(&self) -> usize {
        self.split_panes.max(1)
    }

    /// Turn the split on or off in one orientation.
    ///
    /// The one way to set it, because a pane's offsets mean different things
    /// in each orientation: carried across unchanged, a pane scrolled to row
    /// 900,000 would reopen scrolled 20,000 pixels past the last column.
    /// Switching orientation therefore parks every pane where the first one
    /// is, which is also where a fresh split starts. The pane *count* is kept,
    /// so flipping four stacked bands to side by side gives four side-by-side
    /// bands rather than two.
    pub fn set_split(&mut self, on: bool, side_by_side: bool) {
        if !on || side_by_side != self.split_side_by_side {
            self.reset_pane_scroll();
        }
        self.split_panes = if on { self.split_panes.max(2) } else { 1 };
        self.split_side_by_side = side_by_side;
        self.sync_pane_vecs();
    }

    /// Add one pane, up to [`MAX_SPLIT_PANES`]. Returns false when the view is
    /// not split or is already at the cap, so the caller can say why nothing
    /// happened.
    pub fn add_split_pane(&mut self) -> bool {
        if !self.is_split() || self.split_panes >= MAX_SPLIT_PANES {
            return false;
        }
        self.split_panes += 1;
        // The dividers were placed for the old count, so a fifth band would
        // otherwise appear as a sliver at the end. Even spacing is the only
        // arrangement that means anything for a count nobody has dragged yet.
        self.split_fractions.clear();
        self.sync_pane_vecs();
        true
    }

    /// Drop one pane, down to two. Returns false when there is nothing to
    /// drop; turning the split off entirely is `set_split(false, ..)`.
    pub fn remove_split_pane(&mut self) -> bool {
        if self.split_panes <= 2 {
            return false;
        }
        self.split_panes -= 1;
        self.split_fractions.clear();
        self.sync_pane_vecs();
        true
    }

    /// Park every extra pane where the first one is.
    fn reset_pane_scroll(&mut self) {
        self.pane_scroll.fill((0.0, 0.0));
    }

    /// Keep the per-pane vectors the length the pane count implies, and the
    /// active pane inside it. Called from every path that changes the count,
    /// so the draw loop can index without checking.
    fn sync_pane_vecs(&mut self) {
        let extra = self.split_panes().saturating_sub(1);
        self.pane_scroll.resize(extra, (0.0, 0.0));
        if !self.split_fractions.is_empty() {
            self.split_fractions.resize(extra, 1.0);
        }
        self.active_pane = self.active_pane.min(extra);
    }
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

/// Grab band for the row-resize seam, centred on a row's bottom edge. Slightly
/// taller than the column handle's 6px because it is aimed at vertically.
pub(super) const ROW_RESIZE_HANDLE_HEIGHT: f32 = 7.0;

/// The interaction behind a row seam (per-row or the `#` corner): drag to
/// resize, click for the double-click fit, resize cursor while over it.
///
/// The drag is adopted from where the button went down. egui picks its drag
/// target from the pointer's position at the *end* of the frame, so a press
/// and a fast first move batched into one frame land past the 7px band on
/// the click-only row number, and no drag starts at all.
pub(super) fn seam_interact(ui: &Ui, seam: egui::Rect, id: egui::Id) -> egui::Response {
    let pressed_here = ui.input(|i| {
        i.pointer.primary_pressed() && i.pointer.press_origin().is_some_and(|p| seam.contains(p))
    });
    if pressed_here {
        ui.ctx().set_dragged_id(id);
    }
    let resp = ui.interact(seam, id, Sense::click_and_drag());
    if resp.hovered() || resp.dragged() || resp.is_pointer_button_down_on() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    resp
}

/// The height a row has when nobody has dragged it: two lines of the current
/// font, floored. A function because the header's corner seam needs the same
/// answer as [`draw_table`] to drag away from.
pub(super) fn base_row_height(font_size: f32) -> f32 {
    (font_size * 2.0).max(DEFAULT_ROW_HEIGHT)
}

/// Floor for a hand-set row height. Not `DEFAULT_ROW_HEIGHT`: a user shrinking
/// rows to fit more on screen is a real thing to want, and the draw loop walks
/// the prefix sums rather than assuming a uniform height, so shorter rows are
/// handled correctly.
pub(super) const MIN_ROW_HEIGHT: f32 = 12.0;

const SORT_ARROW_SIZE: f32 = 14.0;
/// Side of the drawn facet funnel in the column header, sized to sit beside
/// the sort arrows without crowding them.
const FACET_ICON_SIZE: f32 = 11.0;
/// Width of the funnel's CLICK area, which is deliberately wider than the
/// glyph and spans the header's content height.
///
/// The first version made the two the same 11px square and it was close to
/// unhittable in practice, wedged between two sort arrows and the column
/// resize handle. A drawn icon is a label; the target has to be a target.
const FACET_HIT_WIDTH: f32 = 22.0;
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
/// Numeric-display context needed to reproduce the cell text the renderer
/// paints (thousands separators + per-column rounding). Bundled so the autofit
/// helpers stay under clippy's argument-count limit; the three fields always
/// travel together.
#[derive(Clone, Copy)]
pub(crate) struct NumFmtCtx<'a> {
    pub thousands: bool,
    pub style: crate::data::num_format::SeparatorStyle,
    pub formats: &'a std::collections::HashMap<usize, crate::data::num_format::NumberFormat>,
}

/// Everything `draw_table` needs beyond the table and its view state.
///
/// The old signature took these as 25 positional parameters, ten of them bare
/// `bool`s in a row, behind an `#[allow(clippy::too_many_arguments)]`. At that
/// width a call site can hand `highlight_edits` to `clickable_links` and still
/// compile, producing a wrong grid with a green build. Same reasoning as
/// [`NumFmtCtx`] above, applied to the whole entry point.
#[derive(Clone, Copy)]
pub struct TableCtx<'a> {
    pub theme_mode: ThemeMode,
    pub filtered_rows: &'a [usize],
    pub show_row_numbers: bool,
    /// When true (filter active + setting on), draw a second row-number column
    /// counting the visible rows from 1, beside the original row numbers.
    pub show_sequential_numbers: bool,
    pub alternating_row_colors: bool,
    pub negative_numbers_red: bool,
    pub highlight_edits: bool,
    pub font_size: f32,
    pub cell_line_breaks: bool,
    /// Draw spaces, tabs and invisible characters inside cells as markers.
    pub show_invisibles: bool,
    /// Style cells that hold a web URL as a hyperlink and open on Ctrl+click.
    pub clickable_links: bool,
    pub binary_display_mode: BinaryDisplayMode,
    pub welcome_logo_texture: Option<&'a egui::TextureHandle>,
    pub shortcuts: &'a Shortcuts,
    pub readonly: bool,
    /// `None` when the tab's file is in a Git repository (the cell menu's
    /// **Cell history...** is enabled), else why not, for its hover.
    pub cell_history_unavailable: Option<&'a str>,
    /// Column indices that currently have an active per-column filter. Used
    /// only to paint the header dot marker; the actual row filtering is
    /// already applied in `filtered_rows`.
    /// Per-column allow-sets of values. The header reads it two ways: a key
    /// means "this column is filtered" (the accent dot and a lit funnel), and
    /// the values seed the facet popup's checkboxes so reopening it shows
    /// what is currently kept. Carrying the map rather than a derived set of
    /// keys means the two cannot fall out of step.
    pub column_filters: &'a HashMap<usize, HashSet<String>>,
    /// Column indices the user has hidden via right-click -> "Hide column".
    /// Hidden columns render with width 0 and skip paint entirely. Data
    /// stays in the table (Save / Save As writes them).
    pub hidden_columns: &'a HashSet<usize>,
    /// Whether numeric cells render with thousand separators.
    pub thousands_separators: bool,
    pub separator_style: crate::data::num_format::SeparatorStyle,
    pub column_number_formats:
        &'a std::collections::HashMap<usize, crate::data::num_format::NumberFormat>,
    pub search_matches: &'a HashSet<(usize, usize)>,
    pub current_match: Option<(usize, usize)>,
    pub conditional_format_rules: &'a [crate::data::conditional_format::CondRule],
    pub validation_violations: &'a HashSet<(usize, usize)>,
    pub outlier_cells: &'a HashSet<(usize, usize)>,
    /// Whether this call owns the keyboard, the mouse wheel and paste events.
    /// Always `true` for a whole-panel table; in split view only the pane the
    /// pointer was last over gets it, so one arrow press moves one selection
    /// and one wheel notch scrolls one band. See `split::draw_table_split`.
    pub handles_input: bool,
    /// Take the wheel even without [`Self::handles_input`]: the split view
    /// sets this on every pane while Alt is held, which is how one notch
    /// scrolls all the bands together. Each pane still clamps against its own
    /// content, so the shortest band stops at its own end rather than being
    /// dragged past it. Always `false` for a whole-panel table, which has
    /// nothing to keep in step.
    pub scroll_all: bool,
}

/// What the header and each data row need that does not change between them:
/// palette, geometry, cell styling and the overlay sets.
///
/// Built once inside [`draw_table`] and handed to `header::draw_header_direct`
/// and `rows::draw_data_row_direct`, which between them used to take 17 and 33
/// positional parameters sharing 21 of them.
#[derive(Clone, Copy)]
pub(super) struct PaintCtx<'a> {
    pub colors: ThemeColors,
    pub left_x: f32,
    pub top_y: f32,
    pub panel_rect: egui::Rect,
    pub font_size: f32,
    pub filtered_rows: &'a [usize],
    pub binary_display_mode: BinaryDisplayMode,
    pub column_filters: &'a HashMap<usize, HashSet<String>>,
    pub hidden_columns: &'a HashSet<usize>,
    pub num_fmt: NumFmtCtx<'a>,
    pub frozen_cols: usize,
    pub frozen_width: f32,
    pub show_row_numbers: bool,
    pub alternating_row_colors: bool,
    pub negative_numbers_red: bool,
    pub highlight_edits: bool,
    pub cell_line_breaks: bool,
    pub show_invisibles: bool,
    pub clickable_links: bool,
    pub readonly: bool,
    /// `None` when the tab's file is in a Git repository (the cell menu's
    /// **Cell history...** is enabled), else why not, for its hover.
    pub cell_history_unavailable: Option<&'a str>,
    pub is_rainbow_theme: bool,
    pub search_matches: &'a HashSet<(usize, usize)>,
    pub current_match: Option<(usize, usize)>,
    pub conditional_format_rules: &'a [crate::data::conditional_format::CondRule],
    pub validation_violations: &'a HashSet<(usize, usize)>,
    pub outlier_cells: &'a HashSet<(usize, usize)>,
}

/// Which row is being painted and where it lands.
///
/// `actual` and `display` are both `usize` and sat next to each other in the
/// old argument list. Swapped, every row would paint its neighbour's data at
/// its own position: no crash, no failing test, just a wrong grid.
#[derive(Clone, Copy)]
pub(super) struct RowSlot {
    /// Index into the underlying table.
    pub actual: usize,
    /// Index into the filtered/visible set.
    pub display: usize,
    pub y: f32,
    pub height: f32,
}

fn compute_optimal_col_width(
    ui: &Ui,
    table: &DataTable,
    filtered_rows: &[usize],
    col_idx: usize,
    font_size: f32,
    binary_display_mode: BinaryDisplayMode,
    num_fmt: NumFmtCtx<'_>,
) -> f32 {
    let mono = egui::FontId::new(font_size, egui::FontFamily::Monospace);
    let mut max_w: f32 = 0.0;

    // Numeric columns paint through `format_cell_number` (thousands separators
    // + per-column rounding), so measure that same string - otherwise long
    // numbers fit to their un-grouped width and get clipped.
    let col_numeric = table
        .columns
        .get(col_idx)
        .is_some_and(|c| crate::data::is_numeric_data_type(&c.data_type));
    let col_fmt = num_fmt.formats.get(&col_idx).copied();

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
            let text = if col_numeric {
                crate::data::num_format::format_cell_number(
                    value,
                    col_fmt,
                    num_fmt.thousands,
                    num_fmt.style,
                )
                .unwrap_or_else(|| value.display_with_binary_mode(binary_display_mode))
            } else {
                value.display_with_binary_mode(binary_display_mode)
            };
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

/// How many values the facet popup lists before it asks the user to search.
///
/// Fifty fills a tall popup without turning a 100k-cardinality column into a
/// scroll marathon, and the search box below covers everything else. It is
/// also what keeps the popup cheap to open: the frequency pass is asked for
/// the top N, not the whole distinct set.
pub const FACET_POPUP_TOP_N: usize = 50;

/// What the facet popup decided. Applied by the caller into
/// `TabState::column_filters`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacetPopupResult {
    /// Which column the filter belongs to.
    pub col: usize,
    /// The values to keep. Empty when `cleared`.
    pub allowed: std::collections::HashSet<String>,
    /// Remove the column's filter entirely rather than setting one.
    pub cleared: bool,
}

/// How many distinct values exist beyond the ones the popup is showing.
///
/// `unique_count` is the column's true distinct count even when `rows` was
/// truncated to the top N, which is exactly what lets the popup say "and 900
/// more" honestly instead of implying the list is everything.
pub fn facet_hidden_count(vf: &crate::data::value_frequency::ValueFrequency) -> usize {
    vf.unique_count.saturating_sub(vf.rows.len())
}

/// Turn a tick-set into a filter decision.
///
/// Two selections mean "no filter" rather than a filter: **everything**
/// ticked (an allow-set of every value hides nothing, but would still light
/// the header dot and the chip row, so the user would see a filter that does
/// nothing), and **nothing** ticked (an empty allow-set would hide every row
/// and leave the user staring at an empty table with no obvious way back).
/// Both clear instead.
///
/// `total_distinct` is the column's TRUE distinct count, not the number of
/// values the popup happens to be listing. Measuring against the listed rows
/// was wrong in both directions: a truncated list would read fifty ticks as
/// "everything" and clear a real filter, and a search that widened the list
/// would make the same fifty ticks look like a subset again.
pub fn facet_result(
    col: usize,
    ticked: std::collections::HashSet<String>,
    total_distinct: usize,
) -> FacetPopupResult {
    let cleared = ticked.is_empty() || ticked.len() >= total_distinct;
    FacetPopupResult {
        col,
        allowed: if cleared {
            std::collections::HashSet::new()
        } else {
            ticked
        },
        cleared,
    }
}

/// The key an external popup still waits for: `Some` while the app has not
/// answered for this column and search.
pub fn facet_values_needed(state: &TableViewState, col_idx: usize) -> Option<(usize, String)> {
    if !state.facet_external {
        return None;
    }
    let key = facet_request_key(state, col_idx);
    (state.facet_cache_key.as_ref() != Some(&key)).then_some(key)
}

/// The server value list the popup on `col_idx` shows: the search trimmed
/// (`"ap"` and `"ap "` are one query), none in Shapes mode, whose shapes come
/// from the loaded rows and only need the whole column's distinct count.
pub fn facet_request_key(state: &TableViewState, col_idx: usize) -> (usize, String) {
    let search = if state.facet_shapes_mode {
        ""
    } else {
        state.facet_search.trim()
    };
    (col_idx, search.to_string())
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
    /// Copy the current selection to the clipboard as a Markdown table.
    pub ctx_copy_markdown: bool,
    /// Right-click **Copy as IN list**: the selection as `('a', 'b')`.
    pub ctx_copy_in_list: bool,
    pub ctx_cut: bool,
    pub ctx_paste: bool,
    /// Text received from OS clipboard via Ctrl+V / Paste event
    pub paste_text: Option<String>,
    /// Column rename: (col_idx, new_name).
    pub rename_column: Option<(usize, String)>,
    /// Change column data type: (col_idx, new_type).
    pub change_col_type: Option<(usize, crate::data::retype::TargetType)>,
    /// Copy just the selected cell's value (not row/column selection).
    pub ctx_copy_cell: bool,
    /// Add a session bookmark for the right-clicked cell (its row + column).
    /// Fired by the cell context menu's "Add bookmark..." entry; handled via
    /// `begin_add_bookmark`, the same path as the toolbar Bookmarks dropdown
    /// and the `Ctrl+Alt+B` shortcut.
    pub ctx_add_bookmark: bool,
    /// A row seam (or the `#` corner) was double-clicked to fit rows to their
    /// content while cell line breaks were off. Fitting means wrapping, the
    /// same way it does for **Edit > Auto-fit All Rows**, so the app turns
    /// line breaks on.
    pub fit_rows_wants_wrap: bool,
    /// Signal that more rows should be loaded (scroll near bottom with truncated data).
    pub needs_more_rows: bool,
    /// Set a color mark on one or more keys. The list lets the right-click
    /// "Mark" submenu honour the current multi-selection (cells / rows /
    /// columns) instead of always colouring just the clicked target - the
    /// same precedence Ctrl+M follows via `mark_selection_default`.
    pub set_mark: Option<(Vec<MarkKey>, MarkColor)>,
    /// Clear a color mark from one or more keys.
    pub clear_mark: Option<Vec<MarkKey>>,
    /// Open the "Parse in new tab" modal for the selected scope.
    pub ctx_parse_in_new_tab: Option<super::toolbar::ParseScope>,
    /// Open Cell history for this (row, col); the row indexes the table, not
    /// the filtered view. Fired by the cell menu's **Cell history...**.
    pub ctx_cell_history: Option<(usize, usize)>,
    /// The facet popup was applied or cleared. Written to
    /// `TabState::column_filters` by the caller, exactly as the Column Filter
    /// modal writes it: this is a second door onto that state, never a second
    /// filter mechanism.
    pub facet_result: Option<FacetPopupResult>,
    /// An external facet popup needs the values for `(column, search)`.
    pub facet_values_wanted: Option<(usize, String)>,
    /// Hide a column from the table view. The data is preserved on disk
    /// (Save / Save As writes hidden columns too); only the renderer omits
    /// them. Cleared via Edit -> Show hidden columns.
    pub ctx_hide_column: Option<usize>,
    /// Open the Value Frequency dialog for this column. Fired by the
    /// column-header right-click menu's "Value frequency..." entry; the
    /// `ColumnValueFrequency` keyboard shortcut goes through
    /// `shortcuts_dispatch` instead.
    pub ctx_value_frequency: Option<usize>,
    /// Open the per-column Number-format dialog for this column. Fired by the
    /// column-header right-click menu's "Number format..." entry (numeric
    /// columns only).
    pub ctx_column_format: Option<usize>,
    /// The big logo on the welcome screen (rendered when the active tab has
    /// no columns) was just clicked. Counted by the snow easter egg -
    /// three within 1.5s triggers a 5-second snowfall.
    pub welcome_logo_clicked: bool,
    /// The virtual scrollbar was dragged or clicked to this row of the whole
    /// file. Only ever set when `TableViewState::set_virtual_rows` is on, i.e.
    /// in large-file mode, where it means "page the window over this row".
    pub jump_to_row: Option<usize>,
    /// Screen-space rect the welcome-screen logo image was painted into, if
    /// the welcome screen rendered this frame. Used by the Christmas easter
    /// egg to overlay a Santa hat at a position the binary side can compute
    /// without re-deriving the centred-image math.
    pub welcome_logo_rect: Option<egui::Rect>,
}

/// Geometry of the vertical scrollbar thumb when it stands for a whole file
/// rather than for the loaded rows (large-file mode).
pub(crate) struct VirtualThumb {
    /// Thumb height in pixels, floored so it stays grabbable.
    pub height: f32,
    /// Thumb top, in pixels below the track top.
    pub offset: f32,
    /// Pixels the thumb can travel: track height minus thumb height.
    pub travel: f32,
    /// Highest row the top of the viewport can sit on.
    pub max_row: f32,
}

/// Map "row `top_row` of `file_rows` is at the top of the viewport" onto a
/// thumb. Pure so the arithmetic can be checked without a `Ui`; the drag and
/// click handlers invert it through `travel` / `max_row`.
pub(crate) fn virtual_thumb(
    top_row: f32,
    file_rows: usize,
    visible_rows: f32,
    track_height: f32,
) -> VirtualThumb {
    let visible = visible_rows.max(1.0);
    let max_row = (file_rows as f32 - visible).max(1.0);
    let height = ((visible / file_rows.max(1) as f32) * track_height).clamp(24.0, track_height);
    let travel = (track_height - height).max(0.0);
    VirtualThumb {
        height,
        offset: (top_row / max_row).clamp(0.0, 1.0) * travel,
        travel,
        max_row,
    }
}

/// Draw the data table with true row virtualization.
pub fn draw_table(
    ui: &mut Ui,
    table: &mut DataTable,
    state: &mut TableViewState,
    cx: TableCtx<'_>,
) -> TableInteraction {
    // Destructured into locals with the original names so the body below is
    // untouched by the signature change.
    let TableCtx {
        theme_mode,
        filtered_rows,
        show_row_numbers,
        show_sequential_numbers,
        alternating_row_colors,
        negative_numbers_red,
        highlight_edits,
        font_size,
        cell_line_breaks,
        show_invisibles,
        clickable_links,
        binary_display_mode,
        welcome_logo_texture,
        shortcuts,
        readonly,
        cell_history_unavailable,
        scroll_all,
        column_filters,
        hidden_columns,
        thousands_separators,
        separator_style,
        column_number_formats,
        search_matches,
        current_match,
        conditional_format_rules,
        validation_violations,
        outlier_cells,
        handles_input,
    } = cx;
    let colors = ThemeColors::for_mode(theme_mode);
    let row_height = state
        .uniform_row_height
        .unwrap_or_else(|| base_row_height(font_size));
    state.ensure_widths(table);

    // Numeric-display context shared by autofit measurement (Ctrl+Shift+W and
    // the header-seam double-click), so widths match the painted cell text.
    let num_fmt_ctx = NumFmtCtx {
        thousands: thousands_separators,
        style: separator_style,
        formats: column_number_formats,
    };

    // Fulfil a pending FitAllColumns shortcut request now that we have a Ui
    // for font measurement.
    if state.fit_all_columns_requested {
        state.fit_all_columns_requested = false;
        state.fit_all_columns(
            ui,
            table,
            filtered_rows,
            font_size,
            binary_display_mode,
            num_fmt_ctx,
        );
    }

    // Compute row number column width based on the largest row number. When
    // the sequential column is shown, the gutter holds two numbers side by
    // side (original | 1..N); size each from its own largest value and sum.
    if show_row_numbers {
        let max_row_num = table.row_offset + filtered_rows.len();
        let orig_len = format_number(max_row_num).len() as f32;
        let orig_width = (orig_len * 8.0 + 16.0).max(MIN_ROW_NUMBER_WIDTH);
        if show_sequential_numbers {
            let seq_len = format_number(filtered_rows.len()).len() as f32;
            let seq_width = (seq_len * 8.0 + 12.0).max(MIN_ROW_NUMBER_WIDTH);
            state.seq_number_width = seq_width;
            state.row_number_width = orig_width + seq_width;
        } else {
            state.seq_number_width = 0.0;
            state.row_number_width = orig_width;
        }
    } else {
        state.seq_number_width = 0.0;
        state.row_number_width = 0.0;
    }

    let mut interaction = TableInteraction::default();

    if table.col_count() == 0 {
        // Rainbow easter-egg: paint a large, faded copy of the (now-random)
        // welcome icon as a background watermark behind the normal centred
        // icon. Only on the welcome screen - data views stay clean per the
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

    // A one-column table has nothing to scroll sideways to, so a column wider
    // than the window only pushes its own text off the right edge - the shape
    // a single JSON or log column always takes. Cap it to the window and let
    // the cell wrap instead.
    if table.col_count() == 1 {
        let gutter = state.row_number_width;
        let room =
            (ui.available_rect_before_wrap().width() - gutter - RESIZE_HANDLE_WIDTH - TRAILING_GAP)
                .max(MIN_COL_WIDTH);
        if let Some(w) = state.col_widths.first_mut()
            && *w > room
        {
            *w = room;
            state.invalidate_row_heights();
        }
    }

    let total_col_width: f32 = state.row_number_width
        + state.col_widths.iter().sum::<f32>()
        + RESIZE_HANDLE_WIDTH
        + TRAILING_GAP;
    let row_count = filtered_rows.len();

    let available_rect = ui.available_rect_before_wrap();
    let view_width = available_rect.width();
    let view_height = available_rect.height();

    // Effective frozen band for this frame. Clamped to the column count and
    // shrunk until at least ~100 px of scrollable viewport remains, so a
    // narrow window (or deep zoom) never leaves the table unscrollable; the
    // stored `state.frozen_cols` is untouched, so enlarging the window
    // restores the full band.
    let mut frozen_cols = state.frozen_cols.min(table.col_count());
    let mut frozen_width = frozen_band_width(&state.col_widths, hidden_columns, frozen_cols);
    while frozen_cols > 0 && frozen_width > (view_width - state.row_number_width - 100.0).max(0.0) {
        frozen_cols -= 1;
        frozen_width = frozen_band_width(&state.col_widths, hidden_columns, frozen_cols);
    }

    // Wrapping makes every row a different height; so does a height the user
    // dragged. Either one needs the prefix-sum path.
    let variable_heights = cell_line_breaks || !state.row_heights.is_empty();
    let total_data_height = if variable_heights {
        ensure_row_y_offsets(
            ui,
            state,
            table,
            filtered_rows,
            RowHeightOpts {
                font_size,
                base_row_height: row_height,
                binary_display_mode,
                wrap: cell_line_breaks,
            },
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
    // lands the last row exactly above the scrollbar - no slack, no clipping.
    let horizontal_scrollbar_visible = total_col_width > view_width - vscroll_width;
    let horizontal_scrollbar_height = if horizontal_scrollbar_visible {
        11.0
    } else {
        0.0
    };
    let total_content_height =
        HEADER_HEIGHT + 1.0 + total_data_height + horizontal_scrollbar_height;

    // A row someone else asked us to scroll onto, honoured now that the real
    // row height and viewport are known. Deliberately outside the keyboard
    // block below, which is skipped while a text field has focus: the request
    // comes from a page fetch, not from a keystroke, and must not be swallowed
    // because the search box happens to be focused.
    if handles_input && let Some(display_idx) = state.pending_scroll_row.take() {
        scroll_row_into_view(
            state,
            display_idx.min(row_count.saturating_sub(1)),
            row_height,
            (view_height - HEADER_HEIGHT - 1.0 - horizontal_scrollbar_height).max(0.0),
            (total_content_height - view_height).max(0.0),
        );
    }

    // Handle scroll input and keyboard shortcuts. `scroll_all` is the Alt-held
    // split case: every band takes the same notch, so the condition is an
    // either/or rather than two blocks that would apply it twice to the pane
    // that is also the active one.
    if handles_input || scroll_all {
        ui.input(|input| {
            let scroll_delta = input.smooth_scroll_delta;
            state.scroll_y = (state.scroll_y - scroll_delta.y)
                .clamp(0.0, (total_content_height - view_height).max(0.0));
            state.scroll_x = (state.scroll_x - scroll_delta.x)
                .clamp(0.0, (total_col_width + vscroll_width - view_width).max(0.0));
        });
    }

    // Keyboard navigation (see `input::handle_keyboard_nav`) stands down
    // while a text field has focus.
    let any_text_edit_focused = ui
        .ctx()
        .memory(|m| m.focused())
        .and_then(|id| egui::TextEdit::load_state(ui.ctx(), id).map(|_| ()))
        .is_some();
    if handles_input && state.editing_cell.is_none() && !any_text_edit_focused {
        let max_scroll_y = (total_content_height - view_height).max(0.0);
        let max_scroll_x = (total_col_width + vscroll_width - view_width).max(0.0);
        let data_area_height =
            (view_height - HEADER_HEIGHT - 1.0 - horizontal_scrollbar_height).max(0.0);
        input::handle_keyboard_nav(
            ui,
            state,
            table,
            filtered_rows,
            shortcuts,
            input::NavGeometry {
                row_height,
                data_area_height,
                max_scroll_y,
                max_scroll_x,
                view_width,
                frozen_cols,
                frozen_width,
            },
            &mut interaction,
        );
    }

    // A focused text box (the SQL editor, the search bar) already took the
    // paste; the table taking it as well pasted into the cells behind it.
    input::take_paste_event(
        ui,
        state,
        handles_input && !any_text_edit_focused,
        &mut interaction,
    );

    let (panel_rect, _) =
        ui.allocate_exact_size(Vec2::new(view_width, view_height), Sense::hover());

    let painter = ui.painter_at(panel_rect);

    // --- Draw header ---
    let header_y = panel_rect.top();
    // One context for the header and every data row: the things that do not
    // change between them. Built here so both painters read identical values.
    let paint_cx = PaintCtx {
        colors,
        left_x: panel_rect.left(),
        top_y: header_y,
        panel_rect,
        font_size,
        filtered_rows,
        binary_display_mode,
        column_filters,
        hidden_columns,
        num_fmt: num_fmt_ctx,
        frozen_cols,
        frozen_width,
        show_row_numbers,
        alternating_row_colors,
        negative_numbers_red,
        highlight_edits,
        cell_line_breaks,
        show_invisibles,
        clickable_links,
        readonly,
        cell_history_unavailable,
        is_rainbow_theme: theme_mode.is_rainbow(),
        search_matches,
        current_match,
        conditional_format_rules,
        validation_violations,
        outlier_cells,
    };

    header::draw_header_direct(ui, &painter, table, state, &mut interaction, &paint_cx);

    // Header bottom border
    let header_bottom = header_y + HEADER_HEIGHT;
    painter.line_segment(
        [
            egui::pos2(panel_rect.left(), header_bottom),
            egui::pos2(panel_rect.right(), header_bottom),
        ],
        egui::Stroke::new(1.0_f32, colors.border),
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

    let have_offsets = !state.row_y_offsets.is_empty();
    let (first_visible, first_visible_offset) = if have_offsets {
        let idx = row_at_offset(&state.row_y_offsets, state.scroll_y);
        (idx, state.row_y_offsets[idx])
    } else {
        let idx = (state.scroll_y / row_height).floor() as usize;
        (idx, idx as f32 * row_height)
    };
    // With variable heights the number of rows that fit is NOT
    // `area / row_height`: that estimate over-draws harmlessly for rows taller
    // than the base but drops rows off the bottom for shorter ones. Walk the
    // prefix sums instead - it costs one comparison per visible row.
    let last_visible = if have_offsets {
        let end_y = state.scroll_y + data_area_height;
        let mut idx = first_visible;
        while idx < row_count && state.row_y_offsets[idx] <= end_y {
            idx += 1;
        }
        (idx + 1).min(row_count)
    } else {
        let visible_count = (data_area_height / row_height).ceil() as usize + 2;
        (first_visible + visible_count).min(row_count)
    };

    let mut current_y = data_area_top + first_visible_offset - state.scroll_y;

    // clippy::needless_range_loop is wrong here and this is the one suppression
    // left in the tree. The index addresses three different slices at two
    // offsets - `filtered_rows[display_idx]` plus `state.row_y_offsets` at both
    // `display_idx` and `display_idx + 1` - so the iterator form clippy suggests
    // cannot express it without zipping the offsets against a shifted copy of
    // themselves, which is longer and harder to read than the index.
    #[allow(clippy::needless_range_loop)]
    for display_idx in first_visible..last_visible {
        let actual_row = filtered_rows[display_idx];

        let actual_row_height = if display_idx + 1 < state.row_y_offsets.len() {
            state.row_y_offsets[display_idx + 1] - state.row_y_offsets[display_idx]
        } else {
            row_height
        };

        // Row-resize seam: a thin strip on the row's bottom edge, inside the
        // row-number gutter. Mirrors the column seam in `header.rs` - drag to
        // set a height, double-click to fit the row to its content.
        if show_row_numbers && state.row_number_width > 0.0 {
            let seam = egui::Rect::from_min_max(
                egui::pos2(
                    panel_rect.left(),
                    current_y + actual_row_height - ROW_RESIZE_HANDLE_HEIGHT * 0.5,
                ),
                egui::pos2(
                    panel_rect.left() + state.row_number_width,
                    current_y + actual_row_height + ROW_RESIZE_HANDLE_HEIGHT * 0.5,
                ),
            );
            if seam.intersects(data_clip_rect) && handles_input {
                let resp = seam_interact(
                    ui,
                    seam.intersect(data_clip_rect),
                    ui.id().with(("row_resize", actual_row)),
                );
                if resp.drag_started() {
                    state.resizing_row = Some(actual_row);
                }
                if state.resizing_row == Some(actual_row) && resp.dragged() {
                    let h = (actual_row_height + resp.drag_delta().y).max(MIN_ROW_HEIGHT);
                    state.row_heights.insert(actual_row, h);
                    state.invalidate_row_heights();
                }
                if resp.drag_stopped() {
                    state.resizing_row = None;
                    state.invalidate_row_heights();
                }
                // Double-click fits the row to its content, like the column
                // seam: drop the hand-set height and let the offsets pass
                // measure the wrapped cells. Wrap off means nothing to
                // measure, so ask the app to turn it on.
                if resp.double_clicked() {
                    state.row_heights.remove(&actual_row);
                    state.invalidate_row_heights();
                    interaction.fit_rows_wants_wrap = !cell_line_breaks;
                }
            }
        }

        if current_y + actual_row_height >= data_area_top && current_y <= data_area_bottom {
            rows::draw_data_row_direct(
                ui,
                &data_painter,
                table,
                state,
                &mut interaction,
                &paint_cx,
                RowSlot {
                    actual: actual_row,
                    display: display_idx,
                    y: current_y,
                    height: actual_row_height,
                },
            );
        }

        current_y += actual_row_height;
    }

    // Separator at the frozen-band boundary so the pinned columns read as a
    // distinct region while the rest scrolls underneath.
    if frozen_cols > 0 {
        let sep_x = panel_rect.left() + state.row_number_width + frozen_width;
        painter.line_segment(
            [
                egui::pos2(sep_x, panel_rect.top()),
                egui::pos2(sep_x, data_area_bottom),
            ],
            egui::Stroke::new(2.0_f32, colors.border),
        );
    }

    let scroll_geometry = scrollbars::ScrollGeometry {
        panel_rect,
        view_width,
        view_height,
        row_height,
        row_count,
        total_content_height,
        total_col_width,
        vscroll_width,
    };
    scrollbars::draw_vertical_scrollbar(
        ui,
        &painter,
        state,
        &colors,
        &scroll_geometry,
        &mut interaction,
    );
    if horizontal_scrollbar_visible {
        scrollbars::draw_horizontal_scrollbar(ui, &painter, state, &colors, &scroll_geometry);
    }

    // Signal that more rows should be loaded when scrolled near the bottom
    if table.total_rows.is_some()
        && state.scroll_y + view_height >= total_content_height - row_height * 100.0
    {
        interaction.needs_more_rows = true;
    }

    interaction
}

/// Render the right-click "Mark" submenu.
///
/// `keys` is the full list of marks to apply when the user picks a colour -
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
    ui.menu_button(crate::i18n::t("edit_menu.mark"), |ui| {
        for &color in MarkColor::ALL {
            let swatch = ThemeColors::mark_swatch(color);
            let label = if current_mark == Some(color) {
                format!("{} ({})", color.label_t(), crate::i18n::t("mark.current"))
            } else {
                color.label_t()
            };
            let btn = egui::Button::new(RichText::new(label).color(swatch));
            if ui.add(btn).clicked() {
                interaction.set_mark = Some((keys.clone(), color));
                ui.close();
            }
        }
        if current_mark.is_some() {
            ui.separator();
            if ui.button(crate::i18n::t("edit_menu.clear")).clicked() {
                interaction.clear_mark = Some(keys.clone());
                ui.close();
            }
        }
    });
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
