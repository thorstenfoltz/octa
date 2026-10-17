use super::*;

#[test]
fn the_greeting_starts_on_first_use_and_then_expires() {
    // An unset deadline means "this session has not shown it yet": the
    // first call starts the clock and paints.
    let mut deadline = None;
    assert!(festive_intro_running(&mut deadline));
    assert!(deadline.is_some(), "the first call must arm the deadline");

    // Still inside the window a moment later: the same deadline is
    // reused, never pushed forward, or the greeting would never end.
    let armed = deadline;
    assert!(festive_intro_running(&mut deadline));
    assert_eq!(deadline, armed);

    // Past it, the caller returns before painting and before asking for
    // another frame, which is what lets the app go idle again.
    let mut spent = Some(Instant::now() - Duration::from_secs(1));
    assert!(!festive_intro_running(&mut spent));
}

#[test]
fn the_two_holidays_keep_separate_deadlines() {
    // A session running from Dec 26 into Dec 31 must still get the
    // fireworks: spending the Christmas clock cannot spend this one.
    let mut christmas = Some(Instant::now() - Duration::from_secs(1));
    let mut new_year = None;
    assert!(!festive_intro_running(&mut christmas));
    assert!(festive_intro_running(&mut new_year));
}
