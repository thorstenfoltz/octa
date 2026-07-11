//! Unioning files picked in the directory sidebar. `OctaApp::open_union_for_files`
//! reads each file through the registry and hands the tables to the same
//! `plan_union` / `union_tables` engine the tab-based Union dialog uses; these
//! cover that pipeline end to end without a GUI.
use std::io::Write;

use octa::data::DataTable;

/// Read a file the way `open_union_for_files` does: registry lookup by path,
/// then `read_file`.
fn read(path: &std::path::Path) -> DataTable {
    let registry = octa::formats::FormatRegistry::new();
    registry
        .reader_for_path(path)
        .expect("a reader for this extension")
        .read_file(path)
        .expect("file reads")
}

fn union_all(tables: &[&DataTable]) -> DataTable {
    let schemas: Vec<&[octa::data::ColumnInfo]> =
        tables.iter().map(|t| t.columns.as_slice()).collect();
    let plan = octa::data::union::plan_union(&schemas);
    octa::data::union::union_tables(tables, &plan).expect("union succeeds")
}

#[test]
fn union_two_csv_files_stacks_rows() {
    let mut a = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(a, "id,name").unwrap();
    writeln!(a, "1,alice").unwrap();
    a.flush().unwrap();

    let mut b = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(b, "id,name").unwrap();
    writeln!(b, "2,bob").unwrap();
    b.flush().unwrap();

    let (ta, tb) = (read(a.path()), read(b.path()));
    let out = union_all(&[&ta, &tb]);

    assert_eq!(out.row_count(), 2);
    assert_eq!(out.col_count(), 2);
}

#[test]
fn union_more_than_two_files() {
    // The sidebar selection is an arbitrary set, not a pair: N files must work.
    let mut files = Vec::new();
    for i in 1..=5 {
        let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
        writeln!(f, "id,name").unwrap();
        writeln!(f, "{i},row{i}").unwrap();
        f.flush().unwrap();
        files.push(f);
    }
    let tables: Vec<DataTable> = files.iter().map(|f| read(f.path())).collect();
    let refs: Vec<&DataTable> = tables.iter().collect();
    let out = union_all(&refs);

    assert_eq!(out.row_count(), 5);
}

#[test]
fn union_reconciles_files_with_different_columns() {
    // Mismatched schemas are the reason the dialog shows a reconciliation
    // plan: the union is the superset of columns, missing cells null-filled.
    let mut a = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(a, "id,name").unwrap();
    writeln!(a, "1,alice").unwrap();
    a.flush().unwrap();

    let mut b = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(b, "id,city").unwrap();
    writeln!(b, "2,berlin").unwrap();
    b.flush().unwrap();

    let (ta, tb) = (read(a.path()), read(b.path()));
    let out = union_all(&[&ta, &tb]);

    assert_eq!(out.row_count(), 2);
    let names: Vec<&str> = out.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"id"));
    assert!(names.contains(&"name"));
    assert!(names.contains(&"city"));
}

#[test]
fn union_works_across_formats() {
    // The selection is not restricted to one format: a CSV and a JSON with the
    // same columns union just as well, which is the point of going through the
    // registry rather than a parquet-only path.
    let mut a = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(a, "id,name").unwrap();
    writeln!(a, "1,alice").unwrap();
    a.flush().unwrap();

    let mut b = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    write!(b, r#"[{{"id": 2, "name": "bob"}}]"#).unwrap();
    b.flush().unwrap();

    let (ta, tb) = (read(a.path()), read(b.path()));
    let out = union_all(&[&ta, &tb]);

    assert_eq!(out.row_count(), 2);
}
