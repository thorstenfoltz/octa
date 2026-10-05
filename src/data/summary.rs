//! Localized, configurable per-column Summary statistics.
//!
//! The Summary tab (Analyse -> Summary) shows one row per source column with
//! a chosen set of descriptive statistics. The heavy lifting is a single
//! DuckDB `SUMMARIZE data` pass (min / max / approx-unique / avg / std /
//! quartiles / count / null-percentage); the few extra figures we surface
//! (null count, distinct ratio, total rows) are derived from that pass plus
//! the snapshot's own row count.
//!
//! Output column headers are stable `snake_case` identifiers
//! ([`SummaryStat::column_id`]) so the table is easy to reuse; the localized
//! label and description ([`SummaryStat::i18n_key`] / [`SummaryStat::hint_key`])
//! surface as the header's hover tooltip and the Settings checkboxes. Which
//! statistics appear is driven by the user's Settings (the `enabled` list).
//! [`SummaryStat::ColumnName`] and [`SummaryStat::Type`] are always shown so a
//! row is never anonymous. Modelled one-variant-per-statistic so adding a
//! statistic is a drop-in (see `feedback_modular_features`).

use crate::data::{CellValue, ColumnInfo, DataTable};
use strum::{EnumIter, IntoEnumIterator};

/// One descriptive statistic shown as a column in the Summary tab.
///
/// The variant order here is the column order in the output table.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, serde::Serialize, serde::Deserialize,
)]
pub enum SummaryStat {
    /// Name of the source column (always shown).
    ColumnName,
    /// Declared data type of the source column (always shown).
    Type,
    /// Smallest value.
    Min,
    /// Largest value.
    Max,
    /// Sum of the numeric values (numeric columns only).
    Sum,
    /// Arithmetic mean (numeric columns only).
    Mean,
    /// Median / 50th percentile (numeric columns only).
    Median,
    /// Standard deviation (numeric columns only).
    Std,
    /// Spread: largest minus smallest (numeric columns only).
    Range,
    /// Interquartile range: 75th minus 25th percentile (numeric columns only).
    Iqr,
    /// 25th percentile (numeric columns only).
    Q25,
    /// 75th percentile (numeric columns only).
    Q75,
    /// Most frequent (modal) value.
    Mode,
    /// How many times the most frequent value occurs.
    ModeCount,
    /// Count of non-null (present) values.
    NotNullCount,
    /// Count of null (missing) values.
    NullCount,
    /// Percentage of values that are null.
    NullPercent,
    /// Exact count of distinct values.
    UniqueCount,
    /// Distinct values divided by total rows (0..1).
    DistinctRatio,
    /// Shortest text length (characters) over the column's values.
    TextLenMin,
    /// Longest text length (characters) over the column's values.
    TextLenMax,
    /// Total number of rows in the table (same on every row).
    TotalRows,
}

impl SummaryStat {
    /// Every statistic, in display order.
    pub fn all() -> Vec<SummaryStat> {
        SummaryStat::iter().collect()
    }

    /// Statistics shown by default (the full set).
    pub fn default_enabled() -> Vec<SummaryStat> {
        SummaryStat::all()
    }

    /// Whether this statistic is always shown regardless of Settings.
    /// Column name and type are mandatory so a row is never anonymous.
    pub fn is_mandatory(self) -> bool {
        matches!(self, SummaryStat::ColumnName | SummaryStat::Type)
    }

    /// Stable, machine-friendly column identifier used as the Summary table's
    /// header. Lowercase, underscores only, no spaces, never localized, so the
    /// table can be reused (saved / queried / pasted) without renaming. The
    /// localized friendly name and description stay reachable as the header's
    /// hover tooltip (see [`Self::hint_key`]).
    pub fn column_id(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "column_name",
            SummaryStat::Type => "type",
            SummaryStat::Min => "min",
            SummaryStat::Max => "max",
            SummaryStat::Sum => "sum",
            SummaryStat::Mean => "mean",
            SummaryStat::Median => "median",
            SummaryStat::Std => "std_dev",
            SummaryStat::Range => "range",
            SummaryStat::Iqr => "iqr",
            SummaryStat::Q25 => "q25",
            SummaryStat::Q75 => "q75",
            SummaryStat::Mode => "mode",
            SummaryStat::ModeCount => "mode_count",
            SummaryStat::NotNullCount => "not_null",
            SummaryStat::NullCount => "null_count",
            SummaryStat::NullPercent => "null_percent",
            SummaryStat::UniqueCount => "unique_count",
            SummaryStat::DistinctRatio => "distinct_ratio",
            SummaryStat::TextLenMin => "text_len_min",
            SummaryStat::TextLenMax => "text_len_max",
            SummaryStat::TotalRows => "total_rows",
        }
    }

    /// i18n key for the column title.
    pub fn i18n_key(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "summary_stat.column_name",
            SummaryStat::Type => "summary_stat.type",
            SummaryStat::Min => "summary_stat.min",
            SummaryStat::Max => "summary_stat.max",
            SummaryStat::Sum => "summary_stat.sum",
            SummaryStat::Mean => "summary_stat.mean",
            SummaryStat::Median => "summary_stat.median",
            SummaryStat::Std => "summary_stat.std",
            SummaryStat::Range => "summary_stat.range",
            SummaryStat::Iqr => "summary_stat.iqr",
            SummaryStat::Q25 => "summary_stat.q25",
            SummaryStat::Q75 => "summary_stat.q75",
            SummaryStat::Mode => "summary_stat.mode",
            SummaryStat::ModeCount => "summary_stat.mode_count",
            SummaryStat::NotNullCount => "summary_stat.not_null_count",
            SummaryStat::NullCount => "summary_stat.null_count",
            SummaryStat::NullPercent => "summary_stat.null_percent",
            SummaryStat::UniqueCount => "summary_stat.unique_count",
            SummaryStat::DistinctRatio => "summary_stat.distinct_ratio",
            SummaryStat::TextLenMin => "summary_stat.text_len_min",
            SummaryStat::TextLenMax => "summary_stat.text_len_max",
            SummaryStat::TotalRows => "summary_stat.total_rows",
        }
    }

    /// i18n key for the hover description (header tooltip + Settings hint).
    pub fn hint_key(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "summary_hint.column_name",
            SummaryStat::Type => "summary_hint.type",
            SummaryStat::Min => "summary_hint.min",
            SummaryStat::Max => "summary_hint.max",
            SummaryStat::Sum => "summary_hint.sum",
            SummaryStat::Mean => "summary_hint.mean",
            SummaryStat::Median => "summary_hint.median",
            SummaryStat::Std => "summary_hint.std",
            SummaryStat::Range => "summary_hint.range",
            SummaryStat::Iqr => "summary_hint.iqr",
            SummaryStat::Q25 => "summary_hint.q25",
            SummaryStat::Q75 => "summary_hint.q75",
            SummaryStat::Mode => "summary_hint.mode",
            SummaryStat::ModeCount => "summary_hint.mode_count",
            SummaryStat::NotNullCount => "summary_hint.not_null_count",
            SummaryStat::NullCount => "summary_hint.null_count",
            SummaryStat::NullPercent => "summary_hint.null_percent",
            SummaryStat::UniqueCount => "summary_hint.unique_count",
            SummaryStat::DistinctRatio => "summary_hint.distinct_ratio",
            SummaryStat::TextLenMin => "summary_hint.text_len_min",
            SummaryStat::TextLenMax => "summary_hint.text_len_max",
            SummaryStat::TotalRows => "summary_hint.total_rows",
        }
    }
}

/// Quote a column name as a DuckDB identifier (double quotes, internal quotes
/// doubled) so names with spaces or punctuation survive in a SELECT.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Largest f64 that represents every integer exactly (2^53). Above it a whole
/// number can't be round-tripped through `i64`, so we keep it as a `Float`.
const MAX_EXACT_INT_F64: f64 = 9_007_199_254_740_992.0;

/// Convert a computed numeric statistic (sum / range / iqr / ratio) into its
/// tightest cell type: an exact whole number becomes `Int`, anything else a
/// real `Float`, so the table view's numeric display path groups and
/// right-aligns it. No rounding - statistics are stored at full f64 precision.
/// Non-finite values become a blank cell.
pub fn num_cell(x: f64) -> CellValue {
    if !x.is_finite() {
        return CellValue::String(String::new());
    }
    if x.fract() == 0.0 && x.abs() < MAX_EXACT_INT_F64 {
        CellValue::Int(x as i64)
    } else {
        CellValue::Float(x)
    }
}

/// Parse a `SUMMARIZE` / text value into its tightest cell type: an integer
/// becomes `Int`, a finite decimal `Float`, an empty value a blank string, and
/// anything else (a lexicographic text min/max, a category mode) stays text.
/// This is what lets a numeric column's min/max/mode group like a number while
/// a text column's stay verbatim.
pub(crate) fn typed_cell(s: &str) -> CellValue {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return CellValue::String(String::new());
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return CellValue::Int(n);
    }
    if let Ok(f) = trimmed.parse::<f64>()
        && f.is_finite()
    {
        return CellValue::Float(f);
    }
    CellValue::String(s.to_string())
}

/// Infer a column's Arrow type name from the cell variants it holds: `Int64`
/// when every present value is an integer, `Float64` when they're all numeric
/// with at least one decimal, `Utf8` otherwise. Empty-string / null cells are
/// ignored (a numeric column keeps its type even with blank rows).
pub(crate) fn infer_column_type(cells: impl Iterator<Item = CellValue>) -> String {
    let mut saw_value = false;
    let mut saw_float = false;
    for cell in cells {
        match cell {
            CellValue::Int(_) => saw_value = true,
            CellValue::Float(_) => {
                saw_value = true;
                saw_float = true;
            }
            CellValue::Null => {}
            CellValue::String(s) if s.is_empty() => {}
            // Any real text (text min/max, mode category, type name) -> Utf8.
            _ => return "Utf8".to_string(),
        }
    }
    if !saw_value {
        "Utf8".to_string()
    } else if saw_float {
        "Float64".to_string()
    } else {
        "Int64".to_string()
    }
}

/// Exact distinct-value count per source column, positionally aligned with
/// `snap.columns`. One `COUNT(DISTINCT col)` query; `None` per column if it
/// can't be read. `COUNT(DISTINCT)` ignores nulls, so the result never exceeds
/// the row count (unlike SUMMARIZE's approximate `approx_unique`).
fn exact_distinct_counts(snap: &DataTable) -> Vec<Option<i64>> {
    let mut out = vec![None; snap.columns.len()];
    if snap.columns.is_empty() {
        return out;
    }
    let selects: Vec<String> = snap
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("COUNT(DISTINCT {}) AS d{i}", quote_ident(&c.name)))
        .collect();
    let query = format!("SELECT {} FROM data", selects.join(", "));
    if let Ok(outcome) = crate::sql::run_query(snap, &query)
        && outcome.table.row_count() >= 1
    {
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = outcome
                .table
                .get(0, i)
                .and_then(|v| v.to_string().parse::<i64>().ok());
        }
    }
    out
}

/// Per-column extra aggregates that `SUMMARIZE` does not provide: numeric sum
/// and text-length extremes. Positionally aligned with `snap.columns`.
#[derive(Clone, Default)]
struct ExtraAgg {
    sum: Option<f64>,
    text_len_min: Option<i64>,
    text_len_max: Option<i64>,
}

/// One pass computing [`ExtraAgg`] for every column. `SUM(TRY_CAST(.. AS
/// DOUBLE))` yields `NULL` (rendered blank) on non-numeric columns instead of
/// erroring; text length is measured over `CAST(.. AS VARCHAR)` so it works for
/// any type. Returns all-default on query failure.
fn extra_aggregates(snap: &DataTable) -> Vec<ExtraAgg> {
    let mut out = vec![ExtraAgg::default(); snap.columns.len()];
    if snap.columns.is_empty() {
        return out;
    }
    let selects: Vec<String> = snap
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let q = quote_ident(&c.name);
            format!(
                "SUM(TRY_CAST({q} AS DOUBLE)) AS s{i}, \
                 MIN(length(CAST({q} AS VARCHAR))) AS lmin{i}, \
                 MAX(length(CAST({q} AS VARCHAR))) AS lmax{i}"
            )
        })
        .collect();
    let query = format!("SELECT {} FROM data", selects.join(", "));
    if let Ok(outcome) = crate::sql::run_query(snap, &query)
        && outcome.table.row_count() >= 1
    {
        let parse_i64 = |c: usize| {
            outcome
                .table
                .get(0, c)
                .and_then(|v| v.to_string().parse::<i64>().ok())
        };
        let parse_f64 = |c: usize| {
            outcome
                .table
                .get(0, c)
                .and_then(|v| v.to_string().parse::<f64>().ok())
        };
        for (i, slot) in out.iter_mut().enumerate() {
            // Three result columns per source column, in select order.
            slot.sum = parse_f64(i * 3);
            slot.text_len_min = parse_i64(i * 3 + 1);
            slot.text_len_max = parse_i64(i * 3 + 2);
        }
    }
    out
}

/// Most frequent (modal) value and its count per column, positionally aligned
/// with `snap.columns`. One small `GROUP BY ... ORDER BY count DESC LIMIT 1`
/// query per column; `(None, None)` per column on failure or an all-null
/// column. Nulls are excluded from the mode.
fn mode_values(snap: &DataTable) -> Vec<(Option<String>, Option<i64>)> {
    let mut out = vec![(None, None); snap.columns.len()];
    for (i, c) in snap.columns.iter().enumerate() {
        let q = quote_ident(&c.name);
        // Tie-break on the value so the result is deterministic.
        let query = format!(
            "SELECT CAST({q} AS VARCHAR) AS v, COUNT(*) AS n FROM data \
             WHERE {q} IS NOT NULL GROUP BY v ORDER BY n DESC, v LIMIT 1"
        );
        if let Ok(outcome) = crate::sql::run_query(snap, &query)
            && outcome.table.row_count() >= 1
        {
            let value = outcome.table.get(0, 0).map(|v| v.to_string());
            let count = outcome
                .table
                .get(0, 1)
                .and_then(|v| v.to_string().parse::<i64>().ok());
            out[i] = (value, count);
        }
    }
    out
}

/// Count, per source column, cells that are *missing*: a true `Null` or a
/// zero-length string. DuckDB's `null_percentage` treats `""` as present, so
/// an all-empty UTF-8 column would otherwise report a null count of 0. Indexed
/// by column position, matching the `SUMMARIZE` row order.
fn missing_counts(snap: &DataTable) -> Vec<i64> {
    (0..snap.columns.len())
        .map(|ci| {
            (0..snap.row_count())
                .filter(|&ri| match snap.get(ri, ci) {
                    None | Some(CellValue::Null) => true,
                    Some(CellValue::String(s)) => s.is_empty(),
                    _ => false,
                })
                .count() as i64
        })
        .collect()
}

/// The statistics actually rendered, in canonical (variant) order: every
/// mandatory stat plus any the caller enabled. Order is independent of the
/// order of `enabled`.
pub fn active_stats(enabled: &[SummaryStat]) -> Vec<SummaryStat> {
    SummaryStat::iter()
        .filter(|s| s.is_mandatory() || enabled.contains(s))
        .collect()
}

/// Everything the Summary table shows about one source column, before it is
/// laid out. Two producers fill it - [`column_stats`] from a DuckDB pass
/// over loaded rows, `db::pushdown::summary` from SQL on a server - and
/// [`stats_table`] renders either the same way.
#[derive(Debug, Clone, Default)]
pub struct ColumnStats {
    pub name: String,
    pub type_name: String,
    pub min: Option<CellValue>,
    pub max: Option<CellValue>,
    pub mean: Option<CellValue>,
    pub median: Option<CellValue>,
    pub std: Option<CellValue>,
    pub q25: Option<CellValue>,
    pub q75: Option<CellValue>,
    pub sum: Option<f64>,
    pub mode: Option<String>,
    pub mode_count: Option<i64>,
    /// Null or empty cells.
    pub missing: i64,
    pub unique: Option<i64>,
    pub text_len_min: Option<i64>,
    pub text_len_max: Option<i64>,
}

/// A statistic cell as a number, for Range and IQR. Text that does not
/// parse (a date min) gives `None`, so those cells stay blank.
fn stat_f64(v: &Option<CellValue>) -> Option<f64> {
    match v.as_ref()? {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        other => other.to_string().trim().parse::<f64>().ok(),
    }
}

/// Gather the figures for the `active` statistics from `snap`. Runs a single
/// `SUMMARIZE` plus one extra pass per statistic family that is switched on.
pub fn column_stats(snap: &DataTable, active: &[SummaryStat]) -> anyhow::Result<Vec<ColumnStats>> {
    let outcome = crate::sql::run_query(snap, "SUMMARIZE data")?;
    let summ = outcome.table;

    let field_idx = |name: &str| summ.columns.iter().position(|c| c.name == name);
    let cell_str = |row: usize, name: &str| -> Option<String> {
        let ci = field_idx(name)?;
        summ.get(row, ci).map(|v| v.to_string())
    };

    // Exact distinct counts only when a stat that needs them is shown.
    let need_unique = active
        .iter()
        .any(|s| matches!(s, SummaryStat::UniqueCount | SummaryStat::DistinctRatio));
    let exact_unique = if need_unique {
        exact_distinct_counts(snap)
    } else {
        Vec::new()
    };

    // Sum / text-length extremes share one aggregate pass; only run it when one
    // of those stats is on.
    let need_extra = active.iter().any(|s| {
        matches!(
            s,
            SummaryStat::Sum | SummaryStat::TextLenMin | SummaryStat::TextLenMax
        )
    });
    let extra = if need_extra {
        extra_aggregates(snap)
    } else {
        Vec::new()
    };

    // Mode + its count share a per-column GROUP BY pass; only when on.
    let need_mode = active
        .iter()
        .any(|s| matches!(s, SummaryStat::Mode | SummaryStat::ModeCount));
    let modes = if need_mode {
        mode_values(snap)
    } else {
        Vec::new()
    };

    // Null / empty counts come from a direct pass over the snapshot so empty
    // strings count as missing (DuckDB's null_percentage does not). Only run it
    // when one of the three stats that need it is active.
    let need_missing = active.iter().any(|s| {
        matches!(
            s,
            SummaryStat::NullCount | SummaryStat::NotNullCount | SummaryStat::NullPercent
        )
    });
    let missing_per_col = if need_missing {
        missing_counts(snap)
    } else {
        Vec::new()
    };

    let text = |r: usize, f: &str| cell_str(r, f).map(|s| typed_cell(&s));
    Ok((0..summ.row_count())
        .map(|r| {
            let agg = extra.get(r).cloned().unwrap_or_default();
            let (mode, mode_count) = modes.get(r).cloned().unwrap_or((None, None));
            ColumnStats {
                name: cell_str(r, "column_name").unwrap_or_default(),
                type_name: cell_str(r, "column_type").unwrap_or_default(),
                min: text(r, "min"),
                max: text(r, "max"),
                mean: text(r, "avg"),
                median: text(r, "q50"),
                std: text(r, "std"),
                q25: text(r, "q25"),
                q75: text(r, "q75"),
                sum: agg.sum,
                mode,
                mode_count,
                missing: missing_per_col.get(r).copied().unwrap_or(0),
                unique: exact_unique.get(r).copied().flatten(),
                text_len_min: agg.text_len_min,
                text_len_max: agg.text_len_max,
            }
        })
        .collect())
}

/// Lay gathered statistics out as the Summary table: one row per source
/// column, one column per active statistic. Column headers are the stable
/// `column_id`s; types are refined from the cells so numbers render as numbers.
pub fn stats_table(stats: &[ColumnStats], total_rows: usize, enabled: &[SummaryStat]) -> DataTable {
    let active = active_stats(enabled);
    let mut columns: Vec<ColumnInfo> = active
        .iter()
        .map(|s| ColumnInfo {
            name: s.column_id().to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    let blank = || CellValue::String(String::new());
    let or_blank = |v: &Option<CellValue>| v.clone().unwrap_or_else(blank);
    let rows: Vec<Vec<CellValue>> = stats
        .iter()
        .map(|s| {
            let not_null = (total_rows as i64 - s.missing).max(0);
            let null_percent = if total_rows > 0 {
                s.missing as f64 / total_rows as f64 * 100.0
            } else {
                0.0
            };
            active
                .iter()
                .map(|stat| match stat {
                    SummaryStat::ColumnName => CellValue::String(s.name.clone()),
                    SummaryStat::Type => CellValue::String(s.type_name.clone()),
                    SummaryStat::Min => or_blank(&s.min),
                    SummaryStat::Max => or_blank(&s.max),
                    SummaryStat::Mean => or_blank(&s.mean),
                    SummaryStat::Median => or_blank(&s.median),
                    SummaryStat::Std => or_blank(&s.std),
                    SummaryStat::Q25 => or_blank(&s.q25),
                    SummaryStat::Q75 => or_blank(&s.q75),
                    SummaryStat::Sum => s.sum.map(num_cell).unwrap_or_else(blank),
                    SummaryStat::Range => match (stat_f64(&s.min), stat_f64(&s.max)) {
                        (Some(lo), Some(hi)) => num_cell(hi - lo),
                        _ => blank(),
                    },
                    SummaryStat::Iqr => match (stat_f64(&s.q25), stat_f64(&s.q75)) {
                        (Some(lo), Some(hi)) => num_cell(hi - lo),
                        _ => blank(),
                    },
                    SummaryStat::Mode => s.mode.as_deref().map(typed_cell).unwrap_or_else(blank),
                    SummaryStat::ModeCount => {
                        s.mode_count.map(CellValue::Int).unwrap_or_else(blank)
                    }
                    SummaryStat::TextLenMin => {
                        s.text_len_min.map(CellValue::Int).unwrap_or_else(blank)
                    }
                    SummaryStat::TextLenMax => {
                        s.text_len_max.map(CellValue::Int).unwrap_or_else(blank)
                    }
                    SummaryStat::NotNullCount => CellValue::Int(not_null),
                    SummaryStat::NullCount => CellValue::Int(s.missing),
                    SummaryStat::NullPercent => num_cell(null_percent),
                    SummaryStat::UniqueCount => s.unique.map(CellValue::Int).unwrap_or_else(blank),
                    SummaryStat::DistinctRatio => match s.unique {
                        Some(u) if total_rows > 0 => num_cell(u as f64 / total_rows as f64),
                        _ => blank(),
                    },
                    SummaryStat::TotalRows => CellValue::Int(total_rows as i64),
                })
                .collect()
        })
        .collect();
    // Refine each column's type from the cells it ended up with, so numeric
    // statistics are real Int64 / Float64 columns (grouped + right-aligned by
    // the table view) while mixed or textual ones stay Utf8.
    for (ci, col) in columns.iter_mut().enumerate() {
        col.data_type = infer_column_type(rows.iter().map(|row| row[ci].clone()));
    }
    DataTable {
        columns,
        rows,
        ..DataTable::empty()
    }
}

/// Build the Summary table: one row per source column, one column per active
/// statistic, with stable column ids. Runs a single `SUMMARIZE` over `snap`.
pub fn build_summary_table(snap: &DataTable, enabled: &[SummaryStat]) -> anyhow::Result<DataTable> {
    let stats = column_stats(snap, &active_stats(enabled))?;
    Ok(stats_table(&stats, snap.row_count(), enabled))
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod tests;
