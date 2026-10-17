use super::*;

#[test]
fn params_accept_a_catalog() {
    let p: Params = serde_json::from_value(json!({
        "connection": "wh",
        "catalog": "sales_prod",
        "schema": "analytics",
    }))
    .unwrap();
    assert_eq!(p.catalog.as_deref(), Some("sales_prod"));
    assert_eq!(p.schema.as_deref(), Some("analytics"));
}

#[test]
fn catalog_is_optional() {
    let p: Params = serde_json::from_value(json!({ "connection": "wh" })).unwrap();
    assert!(p.catalog.is_none());
}
