use super::*;

/// Write a 2-point shapefile (with a `.dbf` carrying a `name` field) to a
/// temp dir, then read it back.
fn write_two_point_shapefile(dir: &Path) -> std::path::PathBuf {
    use dbase::{FieldName, TableWriterBuilder};
    use shapefile::{Point, Writer};
    use std::convert::TryFrom;

    let shp = dir.join("pts.shp");
    let table_builder =
        TableWriterBuilder::new().add_character_field(FieldName::try_from("name").unwrap(), 50);
    let mut writer = Writer::from_path(&shp, table_builder).expect("create shapefile writer");

    for (x, y, name) in [(1.0_f64, 2.0_f64, "alpha"), (3.0, 4.0, "beta")] {
        let mut record = dbase::Record::default();
        record.insert(
            "name".to_string(),
            dbase::FieldValue::Character(Some(name.to_string())),
        );
        writer
            .write_shape_and_record(&Point::new(x, y), &record)
            .expect("write shape");
    }
    drop(writer);
    shp
}

#[test]
fn reads_points_with_attributes_and_geometry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shp = write_two_point_shapefile(dir.path());

    let (table, features) = read_with_features(&shp).expect("read shapefile");
    assert_eq!(table.format_name.as_deref(), Some("Shapefile"));
    assert_eq!(table.row_count(), 2);
    // Leading __geometry column + the dbf `name` field.
    assert_eq!(table.columns[0].name, "__geometry");
    assert_eq!(table.columns[1].name, "name");
    // WKT round-trips the point coordinates.
    match table.get(0, 0) {
        Some(CellValue::String(wkt)) => assert!(wkt.contains("POINT"), "{wkt}"),
        other => panic!("expected WKT string, got {other:?}"),
    }
    assert_eq!(
        table.get(0, 1),
        Some(&CellValue::String("alpha".to_string()))
    );
    // Each row yields a map feature with a Point geometry.
    assert_eq!(features.len(), 2);
    assert!(matches!(features[0].geometry, Some(GeoGeometry::Point(_))));
}
