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
    let defaults = AppSettings::default();
    assert_eq!(settings.font_size, 10.0);
    assert_eq!(settings.default_theme, defaults.default_theme);
    assert_eq!(settings.icon_variant, defaults.icon_variant);
    assert_eq!(settings.show_row_numbers, defaults.show_row_numbers);
    assert_eq!(
        settings.show_sequential_row_numbers,
        defaults.show_sequential_row_numbers
    );
    assert_eq!(
        settings.sql_default_row_limit,
        defaults.sql_default_row_limit
    );
    assert_eq!(settings.start_maximized, defaults.start_maximized);
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
