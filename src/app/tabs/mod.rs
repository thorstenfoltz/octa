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
            raw_view_wrap: false,
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
            epub_chapters_md: Vec::new(),
            epub_chapter_titles: Vec::new(),
            epub_image_bytes: std::collections::HashMap::new(),
            epub_image_textures: std::collections::HashMap::new(),
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
            cloud_origin: None,
            api_origin: None,
            db_origin: None,
            compressed_origin: None,
            git_root: None,
            cell_history_cache: None,
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

    /// The welcome tab is asked first (no file, so no repository) and the
    /// first file opened reuses that tab. The answer must follow the file:
    /// it once stuck at "no repository" and greyed out Cell history.
    #[test]
    fn git_location_follows_the_file_when_a_blank_tab_is_reused() {
        let dir = tempfile::tempdir().unwrap();
        let git = |a: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(a)
                .output()
        };
        let Ok(out) = git(&["init", "-q"]) else {
            return;
        };
        if !out.status.success() {
            return;
        }
        let file = dir.path().join("p.csv");
        std::fs::write(&file, "a\n1\n").unwrap();
        let mut tab = TabState::new(octa::data::SearchMode::default());
        assert_eq!(tab.git_location(), None, "the blank tab has no file");
        tab.table.source_path = Some(file.to_string_lossy().into_owned());
        assert_eq!(
            tab.git_location().map(|(_, rel)| rel),
            Some("p.csv".to_string())
        );
    }
}
