//! Shapefile reader (read-only).
//!
//! An ESRI shapefile is really a set of sibling files: `.shp` (geometry),
//! `.dbf` (attribute table) and `.shx` (index). Opening the `.shp` reads the
//! geometry and joins it with the `.dbf` attributes, producing the same table
//! shape as the GeoJSON reader: a leading `__geometry` column carrying WKT,
//! then one column per attribute field. It also returns `geo-types`
//! geometries so the file plots on the [Map view](crate::view_modes) for free
//! (same path as GeoJSON, see `read_with_features`).
//!
//! Writing is not supported: a faithful shapefile needs the `.shx` index and a
//! fixed per-record geometry type, which we don't reconstruct.

use std::path::Path;

use anyhow::{Context, Result};
use geo_types::Geometry as GeoGeometry;
use wkt::ToWkt;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use crate::formats::dbf_reader::{dbf_type_string, field_value_to_cell};
use crate::formats::geojson_reader::MapFeature;

pub struct ShapefileReader;

impl FormatReader for ShapefileReader {
    fn name(&self) -> &str {
        "Shapefile"
    }

    fn extensions(&self) -> &[&str] {
        &["shp"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        read_with_features(path).map(|(t, _)| t)
    }
}

/// Read a shapefile into a table plus the `geo-types` geometries the Map view
/// needs. The table's leading `__geometry` column holds WKT; the rest mirror
/// the `.dbf` fields in their declared order.
pub fn read_with_features(path: &Path) -> Result<(DataTable, Vec<MapFeature>)> {
    // Field names + types come from the sibling .dbf in declaration order;
    // `dbase::Record` itself iterates as an (unordered) map, so we drive the
    // column order from the field descriptors instead.
    let (field_names, field_types) = dbf_fields(path);

    let mut reader = shapefile::Reader::from_path(path)
        .with_context(|| format!("opening shapefile {}", path.display()))?;

    let mut columns: Vec<ColumnInfo> = Vec::with_capacity(1 + field_names.len());
    columns.push(ColumnInfo {
        name: "__geometry".to_string(),
        data_type: "Utf8".to_string(),
    });
    for (name, ty) in field_names.iter().zip(&field_types) {
        columns.push(ColumnInfo {
            name: name.clone(),
            data_type: ty.clone(),
        });
    }

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut features: Vec<MapFeature> = Vec::new();

    for pair in reader.iter_shapes_and_records() {
        let (shape, record) = pair.context("reading shapefile record")?;
        let geo: Option<GeoGeometry<f64>> = GeoGeometry::<f64>::try_from(shape).ok();
        let wkt = geo.as_ref().map(|g| g.wkt_string());
        features.push(MapFeature {
            geometry: geo.clone(),
        });

        let mut row: Vec<CellValue> = Vec::with_capacity(columns.len());
        row.push(wkt.map(CellValue::String).unwrap_or(CellValue::Null));
        for name in &field_names {
            row.push(
                record
                    .get(name)
                    .map(field_value_to_cell)
                    .unwrap_or(CellValue::Null),
            );
        }
        rows.push(row);
    }

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(path.to_string_lossy().to_string());
    table.format_name = Some("Shapefile".to_string());
    Ok((table, features))
}

/// Read the attribute field names + Octa type strings from the sibling `.dbf`,
/// in declared order. Returns empty vecs when there is no `.dbf` (a geometry-
/// only shapefile) or it can't be opened.
fn dbf_fields(shp_path: &Path) -> (Vec<String>, Vec<String>) {
    let dbf_path = shp_path.with_extension("dbf");
    let Ok(reader) = dbase::Reader::from_path(&dbf_path) else {
        return (Vec::new(), Vec::new());
    };
    let names = reader.fields().iter().map(|f| f.name().to_string());
    let types = reader
        .fields()
        .iter()
        .map(|f| dbf_type_string(f).to_string());
    (names.collect(), types.collect())
}

#[cfg(test)]
mod tests {
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
}
