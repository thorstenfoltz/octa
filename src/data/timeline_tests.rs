//! Unit tests for [`timeline`](super). Included via `#[path]`.

use super::*;

fn bookings() -> DataTable {
    let s = |v: &str| CellValue::String(v.into());
    let rows = [
        ("Room A", "Meeting", "2026-10-05 09:00", "2026-10-05 10:30"),
        ("Room A", "Call", "2026-10-05 10:00", "2026-10-05 11:00"), // overlaps 1
        ("Room A", "Lunch", "2026-10-05 11:00", "2026-10-05 12:00"), // touches 2
        ("Room B", "Workshop", "2026-10-05 09:00", "2026-10-05 17:00"),
        (
            "Room B",
            "Interview",
            "2026-10-05 13:00",
            "2026-10-05 14:00",
        ), // inside 4
        (
            "Room B",
            "Interview",
            "2026-10-05 13:30",
            "2026-10-05 14:30",
        ), // 4 and 5
        ("Room C", "Board", "2026-10-06 15:00", "2026-10-06 14:00"), // end < start
        ("Room C", "Drill", "2026-10-08 11:00", ""),                 // a point
        ("Room B", "Alarm", "2026-10-05 16:00", ""),                 // point in 4
        ("Room C", "Nothing", "", "2026-10-06 14:00"),               // no start
    ]
    .iter()
    .map(|(l, t, a, b)| vec![s(l), s(t), s(a), s(b)])
    .collect();
    let mut t = DataTable::empty();
    t.columns = ["room", "title", "start", "end"]
        .iter()
        .map(|n| ColumnInfo {
            name: n.to_string(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows;
    t
}

fn all_rows(t: &DataTable) -> Vec<usize> {
    (0..t.row_count()).collect()
}

/// Overlapping pairs as (row, row), sorted, for readable asserts.
fn pairs(tl: &Timeline) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = overlaps(tl)
        .into_iter()
        .map(|(a, b)| {
            let (x, y) = (tl.bars[a].row, tl.bars[b].row);
            (x.min(y), x.max(y))
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn text_date_columns_are_found_and_picked_as_start_and_end() {
    assert_eq!(detect(&bookings()), Some((2, Some(3))));
}

#[test]
fn overlaps_are_per_lane_and_touching_is_not_overlapping() {
    let t = bookings();
    let tl = build(&t, &all_rows(&t), 2, Some(3), Some(1), Some(0));
    assert_eq!(tl.lanes, ["Room A", "Room B", "Room C"]);
    assert_eq!(pairs(&tl), vec![(0, 1), (3, 4), (3, 5), (3, 8), (4, 5)]);
    assert_eq!(lanes_with_overlaps(&overlaps(&tl), &tl), 2);
}

#[test]
fn a_backwards_row_is_counted_and_a_row_without_a_start_skipped() {
    let t = bookings();
    let tl = build(&t, &all_rows(&t), 2, Some(3), None, Some(0));
    assert_eq!(tl.bad_rows, vec![6]);
    assert_eq!(tl.skipped, 1);
    assert_eq!(tl.bars.len(), 8);
}

#[test]
fn overlapping_bars_get_their_own_track() {
    let t = bookings();
    let tl = build(&t, &all_rows(&t), 2, Some(3), None, Some(0));
    let track = |row: usize| tl.bars.iter().find(|b| b.row == row).unwrap().track;
    assert_eq!((track(0), track(1), track(2)), (0, 1, 0));
    assert_eq!(tl.tracks[1], 3, "Room B needs three tracks at 13:30");
}

#[test]
fn without_a_lane_everything_is_one_lane_and_filtered_rows_are_respected() {
    let t = bookings();
    let tl = build(&t, &[0, 1, 2], 2, Some(3), None, None);
    assert_eq!(tl.lanes.len(), 1);
    assert_eq!(pairs(&tl), vec![(0, 1)]);
}

#[test]
fn the_overlaps_table_names_both_sides() {
    let t = bookings();
    let tl = build(&t, &all_rows(&t), 2, Some(3), Some(1), Some(0));
    let out = overlaps_table(&tl, &overlaps(&tl));
    assert_eq!(out.row_count(), 5);
    let names: Vec<&str> = out.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "room", "row_a", "title_a", "start_a", "end_a", "row_b", "title_b", "start_b", "end_b"
        ]
    );
    let first: Vec<String> = out.rows[0].iter().map(|v| v.to_string()).collect();
    assert_eq!(
        first,
        [
            "Room A",
            "1",
            "Meeting",
            "2026-10-05 09:00",
            "2026-10-05 10:30",
            "2",
            "Call",
            "2026-10-05 10:00",
            "2026-10-05 11:00"
        ]
    );
}

#[test]
fn overlaps_columns_fall_back_to_generic_names() {
    let t = bookings();
    // No label or lane column: those two take generic names.
    let tl = build(&t, &all_rows(&t), 2, Some(3), None, None);
    assert_eq!(overlaps_columns(&tl)[0], "lane");
    assert_eq!(overlaps_columns(&tl)[2], "label_a");
    // Start and end are the same column: the names would collide.
    let tl = build(&t, &all_rows(&t), 2, Some(2), None, Some(0));
    assert_eq!(overlaps_columns(&tl)[3], "start_a");
    assert_eq!(overlaps_columns(&tl)[0], "lane");
}

#[test]
fn time_ticks_follow_the_calendar() {
    let secs = |s: &str| to_secs(&CellValue::String(s.into())).unwrap();
    let (lo, hi) = (secs("2025-01-10"), secs("2025-12-20"));
    // A year at roughly a month per tick: first of each month.
    let (step, ticks) = time_ticks(lo, hi, 20.0 * 86_400.0);
    let labels: Vec<String> = ticks.iter().map(|&t| format_tick(t, step)).collect();
    assert_eq!(labels.first().unwrap(), "2025-02");
    assert_eq!(labels.last().unwrap(), "2025-12");
    assert_eq!(labels.len(), 11);
    // Quarters stay on quarter starts.
    let (step, ticks) = time_ticks(lo, hi, 60.0 * 86_400.0);
    let labels: Vec<String> = ticks.iter().map(|&t| format_tick(t, step)).collect();
    assert_eq!(labels, ["2025-04", "2025-07", "2025-10"]);
    // Weeks land on Mondays.
    let (step, ticks) = time_ticks(lo, lo + 30.0 * 86_400.0, 5.0 * 86_400.0);
    assert_eq!(step, 7.0 * 86_400.0);
    assert_eq!(format_tick(ticks[0], step), "2025-01-13");
    // Decades: steps keep growing, never an endless loop.
    let (step, ticks) = time_ticks(
        secs("1900-01-01"),
        secs("2100-01-01"),
        15.0 * 365.0 * 86_400.0,
    );
    assert!(step >= 15.0 * 365.0 * 86_400.0);
    assert!(!ticks.is_empty() && ticks.len() < 20);
    assert!(time_ticks(1.0, 1.0, 1.0).1.is_empty());
}
