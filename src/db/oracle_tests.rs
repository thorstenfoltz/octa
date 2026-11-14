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
/// decimal. Everything not a fitting whole number is an exact decimal, sized
/// later from the values (`settle_server_decimals`).
#[test]
fn number_scale_decides_int_or_decimal() {
    use crate::data::decimal::SERVER_DECIMAL;
    assert_eq!(arrow_type(&col(OracleType::Number, 18, 0)), "Int64");
    assert_eq!(arrow_type(&col(OracleType::Number, 38, 0)), SERVER_DECIMAL);
    assert_eq!(arrow_type(&col(OracleType::Number, 10, 2)), SERVER_DECIMAL);
    assert_eq!(
        arrow_type(&col(OracleType::Number, 0, -127)),
        SERVER_DECIMAL
    );
}

/// An Oracle DATE carries a time of day, unlike a SQL DATE.
#[test]
fn date_is_a_timestamp() {
    assert_eq!(
        arrow_type(&col(OracleType::Date, 0, 0)),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(arrow_type(&col(OracleType::Blob, 0, 0)), "Binary");
    assert_eq!(arrow_type(&col(OracleType::Varchar, 0, 0)), "Utf8");
}
