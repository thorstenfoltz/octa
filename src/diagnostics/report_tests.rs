use super::*;

#[test]
fn redact_masks_home_user_and_keys() {
    let text = "path /home/alice/data.csv key sk-ABCDEFGHIJKLMNOPQRSTUVWX user alice";
    let out = redact(text, Some("/home/alice"), Some("alice"));
    assert!(out.contains('~'), "home should be masked: {out}");
    assert!(out.contains("%USER%"), "user should be masked: {out}");
    assert!(out.contains("sk-REDACTED"), "key should be masked: {out}");
    assert!(!out.contains("alice"), "username leaked: {out}");
    assert!(
        !out.contains("ABCDEFGHIJKLMNOPQRSTUVWX"),
        "key leaked: {out}"
    );
}

#[test]
fn build_report_includes_meta_and_crash() {
    let meta = ReportMeta {
        version: "9.9.9".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        store_packaged: false,
        locale: "en".into(),
        theme: "Light".into(),
        timestamp: "2026-06-20T10:00:00".into(),
    };
    let out = build_report(&meta, "chat_api_keys = {}", "all good", Some("boom at x"));
    assert!(out.contains("9.9.9"));
    assert!(out.contains("linux"));
    assert!(out.contains("## Last crash"));
    assert!(out.contains("boom at x"));
    assert!(out.contains("all good"));
}
