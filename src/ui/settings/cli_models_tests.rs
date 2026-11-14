use super::*;

#[test]
fn codex_lists_only_the_models_its_own_picker_shows() {
    let json = r#"{"models":[
        {"slug":"gpt-reserve","visibility":"hide"},
        {"slug":"gpt-6-luna","visibility":"list"},
        {"slug":"gpt-5.6-terra","visibility":"list"}
    ]}"#;
    assert_eq!(parse_codex(json), ["gpt-6-luna", "gpt-5.6-terra"]);
}

#[test]
fn codex_output_that_is_not_the_catalogue_gives_no_models() {
    assert!(parse_codex("error: not found").is_empty());
    assert!(parse_codex("{}").is_empty());
}

#[test]
fn copilot_models_are_the_quoted_names_under_the_model_setting() {
    let help = "  `defaultMode`: agent mode.\n    - One of \"interactive\"\n\n  `model`: AI model to use for Copilot CLI; can be changed.\n    - \"claude-sonnet-5.5\"\n    - \"gpt-6.1-sol\"\n\n  `contextTier`: context window tier.\n    - \"default\"\n";
    assert_eq!(
        parse_copilot(help),
        ["auto", "claude-sonnet-5.5", "gpt-6.1-sol"]
    );
}

#[test]
fn copilot_help_without_a_model_list_gives_no_models() {
    assert!(parse_copilot("Usage: copilot [OPTIONS]").is_empty());
}

#[test]
fn a_provider_that_is_no_program_has_no_list() {
    let ctx = egui::Context::default();
    assert!(models(&ctx, ChatProviderKind::Anthropic, "").is_empty());
}
