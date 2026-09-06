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
fn merge_leads_with_the_users_own_names_then_the_dated_ones() {
    let mut cfg = old_anthropic_config();
    assert!(merge_and_order_presets(&mut cfg));
    let models = &cfg.providers["anthropic"].models;
    let builtins = ChatProviderKind::Anthropic.preset_models();

    // The names Octa cannot date lead, in the order the file had them: a
    // model typed by hand is one the built-in list does not know yet, which
    // usually means newer than all of it. `claude-opus-4-8` is not among
    // them - it is a built-in, so it belongs to the dated block below.
    let split = models.len() - builtins.len();
    assert_eq!(models[..split], ["claude-sonnet-4-6", "my-custom-model"]);
    // Then the built-ins, in built-in order, which is newest release first.
    assert_eq!(models[split..], *builtins);
    // Nothing was duplicated on the way through.
    let mut seen = models.clone();
    seen.sort();
    let total = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), total, "merge must not duplicate a model");
    // The user's default is never touched.
    assert_eq!(cfg.providers["anthropic"].default, "my-custom-default");
}

#[test]
fn a_file_grown_by_accretion_comes_back_newest_first() {
    // The shape every real models.toml had: each release appended its new
    // models to the end, so the file listed the OLDEST model first and the
    // newest last. Ordering the built-in list alone never reached these
    // users - only a fresh install saw the intended order.
    let builtins = ChatProviderKind::Anthropic.preset_models();
    let mut providers = BTreeMap::new();
    providers.insert(
        "anthropic".to_string(),
        ProviderModels {
            default: String::new(),
            models: builtins.iter().rev().map(|m| (*m).to_string()).collect(),
        },
    );
    let mut cfg = ChatModelsConfig { providers };

    assert!(merge_and_order_presets(&mut cfg), "the order must be fixed");
    assert_eq!(cfg.providers["anthropic"].models, *builtins);
}

#[test]
fn merge_seeds_missing_providers_wholesale() {
    let mut cfg = old_anthropic_config();
    merge_and_order_presets(&mut cfg);
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
    assert!(merge_and_order_presets(&mut cfg));
    let snapshot = format!("{cfg:?}");
    assert!(
        !merge_and_order_presets(&mut cfg),
        "second merge must add nothing"
    );
    assert_eq!(format!("{cfg:?}"), snapshot);
}

/// A `models.toml` from a release that still wrote a `[prices]` table parses
/// after prices were removed: the unknown key is ignored, not an error.
#[test]
fn an_old_file_with_prices_still_loads() {
    let text = r#"
[providers.anthropic]
default = "claude-sonnet-4-5"
models = ["claude-sonnet-4-5"]

[providers.anthropic.prices."claude-sonnet-4-5"]
input_per_mtok = 3.0
output_per_mtok = 15.0
"#;
    let cfg: ChatModelsConfig = toml::from_str(text).expect("parse");
    assert_eq!(cfg.providers["anthropic"].models, ["claude-sonnet-4-5"]);
}
