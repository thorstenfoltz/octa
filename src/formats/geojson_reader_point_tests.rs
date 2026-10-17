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
