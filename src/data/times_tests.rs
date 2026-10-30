use super::*;

const H: i64 = 3_600 * NANOS_PER_SEC;
const M: i64 = 60 * NANOS_PER_SEC;
const S: i64 = NANOS_PER_SEC;

#[test]
fn clock_times_parse() {
    assert_eq!(parse_time("14:30"), Some(14 * H + 30 * M));
    assert_eq!(parse_time("14:30:05"), Some(14 * H + 30 * M + 5 * S));
    assert_eq!(parse_time("9:05"), Some(9 * H + 5 * M));
    assert_eq!(parse_time(" 06:00:00 "), Some(6 * H));
    assert_eq!(
        parse_time("14:30:00.25"),
        Some(14 * H + 30 * M + 250_000_000)
    );
    assert_eq!(parse_time("00:00:00.000000001"), Some(1));
    assert_eq!(parse_time("23:59:59.999999999"), Some(NANOS_PER_DAY - 1));
}

#[test]
fn am_pm_reads_noon_and_midnight_right() {
    assert_eq!(parse_time("12:00 AM"), Some(0));
    assert_eq!(parse_time("12:00 PM"), Some(12 * H));
    assert_eq!(parse_time("2:30 pm"), Some(14 * H + 30 * M));
    assert_eq!(parse_time("2:30PM"), Some(14 * H + 30 * M));
    assert_eq!(parse_time("11:59:59 am"), Some(11 * H + 59 * M + 59 * S));
    assert_eq!(parse_time("0:30 AM"), None);
    assert_eq!(parse_time("13:00 PM"), None);
}

#[test]
fn not_a_clock_time() {
    for s in [
        "",
        "24:00",
        "23:59:60",
        "14:5",
        "14",
        "-01:00",
        "1:2:3",
        "10:20:30:40",
        "ab:cd",
        "14:30:",
        "14:30.5",
        "14:30:00.",
        "14:30:00.1234567890",
        "100:00",
    ] {
        assert_eq!(parse_time(s), None, "{s:?}");
    }
}

#[test]
fn durations_parse_in_every_spelling() {
    assert_eq!(parse_duration("30:15:00"), Some(30 * H + 15 * M));
    assert_eq!(parse_duration("-01:30:00"), Some(-(H + 30 * M)));
    assert_eq!(
        parse_duration("75:20"),
        Some(75 * H + 20 * M),
        "two parts are hours:minutes"
    );
    assert_eq!(parse_duration("+02:00"), Some(2 * H));
    assert_eq!(parse_duration("1 day 06:15:00"), Some(30 * H + 15 * M));
    assert_eq!(parse_duration("2 days"), Some(48 * H));
    assert_eq!(parse_duration("1 DAY"), Some(24 * H));
    assert_eq!(parse_duration("-1 day -06:15:00"), Some(-(30 * H + 15 * M)));
    assert_eq!(
        parse_duration("-1 days +02:00:00"),
        Some(-22 * H),
        "Postgres signs each part"
    );
    assert_eq!(parse_duration("PT30H15M"), Some(30 * H + 15 * M));
    assert_eq!(parse_duration("P1DT2H"), Some(26 * H));
    assert_eq!(parse_duration("P1W"), Some(7 * 24 * H));
    assert_eq!(parse_duration("-PT1H30M"), Some(-(H + 30 * M)));
    assert_eq!(parse_duration("PT1.5S"), Some(1_500_000_000));
    assert_eq!(parse_duration("pt90s"), Some(90 * S));
    assert_eq!(
        parse_duration("0-0 2 3:0:0"),
        Some(51 * H),
        "BigQuery canonical"
    );
    assert_eq!(parse_duration("0-0 0 -1:30:0"), Some(-(H + 30 * M)));
    assert_eq!(
        parse_duration("2 03:04:05.000"),
        Some(51 * H + 4 * M + 5 * S),
        "Trino / Oracle"
    );
    assert_eq!(
        parse_duration("-2 03:00:00"),
        Some(-51 * H),
        "sign covers the whole value"
    );
    assert_eq!(
        parse_duration("+02 03:04:05.000000"),
        Some(51 * H + 4 * M + 5 * S)
    );
    assert_eq!(
        parse_duration("14:30"),
        Some(14 * H + 30 * M),
        "a clock time is an amount too"
    );
}

#[test]
fn calendar_parts_and_junk_are_not_durations() {
    for s in [
        "",
        "P1M",
        "P1Y",
        "P1Y2M",
        "1 mon 2 days",
        "1 year",
        "3 months",
        "1-2 0 0:0:0",
        "PT",
        "P",
        "2:30 PM",
        "1:2",
        "abc",
        "1 day abc",
        "PT1H2",
        "12:60",
        "1:00:60",
        "--1:00",
    ] {
        assert_eq!(parse_duration(s), None, "{s:?}");
    }
}

#[test]
fn huge_values_do_not_overflow() {
    assert_eq!(parse_duration("999999999999999999:00:00"), None);
    assert_eq!(parse_duration("P999999999999999999D"), None);
    assert_eq!(
        format_duration(i64::MIN, DurationStyle::Clock)
            .chars()
            .next(),
        Some('-')
    );
}

#[test]
fn times_format_with_a_fraction_only_when_present() {
    assert_eq!(format_time(14 * H + 30 * M), "14:30:00");
    assert_eq!(format_time(14 * H + 30 * M + 250_000_000), "14:30:00.25");
    assert_eq!(format_time(1), "00:00:00.000000001");
    assert_eq!(format_time(0), "00:00:00");
}

#[test]
fn durations_format_in_each_style() {
    let d = 30 * H + 15 * M;
    assert_eq!(format_duration(d, DurationStyle::Clock), "30:15:00");
    assert_eq!(format_duration(d, DurationStyle::Days), "1 day 06:15:00");
    assert_eq!(format_duration(d, DurationStyle::Iso), "PT30H15M");
    assert_eq!(format_duration(-d, DurationStyle::Clock), "-30:15:00");
    assert_eq!(format_duration(-d, DurationStyle::Days), "-1 day -06:15:00");
    assert_eq!(format_duration(-d, DurationStyle::Iso), "-PT30H15M");
    assert_eq!(
        format_duration(48 * H, DurationStyle::Days),
        "2 days 00:00:00"
    );
    assert_eq!(format_duration(5 * M, DurationStyle::Days), "00:05:00");
    assert_eq!(format_duration(0, DurationStyle::Iso), "PT0S");
    assert_eq!(format_duration(1_500_000_000, DurationStyle::Iso), "PT1.5S");
    assert_eq!(
        format_duration(-(H + 30 * M), DurationStyle::Clock),
        "-01:30:00"
    );
}

#[test]
fn every_style_reads_back_what_it_wrote() {
    let values = [
        0,
        1,
        5 * S,
        -(H + 30 * M),
        30 * H + 15 * M,
        -(30 * H + 15 * M + 250_000_000),
        3 * 24 * H + 7,
        NANOS_PER_DAY - 1,
    ];
    for style in DurationStyle::ALL {
        for v in values {
            let text = format_duration(v, style);
            assert_eq!(parse_duration(&text), Some(v), "{style:?} {text:?}");
        }
    }
    for v in [0, 1, 14 * H + 30 * M + 250_000_000, NANOS_PER_DAY - 1] {
        assert_eq!(parse_time(&format_time(v)), Some(v));
    }
}

#[test]
fn a_column_is_time_duration_or_neither() {
    fn col<'a>(v: &[&'a str]) -> Vec<Option<&'a str>> {
        v.iter().map(|s| Some(*s)).collect()
    }
    assert_eq!(
        classify_column(&col(&["06:00:00", "14:30:00", "22:15:00"])),
        Some(TimeKind::Time)
    );
    assert_eq!(
        classify_column(&col(&["2:30 PM", "09:00"])),
        Some(TimeKind::Time)
    );
    assert_eq!(
        classify_column(&col(&["08:00", "30:15:00"])),
        Some(TimeKind::Duration)
    );
    assert_eq!(
        classify_column(&col(&["-01:00", "00:30"])),
        Some(TimeKind::Duration)
    );
    assert_eq!(
        classify_column(&col(&["PT1H", "PT2H"])),
        Some(TimeKind::Duration)
    );
    assert_eq!(
        classify_column(&col(&["1 day 02:00:00", "03:00:00"])),
        Some(TimeKind::Duration)
    );
    assert_eq!(
        classify_column(&col(&["2:30 PM", "25:00"])),
        None,
        "AM/PM is never a duration"
    );
    assert_eq!(classify_column(&col(&["14:30", "soon"])), None);
    assert_eq!(classify_column(&col(&["", "  "])), None, "nothing to go on");
    assert_eq!(
        classify_column(&[None, Some("14:30"), None]),
        Some(TimeKind::Time)
    );
}

#[test]
fn type_names_say_kind_and_unit() {
    assert_eq!(TimeKind::of_type("Time64(µs)"), Some(TimeKind::Time));
    assert_eq!(TimeKind::of_type("Time32(ms)"), Some(TimeKind::Time));
    assert_eq!(
        TimeKind::of_type("Time64(Microsecond)"),
        Some(TimeKind::Time)
    );
    assert_eq!(TimeKind::of_type("Duration(ns)"), Some(TimeKind::Duration));
    assert_eq!(TimeKind::of_type("Timestamp(Microsecond, None)"), None);
    assert_eq!(TimeKind::of_type("Utf8"), None);
    assert_eq!(type_unit("Time64(µs)"), Unit::Micro);
    assert_eq!(type_unit("Time64(Nanosecond)"), Unit::Nano);
    assert_eq!(type_unit("Time32(s)"), Unit::Second);
    assert_eq!(type_unit("Duration(Millisecond)"), Unit::Milli);
    assert_eq!(type_unit("Duration"), Unit::Micro);
}

#[test]
fn promotion_types_text_formats_only_and_obeys_the_switch() {
    use crate::data::{CellValue, ColumnInfo, DataTable};
    let table = |format: &str| DataTable {
        columns: vec![
            ColumnInfo {
                name: "clock_in".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "worked".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "note".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::String("06:00:00".into()),
                CellValue::String("8:30".into()),
                CellValue::String("a".into()),
            ],
            vec![
                CellValue::String("".into()),
                CellValue::String("30:15:00".into()),
                CellValue::String("b".into()),
            ],
        ],
        format_name: Some(format.into()),
        ..DataTable::empty()
    };
    let mut csv = table("CSV");
    assert_eq!(
        promote_columns_with(&mut csv, true),
        vec![(0, TimeKind::Time), (1, TimeKind::Duration)]
    );
    assert_eq!(csv.columns[0].data_type, "Time64(Microsecond)");
    assert_eq!(csv.rows[0][0], CellValue::Time(6 * H));
    assert_eq!(csv.rows[1][0], CellValue::Null);
    assert_eq!(csv.rows[1][1], CellValue::Duration(30 * H + 15 * M));
    assert_eq!(csv.columns[2].data_type, "Utf8");
    assert!(promote_columns_with(&mut table("CSV"), false).is_empty());
    assert!(promote_columns_with(&mut table("Parquet"), true).is_empty());
}

#[test]
fn a_month_interval_makes_the_column_text() {
    use crate::data::{CellValue, ColumnInfo, DataTable};
    let mut t = DataTable {
        columns: vec![ColumnInfo {
            name: "i".into(),
            data_type: "Duration(µs)".into(),
        }],
        rows: vec![
            vec![CellValue::Duration(H)],
            vec![CellValue::String("1 mon".into())],
        ],
        ..DataTable::empty()
    };
    settle_interval_columns(&mut t);
    assert_eq!(t.columns[0].data_type, "Utf8");
    assert_eq!(t.rows[0][0], CellValue::String("01:00:00".into()));
    assert_eq!(t.rows[1][0], CellValue::String("1 mon".into()));
}

#[test]
fn narrowing_keeps_durations_longer_than_a_day() {
    use crate::data::{CellValue, ColumnInfo, DataTable};
    let mut t = DataTable {
        columns: vec![ColumnInfo {
            name: "t".into(),
            data_type: "Duration(µs)".into(),
        }],
        rows: vec![vec![CellValue::Duration(14 * H)], vec![CellValue::Null]],
        ..DataTable::empty()
    };
    assert_eq!(narrow_to_time(&mut t, 0), TimeKind::Time);
    assert_eq!(t.rows[0][0], CellValue::Time(14 * H));
    t.rows[0][0] = CellValue::Duration(25 * H);
    t.columns[0].data_type = "Duration(µs)".into();
    assert_eq!(narrow_to_time(&mut t, 0), TimeKind::Duration);
}

#[test]
fn nanosecond_digits_ask_for_a_nanosecond_type() {
    use crate::data::CellValue;
    assert!(!needs_nanos(&[CellValue::Time(1_000), CellValue::Null]));
    assert!(needs_nanos(&[CellValue::Duration(1)]));
}

#[test]
fn the_type_words_name_octas_own_types() {
    assert_eq!(type_from_name("time"), Some("Time64(Microsecond)"));
    assert_eq!(type_from_name(" Duration "), Some("Duration(Microsecond)"));
    assert_eq!(
        type_from_name("Time64(µs)"),
        None,
        "a full name is used as given"
    );
    assert_eq!(type_from_name("timestamp"), None);
}
