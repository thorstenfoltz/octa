use super::*;

fn fk(child: &str, ccol: &str, parent: &str, pcol: &str, name: &str) -> ForeignKey {
    let (cs, ct) = child.split_once('.').expect("schema.table");
    let (ps, pt) = parent.split_once('.').expect("schema.table");
    ForeignKey {
        child_schema: cs.into(),
        child_table: ct.into(),
        child_column: ccol.into(),
        parent_schema: ps.into(),
        parent_table: pt.into(),
        parent_column: pcol.into(),
        constraint: name.into(),
    }
}

fn cols(spec: &[(&str, &[&str])]) -> Vec<ColumnRow> {
    let mut out = Vec::new();
    for (table, columns) in spec {
        let (s, t) = table.split_once('.').expect("schema.table");
        for c in *columns {
            out.push((s.to_string(), t.to_string(), (*c).to_string()));
        }
    }
    out
}

#[test]
fn draws_only_the_linked_tables_by_default() {
    let columns = cols(&[
        ("public.orders", &["id", "customer_id"]),
        ("public.customers", &["id", "name"]),
        ("public.audit_log", &["id", "message"]),
    ]);
    let fks = vec![fk(
        "public.orders",
        "customer_id",
        "public.customers",
        "id",
        "fk_orders_customer",
    )];
    let out = build_db_map(&columns, &fks, None, 30);
    let names: Vec<&str> = out.map.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["public.orders", "public.customers"]);
    assert_eq!(out.map.edges.len(), 1);
    let e = &out.map.edges[0];
    assert_eq!(e.constraint.as_deref(), Some("fk_orders_customer"));
    assert!(!e.scored, "a declared key arrives unmeasured");
    // The anchors address the column list the box shows.
    assert_eq!(
        out.map.nodes[e.left_table].columns[e.left_col],
        "customer_id"
    );
    assert_eq!(out.map.nodes[e.right_table].columns[e.right_col], "id");
}

#[test]
fn an_explicit_table_set_can_add_an_unlinked_table() {
    let columns = cols(&[
        ("public.orders", &["id", "customer_id"]),
        ("public.customers", &["id"]),
        ("public.audit_log", &["id"]),
    ]);
    let fks = vec![fk(
        "public.orders",
        "customer_id",
        "public.customers",
        "id",
        "fk1",
    )];
    let wanted: HashSet<String> = ["public.orders".to_string(), "public.audit_log".to_string()]
        .into_iter()
        .collect();
    let out = build_db_map(&columns, &fks, Some(&wanted), 30);
    let names: Vec<&str> = out.map.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, vec!["public.orders", "public.audit_log"]);
    // customers is not drawn, so its line has nothing to attach to.
    assert!(out.map.edges.is_empty());
    assert_eq!(out.skipped_edges, 1);
}

#[test]
fn a_key_pointing_outside_the_scanned_schemas_is_counted_not_dropped_silently() {
    let columns = cols(&[("sales.orders", &["id", "customer_id"])]);
    let fks = vec![fk(
        "sales.orders",
        "customer_id",
        "crm.customers",
        "id",
        "fk_cross",
    )];
    let out = build_db_map(&columns, &fks, None, 30);
    assert!(out.map.edges.is_empty());
    assert_eq!(out.skipped_edges, 1);
}

#[test]
fn a_composite_key_becomes_one_edge_per_column_pair() {
    let columns = cols(&[
        ("public.lines", &["order_id", "region", "qty"]),
        ("public.orders", &["id", "region"]),
    ]);
    let fks = vec![
        fk("public.lines", "order_id", "public.orders", "id", "fk_line"),
        fk(
            "public.lines",
            "region",
            "public.orders",
            "region",
            "fk_line",
        ),
    ];
    let out = build_db_map(&columns, &fks, None, 30);
    assert_eq!(out.map.edges.len(), 2);
    assert!(
        out.map
            .edges
            .iter()
            .all(|e| e.constraint.as_deref() == Some("fk_line")),
        "both halves name the same constraint"
    );
}

#[test]
fn the_table_cap_truncates_and_says_so() {
    let spec: Vec<(String, Vec<&str>)> = (0..5)
        .map(|i| (format!("public.t{i}"), vec!["id", "other_id"]))
        .collect();
    let columns: Vec<ColumnRow> = spec
        .iter()
        .flat_map(|(t, cs)| {
            let (s, name) = t.split_once('.').unwrap();
            cs.iter()
                .map(move |c| (s.to_string(), name.to_string(), (*c).to_string()))
        })
        .collect();
    let fks: Vec<ForeignKey> = (1..5)
        .map(|i| {
            fk(
                &format!("public.t{i}"),
                "other_id",
                "public.t0",
                "id",
                &format!("fk{i}"),
            )
        })
        .collect();
    let out = build_db_map(&columns, &fks, None, 3);
    assert_eq!(out.map.nodes.len(), 3);
    assert!(out.truncated);
}

#[test]
fn clickhouse_is_refused_rather_than_asked() {
    let err = foreign_key_sql(DbEngine::ClickHouse, None, &["default".into()])
        .expect_err("ClickHouse has no foreign keys");
    assert!(format!("{err}").contains("no foreign keys"), "{err}");
}

#[test]
fn every_other_engine_produces_at_least_one_statement() {
    let schemas = vec!["pub'lic".to_string()];
    for engine in DbEngine::ALL.iter().copied() {
        if !engine.has_foreign_keys() {
            continue;
        }
        let catalog = engine.has_catalogs().then_some("cat");
        let stmts = foreign_key_sql(engine, catalog, &schemas)
            .unwrap_or_else(|e| panic!("{engine:?}: {e}"));
        assert!(!stmts.is_empty(), "{engine:?} produced no statement");
        // The schema name survives, escaped for the context it lands in:
        // a doubled apostrophe in a string literal, or whatever
        // `quote_ident` does when the engine takes it as an identifier
        // (Snowflake's SHOW and BigQuery's table paths do).
        let quoted = engine.quote_ident("pub'lic");
        for s in &stmts {
            assert!(
                s.contains("pub''lic") || s.contains(&quoted),
                "{engine:?} lost or mangled the schema name: {s}"
            );
        }
        assert!(
            !schema_columns_sql(engine, catalog, &schemas, None).is_empty(),
            "{engine:?} produced no column listing"
        );
    }
}

#[test]
fn snowflake_and_bigquery_ask_once_per_schema() {
    let schemas = vec!["a".to_string(), "b".to_string()];
    for engine in [DbEngine::Snowflake, DbEngine::BigQuery] {
        let stmts = foreign_key_sql(engine, Some("cat"), &schemas).expect("sql");
        assert_eq!(stmts.len(), 2, "{engine:?} should ask per schema");
    }
    // The rest answer for every schema in one statement.
    for engine in [DbEngine::Postgres, DbEngine::MySql, DbEngine::Mssql] {
        let stmts = foreign_key_sql(engine, None, &schemas).expect("sql");
        assert_eq!(stmts.len(), 1, "{engine:?} should ask once");
    }
}

#[test]
fn mssql_reads_sys_views_because_its_information_schema_cannot_pair_composites() {
    let sql = &foreign_key_sql(DbEngine::Mssql, None, &["dbo".into()]).expect("sql")[0];
    assert!(sql.contains("sys.foreign_key_columns"), "{sql}");
    assert!(!sql.contains("position_in_unique_constraint"), "{sql}");
}

#[test]
fn mysql_needs_no_join_because_the_referenced_side_is_on_the_same_view() {
    let sql = &foreign_key_sql(DbEngine::MySql, None, &["app".into()]).expect("sql")[0];
    assert!(sql.contains("referenced_table_name IS NOT NULL"), "{sql}");
    assert!(!sql.contains("JOIN"), "{sql}");
}

#[test]
fn a_table_is_found_exactly_first_then_in_any_case() {
    let all = cols(&[
        ("APP.ORDERS", &["ID", "NOTE"]),
        ("APP.Mixed", &["a"]),
        ("APP.mixed", &["b"]),
    ]);
    // Oracle stored an unquoted `orders` as `ORDERS`.
    assert_eq!(
        table_columns(&all, "orders"),
        Some(("ORDERS".into(), vec!["ID".into(), "NOTE".into()]))
    );
    // Both spellings exist: the exact one wins.
    assert_eq!(
        table_columns(&all, "mixed"),
        Some(("mixed".into(), vec!["b".into()]))
    );
    // Neither is exact and both match in any case: no guess.
    assert_eq!(table_columns(&all, "MIXED"), None);
    assert_eq!(table_columns(&all, "nope"), None);
}

#[test]
fn a_name_resolves_exactly_first_then_to_its_only_match_in_any_case() {
    let names: Vec<String> = ["ID", "Note", "note", "x"].map(String::from).to_vec();
    assert_eq!(server_name(&names, "id"), Some("ID"));
    assert_eq!(server_name(&names, "note"), Some("note"));
    assert_eq!(server_name(&names, "NOTE"), None);
    assert_eq!(server_name(&names, "y"), None);
}

/// One table's columns, asked by name in any case: an append resolves its
/// target through this, and the whole schema's listing ran past the Oracle
/// driver's 100-row cap, so the target was never found.
#[test]
fn the_column_listing_can_ask_for_one_table_in_any_case() {
    let schemas = vec!["app".to_string()];
    for &engine in DbEngine::ALL {
        let all = schema_columns_sql(engine, None, &schemas, None);
        let one = schema_columns_sql(engine, None, &schemas, Some("o'rders"));
        assert_eq!(all.len(), one.len(), "{engine:?}");
        for (a, o) in all.iter().zip(&one) {
            assert!(
                !a.contains("o'rders") && !a.contains("UPPER("),
                "{engine:?}: {a}"
            );
            let want = format!(
                "UPPER({})",
                crate::db::pushdown::dialect::str_lit(engine, "o'rders")
            );
            assert!(o.contains(&want), "{engine:?}: {o}");
        }
    }
}
