//! Spatial join: the polygon each point falls in (Inside), or the nearest
//! point of another table with its distance (Nearest). Coordinates are
//! latitude/longitude; distances are great-circle kilometres, which need no
//! projection settings. A layer in another coordinate system is refused, not
//! silently mismatched.
//!
//! Pure. Layers are read as they are, so callers pass tables with edits
//! applied. No `geo` crate: point-in-polygon is a ray cast, the index is
//! `rstar`.

use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use geo_types::{Geometry, LineString, Polygon};
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{AABB, RTree};

use crate::data::geo_detect::{cell_as_coord, detect_lat_lon};
use crate::data::{CellValue, ColumnInfo, DataTable};

/// The column the geometry-bearing readers (GeoJSON, shapefile)
/// put WKT into.
pub const GEOMETRY_COLUMN: &str = "__geometry";
const EARTH_RADIUS_KM: f64 = 6371.0088;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpatialOp {
    Inside,
    Nearest { within_km: Option<f64> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointCols {
    LatLon { lat: usize, lon: usize },
    Geometry(usize),
}

pub struct Layer<'a> {
    /// Column prefix; see [`prefix_for`].
    pub name: String,
    pub table: &'a DataTable,
}

pub struct SpatialResult {
    pub table: DataTable,
    /// Points inside more than one polygon of a layer (the first was taken).
    pub multi_match: usize,
    /// Rows without a readable point.
    pub no_point: usize,
}

/// A tab or file name as a column prefix: file stem, lower case, anything
/// but letters and digits as `_`.
pub fn prefix_for(name: &str) -> String {
    let stem = name.split('.').next().unwrap_or(name);
    let mut out: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

fn geometry_col(table: &DataTable) -> Option<usize> {
    table.columns.iter().position(|c| c.name == GEOMETRY_COLUMN)
}

/// Where a table keeps its points: latitude/longitude columns first (the
/// Map view's detection), else a geometry column that holds points. A
/// geometry column of polygons is not points: calling it that let the Join
/// dialog put a regions file on the points side and fail on Apply.
pub fn point_cols(table: &DataTable) -> Option<PointCols> {
    detect_lat_lon(table)
        .map(|(lat, lon)| PointCols::LatLon { lat, lon })
        .or_else(|| {
            let c = geometry_col(table)?;
            let first = (0..table.row_count())
                .filter_map(|r| table.get(r, c))
                .find(|v| !matches!(v, CellValue::Null) && !v.to_string().is_empty())?;
            matches!(parse_wkt(&first.to_string())?, Geometry::Point(_))
                .then_some(PointCols::Geometry(c))
        })
}

fn parse_wkt(s: &str) -> Option<Geometry<f64>> {
    Geometry::try_from(wkt::Wkt::<f64>::from_str(s).ok()?).ok()
}

/// `(lat, lon)` of row `r`.
fn point_at(table: &DataTable, cols: PointCols, r: usize) -> Option<(f64, f64)> {
    match cols {
        PointCols::LatLon { lat, lon } => Some((
            cell_as_coord(table.get(r, lat)?)?,
            cell_as_coord(table.get(r, lon)?)?,
        )),
        PointCols::Geometry(c) => match parse_wkt(&table.get(r, c)?.to_string())? {
            Geometry::Point(p) => Some((p.y(), p.x())),
            _ => None,
        },
    }
}

fn check_lat_lon(layer: &str, lon: f64, lat: f64) -> Result<()> {
    if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
        bail!(
            "layer `{layer}` is not in latitude/longitude (found the coordinate {lon}, {lat}); \
             reproject it to WGS 84 first"
        );
    }
    Ok(())
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

/// A position on the unit sphere: the nearest point in 3D is the nearest
/// along the surface, so a plain R-tree answers great-circle questions.
fn unit(lat: f64, lon: f64) -> [f64; 3] {
    let (p, l) = (lat.to_radians(), lon.to_radians());
    [p.cos() * l.cos(), p.cos() * l.sin(), p.sin()]
}

fn polygons_of(g: Geometry<f64>, out: &mut Vec<Polygon<f64>>) {
    match g {
        Geometry::Polygon(p) => out.push(p),
        Geometry::MultiPolygon(mp) => out.extend(mp.0),
        Geometry::GeometryCollection(gc) => gc.0.into_iter().for_each(|g| polygons_of(g, out)),
        _ => {}
    }
}

fn ring_contains(ring: &LineString<f64>, x: f64, y: f64) -> bool {
    let pts = &ring.0;
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (xi, yi, xj, yj) = (pts[i].x, pts[i].y, pts[j].x, pts[j].y);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn contains(p: &Polygon<f64>, x: f64, y: f64) -> bool {
    ring_contains(p.exterior(), x, y) && !p.interiors().iter().any(|h| ring_contains(h, x, y))
}

fn bbox(p: &Polygon<f64>) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for c in &p.exterior().0 {
        lo = [lo[0].min(c.x), lo[1].min(c.y)];
        hi = [hi[0].max(c.x), hi[1].max(c.y)];
    }
    (lo, hi)
}

/// Per point, the first layer row whose polygon contains it; plus how many
/// points were in more than one.
fn inside_layer(
    layer: &Layer,
    points: &[Option<(f64, f64)>],
) -> Result<(Vec<Option<usize>>, usize)> {
    let gcol = geometry_col(layer.table).ok_or_else(|| {
        anyhow!(
            "layer `{}` has no geometry; open a GeoJSON or shapefile",
            layer.name
        )
    })?;
    let mut polys: Vec<(Polygon<f64>, usize)> = Vec::new();
    let mut rects = Vec::new();
    for r in 0..layer.table.row_count() {
        let Some(g) = layer
            .table
            .get(r, gcol)
            .and_then(|c| parse_wkt(&c.to_string()))
        else {
            continue;
        };
        let mut found = Vec::new();
        polygons_of(g, &mut found);
        for p in found {
            let (lo, hi) = bbox(&p);
            check_lat_lon(&layer.name, lo[0], lo[1])?;
            check_lat_lon(&layer.name, hi[0], hi[1])?;
            rects.push(GeomWithData::new(
                Rectangle::from_corners(lo, hi),
                polys.len(),
            ));
            polys.push((p, r));
        }
    }
    let tree = RTree::bulk_load(rects);
    let mut multi = 0;
    let mut hits = Vec::with_capacity(points.len());
    for pt in points {
        let Some((lat, lon)) = *pt else {
            hits.push(None);
            continue;
        };
        let mut rows: Vec<usize> = tree
            .locate_in_envelope_intersecting(&AABB::from_point([lon, lat]))
            .map(|g| g.data)
            .filter(|&i| contains(&polys[i].0, lon, lat))
            .map(|i| polys[i].1)
            .collect();
        rows.sort_unstable();
        rows.dedup();
        multi += usize::from(rows.len() > 1);
        hits.push(rows.first().copied());
    }
    Ok((hits, multi))
}

/// Per point, the nearest layer row and its distance, `None` beyond `within_km`.
fn nearest_layer(
    layer: &Layer,
    points: &[Option<(f64, f64)>],
    within_km: Option<f64>,
) -> Result<Vec<Option<(usize, f64)>>> {
    let cols = point_cols(layer.table).ok_or_else(|| {
        anyhow!(
            "layer `{}` has no points (no latitude/longitude columns, no point geometry)",
            layer.name
        )
    })?;
    let mut targets = Vec::new();
    for r in 0..layer.table.row_count() {
        if let Some((lat, lon)) = point_at(layer.table, cols, r) {
            check_lat_lon(&layer.name, lon, lat)?;
            targets.push(GeomWithData::new(unit(lat, lon), (r, lat, lon)));
        }
    }
    let tree = RTree::bulk_load(targets);
    Ok(points
        .iter()
        .map(|pt| {
            let (lat, lon) = (*pt)?;
            let (r, tlat, tlon) = tree.nearest_neighbor(&unit(lat, lon))?.data;
            let d = haversine_km(lat, lon, tlat, tlon);
            within_km.is_none_or(|w| d <= w).then_some((r, d))
        })
        .collect())
}

/// `points` plus, per layer, that layer's columns (prefixed with the layer
/// name, geometry left out) for the matching row; Nearest adds
/// `<layer>_distance_km`.
pub fn spatial_join(
    points: &DataTable,
    cols: PointCols,
    layers: &[Layer],
    op: SpatialOp,
) -> Result<SpatialResult> {
    let pts: Vec<Option<(f64, f64)>> = (0..points.row_count())
        .map(|r| point_at(points, cols, r))
        .collect();
    for (lat, lon) in pts.iter().flatten() {
        check_lat_lon("points", *lon, *lat)?;
    }
    let mut out = DataTable::empty();
    out.columns = points.columns.clone();
    out.rows = (0..points.row_count())
        .map(|r| {
            (0..points.col_count())
                .map(|c| points.get(r, c).cloned().unwrap_or(CellValue::Null))
                .collect()
        })
        .collect();
    let mut multi_match = 0;
    for layer in layers {
        let lcols: Vec<usize> = (0..layer.table.col_count())
            .filter(|&c| layer.table.columns[c].name != GEOMETRY_COLUMN)
            .collect();
        for &c in &lcols {
            out.columns.push(ColumnInfo {
                name: format!("{}_{}", layer.name, layer.table.columns[c].name),
                data_type: layer.table.columns[c].data_type.clone(),
            });
        }
        let value = |row: Option<usize>, c: usize| {
            row.and_then(|r| layer.table.get(r, c).cloned())
                .unwrap_or(CellValue::Null)
        };
        match op {
            SpatialOp::Inside => {
                let (hits, multi) = inside_layer(layer, &pts)?;
                multi_match += multi;
                for (row, hit) in out.rows.iter_mut().zip(&hits) {
                    row.extend(lcols.iter().map(|&c| value(*hit, c)));
                }
            }
            SpatialOp::Nearest { within_km } => {
                out.columns.push(ColumnInfo {
                    name: format!("{}_distance_km", layer.name),
                    data_type: "Float64".into(),
                });
                let hits = nearest_layer(layer, &pts, within_km)?;
                for (row, hit) in out.rows.iter_mut().zip(&hits) {
                    row.extend(lcols.iter().map(|&c| value(hit.map(|h| h.0), c)));
                    row.push(hit.map_or(CellValue::Null, |h| {
                        CellValue::Float((h.1 * 1000.0).round() / 1000.0)
                    }));
                }
            }
        }
    }
    Ok(SpatialResult {
        table: out,
        multi_match,
        no_point: pts.iter().filter(|p| p.is_none()).count(),
    })
}

#[cfg(test)]
#[path = "spatial_join_tests.rs"]
mod tests;
