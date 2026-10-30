use super::*;

#[test]
fn only_plain_time_formats_are_clock_times() {
    for f in ["TIME8", "time11.2", "TIME5"] {
        assert!(is_clock_format(f), "{f}");
    }
    for f in ["DTIME8", "MTIME", "DATETIME20", "F8.2", ""] {
        assert!(!is_clock_format(f), "{f}");
    }
}

#[test]
fn spss_duration_array_reads_as_duration_cells() {
    let a = DurationMicrosecondArray::from(vec![Some(90_500_000), None]);
    assert_eq!(
        arrow57_value_to_cell(&a, 0),
        CellValue::Duration(90_500_000_000)
    );
    assert_eq!(arrow57_value_to_cell(&a, 1), CellValue::Null);
    assert_eq!(
        arrow57_type_string(&Arrow57DataType::Duration(Arrow57TimeUnit::Microsecond)),
        "Duration(µs)"
    );
}

#[test]
fn clock_format_column_narrows_to_time_but_dtime_stays_duration() {
    let mk = |ns: i64| DataTable {
        columns: vec![ColumnInfo {
            name: "t".into(),
            data_type: "Duration(µs)".into(),
        }],
        rows: vec![vec![CellValue::Duration(ns)]],
        ..DataTable::empty()
    };
    let mut clock = mk(3_600_000_000_000);
    narrow_clock_columns(&mut clock, |_| true);
    assert_eq!(clock.rows[0][0], CellValue::Time(3_600_000_000_000));
    let mut dtime = mk(3_600_000_000_000);
    narrow_clock_columns(&mut dtime, |_| false);
    assert_eq!(dtime.rows[0][0], CellValue::Duration(3_600_000_000_000));
}
