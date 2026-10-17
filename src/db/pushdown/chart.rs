//! Charts on the server: the same [`ChartPrep`] the local builder
//! (`crate::data::chart::build_chart`) returns, computed by the database over
//! every row, so the renderer and the exports do not know which path made
//! it. A column is read as the local chart reads it (`cell_to_f64`), decided
//! from the loaded rows ([`column_reads`]).

use std::sync::atomic::AtomicBool;

use crate::data::chart::{
    Aggregation, BoxSummary, ChartConfig, ChartData, ChartError, ChartKind, ChartLimits, ChartPrep,
    DEFAULT_MAX_POINTS, MAX_HIST_BINS, XAxisKind, build_chart, cell_to_f64, sturges_bins,
};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{
    as_float, distinct_key, epoch_days, epoch_seconds, float_lit, modulo, quartiles_sql,
    subquery_alias,
};
use super::{ServerSource, cell_f64, check_cancel};

/// How the local chart reads a column's cells, so the server reads it alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnRead {
    /// Date cells: days since 1970-01-01.
    Date,
    /// DateTime cells: seconds since 1970-01-01.
    DateTime,
    /// Numbers, or text that reads as numbers (Postgres NUMERIC, MySQL
    /// DECIMAL arrive as text).
    Number,
    /// Words, Booleans, nothing: not a number, as locally.
    Other,
}

/// Each column's [`ColumnRead`], from the first 64 non-empty loaded cells:
/// the first one's kind for dates (as the chart's axis sniffing does), any
/// cell reading as a number for numbers. A column with no loaded value (a
/// sparse one whose first page is empty) goes by its type, so the database
/// still reads the values the loaded rows lack.
pub fn column_reads(table: &DataTable) -> Vec<ColumnRead> {
    (0..table.col_count())
        .map(|c| {
            let cells: Vec<&CellValue> = table
                .rows
                .iter()
                .filter_map(|r| r.get(c))
                .filter(|v| !matches!(v, CellValue::Null))
                .take(64)
                .collect();
            match cells.first() {
                Some(CellValue::Date(_)) => ColumnRead::Date,
                Some(CellValue::DateTime(_)) => ColumnRead::DateTime,
                None => read_of_type(&table.columns[c].data_type),
                _ if cells.iter().any(|v| cell_to_f64(v).is_some()) => ColumnRead::Number,
                _ => ColumnRead::Other,
            }
        })
        .collect()
}

fn read_of_type(data_type: &str) -> ColumnRead {
    let t = data_type.to_ascii_lowercase();
    if t.starts_with("date") {
        ColumnRead::Date
    } else if t.starts_with("timestamp") {
        ColumnRead::DateTime
    } else if crate::data::quality::is_numeric_type(&t) {
        ColumnRead::Number
    } else {
        ColumnRead::Other
    }
}

/// The part of a chart's settings that changes its data. Everything else
/// (title, labels, legend, colours, ranges, log scale, trend, forecast) is
/// drawn from the data in hand. Fields a kind does not use are left at a
/// fixed value, so changing them never asks the database again.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartKey {
    pub kind: ChartKind,
    pub x_col: Option<usize>,
    pub y_cols: Vec<usize>,
    pub agg: Aggregation,
    pub hist_bins: Option<usize>,
    pub max_points: usize,
    pub max_categories: usize,
}

impl ChartKey {
    pub fn of(cfg: &ChartConfig, limits: ChartLimits) -> Self {
        let k = cfg.kind;
        let lines = matches!(k, ChartKind::Line | ChartKind::Scatter);
        Self {
            kind: k,
            x_col: if k == ChartKind::Box { None } else { cfg.x_col },
            y_cols: if k == ChartKind::Histogram {
                Vec::new()
            } else {
                cfg.y_cols.clone()
            },
            agg: if k == ChartKind::Bar {
                cfg.agg
            } else {
                Aggregation::default()
            },
            hist_bins: if k == ChartKind::Histogram {
                cfg.hist_bins
            } else {
                None
            },
            max_points: if lines { limits.max_points } else { 0 },
            max_categories: if lines || k == ChartKind::Bar {
                limits.max_categories
            } else {
                0
            },
        }
    }

    /// Enough is picked to draw: the local builder's message covers the rest.
    pub fn complete(&self) -> bool {
        match self.kind {
            ChartKind::Histogram => self.x_col.is_some(),
            ChartKind::Box => !self.y_cols.is_empty(),
            ChartKind::Bar | ChartKind::Line | ChartKind::Scatter => {
                self.x_col.is_some() && !self.y_cols.is_empty()
            }
        }
    }

    /// A config the local builder accepts, for the rows Line / Scatter fetch.
    pub fn config(&self) -> ChartConfig {
        ChartConfig {
            kind: self.kind,
            x_col: self.x_col,
            y_cols: self.y_cols.clone(),
            agg: self.agg,
            hist_bins: self.hist_bins,
            ..ChartConfig::default()
        }
    }

    pub fn limits(&self) -> ChartLimits {
        ChartLimits {
            max_points: self.max_points,
            max_categories: self.max_categories,
        }
    }
}

/// One chart to ask the database for: the key, the chart tab's columns (the
/// key's indices point into them), how each is read, and the table key's
/// columns (ordering rows of equal X for Line / Scatter; may be empty).
#[derive(Debug, Clone)]
pub struct ChartRequest {
    pub key: ChartKey,
    pub columns: Vec<ColumnInfo>,
    pub reads: Vec<ColumnRead>,
    pub tie: Vec<String>,
}

/// What the database made of a chart.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerChart {
    /// The chart over every row (or the local builder's error for these
    /// settings); `total` rows in the source.
    Drawn {
        chart: Result<ChartPrep, ChartError>,
        total: usize,
    },
    /// The engine's SQL cannot express it (MySQL has no percentile for a
    /// Box chart): draw from the loaded rows.
    NotExpressible,
}

fn drawn(chart: Result<ChartPrep, ChartError>, total: usize) -> ServerChart {
    ServerChart::Drawn { chart, total }
}

fn read_of(req: &ChartRequest, i: usize) -> ColumnRead {
    req.reads.get(i).copied().unwrap_or(ColumnRead::Other)
}

/// The value the local chart plots for column `col` (quoted), as a double,
/// or `None` for a column it does not read as a number.
fn value_sql(e: DbEngine, col: &str, read: ColumnRead) -> Option<String> {
    match read {
        ColumnRead::Date => Some(epoch_days(e, col)),
        ColumnRead::DateTime => Some(epoch_seconds(e, col)),
        ColumnRead::Number => Some(as_float(e, col)),
        ColumnRead::Other => None,
    }
}

fn axis_kind(read: ColumnRead) -> XAxisKind {
    match read {
        ColumnRead::Date => XAxisKind::Date,
        ColumnRead::DateTime => XAxisKind::DateTime,
        ColumnRead::Number | ColumnRead::Other => XAxisKind::Numeric,
    }
}

/// The X column index, or the error the local builder gives.
fn x_index(req: &ChartRequest) -> Result<usize, ChartError> {
    let x = req.key.x_col.ok_or(ChartError::NoXColumn)?;
    if x >= req.columns.len() {
        return Err(ChartError::XOutOfRange);
    }
    Ok(x)
}

/// The Y column indices, or the error the local builder gives.
fn y_indices(req: &ChartRequest) -> Result<Vec<usize>, ChartError> {
    if req.key.y_cols.is_empty() {
        return Err(ChartError::NoYColumn);
    }
    if req.key.y_cols.iter().any(|&y| y >= req.columns.len()) {
        return Err(ChartError::YOutOfRange);
    }
    Ok(req.key.y_cols.clone())
}

fn count_at(t: &DataTable, c: usize) -> usize {
    t.get(0, c).and_then(cell_f64).unwrap_or(0.0).max(0.0) as usize
}

/// Histogram of X over every row: MIN, MAX and the counts first, then one
/// `GROUP BY` of the bin number, binned exactly as `build_histogram` does
/// (the top value lands in the last bin).
pub fn histogram(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    req: &ChartRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<ServerChart> {
    let e = src.engine();
    let x = match x_index(req) {
        Ok(x) => x,
        Err(err) => return Ok(drawn(Err(err), 0)),
    };
    let name = req.columns[x].name.clone();
    let read = read_of(req, x);
    let Some(v) = value_sql(e, &e.quote_ident(&name), read) else {
        return Ok(drawn(Err(ChartError::XNotNumeric { col: name }), 0));
    };
    let from = src.from_sql();
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT MIN({v}), MAX({v}), COUNT({v}), COUNT(*) FROM {from}"
    ))?;
    let total = count_at(&t, 3);
    if total == 0 {
        return Ok(drawn(Err(ChartError::EmptyAfterFilter), 0));
    }
    let (Some(min), Some(max)) = (
        t.get(0, 0).and_then(cell_f64),
        t.get(0, 1).and_then(cell_f64),
    ) else {
        return Ok(drawn(Err(ChartError::XNotNumeric { col: name }), total));
    };
    let bins = req
        .key
        .hist_bins
        .unwrap_or_else(|| sturges_bins(count_at(&t, 2)))
        .clamp(1, MAX_HIST_BINS);
    let width = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        (max - min) / bins as f64
    };
    let last = bins - 1;
    let f = format!("FLOOR(({v} - {}) / {})", float_lit(min), float_lit(width));
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT octa_b, COUNT(*) FROM (SELECT CASE WHEN {f} >= {last} THEN {last} ELSE {f} END AS octa_b \
         FROM {from} WHERE {v} IS NOT NULL){} GROUP BY octa_b",
        subquery_alias(e, "x")
    ))?;
    let mut counts = vec![0usize; bins];
    for r in 0..t.row_count() {
        let (Some(i), Some(k)) = (
            t.get(r, 0).and_then(cell_f64),
            t.get(r, 1).and_then(cell_f64),
        ) else {
            continue;
        };
        if let Some(slot) = counts.get_mut(i.max(0.0) as usize) {
            *slot += k as usize;
        }
    }
    let bins = counts
        .into_iter()
        .enumerate()
        .map(|(i, k)| (min + width * i as f64, k as f64))
        .collect();
    Ok(drawn(
        Ok(ChartPrep {
            data: ChartData::Histogram {
                bins,
                bin_width: width,
            },
            total_rows: total,
            used_rows: total,
            x_label: name,
            y_label: "Count".to_string(),
            x_axis_kind: axis_kind(read),
        }),
        total,
    ))
}

fn agg_fn(agg: Aggregation) -> Option<&'static str> {
    match agg {
        Aggregation::Sum => Some("SUM"),
        Aggregation::Avg => Some("AVG"),
        Aggregation::Min => Some("MIN"),
        Aggregation::Max => Some("MAX"),
        Aggregation::Count => None,
    }
}

/// One bar per distinct X (NULL is its own bar, "(null)", last), ordered by
/// X: a database has no row order to keep. The categories are counted first
/// (by exact text, so MySQL's and SQL Server's case-blind collations do not
/// merge `B` and `b`); more than the cap is the local "too many categories"
/// error with the exact count. Count counts rows, as locally; with any other
/// aggregate a Y column that is not a number gets no bars.
pub fn bar(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    req: &ChartRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<ServerChart> {
    let e = src.engine();
    let (x, ys) = match (x_index(req), y_indices(req)) {
        (Ok(x), Ok(ys)) => (x, ys),
        (Err(err), _) | (_, Err(err)) => return Ok(drawn(Err(err), 0)),
    };
    let xi = &req.columns[x];
    let xq = e.quote_ident(&xi.name);
    let group = distinct_key(e, &xq, &xi.data_type);
    let from = src.from_sql();
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT COUNT(DISTINCT {group}), COUNT(*) - COUNT({xq}), COUNT(*) FROM {from}"
    ))?;
    let total = count_at(&t, 2);
    if total == 0 {
        return Ok(drawn(Err(ChartError::EmptyAfterFilter), 0));
    }
    let categories = count_at(&t, 0) + usize::from(count_at(&t, 1) > 0);
    if categories > req.key.max_categories {
        return Ok(drawn(
            Err(ChartError::TooManyCategories {
                count: categories,
                cap: req.key.max_categories,
            }),
            total,
        ));
    }
    // Text groups by its exact form; the label is any of the group's (equal)
    // values. Other types group and label by themselves.
    let label = if group == xq {
        xq.clone()
    } else {
        format!("MIN({xq})")
    };
    let aggs: Vec<String> = ys
        .iter()
        .enumerate()
        .map(|(i, &y)| {
            let expr = match agg_fn(req.key.agg) {
                None => "COUNT(*)".to_string(),
                Some(f) => value_sql(e, &e.quote_ident(&req.columns[y].name), read_of(req, y))
                    .map_or_else(|| as_float(e, "NULL"), |v| format!("{f}({v})")),
            };
            format!("{expr} AS octa_y{i}")
        })
        .collect();
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT * FROM (SELECT {label} AS octa_x, {} FROM {from} GROUP BY {group}){} \
         ORDER BY CASE WHEN octa_x IS NULL THEN 1 ELSE 0 END, octa_x",
        aggs.join(", "),
        subquery_alias(e, "q")
    ))?;
    let categories: Vec<String> = t
        .rows
        .iter()
        .map(|r| match &r[0] {
            CellValue::Null => "(null)".to_string(),
            v => v.to_string(),
        })
        .collect();
    let series = ys
        .iter()
        .enumerate()
        .map(|(yi, &y)| crate::data::chart::ChartSeries {
            name: req.columns[y].name.clone(),
            points: t
                .rows
                .iter()
                .enumerate()
                .filter_map(|(ci, r)| r.get(1 + yi).and_then(cell_f64).map(|v| [ci as f64, v]))
                .collect(),
        })
        .collect();
    let y_label = if ys.len() == 1 {
        format!("{} of {}", req.key.agg.label(), req.columns[ys[0]].name)
    } else {
        req.key.agg.label().to_string()
    };
    Ok(drawn(
        Ok(ChartPrep {
            data: ChartData::Bars { categories, series },
            total_rows: total,
            used_rows: total,
            x_label: xi.name.clone(),
            y_label,
            x_axis_kind: XAxisKind::Numeric,
        }),
        total,
    ))
}

/// Line and Scatter: the rows with an X, every `ceil(count / max points)`-th
/// in X order (the table key breaks ties), at most `max_points` of them,
/// drawn by the local builder exactly as it draws loaded rows. Raw values,
/// no averaging. `total_rows` is the count, so "Sampled a / b" shows.
pub fn sampled(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    req: &ChartRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<ServerChart> {
    let e = src.engine();
    let q = |n: &str| e.quote_ident(n);
    let (x, ys) = match (x_index(req), y_indices(req)) {
        (Ok(x), Ok(ys)) => (x, ys),
        (Err(err), _) | (_, Err(err)) => return Ok(drawn(Err(err), 0)),
    };
    let xq = q(&req.columns[x].name);
    let from = src.from_sql();
    check_cancel(cancel)?;
    let t = c.query(&format!("SELECT COUNT({xq}), COUNT(*) FROM {from}"))?;
    let (n, total) = (count_at(&t, 0), count_at(&t, 1));
    if n == 0 {
        // Rows, but not one with an X: say so, not "no rows match".
        let err = match total {
            0 => ChartError::EmptyAfterFilter,
            _ => ChartError::XNotNumeric {
                col: req.columns[x].name.clone(),
            },
        };
        return Ok(drawn(Err(err), total));
    }
    // Sampling off (0) still caps a database chart at the default: the
    // remote table may be any size and every data change asks again.
    let step = match req.key.max_points {
        0 => n.div_ceil(DEFAULT_MAX_POINTS),
        m => n.div_ceil(m),
    };
    let mut needed: Vec<usize> = std::iter::once(x).chain(ys.iter().copied()).collect();
    needed.sort_unstable();
    needed.dedup();
    let cols: Vec<String> = needed.iter().map(|&i| q(&req.columns[i].name)).collect();
    // Numbers stored as text sort as numbers, as the local axis does.
    let order_x = match read_of(req, x) {
        ColumnRead::Number => as_float(e, &xq),
        _ => xq.clone(),
    };
    let order: Vec<String> = std::iter::once(order_x)
        .chain(req.tie.iter().map(|k| q(k)))
        .collect();
    check_cancel(cancel)?;
    let rows = c.query(&format!(
        "SELECT {cols} FROM (SELECT {cols}, ROW_NUMBER() OVER (ORDER BY {}) AS octa_rn \
         FROM {from} WHERE {xq} IS NOT NULL){} WHERE {} = 0 ORDER BY octa_rn",
        order.join(", "),
        subquery_alias(e, "x"),
        modulo(e, "octa_rn - 1", step),
        cols = cols.join(", ")
    ))?;
    // The chart tab's full width, so the key's column indices still hold.
    let mut t = DataTable::empty();
    t.columns = req.columns.clone();
    t.rows = rows
        .rows
        .iter()
        .map(|r| {
            let mut full = vec![CellValue::Null; req.columns.len()];
            for (j, &i) in needed.iter().enumerate() {
                if let Some(v) = r.get(j) {
                    full[i] = v.clone();
                }
            }
            full
        })
        .collect();
    let all: Vec<usize> = (0..t.row_count()).collect();
    let chart = build_chart(&t, &all, &req.key.config(), req.key.limits()).map(|mut p| {
        p.total_rows = n;
        p
    });
    Ok(drawn(chart, total))
}

/// One box per Y column that reads as a number: its quartiles
/// (`dialect::quartiles_sql`, the same interpolation as the local quantile),
/// then one statement for every whisker: the smallest value at or above
/// `q1 - 1.5 IQR` and the largest at or below `q3 + 1.5 IQR`, as Tukey's
/// whiskers locally. MySQL has no percentile: `NotExpressible`.
pub fn boxes(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    req: &ChartRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<ServerChart> {
    let e = src.engine();
    let ys = match y_indices(req) {
        Ok(ys) => ys,
        Err(err) => return Ok(drawn(Err(err), 0)),
    };
    let from = src.from_sql();
    let mut kept: Vec<(String, String, [f64; 3])> = Vec::new();
    for &y in &ys {
        let name = req.columns[y].name.clone();
        let Some(v) = value_sql(e, &e.quote_ident(&name), read_of(req, y)) else {
            continue;
        };
        let Some(sql) = quartiles_sql(e, &from, &v) else {
            return Ok(ServerChart::NotExpressible);
        };
        check_cancel(cancel)?;
        let t = c.query(&sql)?;
        let at = |i| t.get(0, i).and_then(cell_f64);
        if let (Some(q1), Some(m), Some(q3)) = (at(0), at(1), at(2)) {
            kept.push((name, v, [q1, m, q3]));
        }
    }
    let mut items: Vec<String> = Vec::new();
    for (_, v, [q1, _, q3]) in &kept {
        let iqr = q3 - q1;
        items.push(format!(
            "MIN(CASE WHEN {v} >= {} THEN {v} END)",
            float_lit(q1 - 1.5 * iqr)
        ));
        items.push(format!(
            "MAX(CASE WHEN {v} <= {} THEN {v} END)",
            float_lit(q3 + 1.5 * iqr)
        ));
    }
    items.push("COUNT(*)".to_string());
    check_cancel(cancel)?;
    let t = c.query(&format!("SELECT {} FROM {from}", items.join(", ")))?;
    let total = count_at(&t, items.len() - 1);
    if total == 0 {
        return Ok(drawn(Err(ChartError::EmptyAfterFilter), 0));
    }
    if kept.is_empty() {
        let col = req.columns[ys[0]].name.clone();
        return Ok(drawn(Err(ChartError::YNotNumeric { col }), total));
    }
    let summaries = kept
        .into_iter()
        .enumerate()
        .map(|(i, (name, _, [q1, median, q3]))| BoxSummary {
            name,
            lower_whisker: t.get(0, 2 * i).and_then(cell_f64).unwrap_or(q1),
            q1,
            median,
            q3,
            upper_whisker: t.get(0, 2 * i + 1).and_then(cell_f64).unwrap_or(q3),
        })
        .collect();
    Ok(drawn(
        Ok(ChartPrep {
            data: ChartData::Boxes(summaries),
            total_rows: total,
            used_rows: total,
            x_label: "Series".to_string(),
            y_label: "Value".to_string(),
            x_axis_kind: XAxisKind::Numeric,
        }),
        total,
    ))
}

/// The chart `req` describes, over every row of `src`.
pub fn run(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    req: &ChartRequest,
    cancel: &AtomicBool,
) -> anyhow::Result<ServerChart> {
    check_cancel(cancel)?;
    match req.key.kind {
        ChartKind::Histogram => histogram(c, src, req, cancel),
        ChartKind::Bar => bar(c, src, req, cancel),
        ChartKind::Line | ChartKind::Scatter => sampled(c, src, req, cancel),
        ChartKind::Box => boxes(c, src, req, cancel),
    }
}

#[cfg(test)]
#[path = "chart_tests.rs"]
pub(crate) mod tests;
