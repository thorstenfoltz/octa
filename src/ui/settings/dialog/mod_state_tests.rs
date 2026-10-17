use super::*;
use crate::ui::settings::AppSettings;

#[test]
fn apply_keeps_settings_written_outside_the_dialog() {
    // The dialog opened, then the sidebar cleared a saved cloud secret and a
    // tab got pinned. Applying must not resurrect the secret or drop the pin.
    let mut dialog = SettingsDialog::default();
    dialog.open(&AppSettings::default());
    dialog
        .draft
        .cloud_secrets
        .insert("conn".into(), "sekrit".into());
    dialog
        .seed
        .cloud_secrets
        .insert("conn".into(), "sekrit".into());

    let mut live = dialog.seed.clone();
    live.cloud_secrets.remove("conn");
    live.pinned_tabs.push("/data/sales.parquet".into());

    let mut applied = dialog.draft.clone();
    dialog.carry_external_edits(&mut applied, &live);

    assert!(applied.cloud_secrets.is_empty(), "cleared secret came back");
    assert_eq!(applied.pinned_tabs, live.pinned_tabs, "pin was reverted");
}

#[test]
fn apply_still_wins_for_fields_the_dialog_changed() {
    // Same contested field, but this time the user edited it in the dialog:
    // their choice must survive whatever the live settings hold.
    let mut dialog = SettingsDialog::default();
    dialog.open(&AppSettings::default());
    dialog.draft.show_readonly_notice = false;

    let mut live = dialog.seed.clone();
    live.show_readonly_notice = true;

    let mut applied = dialog.draft.clone();
    dialog.carry_external_edits(&mut applied, &live);

    assert!(
        !applied.show_readonly_notice,
        "the dialog's own edit was lost"
    );
}

#[test]
fn reset_to_defaults_keeps_connections_and_secrets() {
    let mut settings = AppSettings {
        font_size: 22.0,
        grep_max_file_size_mb: 999,
        ..Default::default()
    };
    settings.cloud_secrets.insert("s3".into(), "sekrit".into());
    settings.pinned_tabs.push("/data/sales.parquet".into());

    let mut dialog = SettingsDialog::default();
    dialog.open(&settings);
    dialog.reset_draft();

    assert_eq!(dialog.draft.font_size, AppSettings::default().font_size);
    assert_eq!(
        dialog.draft.cloud_secrets.get("s3").map(String::as_str),
        Some("sekrit"),
        "a reset must not orphan the keyring entry it cannot restore"
    );
    assert_eq!(dialog.draft.pinned_tabs.len(), 1);
}

// Lives here rather than in `settings/mod_tests.rs` because it reads
// `SettingsDialog`'s private buffer fields, which are visible only inside
// this module and its children.
#[test]
fn reset_to_defaults_re_seeds_every_buffer() {
    // Apply parses all the text buffers back over the draft, so any buffer
    // the reset forgets silently restores the old value.
    let settings = AppSettings {
        grep_max_file_size_mb: 999,
        excel_max_auto_sheets: 42,
        auto_save_interval_minutes: 17,
        ..Default::default()
    };

    let mut dialog = SettingsDialog::default();
    dialog.open(&settings);
    assert_eq!(dialog.grep_max_file_size_buf, "999");
    dialog.reset_draft();

    let d = AppSettings::default();
    assert_eq!(
        dialog.grep_max_file_size_buf,
        d.grep_max_file_size_mb.to_string()
    );
    assert_eq!(
        dialog.excel_max_auto_sheets_buf,
        d.excel_max_auto_sheets.to_string()
    );
    assert_eq!(
        dialog.auto_save_interval_buf,
        d.auto_save_interval_minutes.to_string()
    );
}
