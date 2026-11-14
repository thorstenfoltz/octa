use super::*;
use eframe::egui::{Rect, pos2};

fn area() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 600.0))
}

#[test]
fn a_grid_of_two_is_side_by_side_under_another_name() {
    assert!(grid_adds_nothing(2));
    assert!(!grid_adds_nothing(3));
    assert!(!grid_adds_nothing(4));
}

#[test]
fn shapes() {
    assert_eq!(shape(TileLayout::SideBySide, 3), (3, 1));
    assert_eq!(shape(TileLayout::Stacked, 3), (1, 3));
    assert_eq!(shape(TileLayout::Grid, 4), (2, 2));
    assert_eq!(shape(TileLayout::Grid, 5), (3, 2));
    assert_eq!(shape(TileLayout::Grid, 1), (1, 1));
}

#[test]
fn side_by_side_splits_the_width_evenly() {
    let r = tile_rects(area(), 2, TileLayout::SideBySide, &[], &[], MIN_TILE, 4.0);
    assert_eq!(r.len(), 2);
    assert!((r[0].width() - 498.0).abs() < 0.01 && (r[1].min.x - 502.0).abs() < 0.01);
    assert_eq!(r[0].height(), 600.0);
}

#[test]
fn a_cut_cannot_squeeze_a_tile_below_the_minimum() {
    let r = tile_rects(
        area(),
        2,
        TileLayout::SideBySide,
        &[0.01],
        &[],
        MIN_TILE,
        4.0,
    );
    assert!(r[0].width() >= MIN_TILE - 0.01);
}

#[test]
fn a_grid_fills_row_by_row() {
    let r = tile_rects(area(), 3, TileLayout::Grid, &[], &[], MIN_TILE, 0.0);
    assert_eq!(r.len(), 3);
    assert_eq!(r[2].min, pos2(0.0, 300.0));
    assert!((r[1].min.x - 500.0).abs() < 0.01);
}

#[test]
fn stacked_tiles_share_the_height() {
    let r = tile_rects(area(), 3, TileLayout::Stacked, &[], &[], MIN_TILE, 6.0);
    assert_eq!(r.len(), 3);
    assert_eq!(r[0].width(), 1000.0);
    assert!((r[0].height() - 196.0).abs() < 0.01);
    assert!((r[1].min.y - 202.0).abs() < 0.01);
    assert!((r[2].max.y - 600.0).abs() < 0.01);
}

#[test]
fn an_area_too_small_for_the_minimum_shares_evenly_and_never_goes_negative() {
    let s = sizes_along(100.0, 3, &[0.2, 0.4], MIN_TILE, 4.0);
    assert_eq!(s.len(), 3);
    assert!(s.iter().all(|v| *v >= 0.0));
    assert!((s.iter().sum::<f32>() + 8.0 - 100.0).abs() < 0.01);
    let tiny = sizes_along(5.0, 3, &[], MIN_TILE, 4.0);
    assert!(tiny.iter().all(|v| *v >= 0.0));
}

#[test]
fn no_tiles_no_rects() {
    assert!(tile_rects(area(), 0, TileLayout::Grid, &[], &[], MIN_TILE, 4.0).is_empty());
}
