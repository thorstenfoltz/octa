//! Tab lifecycle: per-tab state initialization, titles, and the top tab bar
//! that lets the user switch or close tabs.

use std::sync::Arc;

use eframe::egui;
use egui::Color32;

use octa::data::{self, DataTable, ViewMode};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{OctaApp, RawCsvEscape, RawCsvQuote, SearchNavState, TabState};

/// Thickness of the tab bar's horizontal scrollbar.
const SCROLL_BAR_WIDTH: f32 = 6.0;
/// Gap between the tab row and the scrollbar under it.
const SCROLL_BAR_INNER_MARGIN: f32 = 2.0;
/// Height the solid scrollbar claims for itself, added to the tab bar panel so
/// the bar sits cleanly beneath the tabs rather than across them.
const SCROLL_BAR_STRIP: f32 = SCROLL_BAR_WIDTH + SCROLL_BAR_INNER_MARGIN;

impl TabState {
    /// Whether Save can write this tab without asking for a path.
    ///
    /// True for a file-backed tab, and for a live-database tab whose Save is
    /// the write-back dialog. A db tab deliberately carries no `source_path`,
    /// so gating on that alone silently routes it to the Save As picker; that
    /// is the bug this predicate exists to prevent, and it is why every save
    /// entry point must ask this question here rather than inline.
    ///
    /// Not the same question as "has a file on disk", which still gates
    /// Reopen as and git compare: those genuinely need a file to re-read.
    pub(crate) fn saves_in_place(&self) -> bool {
        self.db_origin.is_some() || self.table.source_path.is_some()
    }

    /// Build the [`RowMatcher`](octa::data::search::RowMatcher) for this tab's
    /// current search text honouring the case-sensitive / whole-word toggles.
    pub(crate) fn search_matcher(&self) -> octa::data::search::RowMatcher {
        octa::data::search::RowMatcher::with_options(
            &self.search_text,
            self.search_mode,
            self.search_case_sensitive,
            self.search_whole_word,
        )
    }

    /// Keep the index-keyed view state pointing at the columns it was set on,
    /// returning whether anything had to move.
    ///
    /// `column_filters`, `predicate_filters`, `hidden_columns` and
    /// `column_number_formats` all address columns by index, and inserting,
    /// deleting, moving or reordering a column renumbers them. Nothing
    /// remapped them, so a filter set on `status` silently became a filter on
    /// whatever column landed on that index - and `filtered_rows` is exactly
    /// what a filtered **Save As** writes to disk.
    ///
    /// Rather than shifting at each of the ~20 places that mutate columns,
    /// this remembers the names the keys were set against and remaps the whole
    /// lot by name whenever they stop matching. One caller, in the frame loop,
    /// so no column-editing path can forget it.
    ///
    /// ponytail: a renamed column drops its filter instead of following the
    /// rename, and duplicate names resolve to the first match. Both are
    /// visible on screen; filtering the wrong column silently is not. Key the
    /// maps by name if that ever matters.
    pub(crate) fn sync_column_keys(&mut self) -> bool {
        let current: Vec<String> = self.table.columns.iter().map(|c| c.name.clone()).collect();
        if current == self.column_key_names {
            return false;
        }
        let remap: std::collections::HashMap<usize, usize> = self
            .column_key_names
            .iter()
            .enumerate()
            .filter_map(|(old, name)| current.iter().position(|n| n == name).map(|new| (old, new)))
            .collect();
        self.column_key_names = current;

        self.column_filters = std::mem::take(&mut self.column_filters)
            .into_iter()
            .filter_map(|(col, values)| remap.get(&col).map(|&new| (new, values)))
            .collect();
        self.column_number_formats = std::mem::take(&mut self.column_number_formats)
            .into_iter()
            .filter_map(|(col, fmt)| remap.get(&col).map(|&new| (new, fmt)))
            .collect();
        self.hidden_columns = self
            .hidden_columns
            .iter()
            .filter_map(|col| remap.get(col).copied())
            .collect();
        if let Some(snapshot) = self.mark_filter_hidden_snapshot.as_mut() {
            *snapshot = snapshot
                .iter()
                .filter_map(|c| remap.get(c).copied())
                .collect();
        }
        self.predicate_filters
            .retain_mut(|p| match remap.get(&p.col) {
                Some(&new) => {
                    p.col = new;
                    true
                }
                None => false,
            });
        true
    }

    pub(crate) fn new(search_mode: data::SearchMode) -> Self {
        Self {
            table: DataTable::empty(),
            assistant_modified: false,
            table_state: TableViewState::default(),
            search_text: String::new(),
            search_mode,
            search_case_sensitive: false,
            search_whole_word: false,
            search_scope_col: None,
            show_replace_bar: false,
            replace_text: String::new(),
            filtered_rows: Vec::new(),
            filter_dirty: true,
            search_nav: SearchNavState::default(),
            search_cell_matches: Vec::new(),
            view_mode: ViewMode::Table,
            raw_content: None,
            raw_content_modified: false,
            raw_content_original: None,
            raw_color_enabled: true,
            raw_file_size: None,
            raw_perf_prompt_resolved: false,
            raw_view_formatted: false,
            sheet_name: None,
            csv_delimiter: b',',
            raw_csv_quote: RawCsvQuote::default(),
            raw_csv_escape: RawCsvEscape::default(),
            bg_row_buffer: None,
            bg_loading_done: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            bg_can_load_more: false,
            bg_file_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            markdown_scroll_target: None,
            markdown_layout: data::MarkdownLayout::default(),
            markdown_render_cache: None,
            json_tree_expanded: std::collections::HashSet::new(),
            json_value: None,
            yaml_value: None,
            json_expand_depth: 1,
            json_expand_depth_str: "1".to_string(),
            json_file_max_depth: 0,
            json_edit_path: None,
            json_edit_buffer: String::new(),
            json_edit_width: None,
            tree_key_edit_path: None,
            tree_key_edit_buffer: String::new(),
            tree_add_key_path: None,
            tree_add_key_buffer: String::new(),
            show_add_column_dialog: false,
            new_col_name: String::new(),
            new_col_type: "String".to_string(),
            new_col_formula: String::new(),
            insert_col_at: None,
            insert_col_at_text: String::new(),
            show_delete_columns_dialog: false,
            delete_col_selection: Vec::new(),
            time_calc: None,
            sql_query: String::new(),
            sql_ask_input: String::new(),
            sql_result: None,
            sql_error: None,
            sql_result_selected: None,
            sql_panel_open: false,
            sql_run_on_server: false,
            sql_editor_focus_pending: false,
            sql_ac_selected: 0,
            sql_ac_visible: true,
            sql_workspace: None,
            sql_last_query: String::new(),
            sql_last_duration_ms: None,
            file_stamp: None,
            sql_history: Vec::new(),
            sql_diff_marks: Vec::new(),
            sql_diff_highlight_until: None,
            sql_workspace_open: false,
            sql_inspector_selection: None,
            sql_inspector_cache: std::collections::HashMap::new(),
            sql_workspace_tree_expanded: std::collections::HashSet::new(),
            sql_write_back: None,
            first_row_is_header: true,
            value_frequency_col: None,
            value_frequency_top_n: Some(50),
            value_frequency_bin_numeric: true,
            value_frequency_bins: None,
            value_frequency_bins_buf: String::new(),
            value_frequency_size: octa::ui::settings::DialogSize::default(),
            value_frequency_pick: false,
            column_number_formats: std::collections::HashMap::new(),
            conditional_format_rules: Vec::new(),
            show_conditional_format: false,
            conditional_format_size: ui::settings::DialogSize::default(),
            validation_rules: Vec::new(),
            validation_violations: std::collections::HashSet::new(),
            outlier_cells: std::collections::HashSet::new(),
            show_validation: false,
            validation_size: ui::settings::DialogSize::default(),
            column_format_col: None,
            column_format_cols: Vec::new(),
            column_format_decimals_buf: String::new(),
            show_find_duplicates: false,
            find_duplicates_key_cols: std::collections::HashSet::new(),
            find_duplicates_mode: super::state::FindDuplicatesMode::default(),
            duplicate_filter: None,
            duplicate_filter_cache: None,
            hidden_columns: std::collections::HashSet::new(),
            mark_filter_active: false,
            mark_filter_hidden_snapshot: None,
            bookmarks: Vec::new(),
            pinned: false,
            is_chart_tab: false,
            chart_tab_label: None,
            custom_tab_label: None,
            tab_hint: None,
            user_tab_name: None,
            column_filters: std::collections::HashMap::new(),
            column_key_names: Vec::new(),
            predicate_filters: Vec::new(),
            search_ask_mode: false,
            search_ask_profile: String::new(),
            sql_ask_profile: String::new(),
            show_column_filter: false,
            column_filter_size: octa::ui::settings::DialogSize::default(),
            column_filter_picker_col: None,
            column_filter_value_search: String::new(),
            column_filter_draft_allowed: std::collections::HashSet::new(),
            column_filter_needs_seed: false,
            empty_file_placeholder: false,
            parse_error_banner: None,
            compare_right_path: None,
            compare_right_raw: None,
            compare_right_table: None,
            compare_mode: data::CompareMode::default(),
            compare_columns_left: Vec::new(),
            compare_columns_right: Vec::new(),
            compare_error: None,
            epub_chapters_md: Vec::new(),
            epub_chapter_titles: Vec::new(),
            epub_image_bytes: std::collections::HashMap::new(),
            epub_image_textures: std::collections::HashMap::new(),
            epub_active_chapter: 0,
            epub_title: None,
            geojson_features: Vec::new(),
            map_coord_cols: None,
            map_mode: data::MapMode::default(),
            map_tiles: None,
            map_memory: None,
            chart_config: data::chart::ChartConfig::default(),
            chart_buffers: super::state::ChartInputBuffers::default(),
            cloud_origin: None,
            db_origin: None,
            compressed_origin: None,
            large: None,
            large_page_key: None,
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.table.is_modified() || self.raw_content_modified
    }

    /// Ordered list of view modes that make sense for this tab - same order
    /// as the View menu radio buttons. Used by the toolbar (gating which
    /// options are clickable) and by the `CycleViewMode` shortcut handler
    /// (advancing to the next available mode).
    pub(crate) fn available_view_modes(&self) -> Vec<ViewMode> {
        // Chart tabs are single-mode: the tab IS the chart, switching to
        // Table / Raw / anything else here would just confuse the user
        // (the tab has no source path, no readers, etc).
        if self.is_chart_tab {
            return vec![ViewMode::Chart];
        }
        let mut modes = Vec::new();
        let has_notebook = self.table.format_name.as_deref() == Some("Jupyter Notebook");
        let has_markdown = self.table.format_name.as_deref() == Some("Markdown");
        let has_epub = !self.epub_chapters_md.is_empty();
        // GeoJSON and Shapefile always offer Map (geometry comes from the
        // file); any other table offers it when it has detectable
        // latitude/longitude columns (plotted as points).
        let has_map = matches!(
            self.table.format_name.as_deref(),
            Some("GeoJSON") | Some("Shapefile")
        ) || octa::data::geo_detect::detect_lat_lon(&self.table).is_some();
        let has_json = self.json_value.is_some();
        let has_yaml = self.yaml_value.is_some();
        let has_raw = self.raw_content.is_some();

        if !has_notebook && !has_epub {
            modes.push(ViewMode::Table);
        } else if has_epub {
            // EPUBs still expose the flat paragraph table for searching /
            // exporting, just not as the default.
            modes.push(ViewMode::Table);
        }
        if has_raw {
            modes.push(ViewMode::Raw);
        }
        if has_markdown {
            modes.push(ViewMode::Markdown);
        }
        if has_notebook {
            modes.push(ViewMode::Notebook);
        }
        if has_epub {
            modes.push(ViewMode::EpubReader);
        }
        if has_map {
            modes.push(ViewMode::Map);
        }
        // Record view reads whatever the Table view reads, so it is offered
        // for any tab that has columns. Chart tabs returned early above.
        if self.table.col_count() > 0 {
            modes.push(ViewMode::Record);
        }
        // Chart is **not** in the View menu - it opens via the Analyse ->
        // Chart toolbar button as its own dedicated tab. Adding it here
        // would let the user mode-switch a data tab into a chart, which
        // breaks the tab-identity expectation (no source_path, no readers,
        // single view mode).
        if has_json {
            modes.push(ViewMode::JsonTree);
        }
        if has_yaml {
            modes.push(ViewMode::YamlTree);
        }
        if self.compare_right_path.is_some() {
            modes.push(ViewMode::Compare);
        }
        modes
    }

    /// Open the Number-format dialog for `col`, seeding the decimals text
    /// buffer from any existing format on that column.
    pub(crate) fn open_column_format(&mut self, col: usize) {
        self.column_format_decimals_buf = match self
            .column_number_formats
            .get(&col)
            .and_then(|f| f.decimals)
        {
            Some(n) => n.to_string(),
            None => String::new(),
        };
        self.column_format_col = Some(col);
        self.column_format_cols = vec![col];
    }

    pub(crate) fn title_display(&self) -> String {
        // A user-chosen name (via "Rename tab...") overrides every auto label.
        // For an editable file tab keep the " *" modified marker so unsaved
        // changes are still visible.
        if let Some(name) = &self.user_tab_name {
            if !self.is_chart_tab && self.custom_tab_label.is_none() && self.is_modified() {
                return format!("{} *", name);
            }
            return name.clone();
        }
        if self.is_chart_tab {
            return self
                .chart_tab_label
                .clone()
                .unwrap_or_else(|| "Chart".to_string());
        }
        if let Some(label) = &self.custom_tab_label {
            return label.clone();
        }
        let mut name = if let Some(ref path) = self.table.source_path {
            std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        } else {
            "Untitled".to_string()
        };
        // One workbook opened as several tabs: say which sheet each one is.
        if let Some(sheet) = &self.sheet_name {
            name = format!("{name} - {sheet}");
        }
        if self.is_modified() {
            format!("{} *", name)
        } else {
            name
        }
    }
}

impl OctaApp {
    /// Place a finished result tab: reuse the active tab while it is still
    /// completely blank, else push a new one and activate it.
    ///
    /// Octa starts with one empty tab and hides the tab bar until a second
    /// exists, so an unconditional push strands that empty tab as a visible
    /// "Untitled" beside the result. Same guard as `load_file_in_new_tab`.
    ///
    /// Only the result paths that can run with *no* file open need this (union
    /// from the sidebar, cloud inventory, a DB table opened from the tree). The
    /// others - chart, summary, pivot, join, ... - all require a loaded table,
    /// so their active tab is never blank.
    pub(crate) fn push_result_tab(&mut self, new_tab: super::state::TabState) {
        let blank = self
            .tabs
            .get(self.active_tab)
            .map(|t| t.table.col_count() == 0 && t.raw_content.is_none() && !t.is_modified())
            .unwrap_or(false);
        if blank {
            self.tabs[self.active_tab] = new_tab;
        } else {
            self.tabs.push(new_tab);
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Open a new chart tab seeded from the active tab's table.
    ///
    /// The new tab gets a deep clone of the table (so subsequent edits in
    /// the source don't drift the chart), an empty `ChartConfig`, and a
    /// title derived from the source filename. Triggered by the
    /// **Analyse -> Chart** toolbar button or the `OpenChart` shortcut.
    ///
    /// No-ops with a status message when there's no active table or the
    /// table has no numeric columns - charting either is useless and the
    /// rfd dialog cost would be wasted.
    /// Open the per-column Number-format dialog. Pre-selects every numeric
    /// column implied by the current selection (selected columns, plus the
    /// selected cell's column); with no numeric selection it still opens,
    /// seeded on the first numeric column so the user can pick targets from the
    /// dialog's "Apply to" list. Only no-ops (with a status hint) when the
    /// table has no numeric columns at all - rounding applies to numbers only.
    pub(crate) fn open_column_format_for_selection(&mut self) {
        let tab = &mut self.tabs[self.active_tab];
        if tab.table.col_count() == 0 {
            return;
        }
        // Numeric columns currently selected (a cell selection counts as its
        // column). The dialog only formats numeric columns.
        let mut cols: Vec<usize> = tab
            .table_state
            .selected_cols
            .iter()
            .copied()
            .chain(tab.table_state.selected_cell.map(|(_, c)| c))
            .filter(|&c| {
                c < tab.table.col_count()
                    && octa::data::is_numeric_data_type(&tab.table.columns[c].data_type)
            })
            .collect();
        cols.sort_unstable();
        cols.dedup();
        // Seed column: the first selected numeric column, else fall back to the
        // first numeric column in the table so the dialog opens even with
        // nothing (or a non-numeric column) selected.
        let col = cols.first().copied().or_else(|| {
            (0..tab.table.col_count())
                .find(|&c| octa::data::is_numeric_data_type(&tab.table.columns[c].data_type))
        });
        let Some(col) = col else {
            self.status_message = Some((
                "Number format applies to numeric columns only.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        };
        if cols.is_empty() {
            cols.push(col);
        }
        tab.open_column_format(col);
        tab.column_format_cols = cols;
    }

    pub(crate) fn open_chart_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            self.status_message = Some((
                "Open a file with columns before charting.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        if !octa::data::chart::has_numeric_column(&source.table) {
            self.status_message = Some((
                "Chart needs at least one numeric column.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());
        let chart_label = format!("Chart - {source_label}");

        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = source.table.clone();
        // Detach from disk: a chart tab can't be saved back over its
        // source - that would silently overwrite the user's data file.
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.table.structural_changes = false;
        new_tab.table.edits.clear();
        new_tab.filtered_rows = source.filtered_rows.clone();
        new_tab.is_chart_tab = true;
        new_tab.chart_tab_label = Some(chart_label);
        new_tab.view_mode = octa::data::ViewMode::Chart;

        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Open a Summary tab for the active table: one row per source column
    /// with min / max / approximate uniques / average / quartiles / null
    /// percentage, computed by DuckDB's `SUMMARIZE` over a snapshot that
    /// includes unsaved edits. Triggered by **Analyse -> Summary...**.
    ///
    /// The result is an ordinary detached table tab (sortable, filterable,
    /// exportable via Save As); it has no source path so it can never be
    /// saved over the original file by accident.
    pub(crate) fn open_describe_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            self.status_message = Some((
                "Open a file with columns before summarising.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let mut snap = source.table.clone();
        snap.apply_edits();
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());

        let enabled = self.settings.summary_stats.clone();
        // `build_summary_table` types its numeric columns as Int64 / Float64, so
        // the table view's normal numeric display path groups them per the
        // thousand-separator settings and right-aligns them like real numbers.
        let table = match octa::data::summary::build_summary_table(&snap, &enabled) {
            Ok(t) => t,
            Err(e) => {
                self.status_message =
                    Some((format!("Summary failed: {e}"), std::time::Instant::now()));
                return;
            }
        };

        // Header tooltips: one localized description per active statistic, in
        // the same column order the Summary table was built with.
        let header_tooltips: Vec<String> = octa::data::summary::active_stats(&enabled)
            .iter()
            .map(|s| octa::i18n::t(s.hint_key()))
            .collect();

        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.table_state.header_tooltips = header_tooltips;
        new_tab.custom_tab_label = Some(format!(
            "{} - {source_label}",
            octa::i18n::t("summary.tab_label")
        ));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Open the active file's physical layout in a detached read-only tab: one
    /// row per column per row group, with the file-level facts and any layout
    /// hints in the tab's banner. Needs a file on disk, since an in-memory or
    /// unsaved tab has no internals to report.
    pub(crate) fn open_file_internals_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        let Some(path) = source.table.source_path.clone() else {
            self.status_message = Some((
                octa::i18n::t("internals.needs_file"),
                std::time::Instant::now(),
            ));
            return;
        };
        let path = std::path::PathBuf::from(path);
        let source_label = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| source.title_display());

        let internals = match octa::data::file_internals::inspect(&path) {
            Ok(i) => i,
            Err(e) => {
                self.status_message = Some((
                    format!("{}: {e}", octa::i18n::t("internals.failed")),
                    std::time::Instant::now(),
                ));
                return;
            }
        };

        // Facts and hints ride above the grid in the tab's dismissible banner;
        // the grid itself is the row-group / column matrix.
        let mut banner = internals
            .facts
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("  |  ");
        for hint in &internals.hints {
            banner.push('\n');
            banner.push_str(hint);
        }

        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = internals.chunks;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!(
            "{} - {source_label}",
            octa::i18n::t("internals.tab_label")
        ));
        new_tab.parse_error_banner = Some(banner);
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Transpose the active table (rows <-> columns) into a detached tab. Capped
    /// at `TRANSPOSE_MAX_ROWS` source rows, since each row becomes a column.
    pub(crate) fn open_transpose_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            return;
        }
        if source.table.row_count() > octa::data::transpose::TRANSPOSE_MAX_ROWS {
            self.status_message = Some((
                octa::i18n::t("transpose.too_many_rows").replace(
                    "{n}",
                    &octa::data::transpose::TRANSPOSE_MAX_ROWS.to_string(),
                ),
                std::time::Instant::now(),
            ));
            return;
        }
        let mut snap = source.table.clone();
        snap.apply_edits();
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());

        let table = octa::data::transpose::transpose_table(&snap);
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!(
            "{} - {source_label}",
            octa::i18n::t("transpose.tab_label")
        ));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Compare the selected (or marked) rows of the active table field by
    /// field, in a detached tab. Same pattern as `open_transpose_tab`, which it
    /// reuses via `row_compare::compare_rows`.
    ///
    /// No row cap: the menu entry needs two rows and both selecting and marking
    /// are per-row gestures, so the count is whatever the user set by hand.
    pub(crate) fn open_row_compare_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            return;
        }
        let rows = octa::data::row_compare::rows_to_compare(
            &source.table,
            &source.table_state.selected_rows,
        );
        if rows.len() < 2 {
            return;
        }
        let mut snap = source.table.clone();
        snap.apply_edits();
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());

        let table = octa::data::row_compare::compare_rows(&snap, &rows);
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.custom_tab_label = Some(format!(
            "{} - {source_label}",
            octa::i18n::t("row_compare.tab_label")
        ));
        new_tab.tab_hint = Some(octa::i18n::t("row_compare.tab_hint"));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Open a detached tab holding `n` randomly chosen rows from the active
    /// table (all rows if `n` exceeds the row count). Same pattern as
    /// `open_describe_tab`.
    /// File -> New Table...: a blank editable grid of `cols` x `rows` in a new
    /// tab, columns named `col1..colN`, no source path so Save asks where.
    pub(crate) fn open_new_table_tab(&mut self, cols: usize, rows: usize) {
        let mut table = octa::data::DataTable::empty();
        table.columns = (1..=cols)
            .map(|i| octa::data::ColumnInfo {
                name: format!("col{i}"),
                data_type: "Utf8".to_string(),
            })
            .collect();
        table.rows = vec![vec![octa::data::CellValue::Null; cols]; rows];
        table.structural_changes = true;
        let mut new_tab = super::state::TabState::new(self.settings.default_search_mode);
        new_tab.table = table;
        new_tab.filter_dirty = true;
        if rows > 0 {
            new_tab.table_state.selected_cell = Some((0, 0));
        }
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    pub(crate) fn open_random_sample_tab(&mut self, n: usize) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            return;
        }
        let mut snap = source.table.clone();
        snap.apply_edits();
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let table = octa::data::sample::sample_table(&snap, n, seed);
        let rows = table.row_count();

        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!(
            "{} - {source_label}",
            octa::i18n::t("sample.tab_label").replace("{n}", &rows.to_string())
        ));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Run the chosen tidy-up passes on the active table as one undoable step:
    /// trim whitespace from string cells + titles, and/or snake_case the column
    /// names. Reuses the same engines as the on-load passes, applied through the
    /// undoable edit path so a single Undo reverts everything.
    pub(crate) fn apply_tidy_up(&mut self, trim: bool, headers: bool) {
        if self.is_readonly() {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        // Compute the target state on a clone using the existing engines, then
        // apply the diff to the live table via set() / rename_column() so it is
        // undoable. Materialise pending edits on both so indices line up.
        let mut target = tab.table.clone();
        target.apply_edits();
        if trim {
            octa::data::trim::trim_string_columns(&mut target);
        }
        if headers {
            octa::data::trim::clean_headers(&mut target);
        }
        tab.table.apply_edits();

        let start = tab.table.undo_stack.len();
        let rows = tab.table.row_count();
        let cols = tab.table.col_count().min(target.col_count());
        let mut changed = 0usize;
        for c in 0..cols {
            for r in 0..rows {
                let new_val = target.get(r, c).cloned();
                if let Some(new_val) = new_val
                    && tab.table.get(r, c) != Some(&new_val)
                {
                    tab.table.set(r, c, new_val);
                    changed += 1;
                }
            }
        }
        for c in 0..cols {
            let new_name = target.columns[c].name.clone();
            if tab.table.columns[c].name != new_name {
                tab.table.rename_column(c, new_name);
                changed += 1;
            }
        }
        tab.table.apply_edits();
        tab.table.coalesce_undo_since(start);
        // Keep the DB diff-save baseline in step so a later save is not rejected
        // as a schema change (same as bulk rename / clean-headers-on-load).
        crate::app::file_io::resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;

        self.status_message = Some((
            octa::i18n::t("tidyup.done").replace("{n}", &changed.to_string()),
            std::time::Instant::now(),
        ));
    }

    /// Build a data-quality report for the active table and open it as a
    /// detached tab (same pattern as `open_describe_tab`). Surfaces the overall
    /// score in the status bar.
    pub(crate) fn open_quality_tab(&mut self) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            self.status_message = Some((
                "Open a file with columns first.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let mut snap = source.table.clone();
        snap.apply_edits();
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());

        let report = match octa::data::quality::build_quality_report(&snap) {
            Ok(r) => r,
            Err(e) => {
                self.status_message = Some((
                    format!("Quality report failed: {e}"),
                    std::time::Instant::now(),
                ));
                return;
            }
        };

        // Header tooltips: one localized hint per report column, in the same
        // order the engine emitted them.
        let header_tooltips: Vec<String> = octa::data::quality::quality_column_hint_keys()
            .iter()
            .map(|k| octa::i18n::t(k))
            .collect();

        // Cell tooltips: the two verdict columns hold a short label standing
        // for a judgement, so hovering one says what that judgement means and
        // what to do about it. Every other column is empty here.
        let cell_tooltips: Vec<std::collections::HashMap<String, String>> =
            octa::data::quality::quality_column_value_hint_keys()
                .iter()
                .map(|legend| {
                    legend
                        .iter()
                        .map(|(value, key)| ((*value).to_string(), octa::i18n::t(key)))
                        .collect()
                })
                .collect();

        let overall = report.overall_score.round() as i64;
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = report.table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.table_state.header_tooltips = header_tooltips;
        new_tab.table_state.cell_tooltips = cell_tooltips;
        // The score in the tab label, not only in a status message that fades:
        // it is the headline of the whole report, and hovering the `score`
        // column header explains how it is arrived at.
        new_tab.custom_tab_label = Some(format!(
            "{} {overall}/100 - {source_label}",
            octa::i18n::t("quality.tab_label")
        ));
        new_tab.tab_hint = Some(octa::i18n::t("quality.overall_explained"));
        self.tabs.push(new_tab);
        let main_tab = self.tabs.len() - 1;

        // Findings that are not one row per column get a tab of their own,
        // next to the main report. Focus stays on the main tab: the sections
        // are extra detail, not the headline, and landing on the last one
        // would hide the score the user asked for.
        let section_count = report.sections.len();
        for section in report.sections {
            let mut tab = super::state::TabState::new(default_search_mode);
            tab.table = section.table;
            tab.table.source_path = None;
            tab.table.format_name = None;
            tab.table_state.header_tooltips =
                section.hint_keys.iter().map(|k| octa::i18n::t(k)).collect();
            tab.tab_hint = Some(octa::i18n::t(&section.intro_key));
            tab.custom_tab_label = Some(format!(
                "{} - {source_label}",
                octa::i18n::t(&section.title_key)
            ));
            self.tabs.push(tab);
        }
        self.active_tab = main_tab;

        let score = format!(
            "{}: {}/100. {}",
            octa::i18n::t("quality.overall"),
            overall,
            octa::i18n::t("quality.overall_explained")
        );
        self.status_message = Some((
            if section_count == 0 {
                score
            } else {
                // `score` already ends in a full stop: the explanation is a
                // sentence, not a bare number, so no second one is added.
                format!(
                    "{score} {}",
                    octa::i18n::t("quality.sections_added")
                        .replace("{n}", &section_count.to_string())
                )
            },
            std::time::Instant::now(),
        ));
    }

    /// Compute a correlation matrix over the active table's numeric columns and
    /// open it as a detached tab (same pattern as `open_describe_tab`).
    pub(crate) fn open_correlation_tab(&mut self, method: octa::data::correlation::CorrMethod) {
        let Some(source) = self.tabs.get(self.active_tab) else {
            return;
        };
        if source.table.col_count() == 0 {
            self.status_message = Some((
                "Open a file with columns first.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        let mut snap = source.table.clone();
        snap.apply_edits();
        let source_label = source
            .table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| source.title_display());

        let matrix = octa::data::correlation::correlation_matrix(&snap, method);
        if matrix.columns.is_empty() {
            self.status_message = Some((
                octa::i18n::t("dialog.corr_no_numeric"),
                std::time::Instant::now(),
            ));
            return;
        }
        let table = octa::data::correlation::matrix_to_table(&matrix);
        let method_label = match method {
            octa::data::correlation::CorrMethod::Pearson => octa::i18n::t("dialog.corr_pearson"),
            octa::data::correlation::CorrMethod::Spearman => octa::i18n::t("dialog.corr_spearman"),
        };
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!(
            "{} ({method_label}) - {source_label}",
            octa::i18n::t("dialog.corr_tab_label")
        ));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// Compare two columns' distributions and open the answer as a detached
    /// tab, the same shape as the correlation matrix above.
    pub(crate) fn open_dist_compare_tab(
        &mut self,
        tab_a: usize,
        col_a: usize,
        tab_b: usize,
        col_b: usize,
    ) {
        let (Some(a), Some(b)) = (self.tabs.get(tab_a), self.tabs.get(tab_b)) else {
            return;
        };
        // Snapshots with edits applied, so the comparison sees what the grid
        // shows rather than what the file held.
        let (mut sa, mut sb) = (a.table.clone(), b.table.clone());
        sa.apply_edits();
        sb.apply_edits();
        let label_a = sa
            .columns
            .get(col_a)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let label_b = sb
            .columns
            .get(col_b)
            .map(|c| c.name.clone())
            .unwrap_or_default();

        let outcome = octa::data::distribution_compare::compare_columns(&sa, col_a, &sb, col_b);
        let table = octa::data::distribution_compare::result_table(&label_a, &label_b, &outcome);

        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!(
            "{} - {label_a} / {label_b}",
            octa::i18n::t("distcmp.tab_label")
        ));
        new_tab.tab_hint = Some(octa::i18n::t("distcmp.tab_hint"));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;

        // The headline is the answer; the status bar says it so it is visible
        // before the reader has parsed the table.
        if let octa::data::distribution_compare::Outcome::Compared(c) = &outcome {
            self.status_message = Some((c.headline.sentence(), std::time::Instant::now()));
        }
    }

    /// Check a foreign key and open the orphans as a detached tab.
    ///
    /// A clean check opens no tab: there is nothing to look at, and the status
    /// bar saying so is the whole answer.
    pub(crate) fn open_referential_tab(
        &mut self,
        parent_tab: usize,
        parent_col: usize,
        child_tab: usize,
        child_col: usize,
    ) {
        let (Some(p), Some(c)) = (self.tabs.get(parent_tab), self.tabs.get(child_tab)) else {
            return;
        };
        let (mut sp, mut sc) = (p.table.clone(), c.table.clone());
        sp.apply_edits();
        sc.apply_edits();
        let child_label = sc
            .columns
            .get(child_col)
            .map(|col| col.name.clone())
            .unwrap_or_default();
        let parent_label = sp
            .columns
            .get(parent_col)
            .map(|col| col.name.clone())
            .unwrap_or_default();

        let report = octa::data::referential::check(&sp, parent_col, &sc, child_col);
        let sentence = report.sentence();
        if report.is_clean() {
            self.status_message = Some((sentence, std::time::Instant::now()));
            return;
        }

        let table = octa::data::referential::report_table(&child_label, &report);
        let default_search_mode = self.settings.default_search_mode;
        let mut new_tab = super::state::TabState::new(default_search_mode);
        new_tab.table = table;
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label = Some(format!(
            "{} - {child_label} -> {parent_label}",
            octa::i18n::t("refint.tab_label")
        ));
        new_tab.tab_hint = Some(octa::i18n::t("refint.tab_hint"));
        // The counts do not fit a column, so they ride the tab's notice banner
        // the way the dataset and SQL-dump notices do.
        new_tab.parse_error_banner = Some(sentence.clone());
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
        self.status_message = Some((sentence, std::time::Instant::now()));
    }

    pub(crate) fn close_tab(&mut self, idx: usize) {
        // Pinned tabs refuse to close. The user has to unpin them from the
        // tab right-click context menu first; the status bar tells them.
        if self.tabs.get(idx).is_some_and(|t| t.pinned) {
            self.status_message = Some((
                "Tab is pinned; unpin from the tab right-click menu first.".to_string(),
                std::time::Instant::now(),
            ));
            return;
        }
        // Take a snapshot before removal so Ctrl+Shift+T can restore it.
        // Skip wholly empty tabs (no source path, no raw content, no
        // columns) - those would just be re-created empty.
        if let Some(tab) = self.tabs.get(idx) {
            let snapshot = if let Some(ref p) = tab.table.source_path {
                Some(super::state::ClosedTabSnapshot::Path(
                    std::path::PathBuf::from(p),
                ))
            } else if let Some(ref content) = tab.raw_content {
                if !content.is_empty() || tab.table.col_count() > 0 {
                    Some(super::state::ClosedTabSnapshot::Scratch {
                        raw_content: content.clone(),
                        view_mode: tab.view_mode,
                        format_name: tab.table.format_name.clone(),
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(snap) = snapshot {
                if self.recently_closed_tabs.len() >= super::state::MAX_CLOSED_TAB_HISTORY {
                    self.recently_closed_tabs.pop_front();
                }
                self.recently_closed_tabs.push_back(snap);
            }
        }

        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs
                .push(TabState::new(self.settings.default_search_mode));
        }
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Toggle the pinned state for the tab at `idx`. Pinning a file-backed
    /// tab adds its absolute path to `AppSettings.pinned_tabs` so the file
    /// re-opens on next launch; unpinning removes it. Settings are saved
    /// immediately so the change survives a crash.
    ///
    /// Pinning a scratch tab (no `source_path`) is a no-op - the UI already
    /// greys out the menu entry for those.
    pub(crate) fn toggle_tab_pinned(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        let Some(path) = tab.table.source_path.clone() else {
            return;
        };
        tab.pinned = !tab.pinned;
        let now_pinned = tab.pinned;
        let pinned_list = &mut self.settings.pinned_tabs;
        if now_pinned {
            if !pinned_list.contains(&path) {
                pinned_list.push(path);
            }
        } else {
            pinned_list.retain(|p| p != &path);
        }
        self.settings.save();
    }

    /// Restore the most-recently-closed tab (Ctrl+Shift+T). Path-backed tabs
    /// reload through the standard `load_file` pipeline; scratch tabs
    /// recreate from the stored raw_content. No-op when the close stack
    /// is empty.
    pub(crate) fn reopen_last_closed_tab(&mut self, ctx: &egui::Context) {
        let Some(snap) = self.recently_closed_tabs.pop_back() else {
            return;
        };
        match snap {
            super::state::ClosedTabSnapshot::Path(path) => {
                self.load_file(path);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
            super::state::ClosedTabSnapshot::Scratch {
                raw_content,
                view_mode,
                format_name,
            } => {
                let mut tab = TabState::new(self.settings.default_search_mode);
                tab.raw_content = Some(raw_content);
                tab.raw_content_original = tab.raw_content.clone();
                tab.view_mode = view_mode;
                tab.table.format_name = format_name;
                self.tabs.push(tab);
                self.active_tab = self.tabs.len() - 1;
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                    self.tabs[self.active_tab].title_display(),
                ));
            }
        }
    }

    /// Render the top tab bar (only shown when at least one file is open).
    pub(crate) fn render_tab_bar(&mut self, parent_ui: &mut egui::Ui) {
        let has_open_file = self.tabs.iter().any(|t| {
            t.table.source_path.is_some() || t.raw_content.is_some() || t.table.col_count() > 0
        });
        if !has_open_file {
            return;
        }
        let ctx = parent_ui.ctx().clone();
        let ctx = &ctx;
        let colors = ui::theme::ThemeColors::for_mode(self.theme_mode);
        let tab_frame = egui::Frame::new()
            .fill(colors.bg_secondary)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .stroke(egui::Stroke::new(1.0_f32, colors.border_subtle));
        egui::Panel::top("tab_bar")
            // Tall enough for a tab row (28) plus the scrollbar strip beneath
            // it (`SCROLL_BAR_STRIP`), so the bar never sits on top of the tab
            // labels.
            .exact_size(28.0 + SCROLL_BAR_STRIP)
            .frame(tab_frame)
            .show(parent_ui, |ui| {
                // Scroll the row sideways once the tabs outrun the window,
                // instead of clipping the overflow out of reach. egui only
                // routes a plain (vertical) mouse wheel to a horizontal-only
                // scroll area when `always_scroll_the_only_direction` is set;
                // without it the wheel would need Shift held down here.
                ui.style_mut().always_scroll_the_only_direction = true;
                // egui's default scrollbars *float*: they are drawn on top of
                // the content, which left the bar lying across the tab labels.
                // A solid bar claims its own strip below them instead.
                let mut scroll_style = egui::style::ScrollStyle::solid();
                scroll_style.bar_inner_margin = SCROLL_BAR_INNER_MARGIN;
                scroll_style.bar_outer_margin = 0.0;
                scroll_style.bar_width = SCROLL_BAR_WIDTH;
                ui.style_mut().spacing.scroll = scroll_style;
                egui::ScrollArea::horizontal()
                    .id_salt("tab_bar_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let mut tab_to_close: Option<usize> = None;
                            let mut tab_to_activate: Option<usize> = None;
                            // Set when the user picks "Compare with active tab" from
                            // a tab's right-click context menu.
                            let mut tab_to_compare_with: Option<usize> = None;
                            // Set when the user picks "Pin tab" / "Unpin tab".
                            let mut tab_to_toggle_pin: Option<usize> = None;
                            // Set when the user picks "Rename tab..." from the menu.
                            let mut tab_to_rename: Option<usize> = None;
                            let mut tab_to_export_pdf: Option<usize> = None;

                            for (idx, tab) in self.tabs.iter().enumerate() {
                                let is_active = idx == self.active_tab;
                                let is_multi_selected = self.tab_multi_selection.contains(&idx);
                                let full_label = tab.title_display();
                                let raw_label = full_label.clone();
                                // 📌 prefix marks pinned tabs at a glance. U+1F4CC,
                                // supplementary plane - covered by the bundled
                                // NotoEmoji font.
                                let label = if tab.pinned {
                                    format!("\u{1f4cc} {}", raw_label)
                                } else {
                                    raw_label
                                };
                                let pinned = tab.pinned;
                                let has_source = tab.table.source_path.is_some();
                                let has_data = tab.table.col_count() > 0;
                                // Hover shows the full path when file-backed, else the
                                // full (untruncated) tab title, so a shortened title is
                                // always recoverable on hover.
                                let hover_path = tab.tab_hint.clone().unwrap_or_else(|| {
                                    tab.table
                                        .source_path
                                        .clone()
                                        .unwrap_or_else(|| full_label.clone())
                                });

                                // Distinct visual states: active uses the accent at
                                // 30% alpha; Ctrl-click-selected (but not active) uses
                                // the accent at 15% so users see which tabs they
                                // staged for compare without confusing them with the
                                // active one.
                                let bg = if is_active {
                                    colors.accent.gamma_multiply(0.3)
                                } else if is_multi_selected {
                                    colors.accent.gamma_multiply(0.15)
                                } else {
                                    Color32::TRANSPARENT
                                };

                                let frame = egui::Frame::new()
                                    .fill(bg)
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .corner_radius(4.0);

                                frame.show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let text = if is_active {
                                            egui::RichText::new(&label)
                                                .strong()
                                                .color(colors.text_primary)
                                        } else {
                                            egui::RichText::new(&label).color(colors.text_secondary)
                                        };
                                        let tab_label_resp = ui
                                            .add(egui::Label::new(text).sense(egui::Sense::click()))
                                            .on_hover_text(&hover_path);
                                        if tab_label_resp.hovered() {
                                            ctx.set_cursor_icon(egui::CursorIcon::Default);
                                        }
                                        // Right-click context menu - "Compare with
                                        // active tab" only makes sense on a non-active
                                        // tab; "Pin tab" / "Unpin tab" applies to any
                                        // tab (active or not) but only file-backed
                                        // ones (scratch tabs have nowhere to persist).
                                        tab_label_resp.context_menu(|ui| {
                                            if !is_active
                                                && ui
                                                    .button(octa::i18n::t(
                                                        "context_menu.compare_active",
                                                    ))
                                                    .clicked()
                                            {
                                                tab_to_compare_with = Some(idx);
                                                ui.close();
                                            }
                                            let pin_label =
                                                if pinned { "Unpin tab" } else { "Pin tab" };
                                            // Size the button to the wider of the two
                                            // labels (plus padding) so the context-menu
                                            // entry stays the same width whether the
                                            // tab is pinned or not. Without this the
                                            // button shrink-wraps "Pin tab" and looks
                                            // cramped when the user right-clicks a
                                            // pinned tab.
                                            let pin_btn = ui.add_enabled(
                                                has_source,
                                                egui::Button::new(pin_label)
                                                    .min_size(egui::vec2(140.0, 0.0)),
                                            );
                                            let pin_btn = if !has_source {
                                                pin_btn.on_disabled_hover_text(
                                            "Pinning is for file-backed tabs; save the tab first.",
                                        )
                                            } else {
                                                pin_btn
                                            };
                                            if pin_btn.clicked() {
                                                tab_to_toggle_pin = Some(idx);
                                                ui.close();
                                            }
                                            if ui
                                                .button(octa::i18n::t("context_menu.rename_tab"))
                                                .clicked()
                                            {
                                                tab_to_rename = Some(idx);
                                                ui.close();
                                            }
                                            // Every result tab (Summary, the
                                            // Quality report, a compare) is a
                                            // table, so the PDF export belongs
                                            // on all of them, not just the
                                            // file-backed ones.
                                            if ui
                                                .add_enabled(
                                                    has_data,
                                                    egui::Button::new(octa::i18n::t(
                                                        "context_menu.export_pdf",
                                                    )),
                                                )
                                                .on_hover_text(octa::i18n::t(
                                                    "file_menu.export_pdf_hint",
                                                ))
                                                .on_disabled_hover_text(octa::i18n::t(
                                                    "file_menu.export_pdf_hint",
                                                ))
                                                .clicked()
                                            {
                                                tab_to_export_pdf = Some(idx);
                                                ui.close();
                                            }
                                        });
                                        if tab_label_resp.clicked() {
                                            // Ctrl-click toggles multi-selection
                                            // without changing the active tab. Plain
                                            // click activates and clears the staged
                                            // selection.
                                            let cmd_held = ctx.input(|i| i.modifiers.command);
                                            if cmd_held && !is_active {
                                                if is_multi_selected {
                                                    self.tab_multi_selection.remove(&idx);
                                                } else {
                                                    self.tab_multi_selection.insert(idx);
                                                }
                                            } else {
                                                tab_to_activate = Some(idx);
                                                self.tab_multi_selection.clear();
                                            }
                                        }
                                        // Close button (hidden on pinned tabs - the
                                        // user has to unpin first via the right-click
                                        // context menu). The leading spacing lives
                                        // outside the label so the response rect tightly
                                        // hugs the × glyph - that way the hover overlay
                                        // (painted at rect.center) sits exactly where
                                        // egui drew the original glyph and no horizontal
                                        // shift is visible on hover.
                                        if !pinned {
                                            ui.add_space(6.0);
                                            let close_resp = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new("\u{00D7}")
                                                        .size(14.0)
                                                        .color(colors.text_muted),
                                                )
                                                .sense(egui::Sense::click() | egui::Sense::hover()),
                                            );
                                            if close_resp.hovered() {
                                                ctx.set_cursor_icon(egui::CursorIcon::Default);
                                                let r =
                                                    close_resp.rect.expand2(egui::vec2(3.0, 1.0));
                                                ui.painter().rect_filled(
                                                    r,
                                                    3.0,
                                                    colors.accent.gamma_multiply(0.25),
                                                );
                                                ui.painter().text(
                                                    close_resp.rect.center(),
                                                    egui::Align2::CENTER_CENTER,
                                                    "\u{00D7}",
                                                    egui::FontId::proportional(14.0),
                                                    colors.error,
                                                );
                                            }
                                            if close_resp.clicked() {
                                                tab_to_close = Some(idx);
                                            }
                                        }
                                    });
                                });
                            }

                            // "+" button to add new empty tab (opens editor)
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new("+").size(14.0).color(colors.text_muted),
                                ))
                                .clicked()
                            {
                                let mut new_tab = TabState::new(self.settings.default_search_mode);
                                new_tab.view_mode = ViewMode::Raw;
                                new_tab.raw_content = Some(String::new());
                                self.tabs.push(new_tab);
                                tab_to_activate = Some(self.tabs.len() - 1);
                            }

                            // Process tab actions
                            if let Some(idx) = tab_to_activate {
                                self.active_tab = idx;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                    self.tabs[self.active_tab].title_display(),
                                ));
                            }
                            if let Some(idx) = tab_to_close {
                                if self.tabs[idx].is_modified() {
                                    self.pending_close_tab = Some(idx);
                                    self.show_close_confirm = true;
                                } else {
                                    self.close_tab(idx);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                                        self.tabs[self.active_tab].title_display(),
                                    ));
                                }
                            }
                            if let Some(idx) = tab_to_compare_with {
                                self.begin_compare_with_tab(idx);
                            }
                            if let Some(idx) = tab_to_toggle_pin {
                                self.toggle_tab_pinned(idx);
                            }
                            if let Some(idx) = tab_to_rename {
                                self.begin_rename_tab(idx);
                            }
                            if let Some(idx) = tab_to_export_pdf {
                                // The dialog prints the active tab, so make it
                                // the one that was right-clicked.
                                self.active_tab = idx;
                                self.pdf_export_dialog =
                                    Some(crate::app::state::PdfExportState::default());
                            }
                        });
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::ColumnInfo;
    use std::collections::HashSet;

    fn tab_with_columns(names: &[&str]) -> TabState {
        let mut tab = TabState::new(data::SearchMode::Plain);
        tab.table.columns = names
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).to_string(),
                data_type: "Utf8".into(),
            })
            .collect();
        assert!(tab.sync_column_keys(), "first sync records the names");
        tab
    }

    #[test]
    fn sheets_of_one_workbook_get_tabs_that_can_be_told_apart() {
        // Three sheets of one file are three tabs, and the file name is the
        // same on all three, so the sheet has to be in the label.
        let mut tab = TabState::new(data::SearchMode::Plain);
        tab.table.source_path = Some("/tmp/book.xlsx".to_string());
        assert_eq!(tab.title_display(), "book.xlsx");

        tab.sheet_name = Some("Costs".to_string());
        assert_eq!(tab.title_display(), "book.xlsx - Costs");

        // The modified marker still has to survive - that is why this is its
        // own field and not `custom_tab_label`, which swallows it.
        tab.table.insert_row(0);
        assert_eq!(tab.title_display(), "book.xlsx - Costs *");

        // A name the user typed themselves still wins over both.
        tab.user_tab_name = Some("mine".to_string());
        assert_eq!(tab.title_display(), "mine *");
    }

    #[test]
    fn a_database_tab_saves_in_place_even_without_a_source_path() {
        // A live-database tab deliberately has no file on disk, so any save
        // path that gates on `source_path` alone silently routes it to the
        // Save As picker instead of the write-back dialog.
        let mut tab = TabState::new(data::SearchMode::Plain);
        assert!(!tab.saves_in_place(), "a blank tab has nowhere to save");

        tab.table.source_path = Some("/tmp/x.csv".to_string());
        assert!(tab.saves_in_place(), "a file-backed tab saves to its file");

        let mut db = TabState::new(data::SearchMode::Plain);
        db.db_origin = Some(crate::app::state::DbOrigin {
            conn_id: "conn-1".to_string(),
            catalog: None,
            schema: "public".to_string(),
            table: "people".to_string(),
            identity: Some(octa::db::write_back::RowIdentity::Key(vec![
                "id".to_string(),
            ])),
        });
        assert!(
            db.table.source_path.is_none(),
            "db tabs never carry a source path"
        );
        assert!(
            db.saves_in_place(),
            "Ctrl+S on a db tab must reach the write-back dialog, not Save As"
        );
    }

    #[test]
    fn deleting_a_column_renumbers_the_filters_that_follow_it() {
        let mut tab = tab_with_columns(&["id", "name", "status"]);
        tab.column_filters
            .insert(2, HashSet::from(["active".to_string()]));
        tab.hidden_columns.insert(1);
        tab.predicate_filters
            .push(octa::data::predicate_filter::PredicateFilter {
                col: 2,
                op: octa::data::conditional_format::CondOp::Eq,
                value: "active".into(),
                case_sensitive: false,
            });

        tab.table.columns.remove(0); // the user deletes `id`
        assert!(tab.sync_column_keys());

        assert!(
            tab.column_filters.contains_key(&1),
            "filter follows `status`"
        );
        assert!(tab.hidden_columns.contains(&0), "hidden follows `name`");
        assert_eq!(tab.predicate_filters[0].col, 1);
    }

    #[test]
    fn moving_a_column_moves_its_filter() {
        let mut tab = tab_with_columns(&["id", "name", "status"]);
        tab.column_filters
            .insert(0, HashSet::from(["7".to_string()]));
        tab.table.move_column(0, 2); // id goes last
        assert!(tab.sync_column_keys());
        assert!(tab.column_filters.contains_key(&2));
    }

    #[test]
    fn a_column_that_is_gone_loses_its_filter() {
        let mut tab = tab_with_columns(&["id", "status"]);
        tab.column_filters
            .insert(1, HashSet::from(["active".to_string()]));
        tab.table.columns.pop(); // `status` deleted
        assert!(tab.sync_column_keys());
        assert!(
            tab.column_filters.is_empty(),
            "a filter must never survive onto another column"
        );
    }

    #[test]
    fn an_unchanged_column_list_is_a_no_op() {
        let mut tab = tab_with_columns(&["id", "status"]);
        assert!(!tab.sync_column_keys());
    }
}
