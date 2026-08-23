//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn partial_toml_loads_with_defaults_for_missing_fields() {
    // Writing only `font_size` should still deserialize cleanly: every
    // other field is filled from `AppSettings::default()` thanks to the
    // struct-level `#[serde(default)]`. This is the upgrade-survivability
    // contract.
    let partial = "font_size = 10.0\n";
    let settings: AppSettings = toml::from_str(partial).expect("partial TOML must deserialize");
    assert_eq!(settings.font_size, 10.0);

    // Every OTHER field, not a hand-picked handful: a field-level
    // `#[serde(default = ...)]` that disagrees with `AppSettings::default()`
    // gives upgrading users and fresh installs different values forever, and
    // spot-checks are exactly how two of them went unnoticed.
    let loaded = toml::Table::try_from(&settings).expect("settings serialize");
    let defaults = toml::Table::try_from(AppSettings::default()).expect("defaults serialize");
    for (key, want) in &defaults {
        if key == "font_size" {
            continue;
        }
        assert_eq!(
            loaded.get(key),
            Some(want),
            "`{key}` did not come from AppSettings::default()"
        );
    }
}

#[test]
fn unknown_fields_are_silently_ignored() {
    // A field this binary doesn't know about (e.g. left over from a future
    // release downgraded back to the current one) must not blow up the
    // whole config - just skip it.
    let with_unknown = "font_size = 11.0\nmysterious_future_field = \"hi\"\n";
    let settings: AppSettings =
        toml::from_str(with_unknown).expect("unknown fields should be tolerated");
    assert_eq!(settings.font_size, 11.0);
}

#[test]
fn defaults_round_trip_through_toml() {
    let defaults = AppSettings::default();
    let serialized = toml::to_string_pretty(&defaults).expect("serialize");
    let parsed: AppSettings = toml::from_str(&serialized).expect("round-trip");
    assert_eq!(parsed.font_size, defaults.font_size);
    assert_eq!(parsed.default_theme, defaults.default_theme);
    assert_eq!(parsed.icon_variant, defaults.icon_variant);
    assert_eq!(parsed.start_maximized, defaults.start_maximized);
    // Chat settings survive the round-trip too.
    assert_eq!(parsed.chat_provider, defaults.chat_provider);
    assert_eq!(parsed.chat_panel_position, defaults.chat_panel_position);
    assert_eq!(parsed.chat_temperature, defaults.chat_temperature);
    assert_eq!(
        parsed.chat_max_tool_iterations,
        defaults.chat_max_tool_iterations
    );
    assert_eq!(parsed.chat_max_tokens, defaults.chat_max_tokens);
    assert_eq!(
        parsed.chat_max_tokens_unlimited,
        defaults.chat_max_tokens_unlimited
    );
    assert_eq!(parsed.chat_export_dir, defaults.chat_export_dir);
    assert_eq!(parsed.chat_models, defaults.chat_models);
    assert_eq!(parsed.chat_api_keys, defaults.chat_api_keys);
}

#[cfg(unix)]
#[test]
fn restrict_file_to_owner_sets_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secrets.toml");
    std::fs::write(&path, "x = 1\n").expect("write");
    restrict_file_to_owner(&path);
    let mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[cfg(unix)]
#[test]
fn restrict_dir_to_owner_sets_0700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("chat_sessions");
    std::fs::create_dir_all(&sub).expect("create dir");
    restrict_dir_to_owner(&sub);
    let mode = std::fs::metadata(&sub)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn folder_union_cap_honours_unlimited() {
    // The cap is fed straight to `Vec::truncate`, so "unlimited" has to come
    // back as a value that truncates nothing.
    let mut s = AppSettings::default();
    assert_eq!(s.folder_union_cap(), 500);
    s.folder_union_max_files = 12;
    assert_eq!(s.folder_union_cap(), 12);
    s.folder_union_max_files_unlimited = true;
    assert_eq!(s.folder_union_cap(), usize::MAX);
}

#[test]
fn grep_max_file_bytes_honours_unlimited_and_legacy_zero() {
    // The scan worker's contract is "0 bytes = no cap", so both the checkbox
    // and a legacy `grep_max_file_size_mb = 0` have to arrive there as 0.
    let mut s = AppSettings::default();
    assert_eq!(s.grep_max_file_bytes(), 50 * 1024 * 1024);
    s.grep_max_file_size_mb = 0;
    assert_eq!(s.grep_max_file_bytes(), 0);
    s.grep_max_file_size_mb = 8;
    s.grep_max_file_size_unlimited = true;
    assert_eq!(s.grep_max_file_bytes(), 0);
}

#[test]
fn chat_provider_ids_are_stable_and_distinct() {
    // The ids key persisted maps and the keyring entry; they must stay
    // unique and must not change silently.
    let ids: Vec<&str> = ChatProviderKind::ALL.iter().map(|p| p.id()).collect();
    assert_eq!(
        ids,
        ["ollama", "anthropic", "openai", "openai_compat", "gemini"]
    );
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "provider ids must be distinct");
}

#[test]
fn mcp_unlimited_row_limit_survives_a_save_and_load() {
    // `None` means "no default cap". Serde omits a `None` field, and the
    // omitted key then re-reads through `default_mcp_row_limit` as Some(1000),
    // so ticking Settings -> MCP -> Unlimited silently reverted on restart.
    // It is persisted as `0` instead, which is also what a per-call `limit: 0`
    // means.
    let mut s = AppSettings {
        mcp_default_row_limit: None,
        ..Default::default()
    };
    let text = toml::to_string_pretty(&s).expect("serialize");
    assert!(
        text.contains("mcp_default_row_limit = 0"),
        "unlimited must be written as an explicit 0, got:\n{text}"
    );
    let back: AppSettings = toml::from_str(&text).expect("round-trip");
    assert_eq!(back.mcp_default_row_limit, None);

    // A real cap still round-trips as itself, and an absent key still falls
    // back to the 1000-row default.
    s.mcp_default_row_limit = Some(5000);
    let text = toml::to_string_pretty(&s).expect("serialize");
    let back: AppSettings = toml::from_str(&text).expect("round-trip");
    assert_eq!(back.mcp_default_row_limit, Some(5000));

    let bare: AppSettings = toml::from_str("font_size = 13.0").expect("partial settings parse");
    assert_eq!(bare.mcp_default_row_limit, Some(1000));
}

/// The Store's generative-AI clause needs every provider to lead somewhere:
/// a hosted one to its own support page, a local/custom one to prose the
/// dialog writes instead. A new `ChatProviderKind` must pick a side.
#[test]
fn every_provider_has_a_content_report_route() {
    for kind in ChatProviderKind::ALL {
        match kind.report_url() {
            Some(url) => assert!(
                url.starts_with("https://"),
                "{kind:?} report URL must be https, got {url:?}"
            ),
            // Nothing hosted stands behind these two, so the dialog names the
            // local model / the user's own endpoint rather than linking out.
            None => assert!(
                matches!(
                    kind,
                    ChatProviderKind::Ollama | ChatProviderKind::OpenAiCompatible
                ),
                "{kind:?} is a hosted provider, so it needs a report URL"
            ),
        }
    }
}

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
fn an_empty_environment_variable_is_not_a_config_dir() {
    // On Unix an exported-but-empty variable reads as Ok(""), and
    // PathBuf::from("").join("octa") is the relative path `octa` - which put
    // settings.toml, plaintext secrets and all, in the working directory.
    assert_eq!(non_empty_path(None), None);
    assert_eq!(non_empty_path(Some(String::new())), None);
    assert_eq!(non_empty_path(Some("   ".into())), None);
    assert_eq!(
        non_empty_path(Some("/home/someone/.config".into())),
        Some(std::path::PathBuf::from("/home/someone/.config"))
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

#[test]
fn size_unit_covers_gigabytes() {
    assert_eq!(SizeUnit::GB.factor(), 1_024 * 1_024 * 1_024);
    assert_eq!(SizeUnit::best_fit(10 * 1_024 * 1_024 * 1_024), SizeUnit::GB);
    assert_eq!(SizeUnit::best_fit(5 * 1_024 * 1_024), SizeUnit::MB);
    assert_eq!(SizeUnit::best_fit(1_500), SizeUnit::Bytes);
    assert!(SizeUnit::ALL.contains(&SizeUnit::GB));
}
