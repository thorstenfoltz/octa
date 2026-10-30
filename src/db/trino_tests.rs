use super::*;

#[test]
fn types_map_including_parameterised_ones() {
    assert_eq!(trino_type_to_arrow("bigint"), "Int64");
    assert_eq!(trino_type_to_arrow("varchar(10)"), "Utf8");
    assert_eq!(trino_type_to_arrow("decimal(38,2)"), "Float64");
    assert_eq!(
        trino_type_to_arrow("timestamp(6) with time zone"),
        "Timestamp(Microsecond, None)"
    );
    // A time has no date part, so it is not a timestamp.
    assert_eq!(trino_type_to_arrow("time(3)"), "Time64(µs)");
    assert_eq!(trino_type_to_arrow("array(varchar)"), "Utf8");
}

/// Trino sends real JSON types, and a decimal as text so it keeps its
/// precision on the wire: both have to land in the same bucket.
#[test]
fn cells_take_json_types_and_text_numbers() {
    assert_eq!(trino_cell(&Value::Null, "Int64"), CellValue::Null);
    assert_eq!(
        trino_cell(&serde_json::json!(42), "Int64"),
        CellValue::Int(42)
    );
    assert_eq!(
        trino_cell(&serde_json::json!("1.50"), "Float64"),
        CellValue::Float(1.5)
    );
    assert_eq!(
        trino_cell(&serde_json::json!(["a", "b"]), "Utf8"),
        CellValue::Nested("[\"a\",\"b\"]".into())
    );
}

/// A failed Trino statement comes back as HTTP 200 with an `error` object,
/// so the body is the only place the failure shows.
#[test]
fn statement_error_is_read_from_the_body() {
    let v = serde_json::json!({
        "error": {"message": "line 1:8: Column 'nope' cannot be resolved",
                  "errorName": "COLUMN_NOT_FOUND"}
    });
    assert_eq!(
        trino_error(&v).unwrap(),
        "COLUMN_NOT_FOUND: line 1:8: Column 'nope' cannot be resolved"
    );
    assert!(trino_error(&serde_json::json!({"stats": {"state": "RUNNING"}})).is_none());
}

#[test]
fn time_and_interval_types_read_typed() {
    assert_eq!(trino_type_to_arrow("time"), "Time64(µs)");
    assert_eq!(trino_type_to_arrow("time(3)"), "Time64(µs)");
    assert_eq!(trino_type_to_arrow("time(3) with time zone"), "Utf8");
    assert_eq!(trino_type_to_arrow("time with time zone"), "Utf8");
    assert_eq!(
        trino_type_to_arrow("interval day to second"),
        "Duration(µs)"
    );
    assert_eq!(trino_type_to_arrow("interval year to month"), "Utf8");
    assert_eq!(
        trino_cell(&serde_json::json!("14:30:00.250"), "Time64(µs)"),
        CellValue::Time(52_200_250_000_000)
    );
    assert_eq!(
        trino_cell(&serde_json::json!("2 03:04:05.000"), "Duration(µs)"),
        CellValue::Duration((2 * 86_400 + 3 * 3_600 + 4 * 60 + 5) * 1_000_000_000)
    );
    assert_eq!(
        trino_cell(&serde_json::json!("oops"), "Duration(µs)"),
        CellValue::String("oops".into())
    );
}
