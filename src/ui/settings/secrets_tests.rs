use super::*;

/// These exercise the plaintext leg of the chain, the leg the fallback
/// guarantees.
///
/// They must never assert on a value that `get_key_for` resolves for a
/// *real* key id. The keyring is NOT reliably absent under `cargo test`: on
/// a developer machine with a running Secret Service, `chat.anthropic.api_key`
/// exists, wins the precedence chain over the plaintext fixture, and the
/// assertion diff then prints the developer's live API key to stdout. Use a
/// synthetic id no keyring will hold, or assert on `chat_api_keys` directly.
#[test]
fn a_key_id_resolves_from_the_plaintext_map() {
    let mut s = AppSettings::default();
    s.chat_api_keys
        .insert("profile.opus-deep".into(), "sk-test".into());

    assert_eq!(
        get_key_for("profile.opus-deep", None, &s).as_deref(),
        Some("sk-test")
    );
    assert!(get_key_for("profile.missing", None, &s).is_none());
}

#[test]
fn profile_keys_are_namespaced_away_from_provider_keys() {
    // A profile the user names "Anthropic" slugs to the id `anthropic`,
    // which is also the provider id. The two keys must not be the same
    // entry, or saving one would silently overwrite the other.
    assert_eq!(profile_key_id("anthropic"), "profile.anthropic");
    assert_ne!(
        profile_key_id("anthropic"),
        ChatProviderKind::Anthropic.id()
    );

    let mut s = AppSettings::default();
    s.chat_api_keys
        .insert(ChatProviderKind::Anthropic.id().into(), "shared".into());
    s.chat_api_keys
        .insert(profile_key_id("anthropic"), "own".into());

    // Asserted against the map rather than through get_api_key /
    // get_profile_key on purpose: those consult the OS keyring first, and
    // `anthropic` is a real provider id that a developer machine genuinely
    // has stored. Going through them makes the test depend on the machine
    // and prints a live secret when it fails. The property under test is
    // that the two ids address two separate entries, which the map shows
    // directly; the getters' precedence chain is covered above with a
    // synthetic id.
    assert_eq!(
        s.chat_api_keys
            .get(ChatProviderKind::Anthropic.id())
            .map(String::as_str),
        Some("shared")
    );
    assert_eq!(
        s.chat_api_keys
            .get(&profile_key_id("anthropic"))
            .map(String::as_str),
        Some("own")
    );
}

#[test]
fn deleting_a_profile_key_leaves_the_provider_key_alone() {
    let mut s = AppSettings::default();
    s.chat_api_keys
        .insert(ChatProviderKind::Anthropic.id().into(), "shared".into());
    s.chat_api_keys
        .insert(profile_key_id("opus-deep"), "own".into());

    delete_profile_key("opus-deep", &mut s);

    // `opus-deep` is synthetic, so the getter is safe to use here: no
    // keyring holds `chat.profile.opus-deep.api_key`. The provider key is
    // checked against the map for the reason given in the test above.
    assert!(get_profile_key("opus-deep", &s).is_none());
    assert_eq!(
        s.chat_api_keys
            .get(ChatProviderKind::Anthropic.id())
            .map(String::as_str),
        Some("shared")
    );
}

#[test]
fn storage_reports_none_when_nothing_is_set() {
    let s = AppSettings::default();
    assert_eq!(key_storage_for("profile.nope", None, &s), KeyStorage::None);
}
