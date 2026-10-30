use super::*;

#[test]
fn time_columns_read_as_durations() {
    let h = 3_600_000_000_000i64;
    let read = |s: &str| mysql_value_to_cell(&mysql_async::Value::Bytes(s.into()), "Duration(µs)");
    assert_eq!(
        read("-838:59:59"),
        CellValue::Duration(-(838 * h + 59 * 60_000_000_000 + 59_000_000_000))
    );
    assert_eq!(
        read("30:15:00.500000"),
        CellValue::Duration(30 * h + 15 * 60_000_000_000 + 500_000_000)
    );
    assert_eq!(read("garbage"), CellValue::String("garbage".into()));
    // The binary protocol's own TIME value.
    assert_eq!(
        mysql_value_to_cell(
            &mysql_async::Value::Time(true, 34, 22, 59, 59, 0),
            "Duration(µs)"
        ),
        CellValue::Duration(-(838 * h + 59 * 60_000_000_000 + 59_000_000_000))
    );
}

#[test]
fn mysql_refuses_a_duration_it_would_clip() {
    let h = 3_600_000_000_000i64;
    assert!(mysql_duration_in_range(
        838 * h + 59 * 60_000_000_000 + 59_000_000_000
    ));
    assert!(mysql_duration_in_range(
        -(838 * h + 59 * 60_000_000_000 + 59_000_000_000)
    ));
    assert!(!mysql_duration_in_range(839 * h));
    assert!(!mysql_duration_in_range(-839 * h));
}

#[test]
fn mysql_refusal_names_connection_column_and_value() {
    let h = 3_600_000_000_000i64;
    let cols = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "elapsed".into(),
            data_type: "Duration(µs)".into(),
        },
    ];
    let ok = vec![CellValue::Int(1), CellValue::Duration(h)];
    let bad = vec![CellValue::Int(2), CellValue::Duration(900 * h)];
    assert!(refuse_unwritable_durations("prod", &cols, &[&ok]).is_ok());
    let msg = format!(
        "{:#}",
        refuse_unwritable_durations("prod", &cols, &[&ok, &bad]).unwrap_err()
    );
    assert!(
        msg.starts_with("Octa did not send this to 'prod', because"),
        "{msg}"
    );
    assert!(
        msg.contains("'elapsed'") && msg.contains("900:00:00"),
        "{msg}"
    );
}
