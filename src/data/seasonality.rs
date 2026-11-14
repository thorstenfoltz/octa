//! Split a series over time into its trend, its repeating pattern and the
//! rest, so a spike can be told from "every December".
//!
//! The classical additive decomposition: the trend is a moving average one
//! season long (centred, so the first and last half season have none), the
//! pattern is the average of what is left at each position of the season,
//! and the rest is what neither explains. The season length is read from the
//! spacing of the x values exactly as the forecast reads it (24 for hourly,
//! 7 for daily, 52 for weekly, 12 for monthly, 4 for quarterly).
//! Pure; the chart, `--seasonality` and the `seasonality` MCP tool share it.

use crate::data::chart::XAxisKind;
use crate::data::forecast::{ForecastError, spacing, x_column};
use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonalityError {
    /// Fewer than four points.
    TooShort,
    /// The points are not evenly spaced; bucket them first.
    Uneven,
    /// The spacing names no season (yearly or plain numbers) and none was given.
    NoSeason,
    /// Fewer than two full seasons of this many points.
    TooFewSeasons(usize),
}

impl SeasonalityError {
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::TooShort => "forecast.err_too_short",
            Self::Uneven => "forecast.err_uneven",
            Self::NoSeason => "seasonality.err_no_season",
            Self::TooFewSeasons(_) => "seasonality.err_too_few",
        }
    }

    /// The message in the active language.
    pub fn message(self) -> String {
        let text = crate::i18n::t(self.i18n_key());
        match self {
            Self::TooFewSeasons(m) => text.replace("{season}", &m.to_string()),
            _ => text,
        }
    }
}

impl From<ForecastError> for SeasonalityError {
    fn from(e: ForecastError) -> Self {
        match e {
            ForecastError::TooShort => Self::TooShort,
            ForecastError::Uneven => Self::Uneven,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decomposition {
    /// The points in time order: `[x, value]`.
    pub points: Vec<[f64; 2]>,
    /// `None` for the first and last half season, where the average has no
    /// full season around the point.
    pub trend: Vec<Option<f64>>,
    /// The repeating pattern at each point; sums to zero over one season.
    pub seasonal: Vec<f64>,
    /// Value minus trend minus pattern; `None` where the trend is.
    pub rest: Vec<Option<f64>>,
    /// Season length in points.
    pub season: usize,
    /// How much of the movement around the trend the pattern explains,
    /// 0 (none) to 1 (all of it).
    pub strength: f64,
}

pub fn decompose(
    points: &[[f64; 2]],
    kind: XAxisKind,
    season_override: Option<usize>,
) -> Result<Decomposition, SeasonalityError> {
    let mut points = points.to_vec();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let xs: Vec<f64> = points.iter().map(|p| p[0]).collect();
    let y: Vec<f64> = points.iter().map(|p| p[1]).collect();
    let m = season_override.unwrap_or(spacing(&xs, kind)?.season);
    if m < 2 {
        return Err(SeasonalityError::NoSeason);
    }
    let n = y.len();
    if n < 2 * m {
        return Err(SeasonalityError::TooFewSeasons(m));
    }

    // Centred moving average over one season. An even season has no middle
    // point, so its two end points count half each.
    let half = m / 2;
    let trend: Vec<Option<f64>> = (0..n)
        .map(|i| {
            if i < half || i + half >= n {
                return None;
            }
            let window = &y[i - half..=i + half];
            Some(if m % 2 == 1 {
                window.iter().sum::<f64>() / m as f64
            } else {
                let ends = (window[0] + window[m]) / 2.0;
                (window[1..m].iter().sum::<f64>() + ends) / m as f64
            })
        })
        .collect();

    // The pattern: what is left of each position of the season, on average,
    // shifted so one season sums to zero.
    let mut sums = vec![0.0; m];
    let mut counts = vec![0usize; m];
    for (i, t) in trend.iter().enumerate() {
        if let Some(t) = t {
            sums[i % m] += y[i] - t;
            counts[i % m] += 1;
        }
    }
    let mut pattern: Vec<f64> = sums
        .iter()
        .zip(&counts)
        .map(|(s, c)| if *c > 0 { s / *c as f64 } else { 0.0 })
        .collect();
    let mean = pattern.iter().sum::<f64>() / m as f64;
    pattern.iter_mut().for_each(|p| *p -= mean);

    let seasonal: Vec<f64> = (0..n).map(|i| pattern[i % m]).collect();
    let rest: Vec<Option<f64>> = (0..n)
        .map(|i| trend[i].map(|t| y[i] - t - seasonal[i]))
        .collect();

    // 1 - Var(rest) / Var(pattern + rest), not below zero.
    let variance = |values: &[f64]| {
        let mean = values.iter().sum::<f64>() / values.len().max(1) as f64;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len().max(1) as f64
    };
    let left: Vec<f64> = rest.iter().flatten().copied().collect();
    let detrended: Vec<f64> = (0..n)
        .filter_map(|i| rest[i].map(|r| r + seasonal[i]))
        .collect();
    let around = variance(&detrended);
    let strength = if around > 0.0 {
        (1.0 - variance(&left) / around).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Ok(Decomposition {
        points,
        trend,
        seasonal,
        rest,
        season: m,
        strength,
    })
}

/// One row per point: `x, value, trend, seasonal, rest`.
pub fn decomposition_table(d: &Decomposition, kind: XAxisKind) -> DataTable {
    let (x_type, x_cell) = x_column(kind);
    let mut t = DataTable::empty();
    t.columns = std::iter::once(("x", x_type))
        .chain(["value", "trend", "seasonal", "rest"].map(|n| (n, "Float64")))
        .map(|(n, ty)| ColumnInfo {
            name: n.into(),
            data_type: ty.into(),
        })
        .collect();
    let r = |v: f64| CellValue::Float((v * 1000.0).round() / 1000.0);
    let opt = |v: Option<f64>| v.map_or(CellValue::Null, r);
    t.rows = (0..d.points.len())
        .map(|i| {
            vec![
                x_cell(d.points[i][0]),
                CellValue::Float(d.points[i][1]),
                opt(d.trend[i]),
                r(d.seasonal[i]),
                opt(d.rest[i]),
            ]
        })
        .collect();
    t
}

#[cfg(test)]
#[path = "seasonality_tests.rs"]
mod tests;
