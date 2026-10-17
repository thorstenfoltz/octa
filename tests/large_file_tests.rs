//! Contracts the GUI's large-file mode depends on.
//!
//! The GUI cannot be driven headlessly, so this pins the two properties the
//! tab relies on: that a page can be fetched from an arbitrary offset, and
//! that getting one never materialises the whole file.

use octa::formats::large;

#[test]
fn a_large_tab_pages_from_the_far_end() {
    use std::io::Write;
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(f, "id").unwrap();
    for i in 0..200_000 {
        writeln!(f, "{i}").unwrap();
    }
    f.flush().unwrap();

    let t = large::open(f.path()).unwrap();
    assert_eq!(t.row_count(), 200_000);
    // Ordered, because a CSV scan is parallel: an OFFSET without an ORDER BY
    // is a slice of an unspecified order.
    let page = t.page(199_990, 10, Some((0, true)), None).unwrap();
    assert_eq!(page.row_count(), 10);
    assert_eq!(page.get(9, 0).unwrap().to_string(), "199999");
    // The page is a window: it knows the size of the whole and where it sits.
    assert_eq!(page.total_rows, Some(200_000));
    assert_eq!(page.row_offset, 199_990);
}

#[test]
fn a_filter_is_counted_without_reading_the_file_into_memory() {
    use std::io::Write;
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(f, "id,tag").unwrap();
    for i in 0..50_000 {
        writeln!(f, "{i},{}", if i % 1000 == 0 { "keep" } else { "drop" }).unwrap();
    }
    f.flush().unwrap();

    let t = large::open(f.path()).unwrap();
    let filter = "\"tag\" = 'keep'";
    assert_eq!(t.filtered_count(Some(filter)).unwrap(), 50);
    let page = t.page(0, 10, Some((0, true)), Some(filter)).unwrap();
    assert_eq!(page.row_count(), 10);
    assert_eq!(page.get(0, 0).unwrap().to_string(), "0");
}

/// A column filter in large-file mode keeps, over the whole file, exactly the
/// rows an ordinary tab keeps: those whose cell *displays* as one of the
/// ticked values. Checked for every value of every column type, by comparing
/// the query's count with the same test run on all rows in memory.
/// A Parquet file with one column of every type the filters handle: whole
/// and fractional numbers, decimals, booleans, dates, timestamps with
/// milliseconds, zoned timestamps across a daylight-saving change, times,
/// text with gaps and quotes, and empty strings.
fn typed_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("typed.parquet");
    duckdb::Connection::open_in_memory()
        .unwrap()
        .execute_batch(&format!(
            "COPY (SELECT range AS i, range * 0.5 AS d, CAST(range AS FLOAT) / 4 AS f, \
             CAST(range / 3 AS DECIMAL(10,2)) AS dec, range % 2 = 0 AS b, \
             DATE '2024-01-01' + CAST(range % 5 AS INTEGER) AS dt, \
             TIMESTAMP '2024-01-01 10:00:00' + INTERVAL (range % 7) HOUR \
               + INTERVAL (range % 3 * 123) MILLISECOND AS ts, \
             CAST(TIMESTAMP '2024-03-31 00:30:00' + INTERVAL (range % 4) HOUR AS TIMESTAMPTZ) AS tz, \
             TIME '10:00:00' + INTERVAL (range % 4) MINUTE AS tm, \
             CASE WHEN range % 10 = 0 THEN NULL ELSE 'name ' || (range % 6) END AS s, \
             CASE WHEN range % 9 = 0 THEN '' ELSE 'x''y' END AS e \
             FROM range(300)) TO '{}' (FORMAT parquet)",
            path.display()
        ))
        .unwrap();
    path
}

#[test]
fn a_value_filter_keeps_the_rows_an_ordinary_tab_keeps() {
    use std::collections::BTreeSet;
    let dir = tempfile::tempdir().unwrap();
    let t = large::open(&typed_file(dir.path())).unwrap();
    let all = t.page(0, 1000, None, None).unwrap();
    assert_eq!(all.row_count(), 300);
    for col in 0..all.col_count() {
        let shown: Vec<String> = (0..300)
            .map(|r| all.get(r, col).unwrap().to_string())
            .collect();
        let distinct: BTreeSet<&str> = shown.iter().map(String::as_str).collect();
        for v in &distinct {
            let filter = t
                .values_filter(col, &[v])
                .unwrap_or_else(|| panic!("column {col} cannot be filtered"));
            let expected = shown.iter().filter(|s| s.as_str() == *v).count();
            assert_eq!(
                t.filtered_count(Some(&filter)).unwrap(),
                expected,
                "column {} value {v:?}: {filter}",
                all.columns[col].name
            );
        }
        // A value no cell displays keeps nothing, as in memory.
        let none = t.values_filter(col, &["no such value"]).unwrap();
        assert_eq!(t.filtered_count(Some(&none)).unwrap(), 0, "column {col}");

        // The value list counts the same cells under the same labels.
        let job = t.value_frequency_job(col, "", 1000).unwrap();
        let vf = job().unwrap();
        for row in &vf.rows {
            let expected = shown.iter().filter(|s| **s == row.label).count();
            assert_eq!(row.count, expected, "column {col} label {:?}", row.label);
        }
        assert_eq!(vf.nulls + vf.total_non_null, 300);
        // Missing and empty both count as missing, as the in-memory list has it.
        assert_eq!(
            vf.nulls,
            shown.iter().filter(|s| s.is_empty()).count(),
            "column {col}"
        );
        assert!(vf.rows.iter().all(|r| !r.label.is_empty()), "column {col}");
    }
}

/// An Ask filter chip (`amount > 1000`, `name contains x`) in large-file mode
/// keeps, over the whole file, exactly the rows the in-memory test keeps:
/// every operator, both case modes, on every column type, against values
/// taken from the column itself as well as numbers, text and the empty value.
#[test]
fn a_comparison_filter_keeps_the_rows_an_ordinary_tab_keeps() {
    use octa::data::conditional_format::CondOp;
    use octa::data::predicate_filter::{PredicateFilter, matches};
    use std::collections::BTreeSet;
    let dir = tempfile::tempdir().unwrap();
    let t = large::open(&typed_file(dir.path())).unwrap();
    let all = t.page(0, 1000, None, None).unwrap();
    let ops = [
        CondOp::Eq,
        CondOp::Ne,
        CondOp::Contains,
        CondOp::NotContains,
        CondOp::Gt,
        CondOp::Lt,
        CondOp::Ge,
        CondOp::Le,
        CondOp::Empty,
        CondOp::NotEmpty,
    ];
    for col in 0..all.col_count() {
        let shown: Vec<String> = (0..300)
            .map(|r| all.get(r, col).unwrap().to_string())
            .collect();
        let distinct: BTreeSet<&str> = shown.iter().map(String::as_str).collect();
        let mut values: Vec<String> = distinct.iter().take(4).map(|s| s.to_string()).collect();
        values.extend(
            [
                "",
                "100",
                "2.5",
                "e",
                "NAME 1",
                "2024-01-03",
                "10:01",
                "x'y",
                "TRUE",
                ".123",
            ]
            .map(String::from),
        );
        for op in ops {
            for value in &values {
                for case_sensitive in [false, true] {
                    let f = PredicateFilter {
                        col,
                        op,
                        value: value.clone(),
                        case_sensitive,
                    };
                    let expected = shown.iter().filter(|s| matches(&f, s)).count();
                    let sql = t.predicate_filter(col, op, value, case_sensitive).unwrap();
                    assert_eq!(
                        t.filtered_count(Some(&sql)).unwrap(),
                        expected,
                        "column {} {op:?} {value:?} case {case_sensitive}: {sql}",
                        all.columns[col].name
                    );
                }
            }
        }
    }
}
