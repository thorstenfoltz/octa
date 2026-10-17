use super::*;

#[test]
fn parse_jsoncompact_names_types_rows() {
    let body = r#"{
          "meta":[{"name":"id","type":"UInt32"},{"name":"name","type":"String"}],
          "data":[[1,"a"],[2,"b"]],
          "rows":2
        }"#;
    let t = parse_jsoncompact(body).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "name"]);
    assert_eq!(t.columns[0].data_type, "Int64"); // UInt32 -> Int64
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.rows[0][0], CellValue::Int(1));
    assert_eq!(t.rows[1][1], CellValue::String("b".into()));
}

#[test]
fn ch_types_map_and_strip_wrappers() {
    assert_eq!(ch_type_to_arrow("Nullable(Int64)"), "Int64");
    assert_eq!(ch_type_to_arrow("LowCardinality(String)"), "Utf8");
    assert_eq!(ch_type_to_arrow("Float32"), "Float64");
    assert_eq!(
        ch_type_to_arrow("DateTime64(3)"),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(ch_type_to_arrow("Date"), "Date32");
}

#[test]
fn int64_as_string_is_parsed() {
    // ClickHouse serialises 64-bit ints as strings.
    let body =
        r#"{"meta":[{"name":"big","type":"UInt64"}],"data":[["9223372036854775807"]],"rows":1}"#;
    let t = parse_jsoncompact(body).unwrap();
    assert_eq!(t.rows[0][0], CellValue::Int(i64::MAX));
}

#[test]
fn kill_query_sql_escapes_the_id() {
    assert_eq!(
        kill_query_sql("abc-123"),
        "KILL QUERY WHERE query_id = 'abc-123'"
    );
    // A quote in the id must not break out of the literal.
    assert_eq!(kill_query_sql("a'b"), "KILL QUERY WHERE query_id = 'a''b'");
}
