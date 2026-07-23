//! Load-time normalization passes run by `apply_loaded_table`: whitespace
//! trim, header cleaning, and date-column inference. Split out of
//! `file_io/mod.rs`.

use crate::app::state::OctaApp;

/// Capture the source-string content of every cell in `col_idx` (None for
/// pre-existing nulls). Used right before date promotion so the warning
/// banner's Dismiss button can revert the column back to its on-disk shape.
fn snapshot_column_strings(table: &octa::data::DataTable, col_idx: usize) -> Vec<Option<String>> {
    use octa::data::CellValue;
    let mut out = Vec::with_capacity(table.row_count());
    for row in 0..table.row_count() {
        out.push(match table.get(row, col_idx) {
            Some(CellValue::String(s)) => Some(s.clone()),
            Some(CellValue::Null) | None => None,
            Some(other) => Some(other.to_string()),
        });
    }
    out
}

impl OctaApp {
    /// Strip leading/trailing whitespace from every string cell in the tab's
    /// table when `trim_whitespace_on_load` is on. For DB-backed tables the
    /// `db_meta.original` snapshot is re-synced from the trimmed rows so the
    /// diff-on-save logic doesn't mistake trimming for user edits. Surfaces a
    /// dismissible banner listing the affected columns when
    /// `warn_on_whitespace_trim` is on.
    pub(crate) fn run_trim_pass(&mut self, tab_idx: usize) {
        if !self.settings.trim_whitespace_on_load || tab_idx >= self.tabs.len() {
            return;
        }
        let tab = &mut self.tabs[tab_idx];
        let (trimmed, undo) = octa::data::trim::trim_string_columns_tracked(&mut tab.table);
        if trimmed.is_empty() {
            return;
        }
        // Re-sync the DB diff-save baseline so trimming (cells or titles)
        // isn't seen as edits / a schema change.
        super::resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        if self.settings.warn_on_whitespace_trim {
            self.pending_trim_warning = Some(crate::app::state::TrimWarning {
                tab_idx,
                columns: trimmed,
                undo,
            });
        }
    }

    /// Normalise the tab's column headers to lower snake_case identifiers when
    /// `clean_headers_on_load` is on. For DB-backed tables the diff-save
    /// baseline is re-synced so the rename isn't seen as a schema change.
    /// Surfaces a status message listing how many headers changed.
    pub(crate) fn run_clean_headers_pass(&mut self, tab_idx: usize) {
        if !self.settings.clean_headers_on_load || tab_idx >= self.tabs.len() {
            return;
        }
        let tab = &mut self.tabs[tab_idx];
        let changed = octa::data::trim::clean_headers(&mut tab.table);
        if changed.is_empty() {
            return;
        }
        super::resync_db_meta_baseline(tab);
        tab.filter_dirty = true;
        tab.table_state.widths_initialized = false;
        self.status_message = Some((
            octa::i18n::t("settings.clean_headers_status")
                .replace("{n}", &changed.len().to_string()),
            std::time::Instant::now(),
        ));
    }

    /// Walk the freshly-loaded tab's columns and either (a) promote a
    /// uniformly-formatted string column to typed `Date`/`DateTime`, or (b)
    /// queue a modal date-ambiguity dialog when the values are consistent
    /// with multiple layouts (US vs European).
    pub(crate) fn run_date_inference_pass(&mut self, tab_idx: usize) {
        if tab_idx >= self.tabs.len() {
            return;
        }

        use octa::data::date_infer;
        let col_count = self.tabs[tab_idx].table.col_count();
        let mut format_changes: Vec<crate::app::state::DatePromotionInfo> = Vec::new();
        let mut parse_failures: Vec<crate::app::state::DateParseFailure> = Vec::new();
        for col_idx in 0..col_count {
            let table = &self.tabs[tab_idx].table;
            if !date_infer::column_is_candidate(table, col_idx) {
                continue;
            }
            let collected = date_infer::collect_column_strings(table, col_idx);
            if collected.is_empty() {
                continue;
            }
            match date_infer::infer_column(&collected) {
                date_infer::InferOutcome::Skip => {}
                date_infer::InferOutcome::PromotedDate(layout) => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    let snapshot = if layout.is_canonical() {
                        Vec::new()
                    } else {
                        snapshot_column_strings(&self.tabs[tab_idx].table, col_idx)
                    };
                    date_infer::apply_date(&mut self.tabs[tab_idx].table, col_idx, layout);
                    self.tabs[tab_idx].filter_dirty = true;
                    self.tabs[tab_idx].table_state.invalidate_row_heights();
                    if !layout.is_canonical() {
                        format_changes.push(crate::app::state::DatePromotionInfo {
                            col_idx,
                            column_name: col_name,
                            source_label: layout.label(),
                            original_values: snapshot,
                        });
                    }
                }
                date_infer::InferOutcome::PromotedDateTime(layout) => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    let snapshot = if layout.is_canonical() {
                        Vec::new()
                    } else {
                        snapshot_column_strings(&self.tabs[tab_idx].table, col_idx)
                    };
                    date_infer::apply_datetime(&mut self.tabs[tab_idx].table, col_idx, layout);
                    self.tabs[tab_idx].filter_dirty = true;
                    self.tabs[tab_idx].table_state.invalidate_row_heights();
                    if !layout.is_canonical() {
                        format_changes.push(crate::app::state::DatePromotionInfo {
                            col_idx,
                            column_name: col_name,
                            source_label: layout.label(),
                            original_values: snapshot,
                        });
                    }
                }
                date_infer::InferOutcome::AmbiguousDate {
                    candidates,
                    samples,
                } => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    self.pending_date_pickers
                        .push_back(crate::app::state::DateAmbiguity {
                            tab_idx,
                            col_idx,
                            col_name,
                            samples,
                            date_candidates: candidates,
                            datetime_candidates: Vec::new(),
                        });
                }
                date_infer::InferOutcome::AmbiguousDateTime {
                    candidates,
                    samples,
                } => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    self.pending_date_pickers
                        .push_back(crate::app::state::DateAmbiguity {
                            tab_idx,
                            col_idx,
                            col_name,
                            samples,
                            date_candidates: Vec::new(),
                            datetime_candidates: candidates,
                        });
                }
                date_infer::InferOutcome::Failed {
                    label,
                    parsed,
                    total,
                    failures,
                } => {
                    let col_name = self.tabs[tab_idx]
                        .table
                        .columns
                        .get(col_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    parse_failures.push(crate::app::state::DateParseFailure {
                        column_name: col_name,
                        source_label: label,
                        parsed,
                        total,
                        samples: failures,
                    });
                }
            }
        }

        if !format_changes.is_empty() && self.settings.warn_on_date_format_change {
            self.pending_date_warning = Some(crate::app::state::DateWarning {
                tab_idx,
                entries: format_changes,
            });
        }
        if !parse_failures.is_empty() && self.settings.warn_on_date_format_change {
            self.pending_date_parse_warning = Some(crate::app::state::DateParseWarning {
                tab_idx,
                entries: parse_failures,
            });
        }
    }
}
