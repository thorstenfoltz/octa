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
pub(crate) mod tests {
    use super::super::test_support::{DuckConn, source};
    use super::*;

    fn col(name: &str, ty: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: ty.into(),
        }
    }

    /// Ordered by `g` with the NULL `g` last and by `f`, so the local
    /// builder's first-seen order and row order equal the server's ORDER BY.
    /// `n` has a NULL; `w` is words; `d` dates.
    pub(crate) fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![
            col("g", "Utf8"),
            col("n", "Int64"),
            col("f", "Float64"),
            col("d", "Date32"),
            col("w", "Utf8"),
        ];
        let g = [
            Some("a"),
            Some("a"),
            Some("b"),
            Some("c"),
            Some("c"),
            Some("c"),
            None,
        ];
        let n = [Some(1), Some(5), None, Some(2), Some(8), Some(3), Some(4)];
        let d = [
            "2024-01-01",
            "2024-01-03",
            "2024-01-10",
            "2024-02-01",
            "2024-02-02",
            "2024-03-15",
            "2024-04-30",
        ];
        t.rows = (0..7)
            .map(|i| {
                vec![
                    g[i].map_or(CellValue::Null, |s| CellValue::String(s.into())),
                    n[i].map_or(CellValue::Null, CellValue::Int),
                    CellValue::Float(0.5 + i as f64),
                    CellValue::Date(d[i].into()),
                    CellValue::String("word".into()),
                ]
            })
            .collect();
        t
    }

    pub(crate) fn request(cfg: &ChartConfig, limits: ChartLimits) -> ChartRequest {
        request_for(&table(), cfg, limits)
    }

    fn request_for(t: &DataTable, cfg: &ChartConfig, limits: ChartLimits) -> ChartRequest {
        ChartRequest {
            key: ChartKey::of(cfg, limits),
            columns: t.columns.clone(),
            reads: column_reads(t),
            tie: Vec::new(),
        }
    }

    /// One column `v` of the given cells.
    fn one_column(ty: &str, cells: Vec<CellValue>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![col("v", ty)];
        t.rows = cells.into_iter().map(|c| vec![c]).collect();
        t
    }

    pub(crate) fn limits() -> ChartLimits {
        ChartLimits {
            max_points: 1_000,
            max_categories: 200,
        }
    }

    /// The local builder over every row of [`table`].
    pub(crate) fn local(cfg: &ChartConfig) -> Result<ChartPrep, ChartError> {
        let t = table();
        let rows: Vec<usize> = (0..t.row_count()).collect();
        build_chart(&t, &rows, cfg, limits())
    }

    pub(crate) fn chart_of(s: ServerChart) -> Result<ChartPrep, ChartError> {
        match s {
            ServerChart::Drawn { chart, .. } => chart,
            ServerChart::NotExpressible => panic!("expected a drawn chart"),
        }
    }

    #[test]
    fn columns_are_read_as_the_local_chart_reads_them() {
        let mut t = table();
        t.columns.push(col("s", "Utf8"));
        t.columns.push(col("b", "Boolean"));
        for r in &mut t.rows {
            r.push(CellValue::String("12.5".into()));
            r.push(CellValue::Bool(true));
        }
        use ColumnRead::*;
        assert_eq!(
            column_reads(&t),
            vec![Other, Number, Number, Date, Other, Number, Other]
        );
        // Nothing loaded: the type decides.
        for (ty, read) in [
            ("Int64", Number),
            ("Decimal128(10, 2)", Number),
            ("Date32", Date),
            ("Timestamp(Microsecond, None)", DateTime),
            ("Utf8", Other),
            ("Boolean", Other),
        ] {
            let empty = one_column(ty, vec![CellValue::Null; 3]);
            assert_eq!(column_reads(&empty), vec![read], "{ty}");
        }
    }

    #[test]
    fn cosmetic_settings_leave_the_key_alone() {
        let mut cfg = ChartConfig {
            kind: ChartKind::Line,
            x_col: Some(2),
            y_cols: vec![1],
            ..ChartConfig::default()
        };
        let key = ChartKey::of(&cfg, limits());
        cfg.title = "Sales".into();
        cfg.show_grid = false;
        cfg.y_log_scale = true;
        cfg.x_min = Some(1.0);
        cfg.agg = Aggregation::Max; // Line does not aggregate
        assert_eq!(ChartKey::of(&cfg, limits()), key);
        cfg.kind = ChartKind::Bar;
        let bar = ChartKey::of(&cfg, limits());
        cfg.agg = Aggregation::Min;
        assert_ne!(ChartKey::of(&cfg, limits()), bar, "Bar aggregates");
    }

    #[test]
    fn the_server_histogram_is_the_local_one() {
        for (x, bins) in [(1, None), (1, Some(3)), (2, None), (3, None)] {
            let cfg = ChartConfig {
                kind: ChartKind::Histogram,
                x_col: Some(x),
                hist_bins: bins,
                ..ChartConfig::default()
            };
            let mut c = DuckConn::new(table());
            let got = histogram(
                &mut c,
                &source(),
                &request(&cfg, limits()),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(chart_of(got), local(&cfg), "x {x} bins {bins:?}");
            // SQL Server before 2022 has no LEAST: the clamp is a CASE.
            assert!(
                c.log.last().unwrap().contains("CASE WHEN FLOOR(("),
                "{:?}",
                c.log
            );
        }
    }

    #[test]
    fn a_word_column_is_not_a_number_on_the_server_either() {
        let cfg = ChartConfig {
            kind: ChartKind::Histogram,
            x_col: Some(4),
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(table());
        let got = histogram(
            &mut c,
            &source(),
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(chart_of(got), local(&cfg));
        assert!(c.log.is_empty(), "nothing sent for a word column");
    }

    #[test]
    fn the_server_bar_chart_is_the_local_one() {
        for agg in Aggregation::ALL.iter().copied() {
            let cfg = ChartConfig {
                kind: ChartKind::Bar,
                x_col: Some(0),
                y_cols: vec![1, 2, 4],
                agg,
                ..ChartConfig::default()
            };
            let mut c = DuckConn::new(table());
            let got = bar(
                &mut c,
                &source(),
                &request(&cfg, limits()),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(chart_of(got), local(&cfg), "{agg:?}");
        }
    }

    #[test]
    fn too_many_bars_counts_every_category() {
        let cfg = ChartConfig {
            kind: ChartKind::Bar,
            x_col: Some(0),
            y_cols: vec![1],
            ..ChartConfig::default()
        };
        let lim = ChartLimits {
            max_points: 1_000,
            max_categories: 2,
        };
        let mut c = DuckConn::new(table());
        let got = bar(
            &mut c,
            &source(),
            &request(&cfg, lim),
            &AtomicBool::new(false),
        )
        .unwrap();
        // a, b, c and the NULL bar: the exact count, not where the local loop stopped.
        assert_eq!(
            chart_of(got),
            Err(ChartError::TooManyCategories { count: 4, cap: 2 })
        );
    }

    #[test]
    fn every_row_fits_so_line_and_scatter_are_the_local_ones() {
        for kind in [ChartKind::Line, ChartKind::Scatter] {
            let cfg = ChartConfig {
                kind,
                x_col: Some(2),
                y_cols: vec![1],
                ..ChartConfig::default()
            };
            let mut c = DuckConn::new(table());
            let got = sampled(
                &mut c,
                &source(),
                &request(&cfg, limits()),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(chart_of(got), local(&cfg), "{kind:?}");
        }
    }

    #[test]
    fn more_rows_than_points_keeps_every_nth_by_x() {
        let cfg = ChartConfig {
            kind: ChartKind::Scatter,
            x_col: Some(2),
            y_cols: vec![2],
            ..ChartConfig::default()
        };
        let lim = ChartLimits {
            max_points: 3,
            max_categories: 200,
        };
        let mut c = DuckConn::new(table());
        let got = chart_of(
            sampled(
                &mut c,
                &source(),
                &request(&cfg, lim),
                &AtomicBool::new(false),
            )
            .unwrap(),
        )
        .unwrap();
        // 7 rows, step ceil(7 / 3) = 3: the 1st, 4th and 7th smallest f.
        let ChartData::Scatter { series, .. } = &got.data else {
            panic!("scatter");
        };
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        assert_eq!(xs, vec![0.5, 3.5, 6.5]);
        assert_eq!((got.used_rows, got.total_rows), (3, 7));
    }

    #[test]
    fn sampling_off_still_caps_a_database_chart() {
        let n = DEFAULT_MAX_POINTS + 1;
        let t = one_column(
            "Float64",
            (0..n).map(|i| CellValue::Float(i as f64)).collect(),
        );
        let cfg = ChartConfig {
            kind: ChartKind::Scatter,
            x_col: Some(0),
            y_cols: vec![0],
            ..ChartConfig::default()
        };
        let lim = ChartLimits {
            max_points: 0,
            max_categories: 200,
        };
        let mut c = DuckConn::new(t.clone());
        let got = chart_of(
            sampled(
                &mut c,
                &source(),
                &request_for(&t, &cfg, lim),
                &AtomicBool::new(false),
            )
            .unwrap(),
        )
        .unwrap();
        // Step ceil(25,001 / 25,000) = 2: every other row, not the whole table.
        assert_eq!((got.used_rows, got.total_rows), (n.div_ceil(2), n));
    }

    #[test]
    fn numbers_stored_as_text_are_spaced_in_number_order() {
        let t = one_column(
            "Utf8",
            ["9", "10", "100", "2", "300"]
                .map(|s| CellValue::String(s.into()))
                .to_vec(),
        );
        let cfg = ChartConfig {
            kind: ChartKind::Scatter,
            x_col: Some(0),
            y_cols: vec![0],
            ..ChartConfig::default()
        };
        let lim = ChartLimits {
            max_points: 3,
            max_categories: 200,
        };
        let mut c = DuckConn::new(t.clone());
        let got = chart_of(
            sampled(
                &mut c,
                &source(),
                &request_for(&t, &cfg, lim),
                &AtomicBool::new(false),
            )
            .unwrap(),
        )
        .unwrap();
        let ChartData::Scatter { series, .. } = &got.data else {
            panic!("scatter");
        };
        let xs: Vec<f64> = series[0].points.iter().map(|p| p[0]).collect();
        // Step 2 over 2, 9, 10, 100, 300; text order would keep 10, 2, 9.
        assert_eq!(xs, vec![2.0, 10.0, 300.0]);
    }

    #[test]
    fn a_column_without_any_x_says_so() {
        let t = one_column("Float64", vec![CellValue::Null; 3]);
        let cfg = ChartConfig {
            kind: ChartKind::Line,
            x_col: Some(0),
            y_cols: vec![0],
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(t.clone());
        let got = sampled(
            &mut c,
            &source(),
            &request_for(&t, &cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            chart_of(got),
            Err(ChartError::XNotNumeric { col: "v".into() })
        );
    }

    fn close(a: &ChartPrep, b: &ChartPrep) -> bool {
        let (ChartData::Boxes(x), ChartData::Boxes(y)) = (&a.data, &b.data) else {
            return false;
        };
        let near = |p: f64, q: f64| (p - q).abs() < 1e-9;
        x.len() == y.len()
            && x.iter().zip(y).all(|(s, t)| {
                s.name == t.name
                    && near(s.lower_whisker, t.lower_whisker)
                    && near(s.q1, t.q1)
                    && near(s.median, t.median)
                    && near(s.q3, t.q3)
                    && near(s.upper_whisker, t.upper_whisker)
            })
            && a.total_rows == b.total_rows
    }

    #[test]
    fn the_server_box_chart_is_the_local_one() {
        let cfg = ChartConfig {
            kind: ChartKind::Box,
            y_cols: vec![1, 2, 3, 4],
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(table());
        let got = chart_of(
            boxes(
                &mut c,
                &source(),
                &request(&cfg, limits()),
                &AtomicBool::new(false),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(close(&got, &local(&cfg).unwrap()), "{got:?}");
    }

    /// 100 lies beyond `q3 + 1.5 IQR`: the whisker stops at the largest
    /// value inside the fence, as locally, not at the maximum.
    #[test]
    fn the_whiskers_stop_at_the_fence() {
        let t = one_column(
            "Float64",
            [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 100.0]
                .map(CellValue::Float)
                .to_vec(),
        );
        let cfg = ChartConfig {
            kind: ChartKind::Box,
            y_cols: vec![0],
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(t.clone());
        let got = chart_of(
            boxes(
                &mut c,
                &source(),
                &request_for(&t, &cfg, limits()),
                &AtomicBool::new(false),
            )
            .unwrap(),
        )
        .unwrap();
        let rows: Vec<usize> = (0..t.row_count()).collect();
        let want = build_chart(&t, &rows, &cfg, limits()).unwrap();
        assert!(close(&got, &want), "{got:?}");
        let ChartData::Boxes(b) = &got.data else {
            panic!("boxes");
        };
        assert_eq!(b[0].upper_whisker, 10.0);
    }

    #[test]
    fn mysql_has_no_percentile_so_box_uses_the_loaded_rows() {
        let cfg = ChartConfig {
            kind: ChartKind::Box,
            y_cols: vec![1],
            ..ChartConfig::default()
        };
        let mut src = source();
        src.conn.engine = DbEngine::MySql;
        let mut c = DuckConn::new(table());
        let got = boxes(
            &mut c,
            &src,
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(got, ServerChart::NotExpressible);
        assert!(c.log.is_empty(), "nothing sent");
    }

    #[test]
    fn run_picks_the_builder_by_kind() {
        let cfg = ChartConfig {
            kind: ChartKind::Histogram,
            x_col: Some(1),
            ..ChartConfig::default()
        };
        let mut c = DuckConn::new(table());
        let got = run(
            &mut c,
            &source(),
            &request(&cfg, limits()),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(chart_of(got), local(&cfg));
    }
}
