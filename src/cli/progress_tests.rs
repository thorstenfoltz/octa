use super::*;

#[test]
fn the_eta_waits_for_a_first_sample() {
    // Nothing done yet: no ETA, because there is nothing to divide by.
    let line = item_line(0, Some(500), "a.csv", Duration::from_secs(0));
    assert_eq!(line, "[0/500] a.csv");
    // Last item: the run is over, an ETA would be noise.
    assert_eq!(
        item_line(500, Some(500), "z.csv", Duration::from_secs(10)),
        "[500/500] z.csv"
    );
}

#[test]
fn the_eta_extrapolates_from_what_has_run() {
    // 10 of 100 in 10s means 1s each and 90 to go.
    let line = item_line(10, Some(100), "a.csv", Duration::from_secs(10));
    assert_eq!(line, "[10/100] a.csv  ETA 1:30");
}

#[test]
fn an_unknown_total_just_counts() {
    assert_eq!(
        item_line(7, None, "a.csv", Duration::from_secs(3)),
        "[7] a.csv"
    );
}

#[test]
fn the_clock_grows_an_hours_field_only_when_it_needs_one() {
    assert_eq!(clock(Duration::from_secs(9)), "0:09");
    assert_eq!(clock(Duration::from_secs(600)), "10:00");
    assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
}
