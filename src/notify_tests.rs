use super::*;
use std::time::Duration;

#[test]
fn notifies_only_when_on_long_enough_and_in_the_background() {
    let long = Duration::from_secs(6);
    assert!(should_notify(true, 5, long, Some(false), false));
    assert!(
        !should_notify(false, 5, long, Some(false), false),
        "setting off"
    );
    assert!(
        !should_notify(true, 5, Duration::from_secs(4), Some(false), false),
        "too short"
    );
    assert!(
        !should_notify(true, 5, long, Some(true), false),
        "Octa is in front"
    );
    assert!(
        !should_notify(true, 5, long, None, false),
        "unknown focus counts as in front"
    );
    assert!(
        should_notify(true, 5, Duration::from_secs(5), Some(false), false),
        "exactly the threshold"
    );
}

#[test]
fn the_in_front_switch_notifies_whatever_the_focus() {
    let long = Duration::from_secs(6);
    for focused in [Some(true), Some(false), None] {
        assert!(should_notify(true, 5, long, focused, true), "{focused:?}");
    }
    assert!(
        !should_notify(false, 5, long, Some(true), true),
        "setting off"
    );
    assert!(
        !should_notify(true, 5, Duration::from_secs(4), Some(true), true),
        "too short"
    );
}

#[test]
fn the_seconds_field_takes_whole_numbers_of_one_or_more() {
    assert_eq!(parse_after_secs("1"), Some(1));
    assert_eq!(parse_after_secs(" 5 "), Some(5));
    for bad in ["0", "-3", "abc", "2.5", ""] {
        assert_eq!(parse_after_secs(bad), None, "{bad:?}");
    }
}

#[test]
fn elapsed_time_reads_naturally() {
    assert_eq!(format_elapsed(Duration::from_secs(38)), "38 s");
    assert_eq!(format_elapsed(Duration::from_secs(252)), "4 min 12 s");
    assert_eq!(format_elapsed(Duration::from_secs(3_720)), "1 h 2 min");
}
