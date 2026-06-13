//! Unit tests for [`chart`](chart). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn sturges_clamps_low_and_high() {
    assert_eq!(sturges_bins(0), 1);
    assert!(sturges_bins(10) >= 5);
    assert!(sturges_bins(1_000_000) <= 50);
}

#[test]
fn quantile_picks_midpoint() {
    let v = vec![1.0, 2.0, 3.0, 4.0];
    assert!((quantile(&v, 0.5) - 2.5).abs() < 1e-9);
}
