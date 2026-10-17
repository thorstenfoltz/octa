use super::*;

#[test]
fn only_the_connection_is_required() {
    let p: Params = serde_json::from_value(json!({ "connection": "warehouse" })).unwrap();
    assert!(p.schemas.is_empty());
    assert!(p.tables.is_none());
    // Measuring reads table data, so it must never be the default.
    assert!(!p.measure);
}

#[test]
fn params_accept_a_catalog_and_an_explicit_table_set() {
    let p: Params = serde_json::from_value(json!({
        "connection": "wh",
        "catalog": "sales_prod",
        "schemas": ["analytics"],
        "tables": ["analytics.orders", "analytics.customers"],
        "measure": true,
    }))
    .unwrap();
    assert_eq!(p.catalog.as_deref(), Some("sales_prod"));
    assert_eq!(p.tables.unwrap().len(), 2);
    assert!(p.measure);
}

#[test]
fn measure_no_longer_takes_a_sample() {
    let p: Params = serde_json::from_value(serde_json::json!({
        "connection": "c", "measure": true, "sample": 50
    }))
    .expect("an old client's sample is ignored, not refused");
    assert!(p.measure);
    assert!(DESCRIPTION.contains("every row"));
}
