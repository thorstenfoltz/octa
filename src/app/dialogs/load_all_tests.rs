use super::*;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn one_cell(name: &str, v: CellValue) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: name.into(),
        data_type: "Int64".into(),
    }];
    t.rows = vec![vec![v]];
    t
}

/// Snowflake hands the alias back as `N`; some drivers return the count
/// as text.
#[test]
fn the_count_is_read_whatever_the_case_or_type() {
    assert_eq!(read_count(&one_cell("N", CellValue::Int(42))), Some(42));
    assert_eq!(
        read_count(&one_cell("n", CellValue::String("7".into()))),
        Some(7)
    );
    assert_eq!(read_count(&DataTable::empty()), None);
}

#[test]
fn the_count_and_the_pages_follow_the_view() {
    use octa::db::pushdown::view::{ServerView, ViewFilter};
    let conn = crate::app::db_view::tests::conn();
    let origin = crate::app::db_view::tests::db_tab().db_origin.unwrap();
    assert_eq!(
        super::count_view_sql(&conn, &origin, None),
        octa::db::count_sql(conn.engine, None, "public", "t")
    );
    let view = ServerView {
        order: vec![],
        filters: vec![ViewFilter::values("name", ["a".to_string()])],
        derived: Vec::new(),
    };
    let sql = super::count_view_sql(&conn, &origin, Some(&view));
    assert!(
        sql.starts_with("SELECT COUNT(*) AS n FROM (SELECT * FROM"),
        "{sql}"
    );
    assert!(sql.contains("WHERE"), "{sql}");
}
