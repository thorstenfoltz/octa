use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::sql::run_query;

/// SQL results honour the initial-load row cap: an unbounded SELECT must
/// stop collecting at the cap instead of materialising every row (a huge
/// server-side result used to OOM-crash the app). Lives in its own test
/// binary because the guard mutates a process-wide atomic.
#[test]
fn test_sql_result_stops_at_initial_load_row_cap() {
    let mut table = DataTable::empty();
    table.columns = vec![ColumnInfo {
        name: "n".to_string(),
        data_type: "Int64".to_string(),
    }];
    table.rows = (0..10).map(|i| vec![CellValue::Int(i)]).collect();

    let _guard = octa::formats::InitialLoadRowsGuard::new(3);
    let result = run_query(&table, "SELECT * FROM data").unwrap();
    assert_eq!(
        result.table.row_count(),
        3,
        "result capped at the guard value"
    );
    // First rows come through intact, in order.
    assert_eq!(result.table.get(0, 0), Some(&CellValue::Int(0)));
    assert_eq!(result.table.get(2, 0), Some(&CellValue::Int(2)));
}
