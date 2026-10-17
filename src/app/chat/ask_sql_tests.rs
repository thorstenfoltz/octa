use super::*;

fn cols() -> Vec<ColumnInfo> {
    vec![
        ColumnInfo {
            name: "amount".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "country".into(),
            data_type: "Utf8".into(),
        },
    ]
}

/// Facts for a plain local tab, so a test only states what it varies.
fn facts(table: &str, dialect: &str, related: &str) -> AskSqlFacts {
    AskSqlFacts {
        table_name: table.to_string(),
        dialect: dialect.to_string(),
        columns: cols(),
        row_count: Some(42),
        related: related.to_string(),
        workspace: String::new(),
    }
}

#[test]
fn parses_a_well_formed_reply() {
    let reply = r#"{"sql":"SELECT country FROM data"}"#;
    assert_eq!(parse_reply(reply).unwrap(), "SELECT country FROM data");
}

/// Models wrap JSON in prose and fences whatever the prompt says.
#[test]
fn tolerates_fenced_and_chatty_replies() {
    let reply = "Sure!\n```json\n{\"sql\":\"SELECT 1 FROM data\"}\n```";
    assert_eq!(parse_reply(reply).unwrap(), "SELECT 1 FROM data");
}

#[test]
fn accepts_a_with_clause() {
    let reply = r#"{"sql":"WITH t AS (SELECT 1) SELECT * FROM t"}"#;
    assert!(parse_reply(reply).is_ok());
}

#[test]
fn allows_one_trailing_semicolon() {
    let reply = r#"{"sql":"SELECT 1 FROM data;"}"#;
    assert_eq!(parse_reply(reply).unwrap(), "SELECT 1 FROM data;");
}

/// A semicolon inside a quoted literal is data, not a statement break.
#[test]
fn allows_a_semicolon_inside_a_string_literal() {
    let reply = r#"{"sql":"SELECT * FROM data WHERE note = 'a;b'"}"#;
    assert!(parse_reply(reply).is_ok());
}

#[test]
fn rejects_a_second_statement() {
    let reply = r#"{"sql":"SELECT 1 FROM data; DROP TABLE data"}"#;
    let err = parse_reply(reply).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn rejects_a_non_select_verb() {
    for sql in [
        "DELETE FROM data",
        "UPDATE data SET a = 1",
        "INSERT INTO data VALUES (1)",
        "DROP TABLE data",
    ] {
        let reply = format!(r#"{{"sql":"{sql}"}}"#);
        assert!(parse_reply(&reply).is_err(), "accepted: {sql}");
    }
}

#[test]
fn rejects_missing_or_empty_sql() {
    assert!(parse_reply(r#"{"query":"SELECT 1"}"#).is_err());
    assert!(parse_reply(r#"{"sql":""}"#).is_err());
    assert!(parse_reply(r#"{"sql":"   "}"#).is_err());
}

#[test]
fn rejects_malformed_json() {
    assert!(parse_reply("not json at all").is_err());
    assert!(parse_reply("").is_err());
    assert!(parse_reply("{not json}").is_err());
}

#[test]
fn splice_inserts_at_the_index() {
    assert_eq!(splice_at("AB", 1, "X"), "A\nX\nB");
}

#[test]
fn splice_past_the_end_appends() {
    assert_eq!(splice_at("AB", 99, "X"), "AB\nX");
}

#[test]
fn splice_into_empty_text_is_the_insert_alone() {
    assert_eq!(splice_at("", 0, "SELECT 1"), "SELECT 1");
}

/// Existing whitespace is not doubled.
#[test]
fn splice_does_not_add_a_second_newline() {
    assert_eq!(splice_at("A\n", 2, "X"), "A\nX");
}

#[test]
fn prompt_names_the_columns_types_table_and_dialect() {
    let p = build_prompt(&facts("data", "DuckDB", ""), "revenue per country");
    assert!(p.contains("amount (Int64)"), "{p}");
    assert!(p.contains("country (Utf8)"), "{p}");
    assert!(p.contains("data"), "{p}");
    assert!(p.contains("DuckDB"), "{p}");
    assert!(p.contains("42 rows"), "{p}");
    assert!(p.contains("revenue per country"), "{p}");
}

fn col_rows(pairs: &[(&str, &str, &str)]) -> Vec<ColumnRow> {
    pairs
        .iter()
        .map(|(s, t, c)| (s.to_string(), t.to_string(), c.to_string()))
        .collect()
}

fn fk(child: &str, ccol: &str, parent: &str, pcol: &str) -> ForeignKey {
    let (cs, ct) = child.split_once('.').unwrap();
    let (ps, pt) = parent.split_once('.').unwrap();
    ForeignKey {
        child_schema: cs.to_string(),
        child_table: ct.to_string(),
        child_column: ccol.to_string(),
        parent_schema: ps.to_string(),
        parent_table: pt.to_string(),
        parent_column: pcol.to_string(),
        constraint: "c".to_string(),
    }
}

/// A warehouse that declares nothing must leave the prompt exactly as it
/// was: this is what keeps the feature additive.
#[test]
fn no_foreign_keys_means_no_block_and_no_new_rules() {
    assert_eq!(related_tables_block("s.orders", &[], &[]), "");
    let p = build_prompt(&facts("s.orders", "PostgreSQL", ""), "how many");
    assert!(p.contains("Refer to the table only as s.orders"), "{p}");
    assert!(!p.contains("Join only"), "{p}");
}

/// Keys are followed in both directions: the parent a table points at and
/// the children that point back at it are both one hop away.
#[test]
fn both_directions_are_neighbours_with_their_columns() {
    let columns = col_rows(&[
        ("s", "customers", "id"),
        ("s", "customers", "name"),
        ("s", "items", "order_id"),
        ("s", "items", "sku"),
        ("s", "elsewhere", "x"),
    ]);
    let fks = [
        fk("s.orders", "customer_id", "s.customers", "id"),
        fk("s.items", "order_id", "s.orders", "id"),
        fk("s.other", "z", "s.elsewhere", "x"),
    ];
    let b = related_tables_block("s.orders", &columns, &fks);
    assert!(b.contains("s.customers columns: id, name"), "{b}");
    assert!(
        b.contains("join: s.orders.customer_id = s.customers.id"),
        "{b}"
    );
    assert!(b.contains("s.items columns: order_id, sku"), "{b}");
    assert!(b.contains("join: s.items.order_id = s.orders.id"), "{b}");
    // A key between two other tables is not this table's business.
    assert!(!b.contains("elsewhere"), "{b}");
}

/// A key pointing at its own table names the join without repeating the
/// column list the prompt already carries.
#[test]
fn a_self_reference_lists_the_join_only() {
    let columns = col_rows(&[("s", "staff", "id"), ("s", "staff", "manager_id")]);
    let fks = [fk("s.staff", "manager_id", "s.staff", "id")];
    let b = related_tables_block("s.staff", &columns, &fks);
    assert!(b.contains("s.staff (this table, joined to itself)"), "{b}");
    assert!(b.contains("join: s.staff.manager_id = s.staff.id"), "{b}");
    assert!(!b.contains("columns:"), "{b}");
}

/// The key query filters on the child's schema, the column query on its
/// own, so a parent living elsewhere comes back nameless. It still names a
/// real join, so it is kept, but never with an empty column list.
#[test]
fn a_neighbour_whose_columns_were_not_read_says_so() {
    let b = related_tables_block(
        "s.orders",
        &col_rows(&[("s", "orders", "country_id")]),
        &[fk("s.orders", "country_id", "ref.countries", "id")],
    );
    assert!(b.contains("ref.countries (columns not read)"), "{b}");
    assert!(!b.contains("columns: \n"), "{b}");
    assert!(
        b.contains("join: s.orders.country_id = ref.countries.id"),
        "{b}"
    );
}

#[test]
fn the_table_and_column_caps_hold_and_say_so() {
    let mut columns = Vec::new();
    let mut fks = Vec::new();
    for t in 0..MAX_RELATED_TABLES + 3 {
        let name = format!("s.t{t:02}");
        fks.push(fk("s.orders", "k", &name, "id"));
        for c in 0..MAX_RELATED_COLUMNS + 5 {
            columns.push(("s".to_string(), format!("t{t:02}"), format!("c{c:02}")));
        }
    }
    let b = related_tables_block("s.orders", &columns, &fks);
    assert_eq!(b.matches("columns:").count(), MAX_RELATED_TABLES, "{b}");
    assert!(
        b.contains("(3 further related tables are not listed.)"),
        "{b}"
    );
    assert!(b.contains("(+5 more)"), "{b}");
}

/// With neighbours the prompt gains them plus the rules that keep the
/// model from inventing a join or double counting through one.
#[test]
fn prompt_carries_the_related_block_and_its_rules() {
    let related = related_tables_block(
        "s.orders",
        &col_rows(&[("s", "customers", "id")]),
        &[fk("s.orders", "customer_id", "s.customers", "id")],
    );
    let p = build_prompt(&facts("s.orders", "PostgreSQL", &related), "who spent most");
    assert!(p.contains("s.customers columns: id"), "{p}");
    assert!(p.contains("Join only on a listed key pair"), "{p}");
    assert!(p.contains("A join can multiply rows"), "{p}");
    assert!(!p.contains("Refer to the table only as"), "{p}");
}

#[test]
fn workspace_tables_are_described_and_replace_the_single_table_rule() {
    let ws = workspace_block(&[(
        "wh.public.orders".to_string(),
        vec![
            ("id".to_string(), "BIGINT".to_string()),
            ("total".to_string(), "DOUBLE".to_string()),
        ],
    )]);
    let mut f = facts("data", "DuckDB", "");
    f.workspace = ws;
    let p = build_prompt(&f, "biggest order");
    assert!(p.contains("wh.public.orders"), "{p}");
    assert!(p.contains("total (DOUBLE)"), "{p}");
    assert!(p.contains("Do not invent tables or columns"), "{p}");
    assert!(!p.contains("Refer to the table only as"), "{p}");
}

/// The tab that exists only to attach a server has no table of its own.
/// The prompt must still be answerable, and must not name a `data` that
/// was never registered.
#[test]
fn a_tab_with_no_table_of_its_own_still_gets_a_usable_prompt() {
    let f = AskSqlFacts {
        table_name: "data".to_string(),
        dialect: "DuckDB".to_string(),
        columns: Vec::new(),
        row_count: None,
        related: String::new(),
        workspace: workspace_block(&[(
            "pg.public.users".to_string(),
            vec![("email".to_string(), "VARCHAR".to_string())],
        )]),
    };
    let p = build_prompt(&f, "how many users");
    assert!(p.contains("pg.public.users"), "{p}");
    assert!(!p.contains("The table is named"), "{p}");
    assert!(p.contains("DuckDB"), "{p}");
}

/// An unknown row count is omitted rather than guessed: a live database
/// tab holds one page, and stating a page size as the table size is a lie
/// the model has no way to detect.
#[test]
fn an_unknown_row_count_is_left_out_of_the_prompt() {
    let mut f = facts("s.orders", "PostgreSQL", "");
    f.row_count = None;
    let p = build_prompt(&f, "how many");
    assert!(!p.contains(" rows"), "{p}");
    assert!(
        p.contains("The table is named s.orders and has these columns"),
        "{p}"
    );
}

#[test]
fn an_empty_workspace_leaves_the_prompt_exactly_as_it_was() {
    assert_eq!(workspace_block(&[]), "");
}

#[test]
fn server_block_lists_question_tables_with_columns_first() {
    let mut columns = Vec::new();
    for t in 0..MAX_WORKSPACE_TABLES + 3 {
        columns.push(("s".to_string(), format!("t{t:02}"), "id".to_string()));
    }
    columns.push(("s".to_string(), "orders".to_string(), "total".to_string()));
    let b = server_tables_block(None, &columns, &[], "biggest Orders this year");
    // Named in the question, so it gets its columns even past the cap.
    assert!(b.contains("- s.orders\n  - total\n"), "{b}");
    assert!(b.contains("Other tables (columns not listed):"), "{b}");
    assert!(b.contains("- s.t14\n"), "{b}");
}

#[test]
fn server_block_qualifies_with_the_catalog_and_lists_declared_joins() {
    let columns = col_rows(&[("s", "orders", "customer_id"), ("s", "customers", "id")]);
    let fks = [fk("s.orders", "customer_id", "s.customers", "id")];
    let b = server_tables_block(Some("main"), &columns, &fks, "x");
    assert!(b.contains("- main.s.orders"), "{b}");
    assert!(
        b.contains("join: main.s.orders.customer_id = main.s.customers.id"),
        "{b}"
    );
}

#[test]
fn server_block_is_empty_without_tables() {
    assert_eq!(server_tables_block(None, &[], &[], "x"), "");
}

/// Fed through `workspace`, the block brings the "only these tables" rules
/// and no single-table sentence about a table the tab does not have.
#[test]
fn server_block_in_the_prompt_uses_the_workspace_rules() {
    let f = AskSqlFacts {
        table_name: String::new(),
        dialect: "PostgreSQL".to_string(),
        columns: Vec::new(),
        row_count: None,
        related: String::new(),
        workspace: server_tables_block(
            None,
            &col_rows(&[("public", "users", "email")]),
            &[],
            "how many users",
        ),
    };
    let p = build_prompt(&f, "how many users");
    assert!(p.contains("PostgreSQL"), "{p}");
    assert!(p.contains("- public.users"), "{p}");
    assert!(p.contains("Do not invent tables or columns"), "{p}");
    assert!(!p.contains("The table is named"), "{p}");
}
