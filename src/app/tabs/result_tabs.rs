//! Result tabs built from the active table: chart, summary, transpose,
//! quality report and the other derived views.

use crate::app::state::OctaApp;

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
    pub(crate) fn push_result_tab(&mut self, new_tab: crate::app::state::TabState) {
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
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
        let partial_source_note = source.table.partial_note();
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
        // Only as complete as the table it came from.
        new_tab.partial_source_note = partial_source_note;
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
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
        let partial_source_note = source.table.partial_note();
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
        // Only as complete as the table it came from.
        new_tab.partial_source_note = partial_source_note;
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
        let partial_source_note = source.table.partial_note();
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
        // Only as complete as the table it came from.
        new_tab.partial_source_note = partial_source_note;
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
        let mut new_tab = crate::app::state::TabState::new(self.settings.default_search_mode);
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
        let partial_source_note = source.table.partial_note();
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
        // Only as complete as the table it came from.
        new_tab.partial_source_note = partial_source_note;
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
        let partial_source_note = source.table.partial_note();
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
        // Only as complete as the table it came from.
        new_tab.partial_source_note = partial_source_note;
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
            let mut tab = crate::app::state::TabState::new(default_search_mode);
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
        let partial_source_note = source.table.partial_note();
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
        // Only as complete as the table it came from.
        new_tab.partial_source_note = partial_source_note;
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
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
        let mut new_tab = crate::app::state::TabState::new(default_search_mode);
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

    /// **Show breaking rows** in the Find lookup tables dialog: every row of
    /// every key whose ticked dependents disagree, grouped by key.
    pub(crate) fn open_lookup_breaking_tab(
        &mut self,
        st: &crate::app::state::LookupsState,
        finding: usize,
    ) {
        let Some(f) = st.findings.as_ref().and_then(|v| v.get(finding)) else {
            return;
        };
        let Some(source) = self.tabs.get(st.tab) else {
            return;
        };
        // `ticked` holds positions in `f.dependents`; the engine wants columns.
        let deps: Vec<usize> = st.ticked[finding]
            .iter()
            .filter_map(|&di| f.dependents.get(di).map(|d| d.col))
            .collect();
        let mut src = source.table.clone();
        src.apply_edits();
        let rows = octa::data::lookups::breaking_rows(&src, f.key, &deps);
        let key = src.columns[f.key].name.clone();
        let mut new_tab = crate::app::state::TabState::new(self.settings.default_search_mode);
        new_tab.table = src.clone_with_rows(&rows);
        new_tab.table.source_path = None;
        new_tab.table.format_name = None;
        new_tab.custom_tab_label =
            Some(format!("{} - {key}", octa::i18n::t("lookups.breaking_tab")));
        new_tab.tab_hint = Some(octa::i18n::t("lookups.breaking_tab_hint"));
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
    }

    /// **Split out** in the Find lookup tables dialog: two new tabs, the
    /// lookup (one row per key) and the source without the ticked columns.
    /// The source tab is never changed.
    pub(crate) fn open_lookup_split_tabs(
        &mut self,
        st: &crate::app::state::LookupsState,
        finding: usize,
    ) {
        let Some(f) = st.findings.as_ref().and_then(|v| v.get(finding)) else {
            return;
        };
        let Some(source) = self.tabs.get(st.tab) else {
            return;
        };
        // `ticked` holds positions in `f.dependents`; the engine wants columns.
        let deps: Vec<usize> = st.ticked[finding]
            .iter()
            .filter_map(|&di| f.dependents.get(di).map(|d| d.col))
            .collect();
        let mut src = source.table.clone();
        src.apply_edits();
        let split = octa::data::lookups::split_out(&src, f.key, &deps);
        let key = src.columns[f.key].name.clone();
        let banner = (split.resolved_keys > 0).then(|| {
            octa::i18n::t("lookups.resolved_banner")
                .replace("{count}", &split.resolved_keys.to_string())
        });
        for (table, label_key, hint_key, banner) in [
            (
                split.lookup,
                "lookups.lookup_tab",
                "lookups.lookup_tab_hint",
                banner,
            ),
            (
                split.main,
                "lookups.main_tab",
                "lookups.main_tab_hint",
                None,
            ),
        ] {
            let mut new_tab = crate::app::state::TabState::new(self.settings.default_search_mode);
            new_tab.table = table;
            new_tab.table.source_path = None;
            new_tab.table.format_name = None;
            new_tab.custom_tab_label = Some(format!("{} - {key}", octa::i18n::t(label_key)));
            new_tab.tab_hint = Some(octa::i18n::t(hint_key));
            new_tab.parse_error_banner = banner;
            self.tabs.push(new_tab);
        }
        // Land on the lookup tab, the first of the two.
        self.active_tab = self.tabs.len() - 2;
    }
}
