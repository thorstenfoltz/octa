use super::should_show;

#[test]
fn a_fresh_install_sees_its_notes() {
    assert!(should_show(true, "", "0.17.1"));
}

#[test]
fn an_acknowledged_version_stays_silent() {
    assert!(!should_show(true, "0.17.1", "0.17.1"));
}

#[test]
fn the_next_release_opens_the_window_again() {
    assert!(should_show(true, "0.17.1", "0.17.2"));
}

#[test]
fn the_setting_wins_over_everything() {
    assert!(!should_show(false, "", "0.17.1"));
}

#[test]
fn a_dev_build_stays_silent() {
    assert!(!should_show(true, "", "0.0.0-dev"));
}
