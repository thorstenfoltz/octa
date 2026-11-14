use super::*;
use crate::ui::settings::AppSettings;
use crate::ui::settings::chat_profiles::seed_profile_from_legacy;

/// Edit shows a profile's own limits and folder, and saving the form unchanged
/// gives the same profile back: the thousands separator the form writes is
/// read again, and an empty field stays "use the general value".
#[test]
fn the_form_keeps_a_profiles_own_limits_and_folder() {
    let mut dialog = SettingsDialog::default();
    dialog.open(&AppSettings::default());
    let own = ChatModelProfile {
        max_tokens: Some(32_000),
        max_tool_iterations: Some(5),
        result_row_limit: Some(1_500),
        unlimited_rows: true,
        export_dir: "/data/exports".to_string(),
        ..seed_profile_from_legacy(&AppSettings::default())
    };
    let plain = ChatModelProfile {
        id: "plain".to_string(),
        ..seed_profile_from_legacy(&AppSettings::default())
    };
    dialog.draft.chat_profiles = vec![own.clone(), plain.clone()];

    dialog.load_chat_profile_form(0);
    assert_eq!(dialog.chat_profile_form_max_tokens, "32,000");
    assert_eq!(dialog.profile_from_form(), own);

    dialog.load_chat_profile_form(1);
    assert!(dialog.chat_profile_form_row_limit.is_empty());
    assert_eq!(dialog.profile_from_form(), plain);

    // Many rounds are allowed, but not without end.
    dialog.chat_profile_form_max_iterations = "400".to_string();
    assert_eq!(dialog.profile_from_form().max_tool_iterations, Some(400));
    dialog.chat_profile_form_max_iterations = "5,000".to_string();
    assert_eq!(dialog.profile_from_form().max_tool_iterations, Some(1000));
}
