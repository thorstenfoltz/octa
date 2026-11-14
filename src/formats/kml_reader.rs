//! KML and KMZ reader (Google Earth, Google My Maps). Read-only. One row per
//! Placemark: `name`, `description`, `folder` (the folders it sits in, joined
//! with ` / `), `kind` (Point, LineString, Polygon, MultiGeometry), and the
//! first point's `lat`, `lon`, `altitude`. Lines and polygons go to the Map
//! view whole, through [`read_with_features`], the way GeoJSON's do.
//!
//! A `.kmz` is a zip holding the KML (`doc.kml`, else the first `.kml`).
//! Parsed with the XML reader's tree (`xml_reader::parse_tree`).
//! ponytail: styles, `gx:Track` and network links are not read.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use geo_types::{Coord, Geometry, GeometryCollection, LineString, Point, Polygon};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use crate::formats::geojson_reader::MapFeature;
use crate::formats::gpx_reader::{child_text, local, text_cell};
use crate::formats::xml_reader::{XmlElement, parse_tree};

pub struct KmlReader;

impl FormatReader for KmlReader {
    fn name(&self) -> &str {
        "KML"
    }

    fn extensions(&self) -> &[&str] {
        &["kml", "kmz"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        read_with_features(path).map(|(t, _)| t)
    }
}

/// Read a KML or KMZ file into the Placemark table plus one geometry per
/// row for the Map view.
pub fn read_with_features(path: &Path) -> Result<(DataTable, Vec<MapFeature>)> {
    let is_kmz = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("kmz"));
    let raw = if is_kmz {
        kml_in_kmz(path)?
    } else {
        crate::data::encoding::read_to_string_detected(path)
            .with_context(|| format!("reading KML {}", path.display()))?
    };
    let (mut table, features) =
        read_str(&raw).with_context(|| format!("parsing KML {}", path.display()))?;
    table.source_path = Some(path.to_string_lossy().to_string());
    Ok((table, features))
}

/// The KML inside a KMZ. The archive is untrusted: the read stops at the
/// decompression limit from Settings.
fn kml_in_kmz(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .with_context(|| format!("{} is not a zip archive", path.display()))?;
    let names: Vec<String> = archive
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.into_owned()))
        .collect();
    let is_kml = |n: &&String| n.to_ascii_lowercase().ends_with(".kml");
    let name = names
        .iter()
        .find(|n| n.eq_ignore_ascii_case("doc.kml"))
        .or_else(|| names.iter().find(is_kml))
        .with_context(|| format!("{} holds no .kml file", path.display()))?;
    let cap = crate::formats::compression::max_decompressed();
    let mut bytes = Vec::new();
    archive
        .by_name(name)?
        .take(cap.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("unpacking {name}"))?;
    if bytes.len() as u64 > cap {
        bail!("{name} unpacks to more than the decompression limit of {cap} bytes");
    }
    Ok(crate::data::encoding::decode_bytes(&bytes).0)
}

/// A position and its altitude, when the file gives one.
type At = (Coord<f64>, Option<f64>);

/// `lon,lat[,alt]` tuples separated by white space. A tuple that does not
/// read as numbers is left out.
fn coordinates(text: &str) -> Vec<At> {
    text.split_whitespace()
        .filter_map(|tuple| {
            let mut parts = tuple.split(',').map(|p| p.trim().parse::<f64>().ok());
            let (lon, lat) = (parts.next()??, parts.next()??);
            let alt = parts.next().flatten();
            (lon.is_finite() && lat.is_finite()).then_some((Coord { x: lon, y: lat }, alt))
        })
        .collect()
}

fn coords_of(e: &XmlElement) -> Vec<At> {
    child_text(e, "coordinates")
        .map(coordinates)
        .unwrap_or_default()
}

fn ring(e: &XmlElement, boundary: &str) -> Vec<LineString<f64>> {
    e.children
        .iter()
        .filter(|c| local(&c.name) == boundary)
        .flat_map(|b| b.children.iter().filter(|c| local(&c.name) == "LinearRing"))
        .map(|r| LineString(coords_of(r).into_iter().map(|(c, _)| c).collect()))
        .collect()
}

/// A geometry element: its kind, the geometry, and its first point with the
/// altitude. `None` for an element that is no geometry or holds no point.
fn geometry(e: &XmlElement) -> Option<(&'static str, Geometry<f64>, At)> {
    match local(&e.name) {
        "Point" => {
            let first = *coords_of(e).first()?;
            Some(("Point", Geometry::Point(Point(first.0)), first))
        }
        "LineString" | "LinearRing" => {
            let pts = coords_of(e);
            let first = *pts.first()?;
            let line = LineString(pts.into_iter().map(|(c, _)| c).collect());
            Some(("LineString", Geometry::LineString(line), first))
        }
        "Polygon" => {
            let outer_el = e
                .children
                .iter()
                .find(|c| local(&c.name) == "outerBoundaryIs")?
                .children
                .iter()
                .find(|c| local(&c.name) == "LinearRing")?;
            let first = *coords_of(outer_el).first()?;
            let outer = ring(e, "outerBoundaryIs").into_iter().next()?;
            let poly = Polygon::new(outer, ring(e, "innerBoundaryIs"));
            Some(("Polygon", Geometry::Polygon(poly), first))
        }
        "MultiGeometry" => {
            let parts: Vec<_> = e.children.iter().filter_map(geometry).collect();
            let first = parts.first()?.2;
            let all = GeometryCollection(parts.into_iter().map(|p| p.1).collect());
            Some(("MultiGeometry", Geometry::GeometryCollection(all), first))
        }
        _ => None,
    }
}

fn walk(
    e: &XmlElement,
    folders: &mut Vec<String>,
    rows: &mut Vec<Vec<CellValue>>,
    features: &mut Vec<MapFeature>,
) {
    match local(&e.name) {
        "Placemark" => {
            let geo = e.children.iter().find_map(geometry);
            let float = |v: Option<f64>| v.map_or(CellValue::Null, CellValue::Float);
            let first = geo.as_ref().map(|g| g.2);
            rows.push(vec![
                text_cell(child_text(e, "name")),
                text_cell(child_text(e, "description")),
                text_cell(Some(folders.join(" / ").as_str()).filter(|f| !f.is_empty())),
                text_cell(geo.as_ref().map(|g| g.0)),
                float(first.map(|f| f.0.y)),
                float(first.map(|f| f.0.x)),
                float(first.and_then(|f| f.1)),
            ]);
            features.push(MapFeature {
                geometry: geo.map(|g| g.1),
            });
        }
        // The Document's own name is the file's title, not a folder.
        kind @ ("Folder" | "Document" | "kml") => {
            let named = kind == "Folder";
            if named {
                folders.push(child_text(e, "name").unwrap_or_default().to_string());
            }
            for c in &e.children {
                walk(c, folders, rows, features);
            }
            if named {
                folders.pop();
            }
        }
        _ => {}
    }
}

/// Parse KML text into the Placemark table and its geometries, row for row.
pub fn read_str(raw: &str) -> Result<(DataTable, Vec<MapFeature>)> {
    let tree = parse_tree(raw)?;
    let kml = tree
        .iter()
        .find(|e| local(&e.name) == "kml")
        .context("no <kml> element, so this is not a KML file")?;
    let (mut rows, mut features) = (Vec::new(), Vec::new());
    walk(kml, &mut Vec::new(), &mut rows, &mut features);
    let columns = [
        ("name", "Utf8"),
        ("description", "Utf8"),
        ("folder", "Utf8"),
        ("kind", "Utf8"),
        ("lat", "Float64"),
        ("lon", "Float64"),
        ("altitude", "Float64"),
    ];
    let table = DataTable {
        columns: columns
            .iter()
            .map(|(n, t)| ColumnInfo {
                name: (*n).into(),
                data_type: (*t).into(),
            })
            .collect(),
        rows,
        format_name: Some("KML".to_string()),
        ..DataTable::empty()
    };
    Ok((table, features))
}

#[cfg(test)]
#[path = "kml_reader_tests.rs"]
mod tests;
