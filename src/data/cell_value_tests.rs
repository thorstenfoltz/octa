use super::*;

#[test]
fn european_strings_convert_to_numbers() {
    let v = CellValue::String("1.234,56".to_string());
    assert!(can_convert_value(&v, "Float64"));
    assert_eq!(convert_value(&v, "Float64"), CellValue::Float(1234.56));

    let v = CellValue::String("3,25".to_string());
    assert!(can_convert_value(&v, "Float64"));
    assert_eq!(convert_value(&v, "Float64"), CellValue::Float(3.25));

    // A European thousands group with no decimal part is a whole number.
    let v = CellValue::String("1.234.567".to_string());
    assert!(can_convert_value(&v, "Int64"));
    assert_eq!(convert_value(&v, "Int64"), CellValue::Int(1_234_567));
}

#[test]
fn plain_english_numbers_are_unchanged() {
    // The relaxed parser tries the bare English form first, so nothing that
    // worked before this feature reads differently now.
    let v = CellValue::String("1.234".to_string());
    assert!(can_convert_value(&v, "Float64"));
    assert_eq!(convert_value(&v, "Float64"), CellValue::Float(1.234));

    let v = CellValue::String("-42".to_string());
    assert!(can_convert_value(&v, "Int64"));
    assert_eq!(convert_value(&v, "Int64"), CellValue::Int(-42));
}

#[test]
fn non_numbers_are_still_refused() {
    let v = CellValue::String("abc".to_string());
    assert!(!can_convert_value(&v, "Float64"));
    assert!(!can_convert_value(&v, "Int64"));

    // A decimal is not an integer under either convention.
    let v = CellValue::String("1.234,56".to_string());
    assert!(!can_convert_value(&v, "Int64"));
}

/// Change type on ISO text kept the `T`, and Parquet / the SQL workspace then
/// wrote every such cell as NULL.
#[test]
fn iso_text_becomes_a_datetime_every_writer_reads() {
    let ts = "Timestamp(Microsecond, None)";
    let v = CellValue::String("2024-01-02T03:04:05".to_string());
    assert!(can_convert_value(&v, ts));
    assert_eq!(
        convert_value(&v, ts),
        CellValue::DateTime("2024-01-02 03:04:05".into())
    );
    assert_eq!(
        canonical_datetime("2024-01-02T03:04:05.123Z"),
        "2024-01-02 03:04:05.123"
    );
    assert_eq!(
        canonical_datetime("2024-01-02 03:04:05.123456"),
        "2024-01-02 03:04:05.123456"
    );
    assert_eq!(canonical_datetime("not a date"), "not a date");
}

#[test]
fn time_and_duration_show_sort_and_parse_back() {
    use crate::data::times::NANOS_PER_SEC;
    let h = 3_600 * NANOS_PER_SEC;
    assert_eq!(
        CellValue::Time(14 * h + 30 * 60 * NANOS_PER_SEC).to_string(),
        "14:30:00"
    );
    assert_eq!(CellValue::Duration(-h).to_string(), "-01:00:00");
    let mut v = vec![
        CellValue::Duration(30 * h),
        CellValue::Duration(-h),
        CellValue::Null,
        CellValue::Duration(9 * h),
    ];
    v.sort_by(cmp_cell_values);
    assert_eq!(
        v,
        vec![
            CellValue::Null,
            CellValue::Duration(-h),
            CellValue::Duration(9 * h),
            CellValue::Duration(30 * h)
        ]
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Time(0), "2:30 PM"),
        CellValue::Time(14 * h + 30 * 60 * NANOS_PER_SEC)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Duration(0), "1 day"),
        CellValue::Duration(24 * h)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Time(0), "lunch"),
        CellValue::String("lunch".into())
    );
    assert!(is_right_aligned_type("Time64(µs)"));
    assert!(is_right_aligned_type("Duration(Microsecond)"));
    assert!(!is_right_aligned_type("Utf8"));
}

#[test]
fn conversions_to_and_from_the_time_types() {
    use crate::data::times::NANOS_PER_SEC;
    let h = 3_600 * NANOS_PER_SEC;
    let t = "Time64(Microsecond)";
    let d = "Duration(Microsecond)";
    let s = |x: &str| CellValue::String(x.into());
    assert!(can_convert_value(&s("14:30"), t));
    assert!(!can_convert_value(&s("25:00"), t));
    assert!(can_convert_value(&s("25:00"), d));
    assert!(!can_convert_value(&s("soon"), d));
    assert_eq!(
        convert_value(&s("2:30 PM"), t),
        CellValue::Time(14 * h + 30 * 60 * NANOS_PER_SEC)
    );
    let dt = CellValue::DateTime("2024-01-02 14:30:00.5".into());
    assert!(can_convert_value(&dt, t));
    assert_eq!(
        convert_value(&dt, t),
        CellValue::Time(14 * h + 30 * 60 * NANOS_PER_SEC + 500_000_000)
    );
    assert!(!can_convert_value(&CellValue::Date("2024-01-02".into()), t));
    assert_eq!(
        convert_value(&CellValue::Time(5 * h), d),
        CellValue::Duration(5 * h)
    );
    assert!(can_convert_value(&CellValue::Duration(5 * h), t));
    assert!(!can_convert_value(&CellValue::Duration(25 * h), t));
    assert!(!can_convert_value(&CellValue::Duration(-h), t));
    assert_eq!(
        convert_value(&CellValue::Duration(-h), "Utf8"),
        s("-01:00:00")
    );
    assert!(
        !can_convert_value(&CellValue::Int(5), d),
        "numbers need an explicit unit (Date/Time calculation)"
    );
}
