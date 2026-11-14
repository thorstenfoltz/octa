use super::*;

fn cfg(model: &str, reasoning: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        model: model.into(),
        base_url: None,
        api_key: String::new(),
        temperature: None,
        max_tokens: None,
        reasoning: reasoning.map(str::to_string),
        verbosity: None,
        pro_mode: false,
    }
}

#[test]
fn nothing_is_allowed_and_the_prompt_is_not_an_argument() {
    let a = args(&cfg("", None), Path::new("/tmp/x"));
    assert!(
        !a.iter()
            .any(|s| s.contains("allow") || s == "--yolo" || s == "--add-dir"),
        "no option that lets the program act: {a:?}"
    );
    assert!(
        !a.iter().any(|s| s == "-p" || s == "--prompt"),
        "the prompt goes in on stdin"
    );
    let dir = a.iter().position(|s| s == "-C").unwrap();
    assert_eq!(a[dir + 1], "/tmp/x");
    assert!(a.iter().any(|s| s == "-s"), "the answer alone");
    assert!(!a.iter().any(|s| s == "--model"));
}

#[test]
fn model_and_effort_from_the_profile_are_passed_on() {
    let a = args(&cfg("gpt-6", Some("high")), Path::new("/tmp/x"));
    let m = a.iter().position(|s| s == "--model").unwrap();
    assert_eq!(a[m + 1], "gpt-6");
    let e = a.iter().position(|s| s == "--reasoning-effort").unwrap();
    assert_eq!(a[e + 1], "high");
}
