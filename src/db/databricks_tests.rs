use super::*;

#[test]
fn parse_databricks_result() {
    let v = serde_json::json!({
        "manifest": { "schema": { "columns": [
            {"name":"id","type_name":"INT"},
            {"name":"name","type_name":"STRING"} ] } },
        "result": { "data_array": [ ["1","alice"], ["2","bob"] ] }
    });
    let t = parse_dbx_result(&v).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "name"]);
    assert_eq!(t.columns[0].data_type, "Int64");
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.rows[0][0], CellValue::Int(1));
}

#[test]
fn external_links_are_read_from_either_shape() {
    // The statement response nests them under `result`.
    let stmt = serde_json::json!({
        "result": { "external_links": [
            {"chunk_index": 0, "external_link": "https://s3/chunk0",
             "next_chunk_internal_link": "/api/2.0/sql/statements/x/result/chunks/1"} ] }
    });
    let links = dbx_external_links(&stmt);
    assert_eq!(
        links,
        vec![DbxLink {
            index: 0,
            url: "https://s3/chunk0".to_string(),
            next_path: Some("/api/2.0/sql/statements/x/result/chunks/1".to_string()),
        }]
    );
    // The chunk-fetch response carries them at the top level, and the last
    // chunk names no successor.
    let chunk = serde_json::json!({
        "external_links": [
            {"chunk_index": 1, "external_link": "https://s3/chunk1"} ]
    });
    let links = dbx_external_links(&chunk);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].index, 1);
    assert_eq!(links[0].next_path, None);
}

#[test]
fn a_result_without_links_walks_nowhere() {
    assert!(dbx_external_links(&serde_json::json!({"result": {}})).is_empty());
    assert!(dbx_external_links(&serde_json::json!({})).is_empty());
}

/// The EXTERNAL_LINKS statement response carries the manifest but no
/// `data_array`, so the parse must still yield the columns.
#[test]
fn external_disposition_parses_columns_without_inline_rows() {
    let v = serde_json::json!({
        "manifest": { "schema": { "columns": [
            {"name":"id","type_name":"INT"} ] } },
        "result": { "external_links": [
            {"chunk_index": 0, "external_link": "https://s3/c0"} ] }
    });
    let t = parse_dbx_result(&v).unwrap();
    assert_eq!(t.columns.len(), 1);
    assert_eq!(t.row_count(), 0);
}

/// A downloaded chunk body is the same JSON_ARRAY shape as inline data,
/// and decodes through the same column types.
#[test]
fn a_chunk_body_decodes_like_inline_data() {
    let columns = vec![
        ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        },
        ColumnInfo {
            name: "name".to_string(),
            data_type: "Utf8".to_string(),
        },
    ];
    let mut rows = Vec::new();
    append_dbx_rows(
        &serde_json::json!([["1", "alice"], ["2", null]]),
        &columns,
        &mut rows,
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], CellValue::Int(1));
    assert_eq!(rows[0][1], CellValue::String("alice".to_string()));
    assert_eq!(rows[1][1], CellValue::Null);
    // A second chunk appends rather than replaces.
    append_dbx_rows(&serde_json::json!([["3", "carol"]]), &columns, &mut rows);
    assert_eq!(rows.len(), 3);
}

#[test]
fn dbx_types_map() {
    assert_eq!(dbx_type_to_arrow("BIGINT"), "Int64");
    assert_eq!(dbx_type_to_arrow("decimal"), "Float64");
    assert_eq!(dbx_type_to_arrow("BOOLEAN"), "Boolean");
    assert_eq!(
        dbx_type_to_arrow("TIMESTAMP_NTZ"),
        "Timestamp(Microsecond, None)"
    );
    assert_eq!(dbx_type_to_arrow("STRUCT"), "Utf8");
}

/// A read-only Databricks connection runs a WITH query that only *mentions* a
/// write word (in a string, a comment, a backtick name, a column), and still
/// refuses one that leads into a write. Before 2026-10-22 any of the four
/// words anywhere refused the query.
#[test]
fn a_with_query_runs_unless_it_leads_into_a_write() {
    for q in [
        "WITH x AS (SELECT 1) SELECT * FROM x",
        "WITH h AS (SELECT * FROM hist WHERE operation IN ('MERGE', 'DELETE', 'UPDATE')) \
         SELECT * FROM h",
        "WITH x AS (SELECT 1) -- insert these later\nSELECT * FROM x",
        "WITH x AS (SELECT `update`, `delete` FROM t) SELECT * FROM x",
        "WITH x AS (SELECT 1 AS delete) SELECT delete FROM x",
        "WITH a (n) AS (SELECT 1), b AS (SELECT n FROM a) SELECT * FROM b",
        "WITH x (SELECT 1) SELECT * FROM x",
        "WITH RECURSIVE r AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM r WHERE n < 3) \
         SELECT * FROM r",
        "/* report */ WITH x AS (SELECT ')' AS p) (SELECT * FROM x)",
        "WITH x AS (SELECT 1) FROM x SELECT *",
        "WITH x AS (SELECT 'EUR', '€' AS sign) VALUES (1)",
        "with x as (select 1) select * from x;",
    ] {
        assert_eq!(with_write_risk(q), None, "{q}");
    }
    for q in [
        "WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x",
        "WITH x AS (SELECT 1) MERGE INTO t USING x ON t.id = x.id WHEN MATCHED THEN DELETE",
        "WITH x AS (SELECT 1) FROM x INSERT INTO t SELECT *",
        "WITH x AS (SELECT ')' AS p) INSERT INTO t SELECT * FROM x",
        "WITH x AS (SELECT 1) /* note */ INSERT INTO t SELECT 1",
        "WITH x AS (SELECT 1) CACHE TABLE c AS SELECT * FROM x",
        // Spark reads `\'` as an escaped quote, so the INSERT is code there;
        // a backslash anywhere falls back to refusing the words outright.
        "WITH x AS (SELECT 'a\\' INSERT INTO t SELECT 1 --') SELECT 1",
        // Nested comments, unclosed strings and stray brackets: no guessing.
        "WITH x AS (SELECT 1) /* /* */ INSERT INTO t SELECT 1 */ SELECT 1",
        "WITH x AS (SELECT 'open) INSERT INTO t SELECT 1",
        "WITH x AS SELECT 1 INSERT INTO t SELECT 1",
    ] {
        assert!(with_write_risk(q).is_some(), "{q}");
    }
    // The blunt fallback still lets a text through that names no write.
    assert_eq!(
        with_write_risk("WITH x AS (SELECT 'a\\b') SELECT * FROM x"),
        None
    );
}
