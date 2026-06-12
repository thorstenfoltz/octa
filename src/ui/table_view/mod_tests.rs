//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

/// The original (pre-freeze) drag-target arithmetic, kept verbatim as the
/// no-regression oracle for `drag_target_at_x` with `frozen_cols == 0`.
fn original_drag_target(rel_x: f32, col_widths: &[f32], scroll_x: f32) -> usize {
    let pointer_x = rel_x + scroll_x;
    let mut acc = 0.0f32;
    let mut target = col_widths.len().saturating_sub(1);
    for (i, &cw) in col_widths.iter().enumerate() {
        if pointer_x < acc + cw / 2.0 {
            target = i;
            break;
        }
        acc += cw;
        target = i;
    }
    target
}

#[test]
fn drag_target_matches_original_when_nothing_is_frozen() {
    let widths = [100.0, 80.0, 120.0, 60.0];
    for scroll_x in [0.0, 50.0, 173.0] {
        for rel_x in [-20.0, 0.0, 10.0, 99.0, 150.0, 250.0, 400.0, 1000.0] {
            assert_eq!(
                drag_target_at_x(rel_x, &widths, 0, 0.0, scroll_x),
                original_drag_target(rel_x, &widths, scroll_x),
                "rel_x={rel_x} scroll_x={scroll_x}"
            );
        }
    }
}

#[test]
fn drag_target_resolves_frozen_band_positions_ignoring_scroll() {
    // Two frozen columns of 100 + 80 px; scrolled hard to the right.
    let widths = [100.0, 80.0, 120.0, 60.0];
    let frozen_width = 180.0;
    let scroll = 500.0;
    // Inside the frozen band the scroll offset is irrelevant.
    assert_eq!(drag_target_at_x(10.0, &widths, 2, frozen_width, scroll), 0);
    assert_eq!(drag_target_at_x(120.0, &widths, 2, frozen_width, scroll), 1);
    // Just right of the band: content coordinate = rel - band + scroll.
    // rel_x 190 -> content 510 -> past both scrolled columns' midpoints.
    assert_eq!(drag_target_at_x(190.0, &widths, 2, frozen_width, scroll), 3);
    // With no scroll, just right of the band is the first scrolled column.
    assert_eq!(drag_target_at_x(190.0, &widths, 2, frozen_width, 0.0), 2);
}

#[test]
fn frozen_band_width_skips_hidden_columns() {
    let widths = vec![100.0, 80.0, 120.0];
    let mut hidden = HashSet::new();
    assert_eq!(frozen_band_width(&widths, &hidden, 0), 0.0);
    assert_eq!(frozen_band_width(&widths, &hidden, 2), 180.0);
    assert_eq!(frozen_band_width(&widths, &hidden, 99), 300.0);
    hidden.insert(1);
    assert_eq!(frozen_band_width(&widths, &hidden, 2), 100.0);
}

#[test]
fn scroll_col_into_view_is_unchanged_with_no_frozen_band() {
    let mut state = TableViewState {
        col_widths: vec![100.0, 100.0, 100.0, 100.0],
        row_number_width: 60.0,
        ..Default::default()
    };
    // Viewport fits two columns beside the gutter; bring column 3 in.
    scroll_col_into_view(&mut state, 3, 260.0, 1000.0, 0, 0.0);
    // col 3 right edge = 400; window = 260 - 60 = 200 -> scroll_x = 200.
    assert_eq!(state.scroll_x, 200.0);
    // Scrolling back to column 0 returns to the origin.
    scroll_col_into_view(&mut state, 0, 260.0, 1000.0, 0, 0.0);
    assert_eq!(state.scroll_x, 0.0);
}

#[test]
fn scroll_col_into_view_accounts_for_the_frozen_band() {
    let mut state = TableViewState {
        col_widths: vec![100.0, 100.0, 100.0, 100.0],
        row_number_width: 60.0,
        ..Default::default()
    };
    // One frozen column: the scrollable window shrinks by its width and
    // offsets are measured from the first scrolled column.
    scroll_col_into_view(&mut state, 3, 360.0, 1000.0, 1, 100.0);
    // cols 1..3 left = 200, right = 300; window = 360 - 60 - 100 = 200
    // -> scroll_x = 300 - 200 = 100.
    assert_eq!(state.scroll_x, 100.0);
    // A frozen column never changes the scroll.
    scroll_col_into_view(&mut state, 0, 360.0, 1000.0, 1, 100.0);
    assert_eq!(state.scroll_x, 100.0);
}
