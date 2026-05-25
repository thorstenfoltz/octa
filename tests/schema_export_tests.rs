//! Tests for `octa::data::schema_export`. Each target's output is
//! checked for: (a) presence of expected type mappings, (b) header /
//! class / interface / struct naming, (c) safe-identifier escaping
//! when column names contain spaces, hyphens, or leading digits, (d)
//! the unknown-type fallback path.

use octa::data::ColumnInfo;
use octa::data::schema_export::SchemaTarget;

fn cols(pairs: &[(&str, &str)]) -> Vec<ColumnInfo> {
    pairs
        .iter()
        .map(|(name, ty)| ColumnInfo {
            name: name.to_string(),
            data_type: ty.to_string(),
        })
        .collect()
}

#[test]
fn target_all_contains_every_variant() {
    // Sanity: ALL must list every public variant once. If a variant is
    // added without updating ALL the export menu would silently miss
    // it. Length check catches the common drift.
    assert_eq!(SchemaTarget::ALL.len(), 9);
}

#[test]
fn postgres_basic_types() {
    let c = cols(&[
        ("id", "Int64"),
        ("name", "Utf8"),
        ("score", "Float64"),
        ("when", "Timestamp(Microsecond, None)"),
    ]);
    let out = SchemaTarget::PostgresSqlDdl.export(&c, "people");
    // Valid identifiers are emitted bare — no quoting noise.
    assert!(out.contains("CREATE TABLE people"));
    assert!(out.contains("id BIGINT"));
    assert!(out.contains("name TEXT"));
    assert!(out.contains("score DOUBLE PRECISION"));
    assert!(out.contains("when TIMESTAMP"));
}

#[test]
fn postgres_timestamp_with_tz_uses_tztype() {
    let c = cols(&[("ts", "Timestamp(Microsecond, Some(\"UTC\"))")]);
    let out = SchemaTarget::PostgresSqlDdl.export(&c, "t");
    assert!(out.contains("TIMESTAMPTZ"), "got: {}", out);
}

#[test]
fn mysql_unsigned_types() {
    let c = cols(&[
        ("uid", "UInt32"),
        ("name", "Utf8"),
        ("ts", "Timestamp(Microsecond, None)"),
    ]);
    let out = SchemaTarget::MysqlSqlDdl.export(&c, "users");
    assert!(out.contains("CREATE TABLE users"));
    assert!(out.contains("uid INT UNSIGNED"));
    assert!(out.contains("name TEXT"));
    assert!(out.contains("ts DATETIME"));
}

#[test]
fn sqlite_collapses_all_ints_to_integer() {
    let c = cols(&[
        ("a", "Int8"),
        ("b", "UInt64"),
        ("c", "Boolean"),
        ("d", "Float64"),
        ("e", "Date32"),
    ]);
    let out = SchemaTarget::SqliteSqlDdl.export(&c, "t");
    assert!(out.contains("a INTEGER"));
    assert!(out.contains("b INTEGER"));
    assert!(out.contains("c INTEGER"));
    assert!(out.contains("d REAL"));
    assert!(out.contains("e TEXT"));
}

#[test]
fn sql_unknown_arrow_type_falls_back_to_text_with_comment() {
    let c = cols(&[("weird", "Decimal256(38, 10)")]);
    let out = SchemaTarget::PostgresSqlDdl.export(&c, "t");
    assert!(out.contains("weird TEXT"));
    assert!(out.contains("Decimal256"), "got: {}", out);
}

#[test]
fn sql_quotes_only_identifiers_that_need_it() {
    // A valid name is emitted bare; a name with a space is wrapped in
    // the dialect's quote characters.
    let c = cols(&[("ok_name", "Int64"), ("bad name", "Utf8")]);

    let pg = SchemaTarget::PostgresSqlDdl.export(&c, "t");
    assert!(pg.contains("    ok_name BIGINT"), "got: {pg}");
    assert!(pg.contains("\"bad name\" TEXT"), "got: {pg}");

    let sf = SchemaTarget::SnowflakeSqlDdl.export(&c, "t");
    assert!(sf.contains("    ok_name BIGINT"), "got: {sf}");
    assert!(sf.contains("\"bad name\" VARCHAR"), "got: {sf}");

    let my = SchemaTarget::MysqlSqlDdl.export(&c, "t");
    assert!(my.contains("    ok_name BIGINT"), "got: {my}");
    assert!(my.contains("`bad name` TEXT"), "got: {my}");

    let db = SchemaTarget::DatabricksSqlDdl.export(&c, "t");
    assert!(db.contains("    ok_name BIGINT"), "got: {db}");
    assert!(db.contains("`bad name` STRING"), "got: {db}");
}

#[test]
fn sql_quoting_escapes_the_embedded_quote_char() {
    // A name carrying the quote char itself: doubled inside the quotes.
    let pg = SchemaTarget::PostgresSqlDdl.export(&cols(&[("evil\"name", "Utf8")]), "t");
    assert!(pg.contains("\"evil\"\"name\""), "got: {pg}");
    let my = SchemaTarget::MysqlSqlDdl.export(&cols(&[("evil`name", "Utf8")]), "t");
    assert!(my.contains("`evil``name`"), "got: {my}");
}

#[test]
fn databricks_spark_types() {
    let c = cols(&[
        ("id", "Int64"),
        ("name", "Utf8"),
        ("score", "Float64"),
        ("ts", "Timestamp(Microsecond, None)"),
    ]);
    let out = SchemaTarget::DatabricksSqlDdl.export(&c, "events");
    assert!(out.contains("CREATE TABLE events"));
    assert!(out.contains("id BIGINT"));
    assert!(out.contains("name STRING"));
    assert!(out.contains("score DOUBLE"));
    assert!(out.contains("ts TIMESTAMP_NTZ"), "got: {}", out);
}

#[test]
fn snowflake_types() {
    let c = cols(&[
        ("id", "Int32"),
        ("name", "Utf8"),
        ("flag", "Boolean"),
        ("ts", "Timestamp(Microsecond, Some(\"UTC\"))"),
    ]);
    let out = SchemaTarget::SnowflakeSqlDdl.export(&c, "records");
    assert!(out.contains("CREATE TABLE records"));
    assert!(out.contains("id INTEGER"));
    assert!(out.contains("name VARCHAR"));
    assert!(out.contains("flag BOOLEAN"));
    assert!(out.contains("ts TIMESTAMP_TZ"), "got: {}", out);
}

#[test]
fn databricks_and_snowflake_widen_unsigned_and_flag_unknowns() {
    let c = cols(&[("big", "UInt64"), ("weird", "Decimal256(38, 10)")]);

    let db = SchemaTarget::DatabricksSqlDdl.export(&c, "t");
    assert!(db.contains("big DECIMAL(20, 0)"), "got: {}", db);
    assert!(db.contains("weird STRING"));
    assert!(db.contains("Decimal256"), "got: {}", db);

    let sf = SchemaTarget::SnowflakeSqlDdl.export(&c, "t");
    assert!(sf.contains("big NUMBER(20, 0)"), "got: {}", sf);
    assert!(sf.contains("weird VARCHAR"));
    assert!(sf.contains("Decimal256"), "got: {}", sf);
}

#[test]
fn pydantic_emits_basemodel_with_imports() {
    let c = cols(&[
        ("id", "Int64"),
        ("when", "Timestamp(Microsecond, None)"),
        ("born", "Date32"),
    ]);
    let out = SchemaTarget::PydanticV2.export(&c, "users");
    assert!(out.contains("from pydantic import BaseModel"));
    assert!(out.contains("from datetime import date, datetime"));
    assert!(out.contains("class Users(BaseModel):"));
    assert!(out.contains("    id: int"));
    assert!(out.contains("    when: datetime"));
    assert!(out.contains("    born: date"));
}

#[test]
fn pydantic_aliases_invalid_identifiers() {
    let c = cols(&[("my column", "Utf8"), ("1st", "Int64")]);
    let out = SchemaTarget::PydanticV2.export(&c, "t");
    assert!(out.contains("from pydantic import BaseModel, Field"));
    assert!(out.contains("Field(..., alias=\"my column\")"));
    assert!(out.contains("Field(..., alias=\"1st\")"));
}

#[test]
fn pydantic_pascalizes_table_name() {
    let c = cols(&[("a", "Int64")]);
    let out = SchemaTarget::PydanticV2.export(&c, "user_orders");
    assert!(out.contains("class UserOrders(BaseModel):"));
}

#[test]
fn typescript_emits_interface_with_quoted_keys_when_needed() {
    let c = cols(&[
        ("id", "Int64"),
        ("first name", "Utf8"),
        ("active", "Boolean"),
    ]);
    let out = SchemaTarget::TypeScript.export(&c, "user");
    assert!(out.contains("export interface User {"));
    assert!(out.contains("  id: number;"));
    assert!(out.contains("  \"first name\": string;"));
    assert!(out.contains("  active: boolean;"));
}

#[test]
fn json_schema_is_valid_json_with_properties_and_required() {
    let c = cols(&[("id", "Int64"), ("name", "Utf8"), ("born", "Date32")]);
    let out = SchemaTarget::JsonSchema.export(&c, "people");
    let v: serde_json::Value = serde_json::from_str(&out).expect("output is JSON");
    assert_eq!(v["type"], "object");
    assert_eq!(v["title"], "people");
    assert_eq!(v["properties"]["id"]["type"], "integer");
    assert_eq!(v["properties"]["name"]["type"], "string");
    assert_eq!(v["properties"]["born"]["type"], "string");
    assert_eq!(v["properties"]["born"]["format"], "date");
    let req = v["required"].as_array().unwrap();
    assert_eq!(req.len(), 3);
}

#[test]
fn json_schema_timestamp_uses_date_time_format() {
    let c = cols(&[("ts", "Timestamp(Nanosecond, Some(\"UTC\"))")]);
    let out = SchemaTarget::JsonSchema.export(&c, "t");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["properties"]["ts"]["format"], "date-time");
}

#[test]
fn rust_struct_emits_serde_derives_and_renames() {
    let c = cols(&[
        ("id", "Int64"),
        ("first name", "Utf8"),
        ("active", "Boolean"),
        ("born", "Date32"),
        ("ts", "Timestamp(Microsecond, None)"),
    ]);
    let out = SchemaTarget::RustStruct.export(&c, "user_event");
    assert!(out.contains("use serde::{Deserialize, Serialize};"));
    assert!(out.contains("use chrono::{NaiveDate, NaiveDateTime};"));
    assert!(out.contains("pub struct UserEvent {"));
    assert!(out.contains("pub id: i64"));
    assert!(out.contains("#[serde(rename = \"first name\")]"));
    assert!(out.contains("pub first_name: String"));
    assert!(out.contains("pub active: bool"));
    assert!(out.contains("pub born: NaiveDate"));
    assert!(out.contains("pub ts: NaiveDateTime"));
}

#[test]
fn rust_unsigned_types_widen_correctly() {
    let c = cols(&[
        ("a", "UInt8"),
        ("b", "UInt32"),
        ("c", "UInt64"),
        ("d", "Float16"),
    ]);
    let out = SchemaTarget::RustStruct.export(&c, "t");
    assert!(out.contains("pub a: u8"));
    assert!(out.contains("pub b: u32"));
    assert!(out.contains("pub c: u64"));
    assert!(out.contains("pub d: f32"));
}

#[test]
fn empty_columns_produces_valid_skeleton_for_every_target() {
    let c: Vec<ColumnInfo> = Vec::new();
    for target in SchemaTarget::ALL {
        let out = target.export(&c, "empty");
        assert!(
            !out.is_empty(),
            "empty-columns output for {:?} was empty",
            target
        );
    }
    // SQL DDL has special-case empty trailing parens — verify it still
    // parses as 'CREATE TABLE <name> (' + ');' at minimum.
    let sql = SchemaTarget::PostgresSqlDdl.export(&c, "empty");
    assert!(sql.contains("CREATE TABLE empty ("));
    assert!(sql.contains(");\n"));
    // JSON Schema must still be valid JSON.
    let js = SchemaTarget::JsonSchema.export(&c, "empty");
    let _: serde_json::Value = serde_json::from_str(&js).expect("empty JSON schema is JSON");
}

#[test]
fn leading_digit_in_table_name_is_sanitized_for_class_targets() {
    // The leading `9` becomes `_` via sanitize_ident, then PascalCase
    // strips empty chunks → "Lives". A purely-numeric stem (no rescue
    // letters) needs the Row_ prefix fallback instead — see the next
    // test.
    let c = cols(&[("a", "Int64")]);
    let py = SchemaTarget::PydanticV2.export(&c, "9lives");
    assert!(py.contains("class Lives(BaseModel):"), "got: {}", py);
    let ts = SchemaTarget::TypeScript.export(&c, "9lives");
    assert!(ts.contains("interface Lives"), "got: {}", ts);
    let rs = SchemaTarget::RustStruct.export(&c, "9lives");
    assert!(rs.contains("pub struct Lives"), "got: {}", rs);
}

#[test]
fn pure_numeric_table_name_falls_back_to_row_prefix() {
    let c = cols(&[("a", "Int64")]);
    let py = SchemaTarget::PydanticV2.export(&c, "123");
    assert!(py.contains("class Row_23(BaseModel):"), "got: {}", py);
}
