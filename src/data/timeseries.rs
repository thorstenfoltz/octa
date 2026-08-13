//! Time bucketing (resampling) and rolling windows, as DuckDB SQL.
//!
//! Pure builders in the shape of `src/data/pivot.rs`: they produce SQL strings
//! and plain-language explanations and execute nothing, so the GUI dialog, the
//! CLI and the MCP tools all emit identical SQL. The caller registers the
//! active table as `data` (which `octa::sql::run_query` does).

use crate::data::pivot::quote_ident;

/// Bucket width for a resample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interval {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl Interval {
    pub const ALL: &'static [Interval] = &[
        Interval::Minute,
        Interval::Hour,
        Interval::Day,
        Interval::Week,
        Interval::Month,
        Interval::Quarter,
        Interval::Year,
    ];

    /// The unit string DuckDB's `date_trunc` takes.
    pub fn unit(self) -> &'static str {
        match self {
            Interval::Minute => "minute",
            Interval::Hour => "hour",
            Interval::Day => "day",
            Interval::Week => "week",
            Interval::Month => "month",
            Interval::Quarter => "quarter",
            Interval::Year => "year",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|i| i.unit().eq_ignore_ascii_case(s.trim()))
    }
}

/// Aggregate applied inside a bucket or a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeAgg {
    Sum,
    Mean,
    Min,
    Max,
    Count,
    First,
    Last,
}

impl TimeAgg {
    pub const ALL: &'static [TimeAgg] = &[
        TimeAgg::Sum,
        TimeAgg::Mean,
        TimeAgg::Min,
        TimeAgg::Max,
        TimeAgg::Count,
        TimeAgg::First,
        TimeAgg::Last,
    ];

    pub fn sql_fn(self) -> &'static str {
        match self {
            TimeAgg::Sum => "sum",
            TimeAgg::Mean => "avg",
            TimeAgg::Min => "min",
            TimeAgg::Max => "max",
            TimeAgg::Count => "count",
            TimeAgg::First => "first",
            TimeAgg::Last => "last",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sum" => Some(TimeAgg::Sum),
            "mean" | "avg" | "average" => Some(TimeAgg::Mean),
            "min" => Some(TimeAgg::Min),
            "max" => Some(TimeAgg::Max),
            "count" => Some(TimeAgg::Count),
            "first" => Some(TimeAgg::First),
            "last" => Some(TimeAgg::Last),
            _ => None,
        }
    }
}

/// One resample: bucket `time_col` by `interval`, aggregate `value_cols`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResampleSpec {
    pub time_col: String,
    pub value_cols: Vec<String>,
    pub interval: Interval,
    pub agg: TimeAgg,
    /// Extra grouping columns: one series per combination. Empty = one series.
    pub group_by: Vec<String>,
}

/// One rolling window over `window` rows, ordered by `order_col`.
#[derive(Debug, Clone, PartialEq)]
pub struct RollingSpec {
    pub order_col: String,
    pub value_col: String,
    /// Rows in the frame, including the current one. Must be at least 1.
    pub window: usize,
    pub agg: TimeAgg,
    pub partition_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimeseriesError {
    UnknownColumn(String),
    NoValueColumns,
    ZeroWindow,
}

impl std::fmt::Display for TimeseriesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeseriesError::UnknownColumn(c) => write!(f, "column \"{c}\" not found"),
            TimeseriesError::NoValueColumns => write!(f, "pick at least one value column"),
            TimeseriesError::ZeroWindow => write!(f, "the window must be at least 1 row"),
        }
    }
}

impl std::error::Error for TimeseriesError {}

fn require(col: &str, cols: &[String]) -> Result<(), TimeseriesError> {
    if cols.iter().any(|c| c == col) {
        Ok(())
    } else {
        Err(TimeseriesError::UnknownColumn(col.to_string()))
    }
}

/// A name for the bucket column that no source column already uses.
fn bucket_name(cols: &[String]) -> String {
    let mut name = "bucket".to_string();
    let mut n = 2;
    while cols.iter().any(|c| c == &name) {
        name = format!("bucket_{n}");
        n += 1;
    }
    name
}

/// `SELECT date_trunc(...) AS bucket, agg(v) ... FROM data GROUP BY ... ORDER BY ...`
pub fn build_resample_sql(spec: &ResampleSpec, cols: &[String]) -> Result<String, TimeseriesError> {
    require(&spec.time_col, cols)?;
    if spec.value_cols.is_empty() {
        return Err(TimeseriesError::NoValueColumns);
    }
    for c in spec.value_cols.iter().chain(spec.group_by.iter()) {
        require(c, cols)?;
    }

    let bucket = bucket_name(cols);
    // TRY_CAST, not CAST: a text timestamp column that has one unparseable row
    // should bucket the rest rather than failing the whole query.
    let trunc = format!(
        "date_trunc('{}', TRY_CAST({} AS TIMESTAMP))",
        spec.interval.unit(),
        quote_ident(&spec.time_col)
    );

    let mut select = vec![format!("{trunc} AS {}", quote_ident(&bucket))];
    select.extend(spec.group_by.iter().map(|g| quote_ident(g)));
    select.extend(spec.value_cols.iter().map(|v| {
        format!(
            "{}({}) AS {}",
            spec.agg.sql_fn(),
            quote_ident(v),
            quote_ident(v)
        )
    }));

    let mut group = vec![trunc.clone()];
    group.extend(spec.group_by.iter().map(|g| quote_ident(g)));

    let mut order = vec![quote_ident(&bucket)];
    order.extend(spec.group_by.iter().map(|g| quote_ident(g)));

    Ok(format!(
        "SELECT {} FROM data GROUP BY {} ORDER BY {}",
        select.join(", "),
        group.join(", "),
        order.join(", ")
    ))
}

/// `SELECT *, agg(v) OVER (... ORDER BY t ROWS BETWEEN n-1 PRECEDING AND CURRENT ROW) ...`
///
/// The `ORDER BY` is mandatory and always emitted: a rolling average over
/// unordered rows is meaningless, so the spec offers no way to omit it.
pub fn build_rolling_sql(spec: &RollingSpec, cols: &[String]) -> Result<String, TimeseriesError> {
    require(&spec.order_col, cols)?;
    require(&spec.value_col, cols)?;
    for c in &spec.partition_by {
        require(c, cols)?;
    }
    if spec.window == 0 {
        return Err(TimeseriesError::ZeroWindow);
    }

    let mut frame = String::new();
    if !spec.partition_by.is_empty() {
        let parts: Vec<String> = spec.partition_by.iter().map(|p| quote_ident(p)).collect();
        frame.push_str(&format!("PARTITION BY {} ", parts.join(", ")));
    }
    frame.push_str(&format!(
        "ORDER BY {} ROWS BETWEEN {} PRECEDING AND CURRENT ROW",
        quote_ident(&spec.order_col),
        spec.window - 1
    ));

    let out_col = format!("{}_rolling_{}", spec.value_col, spec.window);
    Ok(format!(
        "SELECT *, {}({}) OVER ({}) AS {} FROM data ORDER BY {}",
        spec.agg.sql_fn(),
        quote_ident(&spec.value_col),
        frame,
        quote_ident(&out_col),
        quote_ident(&spec.order_col)
    ))
}

/// One plain sentence describing a resample, for the dialog.
pub fn explain_resample(spec: &ResampleSpec) -> String {
    let group = if spec.group_by.is_empty() {
        String::new()
    } else {
        format!(", separately for each {}", spec.group_by.join(" and "))
    };
    format!(
        "Group the rows into one bucket per {} of \"{}\", then take the {} of {}{}.",
        spec.interval.unit(),
        spec.time_col,
        spec.agg.sql_fn(),
        spec.value_cols
            .iter()
            .map(|v| format!("\"{v}\""))
            .collect::<Vec<_>>()
            .join(", "),
        group
    )
}

/// One plain sentence describing a rolling window, for the dialog.
pub fn explain_rolling(spec: &RollingSpec) -> String {
    let part = if spec.partition_by.is_empty() {
        String::new()
    } else {
        format!(", restarting for each {}", spec.partition_by.join(" and "))
    };
    // Names the window size, not just how many rows precede: people ask for a
    // "7-day moving average", so the 7 has to appear.
    format!(
        "For every row, take the {} of \"{}\" over a {}-row window ending at that row, in \"{}\" order{}.",
        spec.agg.sql_fn(),
        spec.value_col,
        spec.window,
        spec.order_col,
        part
    )
}

#[cfg(test)]
#[path = "timeseries_tests.rs"]
mod tests;
