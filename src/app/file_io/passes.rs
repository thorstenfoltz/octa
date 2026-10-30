//! Load-time normalization passes: whitespace trim, header cleaning, date and
//! number promotion. Split in two so none of them runs on the UI thread for a
//! large file: [`run_load_passes`] changes the table and needs nothing but it
//! and a copy of the switches, so the reader thread runs it right after the
//! read; [`OctaApp::report_load_passes`] turns what it found into banners and
//! pickers once the table reaches the UI.

use crate::app::state::{
    DateAmbiguity, DateParseFailure, DatePromotionInfo, NumberAmbiguity, NumberPromotionInfo,
    OctaApp,
};
use octa::data::date_infer::{self, DateLayout, DateTimeLayout, InferOutcome};
use octa::data::{CellValue, DataTable};

/// Capture the source-string content of every cell in `col_idx` (None for
/// pre-existing nulls). Used right before date promotion so the warning
/// banner's Dismiss button can revert the column back to its on-disk shape.
fn snapshot_column_strings(table: &DataTable, col_idx: usize) -> Vec<Option<String>> {
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

/// Whether a loaded table is really a non-tabular text document (Markdown,
/// plain Text/code, a Jupyter notebook, or an EPUB) rather than a data table.
/// These load as a single line/paragraph column, so the whitespace-trim pass
/// must not touch them: it would strip meaningful trailing spaces (e.g.
/// Markdown hard line breaks) and pop a banner on files that aren't tables.
fn format_is_text_document(format_name: Option<&str>) -> bool {
    matches!(
        format_name,
        Some("Text") | Some("Markdown") | Some("Jupyter Notebook") | Some("EPUB")
    )
}

/// The settings the passes read, copied before the read starts so the reader
/// thread needs nothing else.
#[derive(Clone, Copy)]
pub(crate) struct PassSwitches {
    trim: bool,
    clean_headers: bool,
    dates: bool,
}

/// What the date check found. Ambiguous columns carry a `tab_idx` of 0 until
/// [`OctaApp::report_date_findings`] sets it: where the check runs, the tab
/// the table lands in is not known yet.
#[derive(Default)]
pub(crate) struct DateFindings {
    /// Columns that became dates.
    pub(crate) promoted: Vec<usize>,
    /// The layout each of them was read in, for rows streamed in later.
    pub(crate) casts: Vec<(usize, Promotion)>,
    /// The non-ISO ones among them with their text as stored, for the
    /// banner's Dismiss. Left empty by Detect dates, which is undoable.
    pub(crate) reformatted: Vec<DatePromotionInfo>,
    /// Columns two layouts fit, for the picker.
    pub(crate) ambiguous: Vec<DateAmbiguity>,
    /// Columns most but not all of whose values are dates.
    pub(crate) failures: Vec<DateParseFailure>,
}

/// The layout the date check settled a whole column on.
#[derive(Clone, Copy)]
pub(crate) enum Promotion {
    Date(DateLayout),
    DateTime(DateTimeLayout),
}

impl Promotion {
    /// The cell that replaces `cell`, or `None` to leave it.
    pub(crate) fn cell(self, cell: &CellValue) -> Option<CellValue> {
        match self {
            Self::Date(l) => date_infer::date_cell(cell, l),
            Self::DateTime(l) => date_infer::datetime_cell(cell, l),
        }
    }

    pub(crate) fn data_type(self) -> &'static str {
        match self {
            Self::Date(_) => date_infer::DATE_TYPE,
            Self::DateTime(_) => date_infer::DATETIME_TYPE,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Date(l) => l.label(),
            Self::DateTime(l) => l.label(),
        }
    }

    fn is_canonical(self) -> bool {
        match self {
            Self::Date(l) => l.is_canonical(),
            Self::DateTime(l) => l.is_canonical(),
        }
    }
}

/// File the check's `outcome` for one column under `findings`. A settled
/// layout comes back for the caller to apply its own way: in place on load,
/// into a fresh column for Detect dates.
pub(crate) fn sort_date_outcome(
    findings: &mut DateFindings,
    col_idx: usize,
    col_name: &str,
    outcome: InferOutcome,
) -> Option<Promotion> {
    match outcome {
        InferOutcome::Skip => None,
        InferOutcome::PromotedDate(l) => Some(Promotion::Date(l)),
        InferOutcome::PromotedDateTime(l) => Some(Promotion::DateTime(l)),
        InferOutcome::AmbiguousDate {
            candidates,
            samples,
        } => {
            findings.ambiguous.push(DateAmbiguity {
                tab_idx: 0,
                col_idx,
                col_name: col_name.to_string(),
                samples,
                date_candidates: candidates,
                datetime_candidates: Vec::new(),
            });
            None
        }
        InferOutcome::AmbiguousDateTime {
            candidates,
            samples,
        } => {
            findings.ambiguous.push(DateAmbiguity {
                tab_idx: 0,
                col_idx,
                col_name: col_name.to_string(),
                samples,
                date_candidates: Vec::new(),
                datetime_candidates: candidates,
            });
            None
        }
        InferOutcome::Failed {
            label,
            parsed,
            total,
            failures,
        } => {
            findings.failures.push(DateParseFailure {
                column_name: col_name.to_string(),
                source_label: label,
                parsed,
                total,
                samples: failures,
            });
            None
        }
    }
}

/// Promote every text column that holds only dates to `Date`/`DateTime`, in
/// place, and note the ones that need asking about.
fn find_dates(table: &mut DataTable) -> DateFindings {
    let mut findings = DateFindings::default();
    for col_idx in 0..table.col_count() {
        if !date_infer::column_is_candidate(table, col_idx) {
            continue;
        }
        let outcome = {
            let collected = date_infer::collect_column_strings(table, col_idx);
            if collected.is_empty() {
                continue;
            }
            date_infer::infer_column(&collected)
        };
        let name = table.columns[col_idx].name.clone();
        let Some(promotion) = sort_date_outcome(&mut findings, col_idx, &name, outcome) else {
            continue;
        };
        if !promotion.is_canonical() {
            findings.reformatted.push(DatePromotionInfo {
                col_idx,
                column_name: name,
                source_label: promotion.label(),
                original_values: snapshot_column_strings(table, col_idx),
            });
        }
        match promotion {
            Promotion::Date(l) => date_infer::apply_date(table, col_idx, l),
            Promotion::DateTime(l) => date_infer::apply_datetime(table, col_idx, l),
        }
        findings.promoted.push(col_idx);
        findings.casts.push((col_idx, promotion));
    }
    findings
}

/// What the load passes changed, for [`OctaApp::report_load_passes`].
pub(crate) struct LoadPasses {
    trimmed: Option<(Vec<String>, octa::data::trim::TrimUndo)>,
    headers_cleaned: usize,
    /// Columns typed as times; repeated on rows streamed in later.
    time_casts: Vec<(usize, octa::data::times::TimeKind)>,
    dates: DateFindings,
    numbers: Vec<NumberPromotionInfo>,
    /// Number columns to ask about; `tab_idx` set on report, as for dates.
    number_questions: Vec<NumberAmbiguity>,
    /// A promoted column was written the European way.
    european: bool,
    /// The style each promoted number column was read in, and whether the
    /// trim ran: both repeated on rows streamed in later.
    number_casts: Vec<(usize, octa::data::num_parse::NumberStyle)>,
    trim: bool,
}

/// Run the load passes on a freshly read `table`: trim (if on), header
/// cleaning (if on), times, dates (if on), then numbers. Trimming goes first so
/// `" 2024-01-02 "` is still a date; numbers go after dates so a promoted
/// `31.12.2024` is already typed and out of their reach.
pub(crate) fn run_load_passes(table: &mut DataTable, switches: PassSwitches) -> LoadPasses {
    let trimmed = (switches.trim && !format_is_text_document(table.format_name.as_deref()))
        .then(|| octa::data::trim::trim_string_columns_tracked(table))
        .filter(|(columns, _)| !columns.is_empty());
    let headers_cleaned = if switches.clean_headers {
        octa::data::trim::clean_headers(table).len()
    } else {
        0
    };
    let time_casts = octa::data::times::promote_columns(table);
    let dates = if switches.dates {
        find_dates(table)
    } else {
        DateFindings::default()
    };

    let (promoted, ambiguous) = octa::data::num_parse::promote_columns_with_snapshot(table);
    let european = promoted
        .iter()
        .any(|(_, style, _)| *style == octa::data::num_parse::NumberStyle::European);
    let number_casts = promoted.iter().map(|(c, style, _)| (*c, *style)).collect();
    let name = |c: usize| {
        table
            .columns
            .get(c)
            .map(|c| c.name.clone())
            .unwrap_or_default()
    };
    let numbers = promoted
        .into_iter()
        .map(|(col_idx, style, original_values)| NumberPromotionInfo {
            col_idx,
            column_name: name(col_idx),
            style_label: style.label(),
            original_values,
        })
        .collect();
    let number_questions = ambiguous
        .into_iter()
        .map(|col_idx| NumberAmbiguity {
            tab_idx: 0,
            col_idx,
            col_name: name(col_idx),
            samples: table
                .rows
                .iter()
                .filter_map(|r| match r.get(col_idx) {
                    Some(CellValue::String(s)) if !s.trim().is_empty() => Some(s.clone()),
                    _ => None,
                })
                .take(5)
                .collect(),
        })
        .collect();

    LoadPasses {
        trimmed,
        headers_cleaned,
        time_casts,
        dates,
        numbers,
        number_questions,
        european,
        number_casts,
        trim: switches.trim,
    }
}

impl OctaApp {
    pub(crate) fn pass_switches(&self) -> PassSwitches {
        PassSwitches {
            trim: self.settings.trim_whitespace_on_load,
            clean_headers: self.settings.clean_headers_on_load,
            dates: self.settings.detect_dates_on_load,
        }
    }

    /// Turn what [`run_load_passes`] did to tab `tab_idx`'s table into its
    /// banners, pickers and status line. For DB-backed tables the diff-save
    /// baseline is re-synced after a trim or a header clean, so neither is
    /// mistaken for user edits.
    pub(crate) fn report_load_passes(&mut self, tab_idx: usize, passes: LoadPasses) {
        let LoadPasses {
            trimmed,
            headers_cleaned,
            time_casts,
            dates,
            numbers,
            number_questions,
            european,
            number_casts,
            trim,
        } = passes;
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        tab.chunk_trim = trim;
        tab.chunk_casts.extend(
            number_casts
                .into_iter()
                .map(|(c, style)| (c, crate::app::bg_rows::ChunkCast::Number(style))),
        );
        tab.chunk_casts.extend(
            time_casts
                .into_iter()
                .map(|(c, kind)| (c, crate::app::bg_rows::ChunkCast::Times(kind))),
        );
        if let Some((columns, undo)) = trimmed {
            super::resync_db_meta_baseline(tab);
            tab.filter_dirty = true;
            if self.settings.warn_on_whitespace_trim {
                self.pending_trim_warning = Some(crate::app::state::TrimWarning {
                    tab_idx,
                    columns,
                    undo,
                });
            }
        }
        if headers_cleaned > 0 {
            let tab = &mut self.tabs[tab_idx];
            super::resync_db_meta_baseline(tab);
            tab.filter_dirty = true;
            tab.table_state.widths_initialized = false;
            self.status_message = Some((
                octa::i18n::t("settings.clean_headers_status")
                    .replace("{n}", &headers_cleaned.to_string()),
                std::time::Instant::now(),
            ));
        }
        self.report_date_findings(tab_idx, dates, false);

        if !numbers.is_empty() {
            let tab = &mut self.tabs[tab_idx];
            // A file written the European way is saved back the same way.
            if european {
                tab.csv_decimal_comma = true;
            }
            tab.filter_dirty = true;
            tab.table_state.invalidate_row_heights();
            self.pending_number_warning = Some(crate::app::state::NumberWarning {
                tab_idx,
                entries: numbers,
            });
        }
        for mut question in number_questions {
            question.tab_idx = tab_idx;
            self.pending_number_pickers.push_back(question);
        }
    }

    /// Show what a date check found on tab `tab_idx`: the format-change
    /// banner, the near-miss banner and one picker per ambiguous column. The
    /// banners follow "Warn on date format change", except that a check the
    /// user `asked` for (Detect dates) always reports its near misses.
    pub(crate) fn report_date_findings(
        &mut self,
        tab_idx: usize,
        findings: DateFindings,
        asked: bool,
    ) {
        let DateFindings {
            promoted,
            casts,
            reformatted,
            ambiguous,
            failures,
        } = findings;
        let Some(tab) = self.tabs.get_mut(tab_idx) else {
            return;
        };
        tab.chunk_casts.extend(
            casts
                .into_iter()
                .map(|(c, p)| (c, crate::app::bg_rows::ChunkCast::Date(p))),
        );
        if !promoted.is_empty() {
            tab.filter_dirty = true;
            tab.table_state.invalidate_row_heights();
        }
        let warn = self.settings.warn_on_date_format_change;
        if !reformatted.is_empty() && warn {
            self.pending_date_warning = Some(crate::app::state::DateWarning {
                tab_idx,
                entries: reformatted,
            });
        }
        if !failures.is_empty() && (warn || asked) {
            self.pending_date_parse_warning = Some(crate::app::state::DateParseWarning {
                tab_idx,
                entries: failures,
            });
        }
        for mut question in ambiguous {
            question.tab_idx = tab_idx;
            self.pending_date_pickers.push_back(question);
        }
    }
}

#[cfg(test)]
#[path = "passes_tests.rs"]
mod tests;
