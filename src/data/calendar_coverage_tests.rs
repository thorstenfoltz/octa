use super::*;

fn days(start: &str, count: i64) -> Vec<CellValue> {
    let d = NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap();
    (0..count)
        .map(|i| CellValue::Date((d + Duration::days(i)).format("%Y-%m-%d").to_string()))
        .collect()
}

fn run(owned: &[CellValue], ty: &str) -> Outcome {
    let refs: Vec<&CellValue> = owned.iter().collect();
    analyse(&refs, ty)
}

#[test]
fn every_calendar_value_is_explained() {
    // Same guard as Benford's: the constants and the hint table are two
    // lists that have to stay one.
    let listed: Vec<&str> = VALUE_HINTS.iter().map(|(v, _)| *v).collect();
    for value in [
        COMPLETE,
        WEEKDAYS_ONLY,
        COMPLETE_DST,
        GAPS,
        NOT_A_DATE,
        TOO_FEW_POINTS,
        IRREGULAR,
    ] {
        assert!(listed.contains(&value), "{value} has no hint");
    }
    assert_eq!(listed.len(), 7, "VALUE_HINTS has a stray entry");
}

#[test]
fn an_unbroken_daily_series_is_complete() {
    let out = run(&days("2024-01-01", 60), "Date32");
    assert_eq!(out.id(), "complete");
    match out {
        Outcome::Checked { gaps, missing, .. } => {
            assert!(gaps.is_empty());
            assert_eq!(missing, 0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_missing_stretch_is_found_and_measured() {
    let mut owned = days("2024-01-01", 60);
    // Drop the 10th to 12th of January: three absent days.
    owned.retain(|c| !matches!(c, CellValue::Date(s) if s == "2024-01-10" || s == "2024-01-11" || s == "2024-01-12"));
    let out = run(&owned, "Date32");
    assert_eq!(out.id(), "gaps");
    match out {
        Outcome::Checked { gaps, missing, .. } => {
            assert_eq!(missing, 1);
            assert_eq!(gaps.len(), 1);
            assert_eq!(gaps[0].missing_steps, 3);
            assert_eq!(gaps[0].kind, GapKind::Missing);
            assert_eq!(gaps[0].after.date().to_string(), "2024-01-09");
            assert_eq!(gaps[0].before.date().to_string(), "2024-01-13");
        }
        other => panic!("{other:?}"),
    }
}

/// The false alarm that would sink the whole feature: business data skips
/// weekends on purpose, and 104 "gaps" a year is a report nobody reads.
#[test]
fn a_weekday_only_series_is_not_broken() {
    // 2024-01-01 is a Monday.
    let owned: Vec<CellValue> = days("2024-01-01", 90)
        .into_iter()
        .filter(|c| {
            let CellValue::Date(s) = c else { return true };
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
            !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
        })
        .collect();
    let out = run(&owned, "Date32");
    assert_eq!(out.id(), "weekdays only");
    match out {
        Outcome::Checked { gaps, missing, .. } => {
            assert_eq!(missing, 0, "weekends are not missing data");
            assert!(gaps.iter().all(|g| g.kind == GapKind::Weekend));
        }
        other => panic!("{other:?}"),
    }
}

/// A Friday-to-Wednesday hole is not a weekend, even though it contains
/// one. Both ends have to line up or it is real missing data.
#[test]
fn a_hole_that_merely_contains_a_weekend_is_still_missing() {
    let owned: Vec<CellValue> = days("2024-01-01", 90)
        .into_iter()
        .filter(|c| {
            let CellValue::Date(s) = c else { return true };
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
            if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                return false;
            }
            // Also drop the Monday and Tuesday of one week.
            s != "2024-02-05" && s != "2024-02-06"
        })
        .collect();
    match run(&owned, "Date32") {
        Outcome::Checked { gaps, missing, .. } => {
            assert_eq!(missing, 1);
            let real: Vec<&Gap> = gaps.iter().filter(|g| g.kind == GapKind::Missing).collect();
            assert_eq!(real.len(), 1);
            assert_eq!(real[0].after.date().to_string(), "2024-02-02");
            assert_eq!(real[0].before.date().to_string(), "2024-02-07");
        }
        other => panic!("{other:?}"),
    }
}

/// An hourly series over the spring-forward: naive local time simply has no
/// 02:00 that day, which must not read as a hole in the data.
#[test]
fn a_spring_forward_is_not_missing_data() {
    // 2024-03-31 is the last Sunday of March, the EU changeover.
    let start = NaiveDate::from_ymd_opt(2024, 3, 30)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let owned: Vec<CellValue> = (0..72)
        .map(|i| start + Duration::hours(i))
        .filter(|dt| !(dt.date().day() == 31 && dt.hour() == 2))
        .map(|dt| CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
        .collect();
    let out = run(&owned, "Timestamp(Microsecond, None)");
    assert_eq!(out.id(), "complete (clock change)");
    match out {
        Outcome::Checked { gaps, missing, .. } => {
            assert_eq!(missing, 0);
            assert_eq!(gaps.len(), 1);
            assert_eq!(gaps[0].kind, GapKind::DaylightSaving);
        }
        other => panic!("{other:?}"),
    }
}

/// The same one-hour hole on a Wednesday afternoon is missing data, which
/// is what keeps the daylight-saving rule from excusing everything.
#[test]
fn a_one_hour_hole_on_an_ordinary_day_is_missing() {
    let start = NaiveDate::from_ymd_opt(2024, 3, 27)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let owned: Vec<CellValue> = (0..72)
        .map(|i| start + Duration::hours(i))
        .filter(|dt| !(dt.date().day() == 27 && dt.hour() == 14))
        .map(|dt| CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S").to_string()))
        .collect();
    assert_eq!(run(&owned, "DateTime").id(), "gaps");
}

#[test]
fn a_series_with_no_step_is_irregular() {
    let owned: Vec<CellValue> = [0i64, 1, 7, 30, 92, 400, 401, 900]
        .iter()
        .map(|i| {
            let d = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + Duration::days(*i);
            CellValue::Date(d.format("%Y-%m-%d").to_string())
        })
        .collect();
    assert_eq!(run(&owned, "Date32").id(), "not tested: no regular step");
}

#[test]
fn a_text_column_and_a_short_one_are_skipped() {
    assert_eq!(
        run(&[CellValue::String("x".into())], "Utf8").id(),
        "not tested: not dates"
    );
    assert_eq!(
        run(&days("2024-01-01", 2), "Date32").id(),
        "not tested: too few points"
    );
}

#[test]
fn the_section_lists_real_gaps_and_leaves_weekends_out() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "day".into(),
        data_type: "Date32".into(),
    }];
    let owned: Vec<CellValue> = days("2024-01-01", 90)
        .into_iter()
        .filter(|c| {
            let CellValue::Date(s) = c else { return true };
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
            !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) && s != "2024-02-05"
        })
        .collect();
    t.rows = owned.into_iter().map(|c| vec![c]).collect();

    let section = section_table(&t).expect("one real gap");
    assert_eq!(section.row_count(), 1, "weekends must not be listed");
    assert_eq!(section.get(0, 0), Some(&CellValue::String("day".into())));
    assert_eq!(
        section.get(0, 4),
        Some(&CellValue::String("missing".into()))
    );
}

#[test]
fn a_clean_table_has_no_section() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "day".into(),
        data_type: "Date32".into(),
    }];
    t.rows = days("2024-01-01", 60)
        .into_iter()
        .map(|c| vec![c])
        .collect();
    assert!(section_table(&t).is_none());
}
