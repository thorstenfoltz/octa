//! Trend lines and a Holt-Winters forecast for a chart's line series.
//!
//! Additive Holt-Winters with its three smoothing parameters picked by a
//! small grid search, the season length read from the spacing of the x
//! values. No settings are required; Advanced can override the season.
//! Pure; the chart, `--forecast` and the `forecast` MCP tool share it.

use chrono::{DateTime, Months, NaiveDate};

use crate::data::chart::{ChartSeries, XAxisKind, format_days_as_date, format_seconds_as_datetime};
use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrendKind {
    #[default]
    None,
    Straight,
    MovingAverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForecastError {
    /// Fewer than four points.
    TooShort,
    /// The points are not evenly spaced; bucket them first.
    Uneven,
}

impl ForecastError {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::TooShort => "forecast.err_too_short",
            Self::Uneven => "forecast.err_uneven",
        }
    }
}

const MIN_POINTS: usize = 4;
const MAX_FIT_POINTS: usize = 10_000;
const Z80: f64 = 1.2816;
const Z95: f64 = 1.96;

/// How far apart the points are, and what that means for the calendar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    /// Median gap in x units (days for Date, seconds for DateTime).
    pub step: f64,
    /// Step by calendar months instead of `step` (monthly, quarterly, yearly).
    pub months: Option<u32>,
    /// Season length in points; 0 for none.
    pub season: usize,
}

pub fn spacing(xs: &[f64], kind: XAxisKind) -> Result<Spacing, ForecastError> {
    if xs.len() < MIN_POINTS {
        return Err(ForecastError::TooShort);
    }
    let gaps: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    let mut sorted = gaps.clone();
    sorted.sort_by(f64::total_cmp);
    let med = sorted[sorted.len() / 2];
    if med <= 0.0 {
        return Err(ForecastError::Uneven);
    }
    // Months are 28 to 31 days long, so allow 10% per gap and 5% of gaps off.
    let off = gaps
        .iter()
        .filter(|g| (**g - med).abs() > 0.1 * med)
        .count();
    if off as f64 > 0.05 * gaps.len() as f64 {
        return Err(ForecastError::Uneven);
    }
    let plain = |season| {
        Ok(Spacing {
            step: med,
            months: None,
            season,
        })
    };
    let calendar = |months, season| {
        Ok(Spacing {
            step: med,
            months: Some(months),
            season,
        })
    };
    let day = match kind {
        XAxisKind::Numeric => return plain(0),
        XAxisKind::Date => 1.0,
        XAxisKind::DateTime => 86_400.0,
    };
    if kind == XAxisKind::DateTime && (med - 3600.0).abs() <= 360.0 {
        return plain(24);
    }
    let days = med / day;
    if (days - 1.0).abs() <= 0.1 {
        plain(7)
    } else if (days - 7.0).abs() <= 0.7 {
        plain(52)
    } else if (28.0..=31.0).contains(&days) {
        calendar(1, 12)
    } else if (89.0..=92.0).contains(&days) {
        calendar(3, 4)
    } else if (365.0..=366.0).contains(&days) {
        calendar(12, 0)
    } else {
        plain(0)
    }
}

fn next_x(x: f64, sp: &Spacing, kind: XAxisKind) -> f64 {
    let Some(m) = sp.months else {
        return x + sp.step;
    };
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid date");
    let stepped = match kind {
        XAxisKind::Date => (epoch + chrono::Duration::days(x.round() as i64))
            .checked_add_months(Months::new(m))
            .map(|d| (d - epoch).num_days() as f64),
        XAxisKind::DateTime => DateTime::from_timestamp(x as i64, 0)
            .and_then(|d| d.checked_add_months(Months::new(m)))
            .map(|d| d.timestamp() as f64),
        XAxisKind::Numeric => None,
    };
    stepped.unwrap_or(x + sp.step)
}

/// Least-squares line over the points, as its two end points.
pub fn straight_trend(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if points.len() < 2 {
        return Vec::new();
    }
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p[0]).sum::<f64>() / n;
    let my = points.iter().map(|p| p[1]).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p[0] - mx).powi(2)).sum();
    if sxx == 0.0 {
        return Vec::new();
    }
    let slope = points
        .iter()
        .map(|p| (p[0] - mx) * (p[1] - my))
        .sum::<f64>()
        / sxx;
    let icpt = my - slope * mx;
    let lo = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let hi = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    vec![[lo, icpt + slope * lo], [hi, icpt + slope * hi]]
}

/// Trailing average over `window` points (sorted by x).
pub fn moving_average(points: &[[f64; 2]], window: usize) -> Vec<[f64; 2]> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if window < 2 || pts.len() < window {
        return Vec::new();
    }
    pts.windows(window)
        .map(|w| {
            [
                w[window - 1][0],
                w.iter().map(|p| p[1]).sum::<f64>() / window as f64,
            ]
        })
        .collect()
}

struct Fit {
    sse: f64,
    count: usize,
    level: f64,
    trend: f64,
    season: Vec<f64>,
}

/// One pass of additive Holt-Winters (Holt's linear method when `m` is 0).
fn run(y: &[f64], m: usize, a: f64, b: f64, g: f64) -> Fit {
    let (mut level, mut trend, mut season, start) = if m > 0 {
        // The first season's mean sits at its middle, (m - 1) / 2; move the
        // level to its last point, where the fit starts, and take the trend
        // out of the starting season. Without both, the fit began about
        // m / 2 steps of trend behind and never caught up.
        let first = y[..m].iter().sum::<f64>() / m as f64;
        let second = y[m..2 * m].iter().sum::<f64>() / m as f64;
        let trend = (second - first) / m as f64;
        let mid = (m as f64 - 1.0) / 2.0;
        let season = (0..m)
            .map(|i| y[i] - (first + (i as f64 - mid) * trend))
            .collect::<Vec<_>>();
        (first + mid * trend, trend, season, m)
    } else {
        (y[0], y[1] - y[0], Vec::new(), 1)
    };
    let (mut sse, mut count) = (0.0, 0);
    for t in start..y.len() {
        let s = if m > 0 { season[t % m] } else { 0.0 };
        let err = y[t] - (level + trend + s);
        sse += err * err;
        count += 1;
        let prev = level;
        level = a * (y[t] - s) + (1.0 - a) * (level + trend);
        trend = b * (level - prev) + (1.0 - b) * trend;
        if m > 0 {
            season[t % m] = g * (y[t] - level) + (1.0 - g) * s;
        }
    }
    Fit {
        sse,
        count,
        level,
        trend,
        season,
    }
}

/// ponytail: 19^3 full passes; a Nelder-Mead search if long series make the
/// chart feel slow (the result is cached per chart setting, so it runs once).
fn best_fit(y: &[f64], m: usize) -> Fit {
    let grid: Vec<f64> = (1..20).map(|i| i as f64 * 0.05).collect();
    let gammas: &[f64] = if m > 0 { &grid } else { &[0.0] };
    let mut best: Option<Fit> = None;
    for &a in &grid {
        for &b in &grid {
            for &g in gammas {
                let f = run(y, m, a, b, g);
                if best.as_ref().is_none_or(|x| f.sse < x.sse) {
                    best = Some(f);
                }
            }
        }
    }
    best.expect("the grid is never empty")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForecastPoint {
    pub x: f64,
    pub value: f64,
    pub lo80: f64,
    pub hi80: f64,
    pub lo95: f64,
    pub hi95: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub points: Vec<ForecastPoint>,
    /// Season length used; 0 when there was not enough history for one.
    pub season: usize,
}

pub fn forecast(
    points: &[[f64; 2]],
    kind: XAxisKind,
    periods: usize,
    season_override: Option<usize>,
) -> Result<Forecast, ForecastError> {
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if pts.len() > MAX_FIT_POINTS {
        pts.drain(..pts.len() - MAX_FIT_POINTS);
    }
    let xs: Vec<f64> = pts.iter().map(|p| p[0]).collect();
    let y: Vec<f64> = pts.iter().map(|p| p[1]).collect();
    let sp = spacing(&xs, kind)?;
    let wanted = season_override.unwrap_or(sp.season);
    let m = if wanted >= 2 && y.len() >= 2 * wanted {
        wanted
    } else {
        0
    };
    let fit = best_fit(&y, m);
    let sigma = if fit.count > 0 {
        (fit.sse / fit.count as f64).sqrt()
    } else {
        0.0
    };
    let n = y.len();
    let mut x = *xs.last().expect("spacing checked the length");
    // ponytail: band = z * sigma * sqrt(h), the textbook approximation;
    // switch to the exact Holt-Winters variance if the bands look too narrow.
    let points = (1..=periods)
        .map(|h| {
            x = next_x(x, &sp, kind);
            let s = if m > 0 {
                fit.season[(n + h - 1) % m]
            } else {
                0.0
            };
            let value = fit.level + h as f64 * fit.trend + s;
            let w = sigma * (h as f64).sqrt();
            ForecastPoint {
                x,
                value,
                lo80: value - Z80 * w,
                hi80: value + Z80 * w,
                lo95: value - Z95 * w,
                hi95: value + Z95 * w,
            }
        })
        .collect();
    Ok(Forecast { points, season: m })
}

/// A shaded band for the on-screen chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    pub lo: Vec<[f64; 2]>,
    pub hi: Vec<[f64; 2]>,
    /// 80 or 95.
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overlays {
    /// Extra lines, appended after the chart's own series (so exports carry them).
    pub series: Vec<ChartSeries>,
    pub bands: Vec<Band>,
}

/// Trend and forecast lines for every series. The forecast line starts at
/// the series' last real point so the two join up.
pub fn overlays(
    series: &[ChartSeries],
    kind: XAxisKind,
    trend: TrendKind,
    periods: usize,
    season_override: Option<usize>,
) -> Result<Overlays, ForecastError> {
    let mut out = Overlays::default();
    for s in series {
        match trend {
            TrendKind::None => {}
            TrendKind::Straight => out.series.push(ChartSeries {
                name: format!("{} (trend)", s.name),
                points: straight_trend(&s.points),
            }),
            TrendKind::MovingAverage => {
                let xs: Vec<f64> = s.points.iter().map(|p| p[0]).collect();
                let window = spacing(&xs, kind).map(|sp| sp.season).unwrap_or(0).max(2);
                let window = if window == 2 { 5 } else { window };
                out.series.push(ChartSeries {
                    name: format!("{} (trend)", s.name),
                    points: moving_average(&s.points, window),
                });
            }
        }
        if periods == 0 {
            continue;
        }
        let f = forecast(&s.points, kind, periods, season_override)?;
        let last = s
            .points
            .iter()
            .copied()
            .max_by(|a, b| a[0].total_cmp(&b[0]))
            .into_iter();
        let line = |pick: fn(&ForecastPoint) -> f64| -> Vec<[f64; 2]> {
            last.clone()
                .chain(f.points.iter().map(|p| [p.x, pick(p)]))
                .collect()
        };
        out.series.push(ChartSeries {
            name: format!("{} (forecast)", s.name),
            points: line(|p| p.value),
        });
        out.series.push(ChartSeries {
            name: format!("{} (95% low)", s.name),
            points: line(|p| p.lo95),
        });
        out.series.push(ChartSeries {
            name: format!("{} (95% high)", s.name),
            points: line(|p| p.hi95),
        });
        out.bands.push(Band {
            lo: line(|p| p.lo95),
            hi: line(|p| p.hi95),
            level: 95,
        });
        out.bands.push(Band {
            lo: line(|p| p.lo80),
            hi: line(|p| p.hi80),
            level: 80,
        });
    }
    Ok(out)
}

/// The forecast rows for "Forecast to table".
pub fn forecast_table(f: &Forecast, kind: XAxisKind) -> DataTable {
    let (x_type, x_cell): (&str, fn(f64) -> CellValue) = match kind {
        XAxisKind::Date => ("Date32", |x| CellValue::Date(format_days_as_date(x))),
        XAxisKind::DateTime => ("Timestamp(Microsecond, None)", |x| {
            CellValue::DateTime(format_seconds_as_datetime(x))
        }),
        XAxisKind::Numeric => ("Float64", CellValue::Float),
    };
    let mut t = DataTable::empty();
    t.columns = std::iter::once(("x", x_type))
        .chain(["forecast", "lo80", "hi80", "lo95", "hi95"].map(|n| (n, "Float64")))
        .map(|(n, ty)| ColumnInfo {
            name: n.into(),
            data_type: ty.into(),
        })
        .collect();
    let r = |v: f64| CellValue::Float((v * 1000.0).round() / 1000.0);
    t.rows = f
        .points
        .iter()
        .map(|p| {
            vec![
                x_cell(p.x),
                r(p.value),
                r(p.lo80),
                r(p.hi80),
                r(p.lo95),
                r(p.hi95),
            ]
        })
        .collect();
    t
}

/// A line chart's first series and x kind for columns `x`/`y`, via the chart
/// pipeline so dates convert exactly as the GUI's chart does. For the
/// headless surfaces.
pub fn series_for(
    table: &DataTable,
    x: usize,
    y: usize,
) -> Result<(ChartSeries, XAxisKind), String> {
    use crate::data::chart::{ChartConfig, ChartData, ChartKind, ChartLimits, build_chart};
    let cfg = ChartConfig {
        kind: ChartKind::Line,
        x_col: Some(x),
        y_cols: vec![y],
        ..Default::default()
    };
    let rows: Vec<usize> = (0..table.row_count()).collect();
    let limits = ChartLimits {
        max_points: usize::MAX,
        ..Default::default()
    };
    let prep = build_chart(table, &rows, &cfg, limits).map_err(|e| e.message())?;
    match prep.data {
        ChartData::Lines {
            categories: None,
            mut series,
        } if !series.is_empty() => Ok((series.remove(0), prep.x_axis_kind)),
        _ => Err("the x column must hold dates or numbers".into()),
    }
}

#[cfg(test)]
#[path = "forecast_tests.rs"]
mod tests;
