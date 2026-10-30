use super::*;

#[test]
fn panel_positions_default_and_round_trip_through_toml() {
    let s = AppSettings::default();
    assert_eq!(s.chat_panel_position, PanelPosition::Right);
    assert_eq!(s.column_navigator_position, PanelPosition::Left);
    assert_eq!(s.edit_audit_position, PanelPosition::Bottom);

    // An existing settings file written before the rename still loads: the
    // variant names are what serialise, and they have not changed.
    let toml = r#"chat_panel_position = "Bottom""#;
    let loaded: AppSettings = toml::from_str(toml).expect("parses");
    assert_eq!(loaded.chat_panel_position, PanelPosition::Bottom);
}

#[test]
fn folder_compare_cap_defaults_to_100k_and_unlimited_lifts_it() {
    let mut s = AppSettings::default();
    assert_eq!(s.folder_compare_max_files, 100_000);
    assert_eq!(s.folder_compare_cap(), 100_000);
    // A file written before the field existed gets the same default.
    let loaded: AppSettings = toml::from_str("").expect("parses");
    assert_eq!(loaded.folder_compare_max_files, 100_000);
    s.folder_compare_max_files_unlimited = true;
    assert_eq!(s.folder_compare_cap(), usize::MAX);
}

#[test]
fn time_settings_default_on_and_clock() {
    let s = AppSettings::default();
    assert!(s.detect_times_on_open);
    assert_eq!(s.duration_style, crate::data::times::DurationStyle::Clock);
    let old: AppSettings = toml::from_str("").unwrap();
    assert!(
        old.detect_times_on_open,
        "a settings.toml from before keeps detection on"
    );
}

#[test]
fn zoom_step_defaults_to_5_and_stays_between_1_and_25() {
    assert_eq!(AppSettings::default().zoom_step(), 5);
    let old: AppSettings = toml::from_str("").unwrap();
    assert_eq!(old.zoom_step(), 5, "a settings.toml from before keeps 5");
    for (written, used) in [(0, 1), (12, 12), (99, 25)] {
        let s: AppSettings = toml::from_str(&format!("zoom_step = {written}")).unwrap();
        assert_eq!(s.zoom_step(), used, "zoom_step = {written}");
    }
}
