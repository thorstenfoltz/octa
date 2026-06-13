//! GeoJSON reader. Pure-Rust parsing via `geojson` (MIT/Apache-2.0).
//!
//! ## Scope
//!
//! Read-only. The table representation is one row per `Feature`. Columns
//! are the union of every feature's `properties` keys, plus a leading
//! `__geometry: Utf8` column carrying the geometry in WKT form (e.g.
//! `"POINT(2 9)"`, `"POLYGON((-5 -5, 5 -5, 0 5, -5 -5))"`).
//!
//! Why WKT in the table cell: it's a compact, human-readable serialisation
//! that survives copy-paste and round-trips through DuckDB's spatial
//! extension if the user wants to run SQL against it. The richer parsed
//! geometry is kept separately for the [`Map`](crate::data::ViewMode::Map)
//! view via [`read_with_features`].
//!
//! ## Extension
//!
//! The reader claims **only `.geojson`**. `.json` is intentionally left to
//! `JsonReader` - auto-detecting "this `.json` is actually a
//! FeatureCollection" would conflict with the registry's
//! first-match-by-extension rule. A future enhancement could expose a
//! "Reopen as GeoJSON" menu item for `.json` tabs whose root parses as a
//! `FeatureCollection`.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use geo_types::Geometry as GeoGeometry;
use geojson::{Feature, GeoJson, GeometryValue};
use serde_json::Value as JsonValue;
use wkt::ToWkt;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;

pub struct GeoJsonReader;

/// Side-channel state for the Map view. Holds the parsed geometries paired
/// with the same property index used in the table, so the view can render
/// features and the table can search/sort properties without keeping two
/// copies of the data in sync.
#[derive(Debug, Default, Clone)]
pub struct GeoJsonExtras {
    /// Parsed geometries in the same order as the rows of the
    /// `DataTable`. `None` for features that had no `geometry` field.
    pub features: Vec<MapFeature>,
}

/// A single map-displayable feature. The geometry uses `geo-types` so the
/// view can iterate coordinates without re-parsing JSON.
#[derive(Debug, Clone)]
pub struct MapFeature {
    /// `geo-types` geometry. `None` for features with a null/missing
    /// geometry field - they still get a row in the table (with empty
    /// `__geometry`) so property filtering doesn't drop them.
    pub geometry: Option<GeoGeometry<f64>>,
}

/// Build map point features from a plain table's latitude/longitude columns,
/// one feature per row (aligned with the table rows, like [`GeoJsonExtras`]).
/// Rows whose lat or lon cell is missing/non-numeric get a `None` geometry so
/// the row/feature alignment is preserved. Used by the Map view to plot any
/// tabular file with coordinate columns (see [`crate::data::geo_detect`]).
pub fn points_to_features(table: &DataTable, lat_col: usize, lon_col: usize) -> Vec<MapFeature> {
    use crate::data::geo_detect::cell_as_coord;
    (0..table.row_count())
        .map(|r| {
            let lat = table.get(r, lat_col).and_then(cell_as_coord);
            let lon = table.get(r, lon_col).and_then(cell_as_coord);
            let geometry = match (lat, lon) {
                (Some(la), Some(lo)) if la.is_finite() && lo.is_finite() => {
                    // geo-types Point is (x = lon, y = lat).
                    Some(GeoGeometry::Point(geo_types::Point::new(lo, la)))
                }
                _ => None,
            };
            MapFeature { geometry }
        })
        .collect()
}

impl FormatReader for GeoJsonReader {
    fn name(&self) -> &str {
        "GeoJSON"
    }

    fn extensions(&self) -> &[&str] {
        &["geojson"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        read_with_features(path).map(|(t, _)| t)
    }
}

/// Open a GeoJSON file and return both the flat table representation AND
/// the parsed geometries for the Map view. Mirrors the
/// `read_with_extras` pattern used by [`crate::formats::epub_reader`].
///
/// Why one entry point: the Map view needs `geo-types` geometries with
/// f64 coordinates, and the table needs WKT strings derived from the same
/// geometries. Parsing twice would double the JSON deserialisation cost
/// on large feature collections.
pub fn read_with_features(path: &Path) -> Result<(DataTable, GeoJsonExtras)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading GeoJSON {}", path.display()))?;
    let gj: GeoJson = raw
        .parse()
        .with_context(|| format!("parsing GeoJSON {}", path.display()))?;

    let features: Vec<Feature> = match gj {
        GeoJson::FeatureCollection(fc) => fc.features,
        GeoJson::Feature(f) => vec![f],
        GeoJson::Geometry(geom) => vec![Feature {
            bbox: None,
            geometry: Some(geom),
            id: None,
            properties: None,
            foreign_members: None,
        }],
    };

    // First pass: collect the union of every feature's property keys, in
    // first-seen order. BTreeSet would alphabetise the column list; we
    // preserve insertion order so columns appear in the order the file
    // mentions them. The set is just a presence check for the Vec.
    let mut property_columns: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for f in &features {
        if let Some(props) = f.properties.as_ref() {
            for key in props.keys() {
                if seen.insert(key.clone()) {
                    property_columns.push(key.clone());
                }
            }
        }
    }

    // Build the column metadata. Leading `__geometry` (WKT) then all
    // property keys, all typed as `Utf8`. We don't try to infer numeric
    // types from values - GeoJSON properties are heterogeneous and any
    // inference would surprise users on mixed columns.
    let mut columns: Vec<ColumnInfo> = Vec::with_capacity(1 + property_columns.len());
    columns.push(ColumnInfo {
        name: "__geometry".to_string(),
        data_type: "Utf8".to_string(),
    });
    for k in &property_columns {
        columns.push(ColumnInfo {
            name: k.clone(),
            data_type: "Utf8".to_string(),
        });
    }

    let mut rows: Vec<Vec<CellValue>> = Vec::with_capacity(features.len());
    let mut parsed_features: Vec<MapFeature> = Vec::with_capacity(features.len());

    for f in features {
        let geometry_wkt = f.geometry.as_ref().and_then(|g| geometry_to_wkt(&g.value));
        let geo: Option<GeoGeometry<f64>> = f
            .geometry
            .as_ref()
            .and_then(|g| GeoGeometry::<f64>::try_from(g).ok());
        parsed_features.push(MapFeature { geometry: geo });

        let mut row: Vec<CellValue> = Vec::with_capacity(1 + property_columns.len());
        row.push(match geometry_wkt {
            Some(s) => CellValue::String(s),
            None => CellValue::Null,
        });
        let props: HashMap<&str, &JsonValue> = f
            .properties
            .as_ref()
            .map(|m| m.iter().map(|(k, v)| (k.as_str(), v)).collect())
            .unwrap_or_default();
        for key in &property_columns {
            row.push(match props.get(key.as_str()) {
                Some(JsonValue::Null) | None => CellValue::Null,
                Some(JsonValue::Bool(b)) => CellValue::Bool(*b),
                Some(JsonValue::Number(n)) => {
                    if let Some(i) = n.as_i64() {
                        CellValue::Int(i)
                    } else if let Some(f) = n.as_f64() {
                        CellValue::Float(f)
                    } else {
                        CellValue::String(n.to_string())
                    }
                }
                Some(JsonValue::String(s)) => CellValue::String(s.clone()),
                // Nested arrays / objects: stringify so the cell stays
                // readable. The Map view doesn't care about properties
                // anyway, and the table column is typed `Utf8`.
                Some(other) => CellValue::String(other.to_string()),
            });
        }
        rows.push(row);
    }

    let mut table = DataTable::empty();
    table.columns = columns;
    table.rows = rows;
    table.source_path = Some(path.to_string_lossy().to_string());
    table.format_name = Some("GeoJSON".to_string());

    let extras = GeoJsonExtras {
        features: parsed_features,
    };
    Ok((table, extras))
}

/// Convert a `GeometryValue` to its WKT serialisation. `GeometryCollection`
/// values stringify each member separately and join with `; ` - strict WKT
/// has a `GEOMETRYCOLLECTION(...)` syntax for these but `wkt::ToWkt` only
/// implements it for `geo_types::GeometryCollection`, which we don't build
/// here. Returns `None` for empty/uninhabited geometries.
fn geometry_to_wkt(value: &GeometryValue) -> Option<String> {
    if let Ok(geo) = GeoGeometry::<f64>::try_from(value) {
        return Some(geo.wkt_string());
    }
    // `GeometryCollection` is the case `try_from` can fail on (the
    // geojson -> geo-types conversion bails when the collection nests).
    if let GeometryValue::GeometryCollection { geometries } = value {
        let parts: Vec<String> = geometries
            .iter()
            .filter_map(|g| geometry_to_wkt(&g.value))
            .collect();
        if parts.is_empty() {
            return None;
        }
        return Some(format!("GEOMETRYCOLLECTION({})", parts.join(", ")));
    }
    None
}

#[cfg(test)]
mod point_tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    #[test]
    fn points_from_lat_lon_columns() {
        let mut t = DataTable::empty();
        for n in ["lat", "lon"] {
            t.columns.push(ColumnInfo {
                name: n.to_string(),
                data_type: "Float64".to_string(),
            });
        }
        t.rows = vec![
            vec![CellValue::Float(51.5), CellValue::Float(-0.12)],
            vec![CellValue::Null, CellValue::Float(2.0)], // missing lat -> None
        ];
        let feats = points_to_features(&t, 0, 1);
        assert_eq!(feats.len(), 2);
        match &feats[0].geometry {
            Some(GeoGeometry::Point(p)) => {
                assert!((p.x() - (-0.12)).abs() < 1e-9); // x = lon
                assert!((p.y() - 51.5).abs() < 1e-9); // y = lat
            }
            other => panic!("expected point, got {other:?}"),
        }
        assert!(feats[1].geometry.is_none());
    }
}
