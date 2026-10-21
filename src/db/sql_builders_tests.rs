use super::*;

#[test]
fn paged_sql_dialects() {
    assert_eq!(
        paged_sql(DbEngine::Postgres, "SELECT * FROM t", 100, 200),
        "SELECT * FROM (SELECT * FROM t) AS _octa_page LIMIT 100 OFFSET 200"
    );
    assert!(
        paged_sql(DbEngine::Mssql, "SELECT * FROM t", 100, 0)
            .contains("OFFSET 0 ROWS FETCH NEXT 100 ROWS ONLY")
    );
    // Oracle takes the same OFFSET/FETCH but neither the `AS` nor the
    // leading underscore: an unquoted identifier must start with a letter
    // (ORA-00911), which cost a real page of every copied Oracle table.
    // Trino rejects LIMIT before OFFSET, which is the order everyone else
    // takes, so the clause order is the thing to pin here.
    assert!(
        paged_sql(DbEngine::Trino, "SELECT * FROM t", 100, 200).ends_with("OFFSET 200 LIMIT 100")
    );
    let oracle = paged_sql(DbEngine::Oracle, "SELECT * FROM t", 100, 0);
    assert!(oracle.contains(") octa_page OFFSET 0 ROWS FETCH NEXT 100 ROWS ONLY"));
    assert!(!oracle.contains("_octa_page"));
}

/// Oracle has neither `DROP TABLE IF EXISTS` (before 23c) nor a multi-row
/// `VALUES` list, and both are on the shared write path, so both need
/// their own spelling or every Replace and every insert batch fails.
#[test]
fn oracle_write_path_avoids_the_syntax_it_lacks() {
    let drop = drop_if_exists_sql(DbEngine::Oracle, "\"S\".\"T\"");
    assert!(drop.starts_with("BEGIN EXECUTE IMMEDIATE 'DROP TABLE"));
    assert!(drop.contains("-942"));
    assert_eq!(
        drop_if_exists_sql(DbEngine::Postgres, "s.t"),
        "DROP TABLE IF EXISTS s.t"
    );

    let rows = ["(1, 'a')".to_string(), "(2, 'b')".to_string()];
    assert_eq!(
        insert_rows_sql(DbEngine::Oracle, "T", "A, B", &rows),
        "INSERT ALL INTO T (A, B) VALUES (1, 'a') INTO T (A, B) VALUES (2, 'b') \
             SELECT * FROM dual"
    );
    assert_eq!(
        insert_rows_sql(DbEngine::Postgres, "t", "a, b", &rows),
        "INSERT INTO t (a, b) VALUES (1, 'a'), (2, 'b')"
    );
}

/// A quoted string reaching an Oracle DATE goes through NLS_DATE_FORMAT,
/// which is `DD-MON-RR` out of the box and rejects every value Octa
/// renders, so the literals carry their type.
#[test]
fn oracle_dates_and_booleans_are_literals_oracle_accepts() {
    use crate::data::CellValue;
    assert_eq!(
        sql_literal(DbEngine::Oracle, &CellValue::Date("2024-01-31".into())),
        "DATE '2024-01-31'"
    );
    assert_eq!(
        sql_literal(
            DbEngine::Oracle,
            &CellValue::DateTime("2024-01-31 10:00:00".into())
        ),
        "TIMESTAMP '2024-01-31 10:00:00'"
    );
    // No BOOLEAN before 23c: NUMBER(1) is what the DDL creates.
    assert_eq!(sql_literal(DbEngine::Oracle, &CellValue::Bool(true)), "1");
    assert_eq!(
        sql_literal(DbEngine::Postgres, &CellValue::Date("2024-01-31".into())),
        "'2024-01-31'"
    );
}

#[test]
fn kill_sql_per_engine() {
    assert_eq!(
        kill_sql(DbEngine::MySql, "42").as_deref(),
        Some("KILL QUERY 42")
    );
    assert_eq!(kill_sql(DbEngine::Mssql, "57").as_deref(), Some("KILL 57"));
    assert_eq!(
        kill_sql(DbEngine::Exasol, "1690000000000000000").as_deref(),
        Some("KILL STATEMENT IN SESSION 1690000000000000000")
    );
    // Engines that cancel some other way must not get a kill statement.
    assert!(kill_sql(DbEngine::Postgres, "1").is_none());
    assert!(kill_sql(DbEngine::BigQuery, "1").is_none());
}

#[test]
fn kill_sql_rejects_a_non_numeric_session_id() {
    // Session ids come from the server as integers; anything else means
    // something is wrong, and it must never reach a concatenated
    // statement.
    assert!(kill_sql(DbEngine::MySql, "42; DROP TABLE t").is_none());
}

#[test]
fn qualified_name_prefixes_the_catalog() {
    let sql = create_table_sql(
        DbEngine::Snowflake,
        "analytics",
        "orders",
        &[crate::data::ColumnInfo {
            name: "id".to_string(),
            data_type: "Int64".to_string(),
        }],
    );
    // Quoted, because the INSERT that follows this DDL quotes too, and
    // Snowflake folds an unquoted identifier to upper case.
    assert!(sql.contains(r#""analytics"."orders""#), "got: {sql}");
    let qualified = qualified_name(
        DbEngine::Snowflake,
        Some("sales_prod"),
        "analytics",
        "orders",
    );
    assert_eq!(qualified, "\"sales_prod\".\"analytics\".\"orders\"");
}

#[test]
fn qualified_name_skips_empty_schema() {
    assert_eq!(
        qualified_name(DbEngine::Postgres, None, "", "orders"),
        "\"orders\""
    );
    assert_eq!(
        qualified_name(DbEngine::Snowflake, Some("db"), "", "orders"),
        "\"db\".\"orders\""
    );
}

#[test]
fn count_sql_qualifies_like_select() {
    assert_eq!(
        count_sql(DbEngine::Postgres, None, "public", "my table"),
        "SELECT COUNT(*) AS n FROM \"public\".\"my table\""
    );
    assert_eq!(
        count_sql(DbEngine::Mssql, None, "dbo", "t"),
        "SELECT COUNT(*) AS n FROM [dbo].[t]"
    );
}

#[test]
fn select_sample_sql_uses_each_dialect() {
    assert_eq!(
        select_sample_sql(DbEngine::Postgres, None, "public", "my table", 10),
        "SELECT * FROM \"public\".\"my table\" LIMIT 10"
    );
    assert_eq!(
        select_sample_sql(DbEngine::MySql, None, "app", "t", 5),
        "SELECT * FROM `app`.`t` LIMIT 5"
    );
    assert_eq!(
        select_sample_sql(DbEngine::Mssql, None, "dbo", "t", 5),
        "SELECT TOP 5 * FROM [dbo].[t]"
    );
}

/// The paged base query must carry no LIMIT of its own, or page two would
/// be sliced out of page one's rows instead of following it.
#[test]
fn select_all_sql_is_the_unlimited_base_for_paging() {
    assert_eq!(
        select_all_sql(DbEngine::Postgres, None, "public", "orders"),
        "SELECT * FROM \"public\".\"orders\""
    );
    let paged = paged_sql(
        DbEngine::Postgres,
        &select_all_sql(DbEngine::Postgres, None, "public", "orders"),
        100,
        200,
    );
    assert_eq!(
        paged,
        "SELECT * FROM (SELECT * FROM \"public\".\"orders\") AS _octa_page LIMIT 100 OFFSET 200"
    );
    assert!(!select_all_sql(DbEngine::Mssql, None, "dbo", "t").contains("TOP"));
}

#[test]
fn select_sample_sql_qualifies_catalog() {
    // Databricks three-part, backticks.
    assert_eq!(
        select_sample_sql(DbEngine::Databricks, Some("main"), "sales", "orders", 5),
        "SELECT * FROM `main`.`sales`.`orders` LIMIT 5"
    );
    // BigQuery three-part, backticks.
    assert_eq!(
        select_sample_sql(DbEngine::BigQuery, Some("proj"), "ds", "tbl", 7),
        "SELECT * FROM `proj`.`ds`.`tbl` LIMIT 7"
    );
    // Snowflake three-part, double quotes.
    assert_eq!(
        select_sample_sql(DbEngine::Snowflake, Some("DB"), "PUBLIC", "T", 3),
        "SELECT * FROM \"DB\".\"PUBLIC\".\"T\" LIMIT 3"
    );
}

/// `usize::MAX` is the "Unlimited" row cap (Settings -> Performance,
/// `--rows all`, MCP `unlimited`). Printed into a limit clause it becomes
/// 18446744073709551615, which no engine accepts: Databricks reads it as
/// DECIMAL(20,0) and refuses, Postgres overflows bigint. Unlimited means
/// no clause at all.
#[test]
fn an_unlimited_cap_emits_no_limit_clause() {
    for engine in [
        DbEngine::Databricks,
        DbEngine::Postgres,
        DbEngine::Mssql,
        DbEngine::Oracle,
        DbEngine::Snowflake,
        DbEngine::BigQuery,
    ] {
        let sql = select_sample_sql(engine, None, "s", "t", usize::MAX);
        assert!(
            !sql.contains("18446744073709551615"),
            "{engine:?} printed the sentinel: {sql}"
        );
        assert_eq!(sql, select_all_sql(engine, None, "s", "t"), "{engine:?}");
    }
    assert_eq!(
        select_sample_sql(
            DbEngine::Databricks,
            Some("main"),
            "sales",
            "orders",
            usize::MAX
        ),
        "SELECT * FROM `main`.`sales`.`orders`"
    );
}

fn cand(ty: &str, name: &str, col: &str, nullable: bool) -> RowKeyCandidate {
    RowKeyCandidate {
        constraint_type: ty.to_string(),
        constraint_name: name.to_string(),
        column_name: col.to_string(),
        nullable,
    }
}

#[test]
fn a_primary_key_always_wins() {
    let rows = vec![
        cand("UNIQUE", "uq_code", "code", false),
        cand("PRIMARY KEY", "pk_orders", "id", false),
    ];
    assert_eq!(choose_row_key(&rows), vec!["id".to_string()]);
}

#[test]
fn a_composite_primary_key_keeps_its_column_order() {
    let rows = vec![
        cand("PRIMARY KEY", "pk", "tenant", false),
        cand("PRIMARY KEY", "pk", "id", false),
    ];
    assert_eq!(
        choose_row_key(&rows),
        vec!["tenant".to_string(), "id".to_string()]
    );
}

/// A UNIQUE constraint over NOT NULL columns is the same server-enforced
/// one-row guarantee a primary key gives, so a table with one is editable.
#[test]
fn a_not_null_unique_constraint_stands_in_for_a_primary_key() {
    let rows = vec![cand("UNIQUE", "uq_code", "code", false)];
    assert_eq!(choose_row_key(&rows), vec!["code".to_string()]);
}

/// `WHERE col = NULL` matches nothing, so a nullable unique column would
/// silently update zero rows and report success.
#[test]
fn a_nullable_unique_constraint_is_refused() {
    let rows = vec![cand("UNIQUE", "uq_code", "code", true)];
    assert!(choose_row_key(&rows).is_empty());
}

/// The key is taken again at save time against a baseline snapshotted at
/// load, so a table with several unique constraints must always yield the
/// same one - otherwise the write addresses different rows than it read.
#[test]
fn several_unique_constraints_resolve_to_the_narrowest_one_deterministically() {
    let rows = vec![
        cand("UNIQUE", "uq_b_wide", "x", false),
        cand("UNIQUE", "uq_b_wide", "y", false),
        cand("UNIQUE", "uq_a_narrow", "code", false),
    ];
    assert_eq!(choose_row_key(&rows), vec!["code".to_string()]);
    // Same inputs in the other order must still pick the same key.
    let reversed: Vec<_> = rows.into_iter().rev().collect();
    assert_eq!(choose_row_key(&reversed), vec!["code".to_string()]);
}

#[test]
fn no_constraint_at_all_leaves_the_tab_read_only() {
    assert!(choose_row_key(&[]).is_empty());
}

#[test]
fn row_key_sql_quotes_literals_and_asks_for_both_constraint_kinds() {
    let sql = row_key_sql(DbEngine::Postgres, None, "pub'lic", "orders");
    assert!(sql.contains("tc.table_schema = 'pub''lic'"), "{sql}");
    assert!(sql.contains("tc.table_name = 'orders'"), "{sql}");
    assert!(
        sql.contains("constraint_type IN ('PRIMARY KEY', 'UNIQUE')"),
        "{sql}"
    );
    // Nullability decides whether a unique constraint can address a row.
    assert!(sql.contains("c.is_nullable"), "{sql}");
    assert!(
        sql.ends_with("ORDER BY tc.constraint_type, tc.constraint_name, kcu.ordinal_position"),
        "{sql}"
    );
}

#[test]
fn table_search_is_one_query_per_catalog_and_escapes_the_needle() {
    let pg = table_search_sql(DbEngine::Postgres, None, "o'r_1").unwrap();
    assert!(pg.contains("information_schema.tables"), "{pg}");
    assert!(pg.contains("o''r\\_1"), "{pg}");
    let sf = table_search_sql(DbEngine::Snowflake, Some("D\"B"), "x").unwrap();
    assert!(sf.contains("\"D\"\"B\".information_schema.tables"), "{sf}");
    let dbx = table_search_sql(DbEngine::Databricks, Some("main"), "x").unwrap();
    assert!(dbx.contains("`main`.information_schema.tables"), "{dbx}");
    // Oracle refuses `AS` before a derived table's alias.
    let ora = table_search_sql(DbEngine::Oracle, None, "x").unwrap();
    assert!(ora.contains(") found WHERE"), "{ora}");
    assert!(table_search_sql(DbEngine::BigQuery, Some("p"), "x").is_none());
    assert!(table_search_sql(DbEngine::Postgres, None, "  ").is_none());
}

#[test]
fn metadata_sql_dialects() {
    // Databricks: three-part backtick name, EXTENDED for the detail block.
    assert_eq!(
        table_metadata_sql(DbEngine::Databricks, Some("main"), "sales", "orders"),
        "DESCRIBE TABLE EXTENDED `main`.`sales`.`orders`"
    );
    // Snowflake / ClickHouse / Exasol keep their DESCRIBE forms.
    assert_eq!(
        table_metadata_sql(DbEngine::Snowflake, Some("DB"), "PUBLIC", "T"),
        "DESCRIBE TABLE \"DB\".\"PUBLIC\".\"T\""
    );
    assert!(table_metadata_sql(DbEngine::Exasol, None, "s", "t").starts_with("DESCRIBE "));
    // BigQuery: dataset INFORMATION_SCHEMA.COLUMNS, table filtered by name.
    let bq = table_metadata_sql(DbEngine::BigQuery, Some("proj"), "ds", "tbl");
    assert!(
        bq.contains("`proj`.`ds`.INFORMATION_SCHEMA.COLUMNS"),
        "{bq}"
    );
    assert!(bq.contains("table_name = 'tbl'"), "{bq}");
    // Generic engines: information_schema.columns, literals quoted.
    let pg = table_metadata_sql(DbEngine::Postgres, None, "pub'lic", "orders");
    assert!(pg.contains("FROM information_schema.columns"), "{pg}");
    assert!(pg.contains("table_schema = 'pub''lic'"), "{pg}");
    assert!(pg.contains("table_name = 'orders'"), "{pg}");
}

#[test]
fn metadata_column_names_across_engine_shapes() {
    use crate::data::{CellValue, ColumnInfo};
    let table = |col: &str, vals: &[&str]| {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "ordinal".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: col.into(),
                data_type: "Utf8".into(),
            },
        ];
        t.rows = vals
            .iter()
            .map(|v| vec![CellValue::Int(0), CellValue::String((*v).into())])
            .collect();
        t
    };
    // information_schema (Postgres, MySQL, MSSQL, Oracle upper-cased).
    assert_eq!(
        column_names_from_metadata(&table("COLUMN_NAME", &["id", "Gr\u{f6}\u{df}e"])),
        ["id", "Gr\u{f6}\u{df}e"]
    );
    // Snowflake / ClickHouse DESCRIBE.
    assert_eq!(column_names_from_metadata(&table("name", &["a"])), ["a"]);
    // Databricks DESCRIBE EXTENDED: the detail sections are not columns.
    assert_eq!(
        column_names_from_metadata(&table(
            "col_name",
            &["a", "b", "", "# Detailed Table Information", "Owner"]
        )),
        ["a", "b"]
    );
    // No recognisable name column: nothing rather than garbage.
    assert!(column_names_from_metadata(&table("other", &["a"])).is_empty());
}

#[test]
fn a_postgres_bytea_is_written_as_its_bytes() {
    let cell = crate::data::CellValue::Binary(vec![0xde, 0xad, 0x00]);
    assert_eq!(
        sql_literal(DbEngine::Postgres, &cell),
        "decode('dead00', 'hex')"
    );
}

#[test]
fn sql_server_and_mysql_binaries_are_written_as_their_bytes() {
    let cell = crate::data::CellValue::Binary(vec![0xde, 0xad, 0x00]);
    assert_eq!(sql_literal(DbEngine::Mssql, &cell), "0xdead00");
    assert_eq!(sql_literal(DbEngine::MySql, &cell), "X'dead00'");
}

#[test]
fn a_mysql_backslash_is_written_as_itself() {
    let cell = crate::data::CellValue::String("C:\\t".into());
    assert_eq!(sql_literal(DbEngine::MySql, &cell), "_utf8mb4 X'433a5c74'");
    // Without a backslash the plain literal stays, and other engines keep it.
    let plain = crate::data::CellValue::String("it's".into());
    assert_eq!(sql_literal(DbEngine::MySql, &plain), "'it''s'");
    assert_eq!(sql_literal(DbEngine::Postgres, &cell), "'C:\\t'");
}

#[test]
fn the_file_engines_list_tables_and_metadata_in_their_own_catalogues() {
    let sqlite = table_search_sql(DbEngine::Sqlite, None, "ord").unwrap();
    assert!(sqlite.contains("FROM sqlite_master"), "{sqlite}");
    let duck = table_search_sql(DbEngine::DuckDb, None, "ord").unwrap();
    assert!(duck.contains("information_schema.tables"), "{duck}");
    let meta = table_metadata_sql(DbEngine::Sqlite, None, "main", "o'rders");
    assert!(
        meta.contains("pragma_table_info('o''rders', 'main')"),
        "{meta}"
    );
    let meta = table_metadata_sql(DbEngine::DuckDb, None, "main", "orders");
    assert!(meta.contains("information_schema.columns"), "{meta}");
}

#[test]
fn file_engines_write_bytes_in_their_own_hex_form() {
    let b = crate::data::CellValue::Binary(vec![0, 255]);
    assert_eq!(sql_literal(DbEngine::DuckDb, &b), "from_hex('00ff')");
    assert_eq!(sql_literal(DbEngine::Sqlite, &b), "X'00ff'");
}
