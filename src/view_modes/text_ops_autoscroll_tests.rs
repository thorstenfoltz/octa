use super::*;

#[test]
fn speed_ramps_with_distance_and_is_capped() {
    // Just inside the edge zone: a slow creep, never zero (holding the
    // pointer on the boundary must still make progress).
    assert!(autoscroll_speed(0.5) >= 1.0);
    // Further out is faster.
    assert!(autoscroll_speed(20.0) > autoscroll_speed(4.0));
    // Dragging far outside the window does not fling the view.
    assert_eq!(autoscroll_speed(10_000.0), AUTOSCROLL_MAX_SPEED);
}
