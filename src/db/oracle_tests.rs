use super::*;

fn col(oracle_type: OracleType, precision: i16, scale: i16) -> oracle_rs::ColumnInfo {
    oracle_rs::ColumnInfo {
        name: "C".to_string(),
        oracle_type,
        data_size: 22,
        buffer_size: 22,
        precision,
        scale,
        nullable: true,
        csfrm: 1,
        type_schema: None,
        type_name: None,
        domain_schema: None,
        domain_name: None,
        is_json: false,
        is_oson: false,
        vector_dimensions: None,
        vector_format: None,
        element_type: None,
    }
}

/// NUMBER is Oracle's only numeric type, so the integer/decimal split has
/// to come out of the precision and scale or every id column reads as a
/// float.
#[test]
fn number_scale_decides_int_or_float() {
    let none = || [].iter();
    assert_eq!(arrow_type(&col(OracleType::Number, 38, 0), none()), "Int64");
    assert_eq!(
        arrow_type(&col(OracleType::Number, 10, 2), none()),
        "Float64"
    );
}

/// A literal or a computed column carries no precision, so its values are
/// the only evidence there is.
#[test]
fn undeclared_number_follows_its_values() {
    let whole = [Value::Null, Value::Integer(1)];
    let fractional = [Value::Integer(1), Value::Float(1.5)];
    assert_eq!(
        arrow_type(&col(OracleType::Number, 0, -127), whole.iter()),
        "Int64"
    );
    assert_eq!(
        arrow_type(&col(OracleType::Number, 0, -127), fractional.iter()),
        "Float64"
    );
}

/// An Oracle DATE carries a time of day, unlike a SQL DATE.
#[test]
fn date_is_a_timestamp() {
    let none = || [].iter();
    assert_eq!(
        arrow_type(&col(OracleType::Date, 0, 0), none()),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(arrow_type(&col(OracleType::Blob, 0, 0), none()), "Binary");
    assert_eq!(arrow_type(&col(OracleType::Varchar, 0, 0), none()), "Utf8");
}
