//! Localized, configurable per-column Summary statistics.
//!
//! The Summary tab (Analyse -> Summary) shows one row per source column with
//! a chosen set of descriptive statistics. The heavy lifting is a single
//! DuckDB `SUMMARIZE data` pass (min / max / approx-unique / avg / std /
//! quartiles / count / null-percentage); the few extra figures we surface
//! (null count, distinct ratio, total rows) are derived from that pass plus
//! the snapshot's own row count.
//!
//! Column titles and hover descriptions are localized via [`crate::i18n`], and
//! which statistics appear is driven by the user's Settings (the `enabled`
//! list). [`SummaryStat::ColumnName`] and [`SummaryStat::Type`] are always
//! shown so a row is never anonymous. Modelled one-variant-per-statistic so
//! adding a statistic is a drop-in (see `feedback_modular_features`).

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
    /// Arithmetic mean (numeric columns only).
    Mean,
    /// Median / 50th percentile (numeric columns only).
    Median,
    /// Standard deviation (numeric columns only).
    Std,
    /// 25th percentile (numeric columns only).
    Q25,
    /// 75th percentile (numeric columns only).
    Q75,
    /// Count of non-null (present) values.
    NotNullCount,
    /// Count of null (missing) values.
    NullCount,
    /// Percentage of values that are null.
    NullPercent,
    /// Approximate count of distinct values.
    UniqueCount,
    /// Distinct values divided by total rows (0..1).
    DistinctRatio,
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

    /// i18n key for the column title.
    pub fn i18n_key(self) -> &'static str {
        match self {
            SummaryStat::ColumnName => "summary_stat.column_name",
            SummaryStat::Type => "summary_stat.type",
            SummaryStat::Min => "summary_stat.min",
            SummaryStat::Max => "summary_stat.max",
            SummaryStat::Mean => "summary_stat.mean",
            SummaryStat::Median => "summary_stat.median",
            SummaryStat::Std => "summary_stat.std",
            SummaryStat::Q25 => "summary_stat.q25",
            SummaryStat::Q75 => "summary_stat.q75",
            SummaryStat::NotNullCount => "summary_stat.not_null_count",
            SummaryStat::NullCount => "summary_stat.null_count",
            SummaryStat::NullPercent => "summary_stat.null_percent",
            SummaryStat::UniqueCount => "summary_stat.unique_count",
            SummaryStat::DistinctRatio => "summary_stat.distinct_ratio",
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
            SummaryStat::Mean => "summary_hint.mean",
            SummaryStat::Median => "summary_hint.median",
            SummaryStat::Std => "summary_hint.std",
            SummaryStat::Q25 => "summary_hint.q25",
            SummaryStat::Q75 => "summary_hint.q75",
            SummaryStat::NotNullCount => "summary_hint.not_null_count",
            SummaryStat::NullCount => "summary_hint.null_count",
            SummaryStat::NullPercent => "summary_hint.null_percent",
            SummaryStat::UniqueCount => "summary_hint.unique_count",
            SummaryStat::DistinctRatio => "summary_hint.distinct_ratio",
            SummaryStat::TotalRows => "summary_hint.total_rows",
        }
    }
}

/// Which `SUMMARIZE` column each direct-mapped statistic reads from.
/// Derived statistics (NullCount, DistinctRatio, TotalRows) are computed
/// separately and have no entry here.
fn summarize_field(stat: SummaryStat) -> Option<&'static str> {
    match stat {
        SummaryStat::ColumnName => Some("column_name"),
        SummaryStat::Type => Some("column_type"),
        SummaryStat::Min => Some("min"),
        SummaryStat::Max => Some("max"),
        SummaryStat::Mean => Some("avg"),
        SummaryStat::Median => Some("q50"),
        SummaryStat::Std => Some("std"),
        SummaryStat::Q25 => Some("q25"),
        SummaryStat::Q75 => Some("q75"),
        SummaryStat::NullPercent => Some("null_percentage"),
        // Derived: NotNull/Null from `count` + `null_percentage` (SUMMARIZE's
        // `count` is the *total* row count, not the non-null count); Unique and
        // DistinctRatio from an exact `COUNT(DISTINCT)` pass (SUMMARIZE's
        // `approx_unique` is a HyperLogLog estimate that can exceed the row
        // count); TotalRows from the snapshot.
        SummaryStat::UniqueCount
        | SummaryStat::NotNullCount
        | SummaryStat::NullCount
        | SummaryStat::DistinctRatio
        | SummaryStat::TotalRows => None,
    }
}

/// Quote a column name as a DuckDB identifier (double quotes, internal quotes
/// doubled) so names with spaces or punctuation survive in a SELECT.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
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

/// The statistics actually rendered, in canonical (variant) order: every
/// mandatory stat plus any the caller enabled. Order is independent of the
/// order of `enabled`.
pub fn active_stats(enabled: &[SummaryStat]) -> Vec<SummaryStat> {
    SummaryStat::iter()
        .filter(|s| s.is_mandatory() || enabled.contains(s))
        .collect()
}

/// Build the Summary table: one row per source column, one column per active
/// statistic, with localized column titles. Pure apart from the i18n lookup
/// for titles. Runs a single `SUMMARIZE` over `snap`.
pub fn build_summary_table(snap: &DataTable, enabled: &[SummaryStat]) -> anyhow::Result<DataTable> {
    let total_rows = snap.row_count();
    let outcome = crate::sql::run_query(snap, "SUMMARIZE data")?;
    let summ = outcome.table;

    let field_idx = |name: &str| summ.columns.iter().position(|c| c.name == name);
    let active = active_stats(enabled);

    let columns: Vec<ColumnInfo> = active
        .iter()
        .map(|s| ColumnInfo {
            name: crate::i18n::t(s.i18n_key()),
            data_type: "Utf8".to_string(),
        })
        .collect();

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

    let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(summ.row_count());
    for r in 0..summ.row_count() {
        let unique: Option<i64> = exact_unique.get(r).copied().flatten();
        // SUMMARIZE's `count` is the total row count; the non-null and null
        // counts come from it and `null_percentage`.
        let null_pct: Option<f64> = cell_str(r, "null_percentage").and_then(|s| s.parse().ok());
        let null_count = null_pct.map(|p| (total_rows as f64 * p / 100.0).round());
        let not_null = null_count.map(|n| (total_rows as f64 - n).max(0.0));

        let row: Vec<CellValue> = active
            .iter()
            .map(|stat| {
                let text = match stat {
                    SummaryStat::NotNullCount => match not_null {
                        Some(n) => format!("{}", n as i64),
                        None => String::new(),
                    },
                    SummaryStat::NullCount => match null_count {
                        Some(n) => format!("{}", n as i64),
                        None => String::new(),
                    },
                    SummaryStat::UniqueCount => match unique {
                        Some(u) => format!("{u}"),
                        None => String::new(),
                    },
                    SummaryStat::DistinctRatio => match unique {
                        Some(u) if total_rows > 0 => {
                            format!("{:.3}", u as f64 / total_rows as f64)
                        }
                        _ => String::new(),
                    },
                    SummaryStat::TotalRows => format!("{total_rows}"),
                    other => summarize_field(*other)
                        .and_then(|f| cell_str(r, f))
                        .unwrap_or_default(),
                };
                CellValue::String(text)
            })
            .collect();
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        edits: std::collections::HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: std::collections::HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    fn sample_table() -> DataTable {
        // 4 rows: one null in `score`, distinct ids.
        let columns = vec![
            ColumnInfo {
                name: "id".to_string(),
                data_type: "Int64".to_string(),
            },
            ColumnInfo {
                name: "score".to_string(),
                data_type: "Float64".to_string(),
            },
        ];
        let rows = vec![
            vec![CellValue::Int(1), CellValue::Float(10.0)],
            vec![CellValue::Int(2), CellValue::Float(20.0)],
            vec![CellValue::Int(3), CellValue::Float(30.0)],
            vec![CellValue::Int(4), CellValue::Null],
        ];
        DataTable {
            columns,
            rows,
            edits: std::collections::HashMap::new(),
            source_path: None,
            format_name: None,
            structural_changes: false,
            total_rows: None,
            row_offset: 0,
            marks: std::collections::HashMap::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            db_meta: None,
        }
    }

    #[test]
    fn active_stats_always_includes_name_and_type() {
        let active = active_stats(&[SummaryStat::Min]);
        assert_eq!(active[0], SummaryStat::ColumnName);
        assert_eq!(active[1], SummaryStat::Type);
        assert!(active.contains(&SummaryStat::Min));
        assert!(!active.contains(&SummaryStat::Max));
    }

    #[test]
    fn active_stats_preserve_canonical_order() {
        // Pass enabled out of order; output must stay in variant order.
        let active = active_stats(&[SummaryStat::Max, SummaryStat::Min]);
        let min_pos = active.iter().position(|s| *s == SummaryStat::Min).unwrap();
        let max_pos = active.iter().position(|s| *s == SummaryStat::Max).unwrap();
        assert!(min_pos < max_pos);
    }

    #[test]
    fn build_summary_has_one_row_per_column() {
        let t = sample_table();
        let out = build_summary_table(&t, &SummaryStat::default_enabled()).unwrap();
        assert_eq!(out.row_count(), 2); // id + score
        // First two output columns are name + type.
        assert_eq!(
            out.columns.len(),
            active_stats(&SummaryStat::default_enabled()).len()
        );
    }

    #[test]
    fn derived_null_and_total_counts() {
        let t = sample_table();
        let enabled = SummaryStat::default_enabled();
        let out = build_summary_table(&t, &enabled).unwrap();
        let active = active_stats(&enabled);

        let col = |stat: SummaryStat| active.iter().position(|s| *s == stat).unwrap();
        let name_col = col(SummaryStat::ColumnName);
        let null_col = col(SummaryStat::NullCount);
        let not_null_col = col(SummaryStat::NotNullCount);
        let total_col = col(SummaryStat::TotalRows);

        // Find the `score` row (has one null).
        let score_row = (0..out.row_count())
            .find(|&r| out.get(r, name_col).map(|v| v.to_string()) == Some("score".to_string()))
            .unwrap();
        assert_eq!(
            out.get(score_row, null_col).map(|v| v.to_string()),
            Some("1".to_string())
        );
        assert_eq!(
            out.get(score_row, not_null_col).map(|v| v.to_string()),
            Some("3".to_string())
        );
        assert_eq!(
            out.get(score_row, total_col).map(|v| v.to_string()),
            Some("4".to_string())
        );
    }

    #[test]
    fn unique_count_is_exact_and_never_exceeds_rows() {
        // `id` has 4 distinct values; `score` has 3 distinct (one null) over
        // 4 rows. Exact COUNT(DISTINCT) must report these, never more.
        let t = sample_table();
        let enabled = SummaryStat::default_enabled();
        let out = build_summary_table(&t, &enabled).unwrap();
        let active = active_stats(&enabled);
        let col = |stat: SummaryStat| active.iter().position(|s| *s == stat).unwrap();
        let name_col = col(SummaryStat::ColumnName);
        let uniq_col = col(SummaryStat::UniqueCount);
        let ratio_col = col(SummaryStat::DistinctRatio);

        let row_for = |want: &str| {
            (0..out.row_count())
                .find(|&r| out.get(r, name_col).map(|v| v.to_string()) == Some(want.to_string()))
                .unwrap()
        };
        let id_row = row_for("id");
        let score_row = row_for("score");
        assert_eq!(
            out.get(id_row, uniq_col).map(|v| v.to_string()),
            Some("4".to_string())
        );
        assert_eq!(
            out.get(score_row, uniq_col).map(|v| v.to_string()),
            Some("3".to_string())
        );
        // Distinct ratio = unique / total rows, in [0, 1].
        assert_eq!(
            out.get(score_row, ratio_col).map(|v| v.to_string()),
            Some("0.750".to_string())
        );
    }
}
