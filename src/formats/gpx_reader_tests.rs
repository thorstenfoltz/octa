use super::*;

const GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="52.5163" lon="13.3777"><ele>34.5</ele><name>Gate &amp; square</name></wpt>
  <rte><name>Plan</name>
    <rtept lat="52.50" lon="13.40"/>
  </rte>
  <trk><name>Morning run</name>
    <trkseg>
      <trkpt lat="52.5200" lon="13.4050"><ele>36</ele><time>2024-05-01T06:00:00Z</time></trkpt>
      <trkpt lat="52.5210" lon="13.4060"><time>2024-05-01T08:00:10+02:00</time></trkpt>
    </trkseg>
    <trkseg>
      <trkpt lat="52.5220" lon="13.4070"><time>later</time></trkpt>
    </trkseg>
  </trk>
</gpx>"#;

#[test]
fn every_point_is_a_row() {
    let t = read_str(GPX).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "kind",
            "track",
            "segment",
            "name",
            "lat",
            "lon",
            "elevation",
            "time"
        ]
    );
    assert_eq!(t.format_name.as_deref(), Some("GPX"));
    assert_eq!(t.rows.len(), 5);
    assert_eq!(
        t.rows[0],
        [
            CellValue::String("waypoint".into()),
            CellValue::Null,
            CellValue::Null,
            CellValue::String("Gate & square".into()),
            CellValue::Float(52.5163),
            CellValue::Float(13.3777),
            CellValue::Float(34.5),
            CellValue::Null,
        ]
    );
    assert_eq!(
        t.rows[1][..2],
        [
            CellValue::String("route".into()),
            CellValue::String("Plan".into())
        ]
    );
    assert_eq!(t.rows[2][1], CellValue::String("Morning run".into()));
    assert_eq!(t.rows[2][2], CellValue::Int(1));
    assert_eq!(t.rows[4][2], CellValue::Int(2));
}

#[test]
fn times_are_utc_and_an_unreadable_one_stays_text() {
    let t = read_str(GPX).unwrap();
    assert_eq!(
        t.rows[2][7],
        CellValue::DateTime("2024-05-01 06:00:00".into())
    );
    assert_eq!(
        t.rows[3][7],
        CellValue::DateTime("2024-05-01 06:00:10".into())
    );
    assert_eq!(t.rows[4][7], CellValue::String("later".into()));
}

#[test]
fn the_map_finds_the_coordinates() {
    let t = read_str(GPX).unwrap();
    assert_eq!(crate::data::geo_detect::detect_lat_lon(&t), Some((4, 5)));
}

#[test]
fn a_file_without_a_gpx_element_is_refused() {
    let err = read_str("<kml><Document/></kml>").unwrap_err();
    assert!(format!("{err:#}").contains("not a GPX file"), "{err:#}");
}

#[test]
fn the_registry_opens_a_gpx_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.gpx");
    std::fs::write(&path, GPX).unwrap();
    let registry = crate::formats::FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    assert_eq!(reader.name(), "GPX");
    assert!(!reader.supports_write());
    assert_eq!(reader.read_file(&path).unwrap().rows.len(), 5);
}
