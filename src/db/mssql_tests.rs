use super::*;
use tiberius::{ColumnData, time::Time};

#[test]
fn time_reads_as_a_clock_time_at_any_scale() {
    // 14:30:00.25 at scale 7 (100 ns increments) and 00:00:03 at scale 0.
    assert_eq!(
        tds_cell(
            &ColumnData::Time(Some(Time::new(522_002_500_000, 7))),
            false
        ),
        CellValue::Time(52_200_250_000_000)
    );
    assert_eq!(
        tds_cell(&ColumnData::Time(Some(Time::new(3, 0))), false),
        CellValue::Time(3_000_000_000)
    );
    assert_eq!(tds_cell(&ColumnData::Time(None), false), CellValue::Null);
    assert_eq!(
        tds_type_to_arrow(tiberius::ColumnType::Timen, false),
        "Time64(ns)"
    );
}
