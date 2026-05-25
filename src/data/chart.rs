//! Chart data preparation for the `ViewMode::Chart` renderer.
//!
//! Pure functions over a [`DataTable`]: pick numeric / categorical values
//! out of the user-selected columns, apply optional aggregation, downsample
//! to honour `chart_max_points`, and return a [`ChartPrep`] the view code
//! can hand straight to `egui_plot`. Nothing in here touches egui — the
//! split keeps the data-prep pipeline integration-testable without a
//! windowed GUI.

use super::{CellValue, DataTable, is_numeric_data_type};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

/// What the numeric X values in a [`ChartPrep`] *mean*. The renderer reads
/// this to decide whether to format ticks as plain numbers, calendar
/// dates (days since the Unix epoch), or datetimes (seconds since the
/// Unix epoch).
///
/// Categorical X axes are signalled separately via
/// [`ChartData::x_axis_categories`]; everything else flows through here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XAxisKind {
    /// Plain f64 values from a numeric column.
    #[default]
    Numeric,
    /// `days since 1970-01-01` from a `Date` column. The renderer
    /// formats each tick back into `YYYY-MM-DD`.
    Date,
    /// `seconds since 1970-01-01 UTC` from a `DateTime` column. The
    /// renderer formats each tick back into `YYYY-MM-DD HH:MM:SS`.
    DateTime,
}

/// Convert a days-since-epoch back to a `YYYY-MM-DD` string. Used by the
/// renderer's tick formatter when the X column was a `Date`.
pub fn format_days_as_date(days: f64) -> String {
    if !days.is_finite() {
        return String::new();
    }
    let Some(epoch) = NaiveDate::from_ymd_opt(1970, 1, 1) else {
        return String::new();
    };
    let offset = chrono::Days::new(days.round().abs() as u64);
    let date = if days >= 0.0 {
        epoch.checked_add_days(offset)
    } else {
        epoch.checked_sub_days(offset)
    };
    date.map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Convert seconds-since-epoch back to a `YYYY-MM-DD HH:MM:SS` string. Used
/// by the renderer's tick formatter when the X column was a `DateTime`.
pub fn format_seconds_as_datetime(seconds: f64) -> String {
    if !seconds.is_finite() {
        return String::new();
    }
    let secs = seconds.trunc() as i64;
    let nanos = ((seconds.fract().abs()) * 1_000_000_000.0) as u32;
    chrono::DateTime::from_timestamp(secs, nanos)
        .map(|dt| dt.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Sniff the dominant variant of an X column to pick an [`XAxisKind`].
/// Looks at the first non-null cell; falls back to `Numeric` for empty
/// columns. We sample one cell — the column's data_type would be more
/// reliable but isn't always populated by every reader.
fn sniff_x_axis_kind(table: &DataTable, x_col: usize, rows: &[usize]) -> XAxisKind {
    for &r in rows.iter().take(64) {
        match table.get(r, x_col) {
            Some(CellValue::Date(_)) => return XAxisKind::Date,
            Some(CellValue::DateTime(_)) => return XAxisKind::DateTime,
            Some(CellValue::Null) | None => continue,
            _ => return XAxisKind::Numeric,
        }
    }
    XAxisKind::Numeric
}

/// Default cap on the number of distinct categorical X values a Bar chart
/// will keep. Above this the chart prep returns an error rather than rendering
/// thousands of unreadable bars. The user can raise / lower this via
/// `AppSettings.chart_max_categories` — callers pass the resolved cap into
/// [`build_chart`]; this constant only seeds the default.
pub const DEFAULT_MAX_BAR_CATEGORIES: usize = 200;

/// Hard ceiling on the number of histogram bins. Sturges' formula caps out
/// well below this for any realistic dataset; the constant exists so a user
/// who manually sets `hist_bins` to something silly still gets a usable plot.
pub const MAX_HIST_BINS: usize = 200;

/// Default histogram bin count when [`ChartConfig::hist_bins`] is `None`,
/// computed via Sturges' formula `ceil(1 + log2(n))` clamped to `[5, 50]`.
pub fn sturges_bins(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let raw = 1.0 + (n as f64).log2();
    let rounded = raw.ceil() as usize;
    rounded.clamp(5, 50)
}

/// Which chart shape to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChartKind {
    /// Frequency of values in a single numeric X column.
    #[default]
    Histogram,
    /// One bar per category in X, height = aggregated Y value(s).
    Bar,
    /// X vs Y as a connected line. Multiple Y columns become multiple lines.
    Line,
    /// X vs Y as a point cloud.
    Scatter,
    /// Five-number summary per Y column.
    Box,
}

impl ChartKind {
    pub fn label(self) -> &'static str {
        match self {
            ChartKind::Histogram => "Histogram",
            ChartKind::Bar => "Bar",
            ChartKind::Line => "Line",
            ChartKind::Scatter => "Scatter",
            ChartKind::Box => "Box",
        }
    }

    pub const ALL: &'static [ChartKind] = &[
        ChartKind::Histogram,
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Scatter,
        ChartKind::Box,
    ];

    /// Whether this chart kind needs an X column. Histogram + Box take only
    /// the X (Histogram bins it, Box treats Y columns as the data and ignores
    /// X). Everything else takes X *and* one or more Y columns.
    pub fn needs_y(self) -> bool {
        matches!(
            self,
            ChartKind::Bar | ChartKind::Line | ChartKind::Scatter | ChartKind::Box
        )
    }
}

/// Aggregation applied when Bar grouping collapses multiple Y rows per X
/// category into a single bar height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Aggregation {
    #[default]
    Sum,
    Avg,
    Count,
    Min,
    Max,
}

impl Aggregation {
    pub fn label(self) -> &'static str {
        match self {
            Aggregation::Sum => "Sum",
            Aggregation::Avg => "Avg",
            Aggregation::Count => "Count",
            Aggregation::Min => "Min",
            Aggregation::Max => "Max",
        }
    }

    pub const ALL: &'static [Aggregation] = &[
        Aggregation::Sum,
        Aggregation::Avg,
        Aggregation::Count,
        Aggregation::Min,
        Aggregation::Max,
    ];

    fn fold(self, values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            return match self {
                Aggregation::Count => Some(0.0),
                _ => None,
            };
        }
        Some(match self {
            Aggregation::Sum => values.iter().copied().sum(),
            Aggregation::Avg => values.iter().copied().sum::<f64>() / values.len() as f64,
            Aggregation::Count => values.len() as f64,
            Aggregation::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
            Aggregation::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    }
}

/// Where the plot legend sits — or off entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LegendPosition {
    Off,
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

impl LegendPosition {
    pub fn label(self) -> &'static str {
        match self {
            LegendPosition::Off => "Off",
            LegendPosition::TopLeft => "Top-left",
            LegendPosition::TopRight => "Top-right",
            LegendPosition::BottomLeft => "Bottom-left",
            LegendPosition::BottomRight => "Bottom-right",
        }
    }

    pub const ALL: &'static [LegendPosition] = &[
        LegendPosition::Off,
        LegendPosition::TopLeft,
        LegendPosition::TopRight,
        LegendPosition::BottomLeft,
        LegendPosition::BottomRight,
    ];
}

/// Per-Y-column display override: a custom legend name and/or color picked
/// by the user. Either may be empty / unset — the renderer falls back to the
/// column name + an auto-cycled color.
#[derive(Debug, Clone, Default)]
pub struct SeriesStyle {
    /// User-set legend label. Empty string = use the column name.
    pub display_name: String,
    /// User-picked RGBA. `None` = auto-color from egui_plot's palette.
    pub color: Option<[u8; 4]>,
}

/// User-driven chart configuration. Lives on `TabState` as transient state —
/// not persisted because the right columns are usually obvious from the
/// table on screen and persisting would re-open the chart on the wrong file.
#[derive(Debug, Clone)]
pub struct ChartConfig {
    pub kind: ChartKind,
    /// Column used for the X axis. `None` until the user picks one.
    pub x_col: Option<usize>,
    /// Y columns. Histogram and Box ignore this; Bar / Line / Scatter need
    /// at least one entry.
    pub y_cols: Vec<usize>,
    /// Aggregation applied when Bar's X is categorical and multiple rows
    /// fall into the same category. Ignored by other kinds.
    pub agg: Aggregation,
    /// Override Sturges' bin count for Histogram. `None` = auto.
    pub hist_bins: Option<usize>,
    /// Optional title rendered above the plot. Empty = no title.
    pub title: String,
    /// Optional X-axis label override. Empty = derive from the X column.
    pub x_label_override: String,
    /// Optional Y-axis label override. Empty = derive from the Y column(s).
    pub y_label_override: String,
    /// Legend visibility + position. Default `TopRight`.
    pub legend: LegendPosition,
    /// Show the background grid behind the plot. Default `true`.
    pub show_grid: bool,
    /// Lower bound for the Y axis (in original-data units). `None` = auto.
    pub y_min: Option<f64>,
    /// Upper bound for the Y axis (in original-data units). `None` = auto.
    pub y_max: Option<f64>,
    /// Custom Y-axis step size. `None` = let egui_plot auto-pick.
    pub y_step: Option<f64>,
    /// Format Y-axis ticks as integers (`{:.0}`). Useful for count-y plots
    /// where the auto-formatter shows `1.0 / 2.0` instead of `1 / 2`.
    pub y_integer_only: bool,
    /// Apply `log10(...)` to the Y values before plotting. Non-positive
    /// values are dropped silently (`log10(0) = -inf`, `log10(<0) = NaN`).
    /// The axis label gets a "(log10)" suffix so the user knows what
    /// they're reading.
    pub y_log_scale: bool,
    /// Per-Y-column display overrides, keyed by the column index. Keeps a
    /// `HashMap` rather than parallel `Vec` so the entries survive Y-column
    /// reorders / removes without renumbering.
    pub series_styles: std::collections::HashMap<usize, SeriesStyle>,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            kind: ChartKind::Histogram,
            x_col: None,
            y_cols: Vec::new(),
            agg: Aggregation::Sum,
            hist_bins: None,
            title: String::new(),
            x_label_override: String::new(),
            y_label_override: String::new(),
            legend: LegendPosition::TopRight,
            show_grid: true,
            y_min: None,
            y_max: None,
            y_step: None,
            y_integer_only: false,
            y_log_scale: false,
            series_styles: std::collections::HashMap::new(),
        }
    }
}

/// One named numeric series — Line / Scatter / Bar all use this shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    pub points: Vec<[f64; 2]>,
}

/// Five-number summary used by box plots.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxSummary {
    pub name: String,
    pub lower_whisker: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub upper_whisker: f64,
}

/// Concrete data shape per chart kind. The view side matches on this enum
/// to emit the right `egui_plot` items.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartData {
    /// Histogram bins, each `(left_edge, count)`. `bin_width` lets the
    /// renderer set bar width without re-deriving from edges.
    Histogram {
        bins: Vec<(f64, f64)>,
        bin_width: f64,
    },
    /// One series per Y column. Each point is `[category_index, value]` so
    /// the renderer can label the X axis with `categories[i]`.
    Bars {
        categories: Vec<String>,
        series: Vec<ChartSeries>,
    },
    /// Connected polylines, one per Y column. `categories` is `Some` when
    /// X was a non-numeric column — each point's X is a category index and
    /// the renderer maps it back to the category label on the axis.
    Lines {
        categories: Option<Vec<String>>,
        series: Vec<ChartSeries>,
    },
    /// Disconnected points, one series per Y column. Same `categories`
    /// semantics as [`ChartData::Lines`].
    Scatter {
        categories: Option<Vec<String>>,
        series: Vec<ChartSeries>,
    },
    /// One box per Y column.
    Boxes(Vec<BoxSummary>),
}

impl ChartData {
    /// Category labels for the X axis, when any. Used by the renderer to
    /// wire `Plot::x_axis_formatter` so Bar, Box, and categorical
    /// Line / Scatter charts show strings at each tick rather than the
    /// 0, 1, 2, ... integer indices the underlying data uses.
    pub fn x_axis_categories(&self) -> Option<Vec<String>> {
        match self {
            ChartData::Bars { categories, .. } => Some(categories.clone()),
            ChartData::Lines { categories, .. } | ChartData::Scatter { categories, .. } => {
                categories.clone()
            }
            ChartData::Boxes(summaries) => Some(summaries.iter().map(|s| s.name.clone()).collect()),
            ChartData::Histogram { .. } => None,
        }
    }
}

/// Prep result handed to the renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPrep {
    pub data: ChartData,
    /// Rows considered after filtering, before sampling.
    pub total_rows: usize,
    /// Rows actually plotted. `< total_rows` means the sampling cap kicked in.
    pub used_rows: usize,
    /// Suggested X-axis title for the renderer.
    pub x_label: String,
    /// Suggested Y-axis title for the renderer.
    pub y_label: String,
    /// What the X-axis numbers *mean* (plain numeric vs. days-since-epoch
    /// vs. seconds-since-epoch). The renderer uses this to format ticks
    /// back into readable dates — without it, a histogram of a `Date`
    /// column shows numbers like `19723` instead of `2024-01-30`.
    pub x_axis_kind: XAxisKind,
}

/// Why a chart couldn't be built. All variants carry a user-displayable
/// message — the renderer surfaces them inline above the empty plot area.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartError {
    NoXColumn,
    NoYColumn,
    XOutOfRange,
    YOutOfRange,
    XNotNumeric { col: String },
    YNotNumeric { col: String },
    EmptyAfterFilter,
    TooManyCategories { count: usize, cap: usize },
}

impl ChartError {
    pub fn message(&self) -> String {
        match self {
            ChartError::NoXColumn => "Pick an X column to continue.".into(),
            ChartError::NoYColumn => "Pick at least one Y column to continue.".into(),
            ChartError::XOutOfRange => "X column index is out of range.".into(),
            ChartError::YOutOfRange => "A Y column index is out of range.".into(),
            ChartError::XNotNumeric { col } => {
                format!("X column '{col}' has no numeric values.")
            }
            ChartError::YNotNumeric { col } => {
                format!("Y column '{col}' has no numeric values.")
            }
            ChartError::EmptyAfterFilter => {
                "No rows match the current filter — clear it to chart the full table.".into()
            }
            ChartError::TooManyCategories { count, cap } => format!(
                "X has {count} distinct categories; the Bar chart caps at {cap}. \
                 Filter the table or aggregate before charting."
            ),
        }
    }
}

/// Coerce a cell to f64 the same way the chart pipeline does.
///
/// Returns `None` for nulls, non-parseable strings, booleans, binary, and
/// nested blobs. Dates become **days since 1970-01-01** (Unix-epoch days);
/// datetimes become **seconds since the Unix epoch**. We accept the most
/// common ISO-ish formats so a `Date` column coming out of CSV / Parquet /
/// Arrow charts cleanly without the user having to convert it first.
pub fn cell_to_f64(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse::<f64>().ok(),
        CellValue::Date(s) => parse_date_to_days(s),
        CellValue::DateTime(s) => parse_datetime_to_seconds(s),
        _ => None,
    }
}

/// Parse a date string into "days since 1970-01-01". Accepts ISO `%Y-%m-%d`
/// plus dotted European `%d.%m.%Y` and slashed `%d/%m/%Y` so the user can
/// chart whatever shape the source file produced without re-formatting.
fn parse_date_to_days(s: &str) -> Option<f64> {
    let s = s.trim();
    const FORMATS: &[&str] = &["%Y-%m-%d", "%d.%m.%Y", "%d/%m/%Y", "%m/%d/%Y"];
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;
    for fmt in FORMATS {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some((d - epoch).num_days() as f64);
        }
    }
    None
}

/// Parse a datetime string into "seconds since 1970-01-01 UTC". Naive (no
/// timezone) — same convention chrono uses for `NaiveDateTime`. Subseconds
/// preserved as the fractional part.
fn parse_datetime_to_seconds(s: &str) -> Option<f64> {
    let s = s.trim();
    const FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y-%m-%dT%H:%M:%S%.fZ",
    ];
    for fmt in FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt.and_utc().timestamp_nanos_opt()? as f64 / 1_000_000_000.0);
        }
    }
    // Fall back to date-only — treat as midnight UTC.
    parse_date_to_days(s).map(|d| d * 86_400.0)
}

/// Stringify a cell for use as a Bar category key. Null becomes `"(null)"`
/// so the chart still has somewhere to put the rows; otherwise we delegate
/// to the cell's `Display`.
fn cell_to_category(cell: &CellValue) -> String {
    match cell {
        CellValue::Null => "(null)".to_string(),
        other => other.to_string(),
    }
}

/// Evenly-spaced downsample of an index list. Returns `indices` itself when
/// it already fits under the cap. Pulled out so Histogram + Line + Scatter
/// can share one sampler.
fn sample_indices(indices: &[usize], cap: usize) -> Vec<usize> {
    if cap == 0 || indices.len() <= cap {
        return indices.to_vec();
    }
    let stride = indices.len() as f64 / cap as f64;
    (0..cap)
        .map(|i| indices[((i as f64) * stride).floor() as usize])
        .collect()
}

/// Collect numeric values from `(rows, col)` skipping non-numeric / null cells.
fn collect_numeric(table: &DataTable, rows: &[usize], col: usize) -> Vec<f64> {
    rows.iter()
        .filter_map(|&r| table.get(r, col).and_then(cell_to_f64))
        .collect()
}

fn col_name(table: &DataTable, col: usize) -> String {
    table
        .columns
        .get(col)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| format!("column_{}", col + 1))
}

/// Runtime caps the renderer can adjust per call. Threading them through as a
/// struct keeps the build_chart signature stable as more knobs land later
/// (per-axis date format, dpi, …).
#[derive(Debug, Clone, Copy)]
pub struct ChartLimits {
    /// Max input rows the pipeline will plot before evenly-spaced sampling.
    /// Honoured by Histogram / Line / Scatter; Bar and Box ignore it.
    pub max_points: usize,
    /// Max distinct categories a Bar chart will accept on its X axis.
    pub max_categories: usize,
}

impl Default for ChartLimits {
    fn default() -> Self {
        Self {
            max_points: 100_000,
            max_categories: DEFAULT_MAX_BAR_CATEGORIES,
        }
    }
}

/// Build the chart prep for the given filter set + config.
///
/// `filtered_rows` is the same index list the table view uses, so the chart
/// always shows whatever the user has filtered to. `limits.max_points` caps
/// the row count for Histogram / Line / Scatter; `limits.max_categories`
/// caps the distinct X-category count for Bar.
pub fn build_chart(
    table: &DataTable,
    filtered_rows: &[usize],
    cfg: &ChartConfig,
    limits: ChartLimits,
) -> Result<ChartPrep, ChartError> {
    if filtered_rows.is_empty() {
        return Err(ChartError::EmptyAfterFilter);
    }
    match cfg.kind {
        ChartKind::Histogram => build_histogram(table, filtered_rows, cfg, limits.max_points),
        ChartKind::Bar => build_bar(table, filtered_rows, cfg, limits.max_categories),
        ChartKind::Line => build_line_or_scatter(
            table,
            filtered_rows,
            cfg,
            limits.max_points,
            limits.max_categories,
            true,
        ),
        ChartKind::Scatter => build_line_or_scatter(
            table,
            filtered_rows,
            cfg,
            limits.max_points,
            limits.max_categories,
            false,
        ),
        ChartKind::Box => build_box(table, filtered_rows, cfg),
    }
}

fn require_x(cfg: &ChartConfig, table: &DataTable) -> Result<usize, ChartError> {
    let x = cfg.x_col.ok_or(ChartError::NoXColumn)?;
    if x >= table.col_count() {
        return Err(ChartError::XOutOfRange);
    }
    Ok(x)
}

fn require_ys(cfg: &ChartConfig, table: &DataTable) -> Result<Vec<usize>, ChartError> {
    if cfg.y_cols.is_empty() {
        return Err(ChartError::NoYColumn);
    }
    for &y in &cfg.y_cols {
        if y >= table.col_count() {
            return Err(ChartError::YOutOfRange);
        }
    }
    Ok(cfg.y_cols.clone())
}

fn build_histogram(
    table: &DataTable,
    rows: &[usize],
    cfg: &ChartConfig,
    max_points: usize,
) -> Result<ChartPrep, ChartError> {
    let x_col = require_x(cfg, table)?;
    let total_rows = rows.len();
    let sampled = sample_indices(rows, max_points);
    let used_rows = sampled.len();
    let values = collect_numeric(table, &sampled, x_col);
    if values.is_empty() {
        return Err(ChartError::XNotNumeric {
            col: col_name(table, x_col),
        });
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let bin_count = cfg
        .hist_bins
        .unwrap_or_else(|| sturges_bins(values.len()))
        .clamp(1, MAX_HIST_BINS);
    let bin_width = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        (max - min) / bin_count as f64
    };
    let mut counts = vec![0usize; bin_count];
    for v in &values {
        let mut idx = ((v - min) / bin_width).floor() as i64;
        if idx < 0 {
            idx = 0;
        }
        let idx_usize = (idx as usize).min(bin_count - 1);
        counts[idx_usize] += 1;
    }
    let bins: Vec<(f64, f64)> = counts
        .into_iter()
        .enumerate()
        .map(|(i, c)| (min + bin_width * i as f64, c as f64))
        .collect();
    Ok(ChartPrep {
        data: ChartData::Histogram { bins, bin_width },
        total_rows,
        used_rows,
        x_label: col_name(table, x_col),
        y_label: "Count".to_string(),
        x_axis_kind: sniff_x_axis_kind(table, x_col, rows),
    })
}

fn build_bar(
    table: &DataTable,
    rows: &[usize],
    cfg: &ChartConfig,
    max_categories: usize,
) -> Result<ChartPrep, ChartError> {
    let x_col = require_x(cfg, table)?;
    let y_cols = require_ys(cfg, table)?;
    let total_rows = rows.len();
    // Preserve first-seen order of categories so the user can predict the
    // X axis from a glance at the table.
    let mut category_order: Vec<String> = Vec::new();
    let mut category_seen: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    // For Count aggregation we count every row regardless of Y being
    // numeric — same semantics SQL's COUNT(*) gives.
    let mut buckets: Vec<Vec<Vec<f64>>> = Vec::new();
    let count_only = matches!(cfg.agg, Aggregation::Count);
    for &row in rows {
        let Some(cell) = table.get(row, x_col) else {
            continue;
        };
        let key = cell_to_category(cell);
        let cat_idx = match category_seen.get(&key) {
            Some(&idx) => idx,
            None => {
                let idx = category_order.len();
                if idx >= max_categories {
                    return Err(ChartError::TooManyCategories {
                        count: idx + 1,
                        cap: max_categories,
                    });
                }
                category_seen.insert(key.clone(), idx);
                category_order.push(key);
                buckets.push(vec![Vec::new(); y_cols.len()]);
                idx
            }
        };
        for (yi, &y_col) in y_cols.iter().enumerate() {
            if count_only {
                buckets[cat_idx][yi].push(1.0);
            } else if let Some(v) = table.get(row, y_col).and_then(cell_to_f64) {
                buckets[cat_idx][yi].push(v);
            }
        }
    }
    if category_order.is_empty() {
        return Err(ChartError::EmptyAfterFilter);
    }
    let mut series: Vec<ChartSeries> = Vec::with_capacity(y_cols.len());
    for (yi, &y_col) in y_cols.iter().enumerate() {
        let points: Vec<[f64; 2]> = (0..category_order.len())
            .filter_map(|cat_idx| {
                cfg.agg
                    .fold(&buckets[cat_idx][yi])
                    .map(|v| [cat_idx as f64, v])
            })
            .collect();
        series.push(ChartSeries {
            name: col_name(table, y_col),
            points,
        });
    }
    let y_label = if y_cols.len() == 1 {
        format!("{} of {}", cfg.agg.label(), col_name(table, y_cols[0]))
    } else {
        cfg.agg.label().to_string()
    };
    Ok(ChartPrep {
        data: ChartData::Bars {
            categories: category_order,
            series,
        },
        total_rows,
        used_rows: total_rows,
        x_label: col_name(table, x_col),
        y_label,
        // Bars always show category labels via `x_axis_categories()`, so
        // the numeric-kind branch never fires here — leave at default.
        x_axis_kind: XAxisKind::Numeric,
    })
}

fn build_line_or_scatter(
    table: &DataTable,
    rows: &[usize],
    cfg: &ChartConfig,
    max_points: usize,
    max_categories: usize,
    is_line: bool,
) -> Result<ChartPrep, ChartError> {
    let x_col = require_x(cfg, table)?;
    let y_cols = require_ys(cfg, table)?;
    let total_rows = rows.len();

    // Probe a few rows to decide whether the X column is numeric (or
    // date-like, which coerces). If not we fall back to the categorical
    // path so a Line of "country → population" works without forcing the
    // user to re-type the column first.
    let x_is_numeric = rows
        .iter()
        .take(64)
        .any(|&r| table.get(r, x_col).and_then(cell_to_f64).is_some());

    let sampled = sample_indices(rows, max_points);
    let used_rows = sampled.len();
    let mut series_buf: Vec<ChartSeries> = y_cols
        .iter()
        .map(|&y| ChartSeries {
            name: col_name(table, y),
            points: Vec::with_capacity(sampled.len()),
        })
        .collect();

    let categories: Option<Vec<String>>;
    if x_is_numeric {
        // Original numeric / date path.
        let mut any_x = false;
        for &row in &sampled {
            let Some(x_cell) = table.get(row, x_col) else {
                continue;
            };
            let Some(x_val) = cell_to_f64(x_cell) else {
                continue;
            };
            any_x = true;
            for (yi, &y_col) in y_cols.iter().enumerate() {
                if let Some(y_val) = table.get(row, y_col).and_then(cell_to_f64) {
                    series_buf[yi].points.push([x_val, y_val]);
                }
            }
        }
        if !any_x {
            return Err(ChartError::XNotNumeric {
                col: col_name(table, x_col),
            });
        }
        if is_line {
            // Line plots only make sense when X is monotonic; otherwise the
            // segments criss-cross. Sort each series by X so the user gets a
            // sensible line even when the underlying table isn't sorted.
            for series in &mut series_buf {
                series
                    .points
                    .sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
        categories = None;
    } else {
        // Categorical path: build a first-seen-order category list,
        // place each row at its category index. No aggregation — multiple
        // rows in the same category produce multiple points (visible as
        // a vertical stack on the bar).
        let mut category_order: Vec<String> = Vec::new();
        let mut category_seen: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for &row in &sampled {
            let Some(cell) = table.get(row, x_col) else {
                continue;
            };
            let key = cell_to_category(cell);
            let cat_idx = match category_seen.get(&key) {
                Some(&idx) => idx,
                None => {
                    let idx = category_order.len();
                    if idx >= max_categories {
                        return Err(ChartError::TooManyCategories {
                            count: idx + 1,
                            cap: max_categories,
                        });
                    }
                    category_seen.insert(key.clone(), idx);
                    category_order.push(key);
                    idx
                }
            };
            for (yi, &y_col) in y_cols.iter().enumerate() {
                if let Some(y_val) = table.get(row, y_col).and_then(cell_to_f64) {
                    series_buf[yi].points.push([cat_idx as f64, y_val]);
                }
            }
        }
        if category_order.is_empty() {
            return Err(ChartError::EmptyAfterFilter);
        }
        // Do NOT sort by X here: row-encounter order is the only sensible
        // sequence for a categorical Line ("connect the bars left to right"),
        // and sorting by category-index is already the encounter order.
        categories = Some(category_order);
    }

    if series_buf.iter().all(|s| s.points.is_empty()) {
        let first = col_name(table, y_cols[0]);
        return Err(ChartError::YNotNumeric { col: first });
    }
    let y_label = if y_cols.len() == 1 {
        col_name(table, y_cols[0])
    } else {
        "Value".to_string()
    };
    let x_axis_kind = if categories.is_some() {
        // Categorical path puts category indices on X; the categorical
        // formatter is what matters, not the date one.
        XAxisKind::Numeric
    } else {
        sniff_x_axis_kind(table, x_col, rows)
    };
    Ok(ChartPrep {
        data: if is_line {
            ChartData::Lines {
                categories,
                series: series_buf,
            }
        } else {
            ChartData::Scatter {
                categories,
                series: series_buf,
            }
        },
        total_rows,
        used_rows,
        x_label: col_name(table, x_col),
        y_label,
        x_axis_kind,
    })
}

fn build_box(
    table: &DataTable,
    rows: &[usize],
    cfg: &ChartConfig,
) -> Result<ChartPrep, ChartError> {
    let y_cols = require_ys(cfg, table)?;
    let total_rows = rows.len();
    let mut summaries: Vec<BoxSummary> = Vec::with_capacity(y_cols.len());
    let mut any_data = false;
    for &y_col in &y_cols {
        let mut values = collect_numeric(table, rows, y_col);
        if values.is_empty() {
            // Skip empty columns but keep going so a partially-bad selection
            // still draws what it can — the renderer adds a hint if the
            // result is shorter than the request.
            continue;
        }
        any_data = true;
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let q1 = quantile(&values, 0.25);
        let median = quantile(&values, 0.50);
        let q3 = quantile(&values, 0.75);
        let iqr = q3 - q1;
        let lower_fence = q1 - 1.5 * iqr;
        let upper_fence = q3 + 1.5 * iqr;
        // Whiskers extend to the actual extreme values within the fences,
        // not all the way to ±IQR — matches Tukey's original definition.
        let lower_whisker = *values
            .iter()
            .find(|v| **v >= lower_fence)
            .unwrap_or(values.first().unwrap());
        let upper_whisker = *values
            .iter()
            .rev()
            .find(|v| **v <= upper_fence)
            .unwrap_or(values.last().unwrap());
        summaries.push(BoxSummary {
            name: col_name(table, y_col),
            lower_whisker,
            q1,
            median,
            q3,
            upper_whisker,
        });
    }
    if !any_data {
        return Err(ChartError::YNotNumeric {
            col: col_name(table, y_cols[0]),
        });
    }
    Ok(ChartPrep {
        data: ChartData::Boxes(summaries),
        total_rows,
        used_rows: total_rows,
        x_label: "Series".to_string(),
        y_label: "Value".to_string(),
        // Box plots show Y column names on X via `x_axis_categories()`,
        // so the numeric formatter never runs here.
        x_axis_kind: XAxisKind::Numeric,
    })
}

/// Linear-interpolation quantile, matching numpy's default. `values` must be
/// sorted ascending.
fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }
    let pos = q * (values.len() as f64 - 1.0);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return values[lo];
    }
    let frac = pos - lo as f64;
    values[lo] + (values[hi] - values[lo]) * frac
}

/// Heuristic: does the table likely have at least one numeric column?
/// Used by `OctaApp::open_chart_tab` and `view_modes::chart` to refuse
/// charting a string-only table.
pub fn has_numeric_column(table: &DataTable) -> bool {
    table
        .columns
        .iter()
        .any(|c| is_numeric_data_type(&c.data_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sturges_clamps_low_and_high() {
        assert_eq!(sturges_bins(0), 1);
        assert!(sturges_bins(10) >= 5);
        assert!(sturges_bins(1_000_000) <= 50);
    }

    #[test]
    fn quantile_picks_midpoint() {
        let v = vec![1.0, 2.0, 3.0, 4.0];
        assert!((quantile(&v, 0.5) - 2.5).abs() < 1e-9);
    }
}
