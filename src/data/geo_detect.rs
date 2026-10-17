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
#[path = "geo_detect_tests.rs"]
mod tests;
