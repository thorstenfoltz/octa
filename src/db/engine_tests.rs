use super::*;

#[test]
fn a_contains_predicate_cannot_be_escaped_by_the_search_box() {
    let cols = vec!["name".to_string()];
    let p = DbEngine::Postgres
        .contains_predicate(&cols, "o'brien'; DROP TABLE users --", false)
        .expect("predicate");
    // The quote is doubled, so the statement never ends early.
    assert!(p.contains("o''brien''; DROP TABLE users --"), "{p}");
    assert_eq!(p.matches('\'').count() % 2, 0, "unbalanced quotes: {p}");
}

#[test]
fn like_wildcards_typed_into_the_search_box_are_literal() {
    let cols = vec!["code".to_string()];
    let p = DbEngine::Postgres
        .contains_predicate(&cols, "50%_off", false)
        .expect("predicate");
    assert!(p.contains("50\\%\\_off"), "{p}");
    assert!(p.contains("ESCAPE"), "{p}");
}

#[test]
fn every_engine_casts_and_quotes_in_its_own_dialect() {
    let cols = vec!["Odd Name".to_string()];
    let mysql = DbEngine::MySql
        .contains_predicate(&cols, "x", false)
        .expect("p");
    assert!(mysql.contains("CAST(`Odd Name` AS CHAR)"), "{mysql}");
    let mssql = DbEngine::Mssql
        .contains_predicate(&cols, "x", false)
        .expect("p");
    assert!(
        mssql.contains("CAST([Odd Name] AS NVARCHAR(MAX))"),
        "{mssql}"
    );
    let ch = DbEngine::ClickHouse
        .contains_predicate(&cols, "x", false)
        .expect("p");
    assert!(ch.contains("toString(`Odd Name`)"), "{ch}");
    // No engine may emit ILIKE: only Postgres and its relatives have it.
    for engine in DbEngine::ALL {
        let p = engine.contains_predicate(&cols, "x", false).expect("p");
        assert!(!p.contains("ILIKE"), "{engine:?} emitted ILIKE: {p}");
    }
}

#[test]
fn a_predicate_needs_something_to_match_against() {
    assert!(
        DbEngine::Postgres
            .contains_predicate(&[], "x", false)
            .is_none()
    );
    assert!(
        DbEngine::Postgres
            .contains_predicate(&["a".to_string()], "   ", false)
            .is_none()
    );
}

#[test]
fn all_lists_fourteen_engines() {
    assert_eq!(DbEngine::ALL.len(), 14);
    assert!(DbEngine::ALL.contains(&DbEngine::Snowflake));
    assert!(DbEngine::ALL.contains(&DbEngine::Oracle));
    assert!(DbEngine::ALL.contains(&DbEngine::DuckDb));
    assert!(DbEngine::ALL.contains(&DbEngine::Sqlite));
}

#[test]
fn the_file_engines_are_files_without_a_sign_in() {
    for e in DbEngine::ALL {
        assert_eq!(
            e.is_file(),
            matches!(e, DbEngine::DuckDb | DbEngine::Sqlite),
            "{e:?}"
        );
    }
    for e in [DbEngine::DuckDb, DbEngine::Sqlite] {
        assert!(e.supported_auth().is_empty(), "{e:?} offers a sign-in");
        assert!(e.supports_row_update(), "{e:?}");
        assert!(
            e.enforces_read_only(),
            "{e:?}: the file is opened read-only"
        );
        assert!(!e.duckdb_attachable(), "{e:?}");
        assert!(!e.has_catalogs(), "{e:?}");
        assert!(!e.polls_for_results(), "{e:?}");
        assert!(e.has_foreign_keys(), "{e:?}");
    }
    assert!(DbEngine::DuckDb.enforces_foreign_keys());
    assert!(
        !DbEngine::Sqlite.enforces_foreign_keys(),
        "SQLite checks foreign keys only with PRAGMA foreign_keys = ON"
    );
}

#[test]
fn only_sqlite_keeps_analyses_off_the_server() {
    for e in DbEngine::ALL {
        assert_eq!(e.pushes_down(), *e != DbEngine::Sqlite, "{e:?}");
    }
}

/// An engine cannot enforce a constraint it does not have, and the
/// relationship map branches on both: `has_foreign_keys` decides whether
/// to ask at all, `enforces_foreign_keys` whether the answer also says
/// anything about the rows.
#[test]
fn only_engines_with_foreign_keys_can_enforce_them() {
    for e in DbEngine::ALL {
        if e.enforces_foreign_keys() {
            assert!(e.has_foreign_keys(), "{e:?} enforces what it does not have");
        }
    }
    assert!(!DbEngine::ClickHouse.has_foreign_keys());
    assert!(DbEngine::Postgres.enforces_foreign_keys());
    // The warehouses take a declaration and check nothing, which is the
    // whole reason the map offers to measure.
    for e in [
        DbEngine::Redshift,
        DbEngine::Snowflake,
        DbEngine::Databricks,
        DbEngine::BigQuery,
    ] {
        assert!(e.has_foreign_keys(), "{e:?} should declare foreign keys");
        assert!(!e.enforces_foreign_keys(), "{e:?} does not enforce them");
    }
}

#[test]
fn quoting_matches_dialect() {
    assert_eq!(DbEngine::ClickHouse.quote_ident("a`b"), "`a``b`");
    assert_eq!(DbEngine::Snowflake.quote_ident("a\"b"), "\"a\"\"b\"");
    assert_eq!(DbEngine::BigQuery.quote_ident("x"), "`x`");
}

#[test]
fn has_catalogs_only_warehouses() {
    for e in [
        DbEngine::Trino,
        DbEngine::Snowflake,
        DbEngine::Databricks,
        DbEngine::BigQuery,
    ] {
        assert!(e.has_catalogs(), "{e:?} should have catalogs");
    }
    for e in [
        DbEngine::Postgres,
        DbEngine::MySql,
        DbEngine::Mssql,
        DbEngine::Oracle,
        DbEngine::Redshift,
        DbEngine::ClickHouse,
        DbEngine::Exasol,
        DbEngine::Athena,
    ] {
        assert!(!e.has_catalogs(), "{e:?} should not have catalogs");
    }
}

#[test]
fn bigquery_quotes_with_backticks() {
    assert_eq!(DbEngine::BigQuery.quote_ident("a`b"), "`a``b`");
}

#[test]
fn duckdb_attachable_only_pg_mysql_redshift() {
    assert!(DbEngine::Postgres.duckdb_attachable());
    assert!(DbEngine::Redshift.duckdb_attachable());
    assert!(!DbEngine::Snowflake.duckdb_attachable());
    assert!(!DbEngine::ClickHouse.duckdb_attachable());
}

#[test]
fn supported_auth_gating() {
    assert_eq!(
        DbEngine::Snowflake.supported_auth(),
        &[
            DbAuthKind::KeyPairJwt,
            DbAuthKind::Password,
            DbAuthKind::OAuthBrowser,
            DbAuthKind::OAuthClientCredentials
        ]
    );
    assert_eq!(
        DbEngine::BigQuery.supported_auth(),
        &[DbAuthKind::GcpAdc, DbAuthKind::GcpServiceAccount]
    );
    assert_eq!(
        DbEngine::Databricks.supported_auth(),
        &[
            DbAuthKind::Token,
            DbAuthKind::AzureAd,
            DbAuthKind::OAuthClientCredentials,
            DbAuthKind::OAuthBrowser
        ]
    );
    assert_eq!(
        DbEngine::ClickHouse.supported_auth(),
        &[DbAuthKind::Password]
    );
}

#[test]
fn default_ports_match_convention() {
    assert_eq!(DbEngine::Postgres.default_port(), 5432);
    assert_eq!(DbEngine::MySql.default_port(), 3306);
    assert_eq!(DbEngine::Mssql.default_port(), 1433);
}

#[test]
fn ident_quoting_per_engine() {
    assert_eq!(DbEngine::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
    assert_eq!(DbEngine::MySql.quote_ident("a`b"), "`a``b`");
    assert_eq!(DbEngine::Mssql.quote_ident("a]b"), "[a]]b]");
}
