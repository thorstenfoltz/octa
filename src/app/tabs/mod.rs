//! Tab lifecycle: per-tab state initialization and titles. The tab bar, the
//! result-tab builders and close/pin/reopen live in the submodules.

mod lifecycle;
mod result_tabs;
mod tab_bar;

use std::sync::Arc;

use octa::data::{self, DataTable, ViewMode};
use octa::ui;
use octa::ui::table_view::TableViewState;

use super::state::{RawCsvEscape, RawCsvQuote, SearchNavState, TabState};

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

    /// Does the source hold rows this tab does not?
    ///
    /// Files say so through `is_partial`. A live-database tab says so by
    /// keeping `total_rows` set at all: there its value is only a "more may
    /// exist" flag (`db_browser` stores the loaded count), which `is_partial`
    /// reads as complete. Ask this, not `is_partial`, wherever a database tab
    /// counts.
    pub(crate) fn source_has_more(&self) -> bool {
        self.table.is_partial() || (self.db_origin.is_some() && self.table.total_rows.is_some())
    }

    /// `(loaded rows, total when known)` when the source holds rows this tab
    /// does not, for the note a result computed from it carries.
    ///
    /// `DataTable::partial_note` alone misses a live-database tab, whose
    /// `total_rows` is a "more may exist" flag, so a Summary of 200 loaded
    /// rows used to arrive with no note at all.
    pub(crate) fn partial_note(&self) -> Option<(usize, Option<usize>)> {
        self.source_has_more()
            .then(|| (self.table.rows.len(), self.table.known_total()))
    }

    /// The name a result tab built from this one carries: the file name, or
    /// the tab title when there is no file (a database table, a result).
    pub(crate) fn result_source_label(&self) -> String {
        self.table
            .source_path
            .as_ref()
            .and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| self.title_display())
    }

    /// The Find & Replace box, when it is open over a view it can search and
    /// holds something to find. Its matches are then the ones the view
    /// highlights and steps through, in place of the toolbar search's.
    pub(crate) fn find_box_active(&self) -> Option<&crate::view_modes::find_box::FindInput> {
        self.find_box
            .as_ref()
            .filter(|f| !f.query.is_empty() && self.find_box_fits())
    }

    /// Views that show the cells or the text a replace changes.
    pub(crate) fn find_box_fits(&self) -> bool {
        use octa::data::ViewMode;
        matches!(self.view_mode, ViewMode::Table | ViewMode::Notebook) || self.shows_text()
    }

    /// What the view highlights: the Find & Replace box's text while it is
    /// active, else the toolbar search's, if any.
    pub(crate) fn highlight_matcher(&self) -> Option<octa::data::search::RowMatcher> {
        match self.find_box_active() {
            Some(f) => Some(crate::view_modes::find_box::matcher(f)),
            None => (!self.search_text.is_empty()).then(|| self.search_matcher()),
        }
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
        // A hash column renamed or deleted stops being the database's: it
        // leaves what the tab asks for and what it holds alike, so no
        // re-query follows.
        let names = &self.column_key_names;
        self.server_hashes.retain(|h| names.contains(&h.name));
        if let Some(v) = self.server_view.as_mut() {
            v.derived.retain(|h| names.contains(&h.name));
            if v.is_empty() {
                self.server_view = None;
            }
        }

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
        // A scope on a deleted column searches every column again.
        self.search_scope_col = self.search_scope_col.and_then(|c| remap.get(&c).copied());
        if let Some(snap) = self.view_ui.as_mut() {
            snap.remap(&remap);
        }
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
            find_box: None,
            filtered_rows: Vec::new(),
            filter_dirty: true,
            search_nav: SearchNavState::default(),
            search_cell_matches: Vec::new(),
            view_mode: ViewMode::Table,
            raw_content: None,
            raw_content_modified: false,
            raw_content_original: None,
            synced_view: (ViewMode::Table, data::CompareMode::default()),
            text_sync_hash: None,
            raw_color_enabled: true,
            raw_file_size: None,
            raw_perf_prompt_resolved: false,
            raw_view_formatted: false,
            raw_view_wrap: false,
            sheet_name: None,
            csv_delimiter: b',',
            csv_decimal_comma: false,
            csv_bom: false,
            csv_save_choice: None,
            raw_csv_quote: RawCsvQuote::default(),
            raw_csv_escape: RawCsvEscape::default(),
            bg_row_buffer: None,
            date_detect: None,
            saving: None,
            bg_loading_done: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            bg_can_load_more: false,
            loading_all: false,
            bg_file_exhausted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            markdown_scroll_target: None,
            markdown_layout: data::MarkdownLayout::default(),
            markdown_render_cache: None,
            json_tree_expanded: std::collections::HashSet::new(),
            json_nested_docs: std::collections::HashMap::new(),
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
            sql: crate::app::state::SqlPane::new(),
            // The placeholder for the pane that is out in `sql`.
            sql_panes: vec![crate::app::state::SqlPane::default()],
            sql_active_pane: 0,
            sql_maximised: false,
            sql_ask_input: String::new(),
            sql_panel_open: false,
            sql_target: None,
            sql_workspace: None,
            sql_data_stamp: None,
            sql_job: None,
            file_stamp: None,
            sql_diff_marks: Vec::new(),
            sql_diff_highlight_until: None,
            sql_auto_registered: Vec::new(),
            sql_auto_register_sig: String::new(),
            sql_workspace_open: false,
            sql_inspector_selection: None,
            sql_inspector_cache: std::collections::HashMap::new(),
            sql_workspace_tree_expanded: std::collections::HashSet::new(),
            sql_write_back: None,
            header_suggestion: None,
            file_rows_read: 0,
            csv_resume: Default::default(),
            chunk_casts: Vec::new(),
            column_casts: Vec::new(),
            sql_import: None,
            sql_other_tabs: None,
            chunk_trim: false,
            value_frequency_col: None,
            vf_server: None,
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
            retype_kept_as_text: std::collections::HashSet::new(),
            recipe: Vec::new(),
            recipe_undone: Vec::new(),
            recipe_seen: 0,
            recipe_key: None,
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
            memory_estimate: None,
            needs_reload: false,
            chart_tab_label: None,
            custom_tab_label: None,
            partial_source_note: None,
            loaded_rows_note: None,
            pushdown_note: None,
            server_sort: Vec::new(),
            server_hashes: Vec::new(),
            server_hash_seen: Vec::new(),
            server_view: None,
            view_task: None,
            view_hold: None,
            view_error: None,
            view_ui: None,
            view_settle: None,
            facet_values: None,
            filter_window_values: None,
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
            column_filter_shapes_mode: false,
            column_filter_shape_draft: None,
            empty_file_placeholder: false,
            parse_error_banner: None,
            compare_right_path: None,
            compare_right_raw: None,
            compare_right_table: None,
            compare_mode: data::CompareMode::default(),
            compare_columns_left: Vec::new(),
            compare_columns_right: Vec::new(),
            compare_error: None,
            compare_tolerance: Default::default(),
            compare_cache: None,
            epub_chapters_md: Vec::new(),
            epub_chapter_titles: Vec::new(),
            epub_image_bytes: std::collections::HashMap::new(),
            epub_image_textures: std::collections::HashMap::new(),
            notebook_outputs: std::collections::HashMap::new(),
            notebook_textures: std::collections::HashMap::new(),
            epub_active_chapter: 0,
            epub_title: None,
            geojson_features: Vec::new(),
            map_coord_cols: None,
            timeline: Default::default(),
            map_mode: data::MapMode::default(),
            map_tiles: None,
            map_memory: None,
            chart_config: data::chart::ChartConfig::default(),
            chart_buffers: super::state::ChartInputBuffers::default(),
            chart_overlay_cache: None,
            chart_server: None,
            cloud_origin: None,
            api_origin: None,
            db_origin: None,
            compressed_origin: None,
            protected_origin: None,
            folder_compare: None,
            git_root: None,
            cell_history_cache: None,
            large: None,
            large_page_key: None,
            large_fetch: None,
            large_export: None,
        }
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.table.is_modified() || self.raw_content_modified
    }

    /// A background job is reading this tab's rows in their current order,
    /// or will put its answer back into them by row number: Detect dates, a
    /// picker answer, a background save, SQL copying `data` or running a
    /// statement that writes. While one runs the tab is read-only, does not
    /// sort and streams in no more rows.
    pub(crate) fn holds_rows(&self) -> bool {
        self.date_detect.is_some()
            || self.saving.is_some()
            || !self.column_casts.is_empty()
            || self.sql_job.as_ref().is_some_and(|j| j.holds_rows())
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
        if crate::view_modes::timeline::offered(&self.table) {
            modes.push(ViewMode::Timeline);
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

impl TabState {
    /// Repository root + relative path of this tab's file, asked of `git`
    /// once per source path, or why there is none: no file of its own
    /// (database, API, cloud, a result tab) or git's own answer.
    pub(crate) fn git_location_or_why(&mut self) -> Result<(std::path::PathBuf, String), String> {
        let source = self.table.source_path.clone();
        if self
            .git_root
            .as_ref()
            .is_none_or(|(asked, _)| *asked != source)
        {
            let own_file = self.db_origin.is_none()
                && self.api_origin.is_none()
                && self.cloud_origin.is_none();
            let loc = match source.as_deref().filter(|_| own_file) {
                Some(p) => octa::git::history::locate_or_why(std::path::Path::new(p)),
                None => Err(octa::i18n::t("context_menu.cell_history_disabled")),
            };
            // The versions read belong to the old file too.
            self.cell_history_cache = None;
            self.git_root = Some((source, loc));
        }
        match &self.git_root {
            Some((_, loc)) => loc.clone(),
            None => Err(String::new()),
        }
    }

    /// [`Self::git_location_or_why`] without the reason.
    pub(crate) fn git_location(&mut self) -> Option<(std::path::PathBuf, String)> {
        self.git_location_or_why().ok()
    }
}

impl super::state::OctaApp {
    /// [`TabState::git_location`] of the active tab.
    pub(crate) fn active_git_location(&mut self) -> Option<(std::path::PathBuf, String)> {
        self.tabs[self.active_tab].git_location()
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
