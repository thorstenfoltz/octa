use super::*;

/// The token is what tells the walk there is more; an absent or empty one
/// is the end of the result, not a page to fetch.
#[test]
fn page_token_marks_the_end_of_a_result() {
    assert_eq!(
        bq_page_token(&serde_json::json!({"pageToken": "abc"})),
        Some("abc".to_string())
    );
    assert_eq!(bq_page_token(&serde_json::json!({"pageToken": ""})), None);
    assert_eq!(bq_page_token(&serde_json::json!({})), None);
}

#[test]
fn page_path_carries_an_escaped_token() {
    assert_eq!(
        bq_page_path("proj", "job_1", 100, "a+b/c=="),
        "bigquery/v2/projects/proj/queries/job_1?maxResults=100&pageToken=a%2Bb%2Fc%3D%3D"
    );
}

#[test]
fn a_page_body_appends_to_the_rows_in_hand() {
    let columns = vec![ColumnInfo {
        name: "id".to_string(),
        data_type: "Int64".to_string(),
    }];
    let mut rows = vec![vec![CellValue::Int(1)]];
    append_bq_rows(
        &serde_json::json!([{"f":[{"v":"2"}]}, {"f":[{"v":null}]}]),
        &columns,
        &mut rows,
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1][0], CellValue::Int(2));
    assert_eq!(rows[2][0], CellValue::Null);
}

#[test]
fn parse_bigquery_query_response() {
    let v = serde_json::json!({
        "schema": { "fields": [
            {"name":"id","type":"INTEGER"},
            {"name":"name","type":"STRING"} ] },
        "rows": [
            {"f":[{"v":"1"},{"v":"alice"}]},
            {"f":[{"v":"2"},{"v":"bob"}]} ]
    });
    let t = parse_bq_result(&v).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "name"]);
    assert_eq!(t.columns[0].data_type, "Int64");
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.rows[0][0], CellValue::Int(1));
    assert_eq!(t.rows[1][1], CellValue::String("bob".into()));
}

#[test]
fn bq_types_and_null_cell() {
    assert_eq!(bq_type_to_arrow("INT64"), "Int64");
    assert_eq!(bq_type_to_arrow("NUMERIC"), "Float64");
    assert_eq!(
        bq_type_to_arrow("TIMESTAMP"),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(bq_cell(&Value::Null, "Int64"), CellValue::Null);
}

/// TIMESTAMP arrives as float seconds, DATETIME as ISO text.
#[test]
fn timestamps_and_datetimes_become_datetime_text() {
    let ts = "Timestamp(Microsecond, None)";
    let cell = |s: &str| bq_cell(&serde_json::json!(s), ts);
    assert_eq!(
        cell("1.7036064E9"),
        CellValue::DateTime("2023-12-26 16:00:00".into())
    );
    assert_eq!(
        cell("1.703606400123456E9"),
        CellValue::DateTime("2023-12-26 16:00:00.123456".into())
    );
    assert_eq!(
        cell("-0.5"),
        CellValue::DateTime("1969-12-31 23:59:59.500".into())
    );
    assert_eq!(
        cell("2024-01-02T03:04:05"),
        CellValue::DateTime("2024-01-02 03:04:05".into())
    );
}

#[test]
fn time_and_interval_columns_read_typed() {
    let v = serde_json::json!({
        "schema": {"fields": [
            {"name": "t", "type": "TIME"}, {"name": "i", "type": "INTERVAL"}]},
        "rows": [
            {"f": [{"v": "14:30:00.250000"}, {"v": "0-0 2 3:0:0"}]},
            {"f": [{"v": "00:00:01"}, {"v": "1-0 0 0:0:0"}]}
        ]
    });
    let t = parse_bq_result(&v).unwrap();
    assert_eq!(t.columns[0].data_type, "Time64(µs)");
    assert_eq!(t.columns[1].data_type, "Duration(µs)");
    assert_eq!(t.rows[0][0], CellValue::Time(52_200_250_000_000));
    assert_eq!(
        t.rows[0][1],
        CellValue::Duration((2 * 86_400 + 3 * 3_600) * 1_000_000_000)
    );
    // A month part cannot be a Duration; it stays the original text.
    assert_eq!(t.rows[1][1], CellValue::String("1-0 0 0:0:0".into()));
}
