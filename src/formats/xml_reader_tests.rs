use super::*;

fn texts(xml: &str) -> Vec<(String, String)> {
    let tree = parse_tree(xml).unwrap();
    tree[0].children[0]
        .children
        .iter()
        .map(|c| (c.name.clone(), c.text.clone()))
        .collect()
}

/// quick-xml hands `&amp;` over as an event of its own; it used to be
/// dropped, so `Tom &amp; Jerry` read as `TomJerry`.
#[test]
fn references_and_cdata_keep_their_text() {
    let got = texts(
        "<rows><row><name>Tom &amp; Jerry</name><code>&#65;BC</code>\
         <html><![CDATA[a<b]]></html><raw>&nbsp;x</raw><pad>  y  </pad></row></rows>",
    );
    let want = [
        ("name", "Tom & Jerry"),
        ("code", "ABC"),
        ("html", "a<b"),
        ("raw", "&nbsp;x"),
        ("pad", "y"),
    ];
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn attribute_values_are_unescaped() {
    let tree = parse_tree(r#"<r><c name="Doe &amp; Co"/></r>"#).unwrap();
    assert_eq!(tree[0].children[0].attributes[0].1, "Doe & Co");
    // An HTML entity is not XML's: it stays as written.
    let tree = parse_tree(r#"<r><c name="a&nbsp;b"/></r>"#).unwrap();
    assert_eq!(tree[0].children[0].attributes[0].1, "a&nbsp;b");
}
