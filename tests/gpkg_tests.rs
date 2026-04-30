use octa::data::CellValue;
use octa::formats::FormatReader;
use octa::formats::gpkg_reader::GeoPackageReader;
use rusqlite::Connection;
use tempfile::NamedTempFile;

/// Build a minimal but spec-shaped GeoPackage with two user data tables
/// (`cities` features + `notes` attributes) plus the standard `gpkg_*`
/// metadata tables. The geometry encoding is not real WKB — we just store a
/// blob — because the reader doesn't decode geometries; it only needs to
/// list user data tables and round-trip rows.
fn seed_gpkg(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE gpkg_spatial_ref_sys (
             srs_name TEXT NOT NULL,
             srs_id INTEGER PRIMARY KEY,
             organization TEXT NOT NULL,
             organization_coordsys_id INTEGER NOT NULL,
             definition TEXT NOT NULL,
             description TEXT
         );
         INSERT INTO gpkg_spatial_ref_sys VALUES
             ('WGS 84', 4326, 'EPSG', 4326, 'GEOGCS[\"WGS 84\"]', NULL);

         CREATE TABLE gpkg_contents (
             table_name TEXT PRIMARY KEY,
             data_type TEXT NOT NULL,
             identifier TEXT,
             description TEXT,
             last_change DATETIME,
             min_x DOUBLE, min_y DOUBLE, max_x DOUBLE, max_y DOUBLE,
             srs_id INTEGER
         );
         INSERT INTO gpkg_contents (table_name, data_type, srs_id) VALUES
             ('cities', 'features', 4326),
             ('notes',  'attributes', NULL),
             ('rasters', 'tiles', NULL);

         CREATE TABLE gpkg_geometry_columns (
             table_name TEXT NOT NULL,
             column_name TEXT NOT NULL,
             geometry_type_name TEXT NOT NULL,
             srs_id INTEGER NOT NULL,
             z TINYINT NOT NULL,
             m TINYINT NOT NULL,
             PRIMARY KEY (table_name, column_name)
         );
         INSERT INTO gpkg_geometry_columns VALUES
             ('cities', 'geom', 'POINT', 4326, 0, 0);

         CREATE TABLE cities (
             fid INTEGER PRIMARY KEY AUTOINCREMENT,
             name TEXT,
             pop INTEGER,
             geom BLOB
         );
         INSERT INTO cities (name, pop, geom) VALUES
             ('Berlin', 3700000, x'0001020304'),
             ('Tokyo',  13900000, x'05060708');

         CREATE TABLE notes (
             id INTEGER PRIMARY KEY,
             text TEXT
         );
         INSERT INTO notes (text) VALUES ('first'), ('second');

         -- Tile table — should NOT show up in the picker.
         CREATE TABLE rasters (
             id INTEGER PRIMARY KEY,
             tile_data BLOB
         );",
    )
    .unwrap();
}

#[test]
fn lists_only_features_and_attributes_tables() {
    let f = NamedTempFile::with_suffix(".gpkg").unwrap();
    seed_gpkg(f.path());
    let tables = GeoPackageReader.list_tables(f.path()).unwrap().unwrap();
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["cities", "notes"]);
}

#[test]
fn reads_attributes_table_round_trips_rows() {
    let f = NamedTempFile::with_suffix(".gpkg").unwrap();
    seed_gpkg(f.path());
    let table = GeoPackageReader.read_table(f.path(), "notes").unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.get(0, 1), Some(&CellValue::String("first".into())));
    assert_eq!(table.get(1, 1), Some(&CellValue::String("second".into())));
    assert_eq!(table.format_name.as_deref(), Some("GeoPackage"));
}

#[test]
fn reads_features_table_keeps_geometry_as_binary() {
    let f = NamedTempFile::with_suffix(".gpkg").unwrap();
    seed_gpkg(f.path());
    let table = GeoPackageReader.read_table(f.path(), "cities").unwrap();
    assert_eq!(table.row_count(), 2);
    let geom_col = table
        .columns
        .iter()
        .position(|c| c.name == "geom")
        .expect("geom column must exist");
    assert!(matches!(
        table.get(0, geom_col),
        Some(CellValue::Binary(_))
    ));
}

#[test]
fn falls_back_to_sqlite_listing_when_not_a_gpkg() {
    // A `.gpkg` extension on a plain SQLite DB without `gpkg_contents` should
    // still surface the user's tables instead of an empty picker.
    let f = NamedTempFile::with_suffix(".gpkg").unwrap();
    let conn = Connection::open(f.path()).unwrap();
    conn.execute_batch(
        "CREATE TABLE foo (x INTEGER);
         INSERT INTO foo VALUES (1);",
    )
    .unwrap();
    drop(conn);

    let tables = GeoPackageReader.list_tables(f.path()).unwrap().unwrap();
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["foo"]);
}

#[test]
fn extension_and_metadata() {
    assert_eq!(GeoPackageReader.extensions(), &["gpkg"]);
    assert_eq!(GeoPackageReader.name(), "GeoPackage");
    assert!(GeoPackageReader.supports_write());
}
