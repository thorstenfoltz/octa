use super::*;
use octa::ui::settings::{AppSettings, MIN_STATUS_MESSAGE_SECS};

/// Every message now gets the same span. Failures used to get 60s against
/// 10s for confirmations so an error could be read and copied; that is the
/// hover pause's job now, and the split is gone deliberately. This test
/// exists so bringing it back is a conscious act, not a quiet regression.
#[test]
fn every_message_gets_the_same_lifetime() {
    let s = AppSettings::default();
    assert_eq!(s.status_message_duration().as_secs(), 10);
}

/// The setting has no "off": a message that never expires covers the
/// status bar until restart.
#[test]
fn the_timeout_cannot_be_disabled() {
    for secs in [0, 1, 2] {
        let s = AppSettings {
            status_message_secs: secs,
            ..Default::default()
        };
        assert_eq!(
            s.status_message_duration().as_secs(),
            MIN_STATUS_MESSAGE_SECS,
            "{secs}s should be floored"
        );
    }
}

/// A longer value is honoured as written.
#[test]
fn a_longer_timeout_is_kept() {
    let s = AppSettings {
        status_message_secs: 45,
        ..Default::default()
    };
    assert_eq!(s.status_message_duration().as_secs(), 45);
}

/// The colour still depends on the message; only the lifetime is uniform.
#[test]
fn colour_classification_is_unchanged() {
    assert!(is_success_message("Saved data.csv"));
    assert!(!is_success_message("Could not save"));
}
