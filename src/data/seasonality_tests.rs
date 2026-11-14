use super::*;

/// Monthly points from 2020-01: a straight rise of 2 a month plus a pattern
/// that repeats every 12 months, and nothing else.
fn monthly(months: usize, pattern: &[f64; 12]) -> Vec<[f64; 2]> {
    let start = chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    (0..months)
        .map(|i| {
            let d = start + chrono::Months::new(i as u32);
            [
                (d - epoch).num_days() as f64,
                100.0 + 2.0 * i as f64 + pattern[i % 12],
            ]
        })
        .collect()
}

const PATTERN: [f64; 12] = [
    -6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 4.0, 2.0, 0.0, -10.0, 4.0,
];

#[test]
fn a_trend_plus_a_pattern_comes_apart_exactly() {
    let d = decompose(&monthly(48, &PATTERN), XAxisKind::Date, None).unwrap();
    assert_eq!(d.season, 12);
    // The pattern is found, for every month of every year.
    for i in 0..48 {
        assert!((d.seasonal[i] - PATTERN[i % 12]).abs() < 1e-9, "month {i}");
    }
    // No trend for the first and last six months, the straight line between.
    assert!(d.trend[..6].iter().all(Option::is_none));
    assert!(d.trend[42..].iter().all(Option::is_none));
    for i in 6..42 {
        assert!((d.trend[i].unwrap() - (100.0 + 2.0 * i as f64)).abs() < 1e-9);
        assert!(d.rest[i].unwrap().abs() < 1e-9, "month {i}");
    }
    assert!(d.strength > 0.999, "{}", d.strength);
}

#[test]
fn a_spike_lands_in_the_rest_and_not_in_the_pattern() {
    let mut points = monthly(48, &PATTERN);
    points[20][1] += 50.0;
    let d = decompose(&points, XAxisKind::Date, None).unwrap();
    let largest = (6..42)
        .max_by(|a, b| {
            d.rest[*a]
                .unwrap()
                .abs()
                .total_cmp(&d.rest[*b].unwrap().abs())
        })
        .unwrap();
    assert_eq!(largest, 20);
    assert!(d.rest[20].unwrap() > 30.0);
    assert!(d.strength < 0.9);
}

#[test]
fn a_series_without_a_pattern_has_no_strength() {
    let flat = monthly(48, &[0.0; 12]);
    let d = decompose(&flat, XAxisKind::Date, None).unwrap();
    assert_eq!(d.strength, 0.0);
    assert!(d.seasonal.iter().all(|s| s.abs() < 1e-9));
}

#[test]
fn an_odd_season_can_be_given_by_hand() {
    // Plain numbers name no season: 1, 2, 3, ... with a pattern of five.
    let pattern = [3.0, -1.0, -2.0, 1.0, -1.0];
    let points: Vec<[f64; 2]> = (0..30).map(|i| [i as f64, 10.0 + pattern[i % 5]]).collect();
    assert_eq!(
        decompose(&points, XAxisKind::Numeric, None),
        Err(SeasonalityError::NoSeason)
    );
    let d = decompose(&points, XAxisKind::Numeric, Some(5)).unwrap();
    for i in 0..30 {
        assert!((d.seasonal[i] - pattern[i % 5]).abs() < 1e-9);
    }
    assert!(d.trend[1].is_none() && d.trend[2].is_some());
}

#[test]
fn too_little_history_is_said_with_the_season() {
    assert_eq!(
        decompose(&monthly(23, &PATTERN), XAxisKind::Date, None),
        Err(SeasonalityError::TooFewSeasons(12))
    );
    assert_eq!(
        decompose(&monthly(3, &PATTERN), XAxisKind::Date, None),
        Err(SeasonalityError::TooShort)
    );
}

#[test]
fn the_table_has_one_row_per_point_and_blanks_at_the_ends() {
    let d = decompose(&monthly(24, &PATTERN), XAxisKind::Date, None).unwrap();
    let t = decomposition_table(&d, XAxisKind::Date);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["x", "value", "trend", "seasonal", "rest"]);
    assert_eq!(t.rows.len(), 24);
    assert_eq!(t.rows[0][0], CellValue::Date("2020-01-01".into()));
    assert_eq!(t.rows[0][2], CellValue::Null);
    assert_eq!(t.rows[0][3], CellValue::Float(-6.0));
    assert_eq!(t.rows[6][2], CellValue::Float(112.0));
    assert_eq!(t.rows[6][4], CellValue::Float(0.0));
}
