use super::*;

#[test]
fn tags_are_listed_newest_first() {
    let body = serde_json::json!({"models": [
        {"name": "llama3.2", "modified_at": "2026-01-04T10:00:00Z"},
        {"name": "qwen3", "modified_at": "2026-08-30T09:00:00Z"},
        {"name": "mistral", "modified_at": "2026-03-12T22:00:00Z"},
    ]});
    assert_eq!(names_newest_first(&body), ["qwen3", "mistral", "llama3.2"]);
}

#[test]
fn a_tag_without_a_timestamp_sorts_last_but_is_kept() {
    // Better a model at the bottom of the list than one the user pulled
    // and cannot find in the dropdown at all.
    let body = serde_json::json!({"models": [
        {"name": "no-date"},
        {"name": "dated", "modified_at": "2026-08-30T09:00:00Z"},
    ]});
    assert_eq!(names_newest_first(&body), ["dated", "no-date"]);
}

#[test]
fn an_empty_or_odd_body_is_an_empty_list() {
    for body in [serde_json::json!({}), serde_json::json!({"models": []})] {
        assert!(names_newest_first(&body).is_empty());
    }
}
