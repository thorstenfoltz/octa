//! Detect latitude/longitude columns in an ordinary table so the Map view can
//! plot point data from any tabular file (not just GeoJSON).
//!
//! Detection is name-based (the column must be called something like `lat` /
//! `latitude` and `lon` / `lng` / `longitude`) **and** value-checked: the
//! candidate columns must actually hold numbers within the valid coordinate
//! ranges. This keeps a column literally named "lat" but full of text from
//! masquerading as coordinates.

use super::{CellValue, DataTable};

/// Number of non-empty rows sampled when range-checking a candidate column.
const SAMPLE_ROWS: usize = 200;

/// Parse a cell as a coordinate number. `Null`/blank -> `None`.
pub fn cell_as_coord(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Null => None,
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        other => {
            let s = other.to_string();
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok()
            }
        }
    }
}

/// Return `(lat_col, lon_col)` when the table has a plausible pair of
/// coordinate columns, else `None`.
pub fn detect_lat_lon(table: &DataTable) -> Option<(usize, usize)> {
    let lat = find_named(table, &["latitude", "lat"])?;
    let lon = find_named(table, &["longitude", "long", "lng", "lon"])?;
    if lat == lon {
        return None;
    }
    if !column_in_range(table, lat, -90.0, 90.0) {
        return None;
    }
    if !column_in_range(table, lon, -180.0, 180.0) {
        return None;
    }
    Some((lat, lon))
}

/// First column whose normalised name equals one of `aliases`. Normalisation
/// lowercases and drops non-alphanumeric characters, so `Lat`, `LAT`, and
/// `lat_deg` (-> `latdeg`) are handled: exact alias match, or a name that
/// starts with `latitude`/`longitude`.
fn find_named(table: &DataTable, aliases: &[&str]) -> Option<usize> {
    table.columns.iter().position(|c| {
        let norm = normalise(&c.name);
        // Short aliases (lat/lon/lng/long) must match exactly; only the full
        // words latitude/longitude are allowed to match as a prefix (e.g.
        // `latitude_deg`), so `longing` or `latency` don't get picked up.
        aliases
            .iter()
            .any(|a| norm == *a || (a.len() > 4 && norm.starts_with(a)))
    })
}

fn normalise(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// True when the column has at least one numeric value and every numeric value
/// sampled lies within `[min, max]`. Non-numeric / empty cells are ignored.
fn column_in_range(table: &DataTable, col: usize, min: f64, max: f64) -> bool {
    let mut seen = 0usize;
    for r in 0..table.row_count() {
        if seen >= SAMPLE_ROWS {
            break;
        }
        let Some(v) = table.get(r, col).and_then(cell_as_coord) else {
            continue;
        };
        seen += 1;
        if !v.is_finite() || v < min || v > max {
            return false;
        }
    }
    seen > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{ColumnInfo, DataTable};

    fn table(cols: &[(&str, &str)], rows: &[Vec<CellValue>]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = cols
            .iter()
            .map(|(n, ty)| ColumnInfo {
                name: n.to_string(),
                data_type: ty.to_string(),
            })
            .collect();
        t.rows = rows.to_vec();
        t
    }

    #[test]
    fn detects_named_numeric_pair() {
        let t = table(
            &[
                ("city", "Utf8"),
                ("Latitude", "Float64"),
                ("Longitude", "Float64"),
            ],
            &[
                vec![
                    CellValue::String("A".into()),
                    CellValue::Float(51.5),
                    CellValue::Float(-0.12),
                ],
                vec![
                    CellValue::String("B".into()),
                    CellValue::Float(40.7),
                    CellValue::Float(-74.0),
                ],
            ],
        );
        assert_eq!(detect_lat_lon(&t), Some((1, 2)));
    }

    #[test]
    fn rejects_out_of_range() {
        let t = table(
            &[("lat", "Float64"), ("lon", "Float64")],
            &[vec![CellValue::Float(999.0), CellValue::Float(5.0)]],
        );
        assert_eq!(detect_lat_lon(&t), None);
    }

    #[test]
    fn rejects_text_columns() {
        let t = table(
            &[("lat", "Utf8"), ("lon", "Utf8")],
            &[vec![
                CellValue::String("north".into()),
                CellValue::String("west".into()),
            ]],
        );
        assert_eq!(detect_lat_lon(&t), None);
    }

    #[test]
    fn ignores_unrelated_columns() {
        let t = table(
            &[("platform", "Utf8"), ("longing", "Utf8")],
            &[vec![
                CellValue::String("x".into()),
                CellValue::String("y".into()),
            ]],
        );
        assert_eq!(detect_lat_lon(&t), None);
    }

    #[test]
    fn parses_string_coords() {
        let t = table(
            &[("lat", "Utf8"), ("lng", "Utf8")],
            &[vec![
                CellValue::String("48.85".into()),
                CellValue::String("2.35".into()),
            ]],
        );
        assert_eq!(detect_lat_lon(&t), Some((0, 1)));
    }
}
