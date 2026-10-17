use super::*;

#[test]
fn a_table_match_is_found_through_catalog_and_schema() {
    let mut listings = HashMap::new();
    let conn = "c".to_string();
    listings.insert(
        (conn.clone(), String::new()),
        DbListState::Catalogs(vec!["main".into()]),
    );
    listings.insert(
        (conn.clone(), "main".into()),
        DbListState::Schemas(vec!["public".into(), "stage".into()]),
    );
    listings.insert(
        (conn.clone(), join_path(&["main", "public"])),
        DbListState::Tables(vec!["orders".into(), "fact_sales".into()]),
    );
    // "stage" was never opened: nothing is known below it.
    assert!(subtree_matches(&listings, &conn, "", "sales"));
    assert!(subtree_matches(&listings, &conn, "main", "sales"));
    assert!(!subtree_matches(
        &listings,
        &conn,
        &join_path(&["main", "stage"]),
        "sales"
    ));
    assert!(!subtree_matches(&listings, &conn, "", "invoices"));
}
