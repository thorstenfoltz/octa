use super::*;

#[test]
fn ensure_profiles_seeds_one_from_legacy() {
    let mut s = AppSettings {
        chat_provider: ChatProviderKind::Anthropic,
        chat_temperature: 0.7,
        ..Default::default()
    };
    s.chat_models
        .insert("anthropic".into(), "claude-opus-4-8".into());
    assert!(s.chat_profiles.is_empty());

    ensure_profiles(&mut s);

    assert_eq!(s.chat_profiles.len(), 1);
    assert_eq!(s.chat_active_profile, "default");
    assert_eq!(s.chat_profiles[0].kind, ChatProviderKind::Anthropic);
    assert_eq!(s.chat_profiles[0].model, "claude-opus-4-8");
    assert_eq!(s.chat_profiles[0].temperature, Some(0.7));
}

#[test]
fn a_fresh_install_sends_no_temperature() {
    // Nothing configured, so the seed lands on the default model, and the
    // current Claude models answer 400 to any temperature at all.
    let mut s = AppSettings::default();
    ensure_profiles(&mut s);
    assert_eq!(
        s.chat_profiles[0].model,
        chat_models::default_model(s.chat_provider)
    );
    assert_eq!(s.chat_profiles[0].temperature, None);
}

#[test]
fn seed_carries_the_ollama_base_url() {
    let mut s = AppSettings {
        chat_provider: ChatProviderKind::Ollama,
        chat_ollama_url: "http://localhost:11434".into(),
        ..Default::default()
    };
    ensure_profiles(&mut s);
    assert_eq!(s.chat_profiles[0].base_url, "http://localhost:11434");
}

#[test]
fn ensure_profiles_is_idempotent() {
    let mut s = AppSettings::default();
    ensure_profiles(&mut s);
    let first = s.chat_profiles.clone();
    ensure_profiles(&mut s);
    ensure_profiles(&mut s);
    assert_eq!(s.chat_profiles, first);
}

#[test]
fn fresh_id_slugifies_and_dedupes() {
    let mut existing: Vec<ChatModelProfile> = Vec::new();
    let id = ChatModelProfile::fresh_id("Opus (deep)", &existing);
    assert_eq!(id, "opus-deep");

    existing.push(ChatModelProfile {
        id,
        name: "Opus (deep)".into(),
        description: String::new(),
        kind: ChatProviderKind::Anthropic,
        model: "claude-opus-4-8".into(),
        temperature: Some(0.0),
        reasoning: String::new(),
        verbosity: String::new(),
        pro_mode: false,
        base_url: String::new(),
        use_own_key: false,
        allow_writes: false,
    });

    // Same name again gets a distinct id, so the two profiles' own-keys
    // never collide in the keyring.
    assert_eq!(
        ChatModelProfile::fresh_id("Opus (deep)", &existing),
        "opus-deep-2"
    );
}

#[test]
fn fresh_id_survives_a_nameless_profile() {
    let existing: Vec<ChatModelProfile> = Vec::new();
    assert_eq!(ChatModelProfile::fresh_id("...", &existing), "profile");
}

#[test]
fn dangling_active_falls_back_to_first() {
    let mut s = AppSettings::default();
    ensure_profiles(&mut s);
    s.chat_active_profile = "deleted-profile".into();

    ensure_profiles(&mut s);

    assert_eq!(s.chat_active_profile, s.chat_profiles[0].id);
}

#[test]
fn settings_with_profiles_round_trip_through_toml() {
    // `chat_profiles` is a Vec<struct>, which TOML writes as an array of
    // tables. `AppSettings::save` swallows a serialisation error
    // (`if let Ok(..)`), so a struct-ordering mistake here would silently
    // stop settings being saved at all rather than failing loudly. Pin it.
    let mut s = AppSettings::default();
    ensure_profiles(&mut s);
    s.chat_profiles.push(ChatModelProfile {
        id: "opus-deep".into(),
        name: "Opus deep".into(),
        description: "for the hard questions".into(),
        kind: ChatProviderKind::Anthropic,
        model: "claude-opus-4-8".into(),
        temperature: Some(0.3),
        reasoning: "8000".into(),
        verbosity: String::new(),
        pro_mode: false,
        base_url: String::new(),
        use_own_key: true,
        allow_writes: true,
    });
    s.chat_active_profile = "opus-deep".into();

    let text = toml::to_string_pretty(&s).expect("settings must serialise");
    let back: AppSettings = toml::from_str(&text).expect("settings must parse back");

    assert_eq!(back.chat_profiles.len(), 2);
    assert_eq!(back.chat_active_profile, "opus-deep");
    let p = back
        .chat_profiles
        .iter()
        .find(|p| p.id == "opus-deep")
        .expect("the profile survives the round trip");
    assert_eq!(p.kind, ChatProviderKind::Anthropic);
    assert_eq!(p.temperature, Some(0.3));
    assert_eq!(p.reasoning, "8000");
    assert_eq!(p.description, "for the hard questions");
    assert!(p.use_own_key);
    assert!(p.allow_writes);
}

#[test]
fn a_profile_without_temperature_round_trips_as_none() {
    // "Send no temperature at all" is the setting that makes Anthropic's
    // newer models usable. `AppSettings::save` swallows serialisation
    // errors, so a `None` the TOML serialiser choked on would silently stop
    // settings being written rather than fail loudly. Pin both directions.
    let mut s = AppSettings::default();
    ensure_profiles(&mut s);
    s.chat_profiles.push(ChatModelProfile {
        id: "opus-5".into(),
        name: "Opus 5".into(),
        description: String::new(),
        kind: ChatProviderKind::Anthropic,
        model: "claude-opus-5".into(),
        temperature: None,
        reasoning: String::new(),
        verbosity: String::new(),
        pro_mode: false,
        base_url: String::new(),
        use_own_key: false,
        allow_writes: false,
    });

    let text = toml::to_string_pretty(&s).expect("settings must serialise");
    let back: AppSettings = toml::from_str(&text).expect("settings must parse back");

    let p = back
        .chat_profiles
        .iter()
        .find(|p| p.id == "opus-5")
        .expect("the profile survives the round trip");
    assert_eq!(p.temperature, None);

    // An older settings.toml wrote a bare number; it must still load.
    let legacy: AppSettings = toml::from_str(
        r#"
            [[chat_profiles]]
            id = "old"
            name = "Old"
            kind = "Anthropic"
            model = "claude-opus-4-8"
            temperature = 0.4
            "#,
    )
    .expect("legacy profile parses");
    assert_eq!(legacy.chat_profiles[0].temperature, Some(0.4));
    // Verbosity and Pro mode postdate this file shape. They must default
    // to off rather than failing the parse and losing every profile.
    assert_eq!(legacy.chat_profiles[0].verbosity, "");
    assert!(!legacy.chat_profiles[0].pro_mode);
}

#[test]
fn legacy_seed_inherits_the_global_write_switch() {
    // The first migration keeps the behaviour the install already had:
    // write protection OFF meant the assistant could write, so the seeded
    // profile allows writes; protection ON seeds a read-only profile.
    for (write_protection, expect_allow) in [(false, true), (true, false)] {
        let mut s = AppSettings {
            write_protection,
            ..Default::default()
        };
        ensure_profiles(&mut s);
        assert_eq!(s.chat_profiles[0].allow_writes, expect_allow);
    }
}

#[test]
fn legacy_settings_without_profiles_still_parse() {
    // An existing settings.toml has no [[chat_profiles]] at all. It must
    // load (serde default) and then migrate, not error out into defaults.
    let legacy = r#"
            chat_provider = "OpenAi"
            chat_temperature = 0.5
        "#;
    let mut s: AppSettings = toml::from_str(legacy).expect("legacy settings parse");
    assert!(s.chat_profiles.is_empty());

    ensure_profiles(&mut s);

    assert_eq!(s.chat_profiles.len(), 1);
    assert_eq!(s.chat_profiles[0].kind, ChatProviderKind::OpenAi);
    assert_eq!(s.chat_profiles[0].temperature, Some(0.5));
}

#[test]
fn active_profile_resolves_the_selected_one() {
    let mut s = AppSettings::default();
    ensure_profiles(&mut s);
    s.chat_profiles.push(ChatModelProfile {
        id: "sonnet-quick".into(),
        name: "Sonnet quick".into(),
        description: String::new(),
        kind: ChatProviderKind::Anthropic,
        model: "claude-sonnet-5".into(),
        temperature: Some(0.2),
        reasoning: String::new(),
        verbosity: String::new(),
        pro_mode: false,
        base_url: String::new(),
        use_own_key: false,
        allow_writes: false,
    });
    s.chat_active_profile = "sonnet-quick".into();

    let p = active_profile(&s);

    assert_eq!(p.name, "Sonnet quick");
    assert_eq!(p.model, "claude-sonnet-5");
}
