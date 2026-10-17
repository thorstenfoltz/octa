use super::looks_like_sql_dump;

#[test]
fn a_dump_is_recognised_and_a_query_file_is_not() {
    let dump = "BEGIN TRANSACTION;\ncreate table \"t\" (a TEXT);\ninsert into t VALUES('x');\n";
    assert!(looks_like_sql_dump(Some(dump)), "case is ignored");
    // A .sql holding queries has no rows to load, so it stays quiet.
    assert!(!looks_like_sql_dump(Some("SELECT * FROM t WHERE a = 1;")));
    // So does a schema-only dump: there is nothing to show but the schema.
    assert!(!looks_like_sql_dump(Some("CREATE TABLE t (a TEXT);")));
    // A file too big to hold in the raw view reports nothing at all.
    assert!(!looks_like_sql_dump(None));
}
