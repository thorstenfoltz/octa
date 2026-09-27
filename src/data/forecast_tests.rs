//! Unit tests for [`forecast`](super). Included via `#[path]`.

use super::*;

/// Four years of monthly data: a trend of +1 per month plus a yearly wave,
/// with a small fixed wobble. Without it the fit is exact, the error is
/// zero and the uncertainty band has no width to grow.
fn seasonal_monthly() -> Vec<[f64; 2]> {
    let start = chrono::NaiveDate::from_ymd_opt(2022, 1, 1).unwrap();
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    (0..48)
        .map(|i| {
            let d = start.checked_add_months(chrono::Months::new(i)).unwrap();
            let wave = [
                0.0, 2.0, 5.0, 9.0, 12.0, 14.0, 12.0, 9.0, 5.0, 2.0, 0.0, -1.0,
            ][i as usize % 12];
            let wobble = f64::from((i * 7) % 5) * 0.3 - 0.6;
            [
                (d - epoch).num_days() as f64,
                100.0 + i as f64 + wave + wobble,
            ]
        })
        .collect()
}

#[test]
fn spacing_detects_monthly_data_with_a_yearly_season() {
    let xs: Vec<f64> = seasonal_monthly().iter().map(|p| p[0]).collect();
    let sp = spacing(&xs, XAxisKind::Date).unwrap();
    assert_eq!(sp.season, 12);
    assert_eq!(sp.months, Some(1));
}

#[test]
fn spacing_refuses_uneven_and_short_series() {
    assert_eq!(
        spacing(&[0.0, 1.0, 2.0], XAxisKind::Numeric).err(),
        Some(ForecastError::TooShort)
    );
    let xs = [0.0, 1.0, 2.0, 10.0, 11.0, 30.0, 31.0, 32.0];
    assert_eq!(
        spacing(&xs, XAxisKind::Numeric).err(),
        Some(ForecastError::Uneven)
    );
}

#[test]
fn holt_winters_continues_trend_and_season() {
    let pts = seasonal_monthly();
    let f = forecast(&pts, XAxisKind::Date, 12, None).unwrap();
    assert_eq!(f.season, 12);
    assert_eq!(f.points.len(), 12);
    // Month 49 is a January (wave 0): about 100 + 48.
    assert!(
        (f.points[0].value - 148.0).abs() < 3.0,
        "{}",
        f.points[0].value
    );
    // Month 54 is a June (wave 14): about 100 + 53 + 14.
    assert!(
        (f.points[5].value - 167.0).abs() < 3.0,
        "{}",
        f.points[5].value
    );
    // Calendar stepping: the first forecast date is 2026-01-01.
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let jan = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    assert_eq!(f.points[0].x, (jan - epoch).num_days() as f64);
    let p = &f.points[3];
    assert!(p.lo95 <= p.lo80 && p.lo80 <= p.value && p.value <= p.hi80 && p.hi80 <= p.hi95);
    assert!(f.points[11].hi95 - f.points[11].lo95 > f.points[0].hi95 - f.points[0].lo95);
}

#[test]
fn a_short_history_forecasts_without_a_season() {
    let pts: Vec<[f64; 2]> = (0..10).map(|i| [i as f64, 2.0 * i as f64]).collect();
    let f = forecast(&pts, XAxisKind::Numeric, 3, None).unwrap();
    assert_eq!(f.season, 0);
    assert!((f.points[0].value - 20.0).abs() < 0.5);
}

#[test]
fn trends_fit_a_line_and_a_trailing_average() {
    let pts: Vec<[f64; 2]> = (0..5).map(|i| [i as f64, 1.0 + 2.0 * i as f64]).collect();
    assert_eq!(straight_trend(&pts), vec![[0.0, 1.0], [4.0, 9.0]]);
    assert_eq!(
        moving_average(&pts, 3),
        vec![[2.0, 3.0], [3.0, 5.0], [4.0, 7.0]]
    );
}

#[test]
fn overlays_name_their_series_after_the_source() {
    let s = ChartSeries {
        name: "sales".into(),
        points: seasonal_monthly(),
    };
    let o = overlays(&[s], XAxisKind::Date, TrendKind::Straight, 6, None).unwrap();
    let names: Vec<&str> = o.series.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "sales (trend)",
            "sales (forecast)",
            "sales (95% low)",
            "sales (95% high)"
        ]
    );
    assert_eq!(o.bands.len(), 2);
}

#[test]
fn forecast_table_formats_dates() {
    let f = forecast(&seasonal_monthly(), XAxisKind::Date, 2, None).unwrap();
    let t = forecast_table(&f, XAxisKind::Date);
    assert_eq!(t.rows[0][0], CellValue::Date("2026-01-01".into()));
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["x", "forecast", "lo80", "hi80", "lo95", "hi95"]);
}

#[test]
fn series_for_reads_dates_as_day_numbers() {
    let mut t = DataTable::empty();
    t.columns = [("month", "Date32"), ("sales", "Float64")]
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: (*n).into(),
            data_type: (*ty).into(),
        })
        .collect();
    t.rows = vec![
        vec![CellValue::Date("2026-01-01".into()), CellValue::Float(1.0)],
        vec![CellValue::Date("2026-02-01".into()), CellValue::Float(2.0)],
    ];
    let (s, kind) = series_for(&t, 0, 1).unwrap();
    assert_eq!(kind, XAxisKind::Date);
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let jan = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    assert_eq!(s.points[0], [(jan - epoch).num_days() as f64, 1.0]);
}
