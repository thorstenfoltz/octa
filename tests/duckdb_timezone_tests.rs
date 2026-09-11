//! `TIMESTAMPTZ` in DuckDB: shown on the session's clock, saved back to the
//! same instant, and labelled with the zone it is being shown in.
//!
//! DuckDB stores a `TIMESTAMPTZ` as a UTC instant and renders it on the
//! session `TimeZone`. Octa reads the raw instant, so before this it showed
//! the UTC wall clock while DuckDB, and every other client, showed the local
//! one - the same value, an offset apart, with nothing on screen saying so.

use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::duckdb_reader::DuckDbReader;

/// Build a one-row table with a naive and a zoned timestamp for the same
/// wall-clock reading, and hand back the file plus the session zone.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.duckdb");
    let conn = duckdb::Connection::open(&path).unwrap();
    let tz: String = conn
        .query_row("SELECT current_setting('TimeZone')", [], |r| r.get(0))
        .unwrap();
    conn.execute_batch(
        "CREATE TABLE t (id INTEGER, naive TIMESTAMP, zoned TIMESTAMPTZ);
         INSERT INTO t VALUES (1, '2024-06-01 18:15:00', '2024-06-01 18:15:00+02:00');",
    )
    .unwrap();
    drop(conn);
    (dir, path, tz)
}

/// The instant 2024-06-01T16:15:00Z on the session's clock, as text.
fn expected_local(tz: &str) -> String {
    let zone: chrono_tz::Tz = tz.parse().expect("session zone parses");
    chrono::DateTime::from_timestamp(1_717_258_500, 0)
        .unwrap()
        .with_timezone(&zone)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[test]
fn a_timestamptz_column_names_its_zone_and_shows_that_clock() {
    let (_dir, path, tz) = fixture();
    let table = DuckDbReader.read_table(&path, "main.t").unwrap();

    let zoned_type = &table.columns[2].data_type;
    assert!(
        zoned_type.contains(&tz),
        "the zoned column must name the zone it is shown in, got {zoned_type}"
    );
    assert_eq!(
        table.columns[1].data_type, "Timestamp(Microsecond, None)",
        "a plain TIMESTAMP stays naive"
    );

    assert_eq!(
        table.get(0, 2).map(|v| v.to_string()),
        Some(expected_local(&tz)),
        "the zoned value reads on the session clock, as DuckDB itself prints it"
    );
    assert_eq!(
        table.get(0, 1).map(|v| v.to_string()),
        Some("2024-06-01 18:15:00".to_string()),
        "a naive value is never shifted"
    );
}

/// The reader converts into the session zone, so the writer has to convert
/// back out. Without that inverse, every save moved the column by the offset.
#[test]
fn saving_a_timestamptz_leaves_the_instant_where_it_was() {
    let (_dir, path, _tz) = fixture();
    let mut table = DuckDbReader.read_table(&path, "main.t").unwrap();
    // Touch an unrelated cell so the row is rewritten.
    table.set(0, 0, CellValue::Int(2));
    table.apply_edits();
    DuckDbReader.write_file(&path, &table).unwrap();

    let conn = duckdb::Connection::open(&path).unwrap();
    let epoch: f64 = conn
        .query_row("SELECT epoch(zoned) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        epoch, 1_717_258_500.0,
        "a save must not move the instant a TIMESTAMPTZ points at"
    );
}

/// Editing the cell writes the instant that reading it would show, so what
/// goes in is what comes back out.
#[test]
fn editing_a_timestamptz_round_trips_through_the_grid() {
    let (_dir, path, tz) = fixture();
    let mut table = DuckDbReader.read_table(&path, "main.t").unwrap();
    // Two hours later on whatever clock the session is using.
    let typed = {
        let zone: chrono_tz::Tz = tz.parse().unwrap();
        chrono::DateTime::from_timestamp(1_717_258_500 + 7200, 0)
            .unwrap()
            .with_timezone(&zone)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    };
    table.set(0, 2, CellValue::DateTime(typed.clone()));
    table.apply_edits();
    DuckDbReader.write_file(&path, &table).unwrap();

    let again = DuckDbReader.read_table(&path, "main.t").unwrap();
    assert_eq!(
        again.get(0, 2).map(|v| v.to_string()),
        Some(typed),
        "the grid must read back exactly what was typed into it"
    );
    let conn = duckdb::Connection::open(&path).unwrap();
    let epoch: f64 = conn
        .query_row("SELECT epoch(zoned) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        epoch,
        1_717_258_500.0 + 7200.0,
        "and the stored instant is the one that clock reading means"
    );
}

/// The SQL panel takes the zone off the result column, which DuckDB fills in
/// from the session setting, so a query result agrees with the table view.
#[test]
fn sql_results_read_a_timestamptz_on_the_session_clock() {
    let (_dir, path, tz) = fixture();
    let table = DuckDbReader.read_table(&path, "main.t").unwrap();
    let out = octa::sql::run_query(
        &table,
        "SELECT TIMESTAMPTZ '2024-06-01 18:15:00+02:00' AS z",
    )
    .unwrap()
    .table;
    assert_eq!(
        out.get(0, 0).map(|v| v.to_string()),
        Some(expected_local(&tz)),
        "a TIMESTAMPTZ in a query result reads on the same clock as the grid"
    );
}
