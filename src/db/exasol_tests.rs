use super::*;

#[test]
fn categorise_decimal_scale() {
    assert!(matches!(categorise("DECIMAL(18, 0)"), Cat::Int));
    assert!(matches!(categorise("DECIMAL(10, 2)"), Cat::Decimal));
    assert!(matches!(categorise("DECIMAL(*, 0)"), Cat::Int));
    assert!(matches!(categorise("DOUBLE"), Cat::Double));
    assert!(matches!(categorise("BOOLEAN"), Cat::Bool));
    assert!(matches!(categorise("VARCHAR(100) UTF8"), Cat::Text));
    assert!(matches!(categorise("TIMESTAMP"), Cat::Timestamp));
    assert!(matches!(categorise("DATE"), Cat::Date));
}

#[test]
fn cats_map_to_arrow() {
    assert_eq!(Cat::Int.arrow(), "Int64");
    assert_eq!(Cat::Decimal.arrow(), "Float64");
    assert_eq!(Cat::Timestamp.arrow(), "Timestamp(Microsecond, None)");
}

#[test]
fn interval_day_to_second_is_a_duration() {
    assert_eq!(
        categorise("INTERVAL DAY(2) TO SECOND(3)").arrow(),
        "Duration(µs)"
    );
    assert_eq!(categorise("INTERVAL YEAR(2) TO MONTH").arrow(), "Utf8");
    assert_eq!(
        text_cell(Cat::Duration, "+02 03:04:05.000".into()),
        CellValue::Duration((2 * 86_400 + 3 * 3_600 + 4 * 60 + 5) * 1_000_000_000)
    );
    assert_eq!(
        text_cell(Cat::Duration, "odd".into()),
        CellValue::String("odd".into())
    );
}
