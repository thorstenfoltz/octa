//! Has this dataset changed shape since last time?
//!
//! Two versions of the same table go in, one report comes out: which columns
//! appeared or disappeared, and per shared column how its null rate, distinct
//! count, range and category values moved. Optional thresholds turn the report
//! into a gate, which is what the `--drift-report` exit code is for.
//!
//! Computes no statistic of its own. Every number is read out of
//! [`summary::build_summary_table`](crate::data::summary::build_summary_table),
//! called once per side, and the category lists come from
//! [`value_frequency`](crate::data::value_frequency). A second implementation
//! here would drift away from what the Summary tab shows, which is exactly the
//! bug this feature exists to catch in the user's data.

use std::collections::{BTreeSet, HashMap};

use crate::data::summary::{SummaryStat, build_summary_table, infer_column_type, num_cell};
use crate::data::value_frequency::{BinningMode, compute_value_frequency};
use crate::data::{CellValue, ColumnInfo, DataTable, is_numeric_data_type};

/// Columns with more distinct values than this are not compared value by
/// value: listing the new entries of a free-text column is noise, not drift.
pub const DEFAULT_CATEGORY_CAP: usize = 50;

/// At most this many category values are named before the report says how many
/// more there were.
const MAX_LISTED_VALUES: usize = 10;

/// One `metric:max_relative_change` gate. `metric` is a metric name as it
/// appears in [`ColumnDrift::metric`]; a threshold naming a metric that never
/// appears is simply never applied.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftThreshold {
    pub metric: String,
    pub max_relative_change: f64,
}

/// How to compare two profiles.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftOptions {
    pub category_cap: usize,
    pub thresholds: Vec<DriftThreshold>,
}

impl Default for DriftOptions {
    fn default() -> Self {
        Self {
            category_cap: DEFAULT_CATEGORY_CAP,
            thresholds: Vec::new(),
        }
    }
}

/// Parse a `metric:change` list, for example `null_rate:0.05,rows:0.1`.
///
/// An empty string is an empty list, not an error; anything else missing its
/// colon or carrying an unparseable number is rejected, because a silently
/// ignored gate is worse than no gate.
pub fn parse_thresholds(s: &str) -> anyhow::Result<Vec<DriftThreshold>> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (metric, value) = part
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("threshold '{part}' is not metric:change"))?;
        let metric = metric.trim();
        if metric.is_empty() {
            anyhow::bail!("threshold '{part}' has no metric name");
        }
        let max_relative_change: f64 = value
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("threshold '{part}' has a non-numeric change"))?;
        if !max_relative_change.is_finite() || max_relative_change < 0.0 {
            anyhow::bail!("threshold '{part}' must be zero or a positive number");
        }
        out.push(DriftThreshold {
            metric: metric.to_string(),
            max_relative_change,
        });
    }
    Ok(out)
}

/// One measured difference. `before` / `after` carry the numeric form where
/// there is one; `before_text` / `after_text` always carry something readable,
/// which is how the category rows say `PL, RO` rather than a count.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDrift {
    pub column: String,
    pub metric: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub before_text: String,
    pub after_text: String,
    pub relative_change: Option<f64>,
    pub breached: bool,
}

/// The full comparison. `failed` is true when any row breached a threshold.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DriftReport {
    pub rows: Vec<ColumnDrift>,
    pub added_columns: Vec<String>,
    pub removed_columns: Vec<String>,
    pub rows_before: usize,
    pub rows_after: usize,
    pub failed: bool,
}

/// The statistics read out of the Summary pass. Deliberately short: every one
/// of these is used below, and each extra stat costs another DuckDB pass.
const PROFILE_STATS: &[SummaryStat] = &[
    SummaryStat::ColumnName,
    SummaryStat::Type,
    SummaryStat::Min,
    SummaryStat::Max,
    SummaryStat::Mean,
    SummaryStat::NullPercent,
    SummaryStat::UniqueCount,
];

/// One column's statistics, keyed by name in [`profile`].
#[derive(Debug, Clone, Default)]
struct ColumnProfile {
    index: usize,
    data_type: String,
    min: Option<f64>,
    max: Option<f64>,
    mean: Option<f64>,
    null_rate: Option<f64>,
    distinct: Option<f64>,
}

/// Run the Summary pass over one side and index it by column name.
fn profile(t: &DataTable) -> anyhow::Result<HashMap<String, ColumnProfile>> {
    let mut out = HashMap::new();
    if t.col_count() == 0 {
        return Ok(out);
    }
    let summary = build_summary_table(t, PROFILE_STATS)?;
    let field = |id: &str| summary.columns.iter().position(|c| c.name == id);
    let (name_i, type_i) = match (field("column_name"), field("type")) {
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(out),
    };
    let num = |row: usize, id: &str| -> Option<f64> {
        let ci = field(id)?;
        summary.get(row, ci)?.to_string().trim().parse::<f64>().ok()
    };

    for r in 0..summary.row_count() {
        let Some(name) = summary.get(r, name_i).map(|c| c.to_string()) else {
            continue;
        };
        let Some(index) = t.columns.iter().position(|c| c.name == name) else {
            continue;
        };
        out.insert(
            name,
            ColumnProfile {
                index,
                data_type: summary
                    .get(r, type_i)
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                min: num(r, "min"),
                max: num(r, "max"),
                mean: num(r, "mean"),
                // Summary reports a percentage; drift reports a rate, so a
                // 0.05 threshold means five percentage points and not five.
                null_rate: num(r, "null_percent").map(|p| p / 100.0),
                distinct: num(r, "unique_count"),
            },
        );
    }
    Ok(out)
}

/// Distinct values of one column as text, or `None` when the column has more
/// than `cap` of them.
fn categories(t: &DataTable, col: usize, cap: usize) -> Option<BTreeSet<String>> {
    let vf = compute_value_frequency(t, col, None, BinningMode::None)?;
    if vf.unique_count > cap {
        return None;
    }
    Some(vf.rows.into_iter().map(|r| r.label).collect())
}

/// `a, b, c and 4 more`, or an empty string.
fn list_values(values: &BTreeSet<String>) -> String {
    let shown: Vec<&str> = values
        .iter()
        .take(MAX_LISTED_VALUES)
        .map(String::as_str)
        .collect();
    let mut s = shown.join(", ");
    if values.len() > shown.len() {
        s.push_str(&format!(" and {} more", values.len() - shown.len()));
    }
    s
}

fn fmt_num(v: Option<f64>) -> String {
    v.map(|x| num_cell(x).to_string()).unwrap_or_default()
}

/// Compare two versions of the same table.
///
/// Columns are matched by name. A column present on one side only is reported
/// as added or removed and produces no metric rows: there is nothing to compare
/// it against, and inventing a zero baseline would breach every threshold.
pub fn compare_profiles(
    a: &DataTable,
    b: &DataTable,
    opts: &DriftOptions,
) -> anyhow::Result<DriftReport> {
    let cap = opts.category_cap;
    let pa = profile(a)?;
    let pb = profile(b)?;

    let mut report = DriftReport {
        rows_before: a.row_count(),
        rows_after: b.row_count(),
        ..Default::default()
    };
    report.removed_columns = a
        .columns
        .iter()
        .filter(|c| !pb.contains_key(&c.name))
        .map(|c| c.name.clone())
        .collect();
    report.added_columns = b
        .columns
        .iter()
        .filter(|c| !pa.contains_key(&c.name))
        .map(|c| c.name.clone())
        .collect();

    let threshold_for = |metric: &str| -> Option<f64> {
        opts.thresholds
            .iter()
            .find(|t| t.metric == metric)
            .map(|t| t.max_relative_change)
    };

    // A row whose numbers can be compared: relative change where the baseline
    // is non-zero, and a breach when the gate exists and the change exceeds it.
    // A zero baseline that moved at all counts as an unbounded change, which is
    // the only reading that makes a null rate of 0.0 -> 0.5 fail a gate.
    let numeric_row = |column: &str, metric: &str, before: Option<f64>, after: Option<f64>| {
        let relative_change = match (before, after) {
            (Some(x), Some(y)) if x != 0.0 => Some((y - x) / x.abs()),
            _ => None,
        };
        let breached = match (threshold_for(metric), before, after) {
            (Some(max), Some(x), Some(y)) if x != 0.0 => ((y - x) / x.abs()).abs() > max,
            (Some(_), Some(x), Some(y)) => x != y,
            _ => false,
        };
        ColumnDrift {
            column: column.to_string(),
            metric: metric.to_string(),
            before,
            after,
            before_text: fmt_num(before),
            after_text: fmt_num(after),
            relative_change,
            breached,
        }
    };

    report.rows.push(numeric_row(
        "",
        "rows",
        Some(report.rows_before as f64),
        Some(report.rows_after as f64),
    ));

    for col in &b.columns {
        let (Some(before), Some(after)) = (pa.get(&col.name), pb.get(&col.name)) else {
            continue;
        };
        report.rows.push(numeric_row(
            &col.name,
            "null_rate",
            before.null_rate,
            after.null_rate,
        ));
        report.rows.push(numeric_row(
            &col.name,
            "distinct_count",
            before.distinct,
            after.distinct,
        ));

        if is_numeric_data_type(&before.data_type) && is_numeric_data_type(&after.data_type) {
            report
                .rows
                .push(numeric_row(&col.name, "min", before.min, after.min));
            report
                .rows
                .push(numeric_row(&col.name, "max", before.max, after.max));
            report
                .rows
                .push(numeric_row(&col.name, "mean", before.mean, after.mean));
        }

        let (Some(was), Some(now)) = (
            categories(a, before.index, cap),
            categories(b, after.index, cap),
        ) else {
            continue;
        };
        let appeared: BTreeSet<String> = now.difference(&was).cloned().collect();
        let vanished: BTreeSet<String> = was.difference(&now).cloned().collect();
        if !appeared.is_empty() {
            report.rows.push(ColumnDrift {
                column: col.name.clone(),
                metric: "new_values".to_string(),
                before: None,
                after: Some(appeared.len() as f64),
                before_text: String::new(),
                after_text: list_values(&appeared),
                relative_change: None,
                breached: false,
            });
        }
        if !vanished.is_empty() {
            report.rows.push(ColumnDrift {
                column: col.name.clone(),
                metric: "vanished_values".to_string(),
                before: Some(vanished.len() as f64),
                after: None,
                before_text: list_values(&vanished),
                after_text: String::new(),
                relative_change: None,
                breached: false,
            });
        }
    }

    report.failed = report.rows.iter().any(|r| r.breached);
    Ok(report)
}

/// The report as a table, for the CLI, the MCP tool and the result tab.
pub fn report_table(report: &DriftReport) -> DataTable {
    let ids = ["column", "metric", "before", "after", "change", "breached"];
    let rows: Vec<Vec<CellValue>> = report
        .rows
        .iter()
        .map(|d| {
            let side = |value: Option<f64>, text: &str| match value {
                Some(x) => num_cell(x),
                None => CellValue::String(text.to_string()),
            };
            vec![
                CellValue::String(d.column.clone()),
                CellValue::String(d.metric.clone()),
                side(d.before, &d.before_text),
                side(d.after, &d.after_text),
                d.relative_change
                    .map(num_cell)
                    .unwrap_or_else(|| CellValue::String(String::new())),
                CellValue::Bool(d.breached),
            ]
        })
        .collect();

    let mut t = DataTable::empty();
    t.columns = ids
        .iter()
        .enumerate()
        .map(|(ci, id)| ColumnInfo {
            name: (*id).to_string(),
            data_type: infer_column_type(rows.iter().map(|r| r[ci].clone())),
        })
        .collect();
    t.rows = rows;
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    fn table(col: &str, ty: &str, vals: Vec<CellValue>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: col.into(),
            data_type: ty.into(),
        }];
        t.rows = vals.into_iter().map(|v| vec![v]).collect();
        t
    }

    #[test]
    fn reports_a_null_rate_increase() {
        let a = table(
            "amount",
            "Int64",
            vec![CellValue::Int(1), CellValue::Int(2)],
        );
        let b = table("amount", "Int64", vec![CellValue::Int(1), CellValue::Null]);
        let r = compare_profiles(&a, &b, &DriftOptions::default()).unwrap();
        let null_row = r
            .rows
            .iter()
            .find(|d| d.column == "amount" && d.metric == "null_rate")
            .expect("null_rate row missing");
        assert_eq!(null_row.before, Some(0.0));
        assert_eq!(null_row.after, Some(0.5));
    }

    #[test]
    fn reports_added_and_removed_columns() {
        let mut a = DataTable::empty();
        a.columns = vec![
            ColumnInfo {
                name: "keep".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "gone".into(),
                data_type: "Utf8".into(),
            },
        ];
        a.rows = vec![vec![
            CellValue::String("x".into()),
            CellValue::String("y".into()),
        ]];
        let mut b = DataTable::empty();
        b.columns = vec![
            ColumnInfo {
                name: "keep".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "fresh".into(),
                data_type: "Utf8".into(),
            },
        ];
        b.rows = vec![vec![
            CellValue::String("x".into()),
            CellValue::String("z".into()),
        ]];

        let r = compare_profiles(&a, &b, &DriftOptions::default()).unwrap();
        assert_eq!(r.removed_columns, vec!["gone".to_string()]);
        assert_eq!(r.added_columns, vec!["fresh".to_string()]);
    }

    #[test]
    fn reports_new_and_vanished_categories() {
        let a = table(
            "country",
            "Utf8",
            vec![
                CellValue::String("DE".into()),
                CellValue::String("PL".into()),
            ],
        );
        let b = table(
            "country",
            "Utf8",
            vec![
                CellValue::String("DE".into()),
                CellValue::String("UA".into()),
            ],
        );
        let r = compare_profiles(&a, &b, &DriftOptions::default()).unwrap();
        let new = r.rows.iter().find(|d| d.metric == "new_values").unwrap();
        assert!(new.after_text.contains("UA"), "got {}", new.after_text);
        let gone = r
            .rows
            .iter()
            .find(|d| d.metric == "vanished_values")
            .unwrap();
        assert!(gone.before_text.contains("PL"), "got {}", gone.before_text);
    }

    #[test]
    fn a_breached_threshold_fails_the_report() {
        let a = table(
            "amount",
            "Int64",
            vec![CellValue::Int(1), CellValue::Int(2)],
        );
        let b = table("amount", "Int64", vec![CellValue::Int(1), CellValue::Null]);
        let opts = DriftOptions {
            thresholds: parse_thresholds("null_rate:0.05").unwrap(),
            ..Default::default()
        };
        let r = compare_profiles(&a, &b, &opts).unwrap();
        assert!(r.failed, "0.0 -> 0.5 must breach a 0.05 threshold");
        assert!(r.rows.iter().any(|d| d.breached));
    }

    #[test]
    fn thresholds_reject_a_malformed_spec() {
        assert!(parse_thresholds("null_rate").is_err());
        assert!(parse_thresholds("null_rate:abc").is_err());
        let ok = parse_thresholds("null_rate:0.05,rows:0.1").unwrap();
        assert_eq!(ok.len(), 2);
    }

    #[test]
    fn identical_tables_have_no_breach_and_equal_row_counts() {
        let a = table("x", "Int64", vec![CellValue::Int(1)]);
        let r = compare_profiles(&a, &a, &DriftOptions::default()).unwrap();
        assert!(!r.failed);
        assert_eq!(r.rows_before, r.rows_after);
    }
}
