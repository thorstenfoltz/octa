//! Unit tests for [`chat_models`](chat_models). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

fn old_anthropic_config() -> ChatModelsConfig {
    // A models.toml seeded by a release that predates claude-fable-5,
    // with one user-added custom model and a user-chosen default.
    let mut providers = BTreeMap::new();
    providers.insert(
        "anthropic".to_string(),
        ProviderModels {
            default: "my-custom-default".to_string(),
            models: vec![
                "claude-sonnet-4-6".to_string(),
                "claude-opus-4-8".to_string(),
                "my-custom-model".to_string(),
            ],
        },
    );
    ChatModelsConfig { providers }
}

#[test]
fn merge_adds_new_builtin_models_without_touching_user_entries() {
    let mut cfg = old_anthropic_config();
    assert!(merge_missing_presets(&mut cfg));
    let anthropic = &cfg.providers["anthropic"];
    // User entries keep their order at the front.
    assert_eq!(anthropic.models[0], "claude-sonnet-4-6");
    assert_eq!(anthropic.models[1], "claude-opus-4-8");
    assert_eq!(anthropic.models[2], "my-custom-model");
    // New built-ins are appended.
    assert!(anthropic.models.iter().any(|m| m == "claude-fable-5"));
    assert!(
        anthropic
            .models
            .iter()
            .any(|m| m == "claude-haiku-4-5-20251001")
    );
    // The user's default is never touched.
    assert_eq!(anthropic.default, "my-custom-default");
}

#[test]
fn merge_seeds_missing_providers_wholesale() {
    let mut cfg = old_anthropic_config();
    merge_missing_presets(&mut cfg);
    for kind in ChatProviderKind::ALL {
        let entry = cfg
            .providers
            .get(kind.id())
            .unwrap_or_else(|| panic!("provider {} missing after merge", kind.id()));
        if kind.id() != "anthropic" {
            assert_eq!(entry.default, kind.default_model());
        }
    }
}

#[test]
fn merge_is_idempotent() {
    let mut cfg = old_anthropic_config();
    assert!(merge_missing_presets(&mut cfg));
    let snapshot = format!("{cfg:?}");
    assert!(
        !merge_missing_presets(&mut cfg),
        "second merge must add nothing"
    );
    assert_eq!(format!("{cfg:?}"), snapshot);
}
