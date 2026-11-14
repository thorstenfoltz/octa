//! Geometry for tiled tabs: the central panel cut into one rect per tile,
//! side by side, stacked or as a grid. Pure, so it is testable without egui
//! painting anything.

use egui::{Rect, pos2};

/// Smallest tile edge worth showing.
pub const MIN_TILE: f32 = 120.0;
/// Space between neighbouring tiles (also the divider hit area).
pub const GAP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TileLayout {
    #[default]
    SideBySide,
    Stacked,
    Grid,
}

/// Columns and rows for `n` tiles.
pub fn shape(layout: TileLayout, n: usize) -> (usize, usize) {
    match layout {
        TileLayout::SideBySide => (n, 1),
        TileLayout::Stacked => (1, n),
        TileLayout::Grid => {
            let cols = (n as f32).sqrt().ceil() as usize;
            (cols, n.div_ceil(cols.max(1)))
        }
    }
}

/// A grid of `n` tiles comes out exactly as side by side (two tabs, one row).
pub fn grid_adds_nothing(n: usize) -> bool {
    shape(TileLayout::Grid, n) == shape(TileLayout::SideBySide, n)
}

/// Size of each of `parts` consecutive cells along an axis of length `total`
/// with `gap` between neighbours.
///
/// `fractions` are cumulative cut positions as fractions of the space left
/// after the gaps, ascending, one per cut (`parts - 1`). An empty or
/// wrong-length slice means evenly spaced. Every cell keeps `min` where the
/// space allows; where it does not, the space is shared evenly and no size
/// goes negative. This is the format and maths the split view always used.
pub fn sizes_along(total: f32, parts: usize, fractions: &[f32], min: f32, gap: f32) -> Vec<f32> {
    if parts == 0 {
        return Vec::new();
    }
    let usable = (total - gap * (parts - 1) as f32).max(0.0);
    if parts == 1 {
        return vec![usable];
    }
    if usable <= min * parts as f32 {
        return vec![usable / parts as f32; parts];
    }

    let even = |i: usize| (i + 1) as f32 / parts as f32;
    let mut bounds: Vec<f32> = (0..parts - 1)
        .map(|i| {
            let f = if fractions.len() == parts - 1 {
                fractions[i]
            } else {
                even(i)
            };
            f.clamp(0.0, 1.0) * usable
        })
        .collect();

    // Each cut must leave a full cell behind it and room for every cell still
    // ahead; `usable > min * parts` keeps the low bound under the high one.
    let mut prev = 0.0;
    for (i, b) in bounds.iter_mut().enumerate() {
        let lo = prev + min;
        let hi = usable - min * (parts - 1 - i) as f32;
        *b = b.clamp(lo, hi);
        prev = *b;
    }

    let mut sizes = Vec::with_capacity(parts);
    let mut start = 0.0;
    for b in &bounds {
        sizes.push(b - start);
        start = *b;
    }
    sizes.push(usable - start);
    sizes
}

/// One rect per tile, row-major, `gap` px between neighbours. `xs` / `ys` are
/// cumulative column / row cut fractions (see [`sizes_along`]; empty = even),
/// clamped so every tile keeps `min` px where the area allows.
// ponytail: a last grid row with fewer tiles keeps its cells, the empty one
// stays background; stretch that row when asked.
pub fn tile_rects(
    area: Rect,
    n: usize,
    layout: TileLayout,
    xs: &[f32],
    ys: &[f32],
    min: f32,
    gap: f32,
) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let (cols, rows) = shape(layout, n);
    let ws = sizes_along(area.width(), cols, xs, min, gap);
    let hs = sizes_along(area.height(), rows, ys, min, gap);
    let offset = |sizes: &[f32], i: usize| sizes[..i].iter().sum::<f32>() + i as f32 * gap;
    (0..n)
        .map(|i| {
            let (c, r) = (i % cols, i / cols);
            let min_pt = pos2(area.min.x + offset(&ws, c), area.min.y + offset(&hs, r));
            Rect::from_min_max(min_pt, pos2(min_pt.x + ws[c], min_pt.y + hs[r]))
        })
        .collect()
}

#[cfg(test)]
#[path = "tiles_tests.rs"]
mod tests;
