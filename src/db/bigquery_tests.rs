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
