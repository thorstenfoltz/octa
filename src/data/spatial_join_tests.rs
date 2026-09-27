//! Unit tests for [`spatial_join`](super). Included via `#[path]`.

use super::*;
use crate::data::ColumnInfo;

fn table(cols: &[&str], rows: &[Vec<CellValue>]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows.to_vec();
    t
}
fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}
fn f(v: f64) -> CellValue {
    CellValue::Float(v)
}

/// Two squares; the west one has a hole around (1.5, 1.5).
fn regions() -> DataTable {
    table(
        &["name", GEOMETRY_COLUMN],
        &[
            vec![
                s("west"),
                s("POLYGON((0 0, 3 0, 3 3, 0 3, 0 0), (1 1, 2 1, 2 2, 1 2, 1 1))"),
            ],
            vec![s("east"), s("MULTIPOLYGON(((3 0, 6 0, 6 3, 3 3, 3 0)))")],
        ],
    )
}

fn customers() -> DataTable {
    table(
        &["id", "lat", "lon"],
        &[
            vec![s("a"), f(0.5), f(0.5)], // west
            vec![s("b"), f(1.5), f(1.5)], // in the hole
            vec![s("c"), f(1.0), f(4.0)], // east
            vec![s("d"), CellValue::Null, CellValue::Null],
        ],
    )
}

#[test]
fn inside_honours_holes_and_multipolygons() {
    let pts = customers();
    let layer = regions();
    let cols = point_cols(&pts).unwrap();
    let r = spatial_join(
        &pts,
        cols,
        &[Layer {
            name: "region".into(),
            table: &layer,
        }],
        SpatialOp::Inside,
    )
    .unwrap();
    let name = r
        .table
        .columns
        .iter()
        .position(|c| c.name == "region_name")
        .unwrap();
    let got: Vec<CellValue> = r.table.rows.iter().map(|row| row[name].clone()).collect();
    assert_eq!(
        got,
        vec![s("west"), CellValue::Null, s("east"), CellValue::Null]
    );
    assert_eq!(r.no_point, 1);
    assert!(
        r.table
            .columns
            .iter()
            .all(|c| c.name != format!("region_{GEOMETRY_COLUMN}"))
    );
}

#[test]
fn nearest_uses_great_circle_distance_and_the_cut_off() {
    let pts = table(
        &["id", "lat", "lon"],
        &[vec![s("berlin"), f(52.52), f(13.405)]],
    );
    let stores = table(
        &["store", "lat", "lon"],
        &[
            vec![s("paris"), f(48.8566), f(2.3522)],
            vec![s("hamburg"), f(53.5511), f(9.9937)],
        ],
    );
    let cols = point_cols(&pts).unwrap();
    let layer = [Layer {
        name: "store".into(),
        table: &stores,
    }];
    let r = spatial_join(&pts, cols, &layer, SpatialOp::Nearest { within_km: None }).unwrap();
    let row = &r.table.rows[0];
    assert_eq!(row[3], s("hamburg"));
    let CellValue::Float(d) = row[row.len() - 1] else {
        panic!("distance is a float")
    };
    assert!((d - 255.0).abs() < 5.0, "{d}");
    let r = spatial_join(
        &pts,
        cols,
        &layer,
        SpatialOp::Nearest {
            within_km: Some(100.0),
        },
    )
    .unwrap();
    assert_eq!(r.table.rows[0][3], CellValue::Null);
}

#[test]
fn haversine_matches_a_known_distance() {
    let d = haversine_km(52.52, 13.405, 48.8566, 2.3522);
    assert!((d - 878.0).abs() < 5.0, "{d}");
}

#[test]
fn a_projected_layer_is_refused() {
    let pts = customers();
    let projected = table(
        &["name", GEOMETRY_COLUMN],
        &[vec![
            s("x"),
            s("POLYGON((500000 5000000, 500100 5000000, 500100 5000100, 500000 5000000))"),
        ]],
    );
    let err = spatial_join(
        &pts,
        point_cols(&pts).unwrap(),
        &[Layer {
            name: "p".into(),
            table: &projected,
        }],
        SpatialOp::Inside,
    )
    .err()
    .expect("must refuse");
    assert!(format!("{err:#}").contains("latitude/longitude"));
}

#[test]
fn prefix_for_makes_a_column_safe_prefix() {
    assert_eq!(prefix_for("Sales Regions.geojson"), "sales_regions");
}

#[test]
fn a_polygon_layer_has_no_points() {
    assert_eq!(point_cols(&regions()), None);
    let pts = table(&["id", GEOMETRY_COLUMN], &[vec![s("a"), s("POINT(1 2)")]]);
    assert_eq!(point_cols(&pts), Some(PointCols::Geometry(1)));
}
