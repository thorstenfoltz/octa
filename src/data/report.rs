//! A profiling report as one self-contained HTML document.
//!
//! This module **invents no analysis**. Every number comes from the engine
//! that already produces it for the corresponding tab: `summary` for the
//! statistics, `value_frequency` for the top values, `correlation` for the
//! matrix, and `chart` plus `chart_export` for the pictures. A second
//! implementation of any of them would drift from what the user sees on
//! screen, which is the one thing a report must never do.
//!
//! Self-contained means inline CSS, inline SVG and no JavaScript, so the file
//! opens from a mail attachment on a machine with no internet.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::DataTable;
use crate::data::summary::{SummaryStat, build_summary_table};

/// Which parts of the report to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSection {
    Stats,
    Distributions,
    TopValues,
    Correlation,
}

impl ReportSection {
    pub const ALL: &'static [ReportSection] = &[
        ReportSection::Stats,
        ReportSection::Distributions,
        ReportSection::TopValues,
        ReportSection::Correlation,
    ];

    /// Stable identifier for the CLI's `--report-sections` list.
    pub fn id(self) -> &'static str {
        match self {
            ReportSection::Stats => "stats",
            ReportSection::Distributions => "distributions",
            ReportSection::TopValues => "top_values",
            ReportSection::Correlation => "correlation",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.id().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Debug, Clone)]
pub struct ReportOptions {
    pub sections: Vec<ReportSection>,
    /// `None` = full pass. `Some(n)` = a random sample of `n` rows, stated in
    /// the output.
    pub sample_rows: Option<usize>,
    pub title: String,
    /// How many columns get a chart before the rest are named and skipped. A
    /// 500-column table would otherwise produce a document nobody scrolls.
    pub max_charted_columns: usize,
    /// How many values the top-values section lists per column.
    pub top_values: usize,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            sections: ReportSection::ALL.to_vec(),
            sample_rows: None,
            title: "Data report".to_string(),
            max_charted_columns: 50,
            top_values: 10,
        }
    }
}

const STYLE: &str = "\
body{font-family:system-ui,sans-serif;margin:2rem;max-width:70rem;color:#222;background:#fff}
h1{font-size:1.6rem}h2{font-size:1.2rem;margin-top:2.5rem;border-bottom:1px solid #ddd}
h3{font-size:1rem;margin-top:1.5rem;font-weight:600}
table{border-collapse:collapse;margin:1rem 0;font-size:.9rem}
th,td{border:1px solid #ddd;padding:.3rem .6rem;text-align:left}
th{background:#f5f5f5}td.num{text-align:right;font-variant-numeric:tabular-nums}
.note{color:#666;font-size:.9rem}
figure{margin:1rem 0}svg{max-width:100%;height:auto}
@media(prefers-color-scheme:dark){body{background:#161616;color:#e8e8e8}
th{background:#242424}th,td{border-color:#3a3a3a}.note{color:#aaa}}";

/// Escape text for HTML. Column names and cell values are user data and can
/// contain anything, including markup.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn check(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }
    Ok(())
}

/// Render a `DataTable` as an HTML table.
fn table_html(t: &DataTable) -> String {
    let mut out = String::from("<table><thead><tr>");
    for c in &t.columns {
        out.push_str(&format!("<th>{}</th>", esc(&c.name)));
    }
    out.push_str("</tr></thead><tbody>");
    for row in 0..t.row_count() {
        out.push_str("<tr>");
        for col in 0..t.col_count() {
            let cell = t.get(row, col).map(|c| c.to_string()).unwrap_or_default();
            let numeric = cell.parse::<f64>().is_ok();
            out.push_str(&format!(
                "<td{}>{}</td>",
                if numeric { " class=\"num\"" } else { "" },
                esc(&cell)
            ));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    out
}

/// Build the report.
///
/// `filtered_rows` is the caller's current view, so a report follows the
/// active filter exactly as the Summary tab does. Sampling draws from that
/// view, never from the whole table: a report that says "50 of 500 rows
/// examined" under an active filter must not have examined rows the filter
/// hides.
pub fn build_report(
    table: &DataTable,
    filtered_rows: &[usize],
    opts: &ReportOptions,
    cancel: &AtomicBool,
) -> anyhow::Result<String> {
    check(cancel)?;

    let view = table.clone_with_rows(filtered_rows);
    let total = view.row_count();
    let (working, examined) = match opts.sample_rows {
        Some(n) if n < total => {
            let picked = crate::data::sample::sample_row_indices(&view, n, 0);
            let taken = picked.len();
            (view.clone_with_rows(&picked), taken)
        }
        _ => (view, total),
    };

    let mut body = String::new();
    body.push_str(&format!("<h1>{}</h1>", esc(&opts.title)));
    body.push_str(&format!(
        "<p class=\"note\">{} column(s), {} row(s){}</p>",
        working.col_count(),
        total,
        if examined < total {
            format!(", sampled: {examined} of {total} rows examined")
        } else {
            String::new()
        }
    ));

    for section in &opts.sections {
        check(cancel)?;
        match section {
            ReportSection::Stats => body.push_str(&stats_section(&working)?),
            ReportSection::Distributions => body.push_str(&distributions_section(&working, opts)),
            ReportSection::TopValues => body.push_str(&top_values_section(&working, opts)),
            ReportSection::Correlation => body.push_str(&correlation_section(&working)),
        }
    }

    Ok(format!(
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{}</title><style>{STYLE}</style></head><body>{body}</body></html>\n",
        esc(&opts.title)
    ))
}

/// One chart per column: a histogram for numbers, a bar of the most common
/// categories for everything else. Rendered through the same exporter the
/// Chart tab's "Export SVG" uses, so a report picture and an exported picture
/// are the same picture.
fn distributions_section(table: &DataTable, opts: &ReportOptions) -> String {
    use crate::data::chart::{
        Aggregation, ChartConfig, ChartKind, ChartLimits, LegendPosition, SeriesStyle, build_chart,
    };
    use crate::data::chart_export::{ExportOptions, to_svg};
    use crate::data::is_numeric_data_type;

    if table.col_count() == 0 || table.row_count() == 0 {
        return String::new();
    }
    let rows: Vec<usize> = (0..table.row_count()).collect();
    let shown = table.col_count().min(opts.max_charted_columns);

    let mut out = String::from("<h2>Distributions</h2>");
    for col in 0..shown {
        let info = &table.columns[col];
        let numeric = is_numeric_data_type(&info.data_type);
        let cfg = ChartConfig {
            kind: if numeric {
                ChartKind::Histogram
            } else {
                ChartKind::Bar
            },
            x_col: Some(col),
            y_cols: if numeric { Vec::new() } else { vec![col] },
            // A categorical column's distribution is how often each value
            // occurs, so the bar counts rows. Summing a text column would
            // aggregate nothing and draw an empty chart.
            agg: Aggregation::Count,
            ..ChartConfig::default()
        };
        let Ok(prep) = build_chart(table, &rows, &cfg, ChartLimits::default()) else {
            // A column a chart cannot express (all null, too many categories)
            // is not an error; it simply has no picture.
            continue;
        };
        let export = ExportOptions::from_prep(
            &prep,
            info.name.clone(),
            "",
            "",
            LegendPosition::Off,
            |_| SeriesStyle::default(),
        );
        out.push_str(&format!(
            "<figure><h3>{}</h3>{}</figure>",
            esc(&info.name),
            to_svg(&prep, &export)
        ));
    }

    let omitted = table.col_count().saturating_sub(shown);
    if omitted > 0 {
        out.push_str(&format!(
            "<p class=\"note\">{omitted} more column(s) not charted; raise the chart limit to include them.</p>"
        ));
    }
    out
}

fn top_values_section(table: &DataTable, opts: &ReportOptions) -> String {
    use crate::data::value_frequency::{BinningMode, compute_value_frequency};

    if table.col_count() == 0 {
        return String::new();
    }
    let mut out = String::from("<h2>Most common values</h2>");
    for col in 0..table.col_count() {
        let Some(vf) =
            compute_value_frequency(table, col, Some(opts.top_values), BinningMode::None)
        else {
            continue;
        };
        out.push_str(&format!(
            "<h3>{}</h3><table><thead><tr><th>value</th><th>count</th><th>share</th>\
</tr></thead><tbody>",
            esc(&vf.column_name)
        ));
        for row in &vf.rows {
            let share = if vf.total_non_null == 0 {
                0.0
            } else {
                row.count as f64 * 100.0 / vf.total_non_null as f64
            };
            out.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{share:.1}%</td></tr>",
                esc(&row.label),
                row.count
            ));
        }
        out.push_str("</tbody></table>");
        out.push_str(&format!(
            "<p class=\"note\">{} distinct value(s), {} null(s).</p>",
            vf.unique_count, vf.nulls
        ));
    }
    out
}

fn correlation_section(table: &DataTable) -> String {
    use crate::data::correlation::{CorrMethod, correlation_matrix, matrix_to_table};

    let m = correlation_matrix(table, CorrMethod::Pearson);
    if m.columns.len() < 2 {
        // Fewer than two numeric columns: nothing to correlate, which is an
        // answer rather than an error. One column would render a 1x1 matrix
        // of its correlation with itself, which is always 1 and always noise.
        return String::new();
    }
    format!("<h2>Correlation</h2>{}", table_html(&matrix_to_table(&m)))
}

fn stats_section(table: &DataTable) -> anyhow::Result<String> {
    if table.col_count() == 0 {
        return Ok(String::new());
    }
    let stats = build_summary_table(table, &SummaryStat::default_enabled())?;
    Ok(format!("<h2>Columns</h2>{}", table_html(&stats)))
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
