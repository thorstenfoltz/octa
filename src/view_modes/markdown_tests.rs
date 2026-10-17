use super::*;

fn run(text: &str, link: Option<&str>) -> (String, RunStyle) {
    (
        text.to_string(),
        RunStyle {
            link: link.map(|s| s.to_string()),
            ..Default::default()
        },
    )
}

#[test]
fn link_spans_finds_char_ranges() {
    // "foo " | "bar"(link) | " baz"  ->  link covers chars 4..7.
    let runs = vec![
        run("foo ", None),
        run("bar", Some("https://example.com")),
        run(" baz", None),
    ];
    let spans = link_spans(&runs);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].0, 4..7);
    assert_eq!(spans[0].1, "https://example.com");
}

#[test]
fn link_spans_empty_when_no_links() {
    let runs = vec![run("plain text", None)];
    assert!(link_spans(&runs).is_empty());
}
