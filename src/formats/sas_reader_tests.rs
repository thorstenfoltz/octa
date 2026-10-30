use super::*;
use sas7bdat::SasTime;

#[test]
fn sas_time_is_a_duration_that_narrows_to_time() {
    let cell = |s| {
        sas_cell_to_octa(&SasCell::Time(SasTime {
            seconds_since_midnight: s,
        }))
    };
    assert_eq!(cell(3_661), CellValue::Duration(3_661_000_000_000));
    assert_eq!(logical_type_name(LogicalType::Time), "Duration(s)");

    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "t".into(),
        data_type: "Duration(s)".into(),
    }];
    t.rows = vec![vec![cell(3_661)], vec![CellValue::Null]];
    narrow_time_columns(&mut t);
    assert_eq!(t.rows[0][0], CellValue::Time(3_661_000_000_000));

    // Past a day it stays an amount of time.
    t.rows = vec![vec![cell(90_000)]];
    narrow_time_columns(&mut t);
    assert_eq!(t.rows[0][0], CellValue::Duration(90_000_000_000_000));
}
