use super::*;

#[test]
fn splits_qualified_names() {
    assert_eq!(
        split_qualified("public.orders"),
        (None, "public".to_string(), "orders".to_string())
    );
    assert_eq!(
        split_qualified("prod.public.orders"),
        (
            Some("prod".to_string()),
            "public".to_string(),
            "orders".to_string()
        )
    );
    // A bare name leaves the schema for the caller to fill in.
    assert_eq!(
        split_qualified("orders"),
        (None, String::new(), "orders".to_string())
    );
}

#[test]
fn extra_dots_go_to_the_catalog() {
    assert_eq!(
        split_qualified("a.b.c.d"),
        (Some("a.b".to_string()), "c".to_string(), "d".to_string())
    );
}
