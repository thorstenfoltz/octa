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
