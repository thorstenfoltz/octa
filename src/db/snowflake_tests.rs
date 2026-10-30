use super::*;

#[test]
fn parse_snowflake_statement_result() {
    let v = serde_json::json!({
        "resultSetMetaData": { "rowType": [
            {"name":"ID","type":"fixed","scale":0},
            {"name":"NAME","type":"text"} ] },
        "data": [ ["1","alice"], ["2","bob"] ]
    });
    let t = parse_sf_result(&v).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["ID", "NAME"]);
    assert_eq!(t.columns[0].data_type, "Int64"); // fixed scale 0 -> Int64
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.rows[0][0], CellValue::Int(1));
    assert_eq!(t.rows[1][1], CellValue::String("bob".into()));
}

/// A one-partition result must not send a second request, and a
/// multi-partition one must be recognised as such: this count is the only
/// thing standing between a large table and silently losing every row
/// past partition 0.
#[test]
fn partition_count_drives_the_walk() {
    let single = serde_json::json!({
        "resultSetMetaData": { "partitionInfo": [ {"rowCount": 100} ] }
    });
    assert_eq!(sf_partition_count(&single), 1);
    let many = serde_json::json!({
        "resultSetMetaData": { "partitionInfo": [
            {"rowCount": 100}, {"rowCount": 100}, {"rowCount": 7} ] }
    });
    assert_eq!(sf_partition_count(&many), 3);
    // An API that says nothing means the one partition already in hand.
    assert_eq!(sf_partition_count(&serde_json::json!({})), 1);
}

#[test]
fn partition_path_addresses_the_same_statement() {
    assert_eq!(
        sf_partition_path("01b2-c3d4", 2),
        "api/v2/statements/01b2-c3d4?partition=2"
    );
}

/// A further partition's `data` appends to the rows already read, through
/// the column types taken from partition 0's metadata.
#[test]
fn a_partition_body_appends_to_the_rows_in_hand() {
    let columns = vec![ColumnInfo {
        name: "ID".to_string(),
        data_type: "Int64".to_string(),
    }];
    let mut rows = vec![vec![CellValue::Int(1)]];
    append_sf_rows(&serde_json::json!([["2"], [null]]), &columns, &mut rows);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1][0], CellValue::Int(2));
    assert_eq!(rows[2][0], CellValue::Null);
}

#[test]
fn fixed_with_scale_is_float() {
    assert_eq!(sf_type_to_arrow("fixed", Some(2)), "Float64");
    assert_eq!(sf_type_to_arrow("fixed", Some(0)), "Int64");
    assert_eq!(
        sf_type_to_arrow("TIMESTAMP_NTZ", None),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(sf_type_to_arrow("boolean", None), "Boolean");
    assert_eq!(sf_type_to_arrow("variant", None), "Utf8");
}

/// The SQL API's own encodings: days for DATE, decimal seconds for every
/// TIMESTAMP (TZ with an offset after a space, here +60 min = 1500 - 1440).
#[test]
fn dates_and_timestamps_are_decoded_from_epoch_numbers() {
    let v = serde_json::json!({
        "resultSetMetaData": { "rowType": [
            {"name":"D","type":"date"},
            {"name":"NTZ","type":"timestamp_ntz"},
            {"name":"TZ","type":"timestamp_tz"} ] },
        "data": [
            ["18262", "1616173619.000000000", "1616173619.250000000 1500"],
            ["-1", "-1.500000000", null] ]
    });
    let t = parse_sf_result(&v).unwrap();
    assert_eq!(t.rows[0][0], CellValue::Date("2020-01-01".into()));
    assert_eq!(
        t.rows[0][1],
        CellValue::DateTime("2021-03-19 17:06:59".into())
    );
    assert_eq!(
        t.rows[0][2],
        CellValue::DateTime("2021-03-19 17:06:59.250".into())
    );
    assert_eq!(t.rows[1][0], CellValue::Date("1969-12-31".into()));
    assert_eq!(
        t.rows[1][1],
        CellValue::DateTime("1969-12-31 23:59:58.500".into())
    );
    assert_eq!(t.rows[1][2], CellValue::Null);
}

#[test]
fn time_reads_exactly_from_seconds_since_midnight() {
    assert_eq!(sf_type_to_arrow("time", Some(9)), "Time64(ns)");
    assert_eq!(
        sf_cell(&serde_json::json!("52200.250000000"), "Time64(ns)"),
        CellValue::Time(52_200_250_000_000)
    );
    assert_eq!(
        sf_cell(&serde_json::json!("86399.999999999"), "Time64(ns)"),
        CellValue::Time(86_399_999_999_999)
    );
    assert_eq!(
        sf_cell(&serde_json::json!("0"), "Time64(ns)"),
        CellValue::Time(0)
    );
    assert_eq!(
        sf_cell(&serde_json::json!("abc"), "Time64(ns)"),
        CellValue::String("abc".into())
    );
}
