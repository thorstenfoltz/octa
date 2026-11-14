use super::*;

const KML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2">
  <Document>
    <name>Trip</name>
    <Placemark>
      <name>Start</name>
      <description><![CDATA[Meet <b>here</b>]]></description>
      <Point><coordinates>13.3777,52.5163,34</coordinates></Point>
    </Placemark>
    <Folder>
      <name>Day 1</name>
      <Folder>
        <name>Morning</name>
        <Placemark>
          <name>Walk</name>
          <LineString><coordinates>
            13.40,52.52,0 13.41,52.53,0
            13.42,52.54,0
          </coordinates></LineString>
        </Placemark>
      </Folder>
      <Placemark>
        <name>Park</name>
        <Polygon>
          <outerBoundaryIs><LinearRing><coordinates>0,0 4,0 4,4 0,4 0,0</coordinates></LinearRing></outerBoundaryIs>
          <innerBoundaryIs><LinearRing><coordinates>1,1 2,1 2,2 1,2 1,1</coordinates></LinearRing></innerBoundaryIs>
        </Polygon>
      </Placemark>
    </Folder>
    <Placemark>
      <name>Both</name>
      <MultiGeometry>
        <Point><coordinates>10,50</coordinates></Point>
        <LineString><coordinates>10,50 11,51</coordinates></LineString>
      </MultiGeometry>
    </Placemark>
    <Placemark><name>Nowhere</name></Placemark>
  </Document>
</kml>"#;

#[test]
fn every_placemark_is_a_row_with_its_first_point() {
    let (t, features) = read_str(KML).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "name",
            "description",
            "folder",
            "kind",
            "lat",
            "lon",
            "altitude"
        ]
    );
    assert_eq!(t.format_name.as_deref(), Some("KML"));
    assert_eq!(t.rows.len(), 5);
    assert_eq!(features.len(), 5, "one geometry per row");
    assert_eq!(
        t.rows[0],
        [
            CellValue::String("Start".into()),
            CellValue::String("Meet <b>here</b>".into()),
            CellValue::Null,
            CellValue::String("Point".into()),
            CellValue::Float(52.5163),
            CellValue::Float(13.3777),
            CellValue::Float(34.0),
        ]
    );
    assert_eq!(t.rows[1][2], CellValue::String("Day 1 / Morning".into()));
    assert_eq!(t.rows[1][3], CellValue::String("LineString".into()));
    assert_eq!(t.rows[1][4], CellValue::Float(52.52));
    assert_eq!(t.rows[2][2], CellValue::String("Day 1".into()));
    assert_eq!(t.rows[3][3], CellValue::String("MultiGeometry".into()));
    assert_eq!(t.rows[3][6], CellValue::Null, "no altitude given");
    assert_eq!(
        t.rows[4][3..],
        [
            CellValue::Null,
            CellValue::Null,
            CellValue::Null,
            CellValue::Null
        ]
    );
}

#[test]
fn lines_and_polygons_reach_the_map_whole() {
    let (_, features) = read_str(KML).unwrap();
    assert!(matches!(features[0].geometry, Some(Geometry::Point(_))));
    match &features[1].geometry {
        Some(Geometry::LineString(l)) => assert_eq!(l.0.len(), 3),
        other => panic!("{other:?}"),
    }
    match &features[2].geometry {
        Some(Geometry::Polygon(p)) => {
            assert_eq!(p.exterior().0.len(), 5);
            assert_eq!(p.interiors().len(), 1);
        }
        other => panic!("{other:?}"),
    }
    match &features[3].geometry {
        Some(Geometry::GeometryCollection(g)) => assert_eq!(g.0.len(), 2),
        other => panic!("{other:?}"),
    }
    assert!(features[4].geometry.is_none());
}

#[test]
fn a_file_without_a_kml_element_is_refused() {
    let err = read_str("<gpx/>").unwrap_err();
    assert!(format!("{err:#}").contains("not a KML file"), "{err:#}");
}

fn kmz(dir: &Path, entries: &[(&str, &str)]) -> std::path::PathBuf {
    use std::io::Write;
    let path = dir.join("trip.kmz");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, body) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
fn a_kmz_is_read_through_the_registry() {
    let dir = tempfile::tempdir().unwrap();
    let path = kmz(dir.path(), &[("files/icon.txt", "x"), ("doc.kml", KML)]);
    let registry = crate::formats::FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();
    assert_eq!(reader.name(), "KML");
    assert!(!reader.supports_write());
    let t = reader.read_file(&path).unwrap();
    assert_eq!(t.rows.len(), 5);
    assert_eq!(crate::data::geo_detect::detect_lat_lon(&t), Some((4, 5)));
}

#[test]
fn a_kmz_without_a_kml_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = kmz(dir.path(), &[("readme.txt", "x")]);
    let err = read_with_features(&path).unwrap_err();
    assert!(format!("{err:#}").contains("holds no .kml file"), "{err:#}");
}
