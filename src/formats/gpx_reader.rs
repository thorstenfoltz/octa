//! GPX reader (GPS tracks from watches, phones and bike computers).
//! Read-only. One row per point: `kind` (waypoint, track or route), `track`
//! (the track's or route's name), `segment` (1-based, tracks only), `name`,
//! `lat`, `lon`, `elevation`, `time` (UTC). The `lat` / `lon` columns are
//! what the Map view looks for, so a GPX file opens there as points.
//!
//! Parsed with the XML reader's tree (`xml_reader::parse_tree`).
//! ponytail: extensions (heart rate, cadence) are not read, and a track is
//! drawn as its points, not as a line; add both when someone asks.

use std::path::Path;

use anyhow::{Context, Result};

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::FormatReader;
use crate::formats::xml_reader::{XmlElement, parse_tree};

pub struct GpxReader;

impl FormatReader for GpxReader {
    fn name(&self) -> &str {
        "GPX"
    }

    fn extensions(&self) -> &[&str] {
        &["gpx"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let raw = crate::data::encoding::read_to_string_detected(path)
            .with_context(|| format!("reading GPX {}", path.display()))?;
        let mut table =
            read_str(&raw).with_context(|| format!("parsing GPX {}", path.display()))?;
        table.source_path = Some(path.to_string_lossy().to_string());
        Ok(table)
    }
}

/// An element's name without a namespace prefix (`gpx:trkpt` is `trkpt`).
pub(crate) fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The text of the first child called `name`, when it holds any.
pub(crate) fn child_text<'a>(e: &'a XmlElement, name: &str) -> Option<&'a str> {
    e.children
        .iter()
        .find(|c| local(&c.name) == name)
        .map(|c| c.text.trim())
        .filter(|t| !t.is_empty())
}

pub(crate) fn text_cell(s: Option<&str>) -> CellValue {
    s.map_or(CellValue::Null, |s| CellValue::String(s.to_string()))
}

pub(crate) fn float_cell(s: Option<&str>) -> CellValue {
    s.and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|f| f.is_finite())
        .map_or(CellValue::Null, CellValue::Float)
}

/// GPX times are ISO 8601, UTC by the standard. One with another offset is
/// moved to UTC; one that does not read as a time stays as the text it is.
fn time_cell(s: Option<&str>) -> CellValue {
    let Some(s) = s else {
        return CellValue::Null;
    };
    let naive = chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.naive_utc())
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f"));
    match naive {
        Ok(n) => CellValue::DateTime(n.format("%Y-%m-%d %H:%M:%S%.f").to_string()),
        Err(_) => CellValue::String(s.to_string()),
    }
}

fn point(e: &XmlElement, kind: &str, track: Option<&str>, segment: Option<i64>) -> Vec<CellValue> {
    let attr = |k: &str| {
        e.attributes
            .iter()
            .find(|(a, _)| local(a) == k)
            .map(|(_, v)| v.as_str())
    };
    vec![
        CellValue::String(kind.to_string()),
        text_cell(track),
        segment.map_or(CellValue::Null, CellValue::Int),
        text_cell(child_text(e, "name")),
        float_cell(attr("lat")),
        float_cell(attr("lon")),
        float_cell(child_text(e, "ele")),
        time_cell(child_text(e, "time")),
    ]
}

/// Parse GPX text into the point table.
pub fn read_str(raw: &str) -> Result<DataTable> {
    let tree = parse_tree(raw)?;
    let gpx = tree
        .iter()
        .find(|e| local(&e.name) == "gpx")
        .context("no <gpx> element, so this is not a GPX file")?;
    let mut rows = Vec::new();
    for e in &gpx.children {
        match local(&e.name) {
            "wpt" => rows.push(point(e, "waypoint", None, None)),
            "rte" => {
                let name = child_text(e, "name");
                for p in e.children.iter().filter(|c| local(&c.name) == "rtept") {
                    rows.push(point(p, "route", name, None));
                }
            }
            "trk" => {
                let name = child_text(e, "name");
                let segments = e.children.iter().filter(|c| local(&c.name) == "trkseg");
                for (i, seg) in segments.enumerate() {
                    for p in seg.children.iter().filter(|c| local(&c.name) == "trkpt") {
                        rows.push(point(p, "track", name, Some(i as i64 + 1)));
                    }
                }
            }
            _ => {}
        }
    }
    let columns = [
        ("kind", "Utf8"),
        ("track", "Utf8"),
        ("segment", "Int64"),
        ("name", "Utf8"),
        ("lat", "Float64"),
        ("lon", "Float64"),
        ("elevation", "Float64"),
        ("time", "Timestamp(Microsecond, None)"),
    ];
    Ok(DataTable {
        columns: columns
            .iter()
            .map(|(n, t)| ColumnInfo {
                name: (*n).into(),
                data_type: (*t).into(),
            })
            .collect(),
        rows,
        format_name: Some("GPX".to_string()),
        ..DataTable::empty()
    })
}

#[cfg(test)]
#[path = "gpx_reader_tests.rs"]
mod tests;
