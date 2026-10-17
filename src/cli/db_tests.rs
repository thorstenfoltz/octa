use crate::cli::{Action, Cli};
use clap::Parser;

#[test]
fn db_query_parses_into_action() {
    let cli = Cli::try_parse_from(["octa", "--db-query", "SELECT 1", "--db", "warehouse"]).unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbQuery { conn, sql, .. }) => {
            assert_eq!(conn, "warehouse");
            assert_eq!(sql, "SELECT 1");
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_tables_parses_into_action() {
    let cli = Cli::try_parse_from(["octa", "--db-tables", "--db", "wh"]).unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbTables { conn, .. }) => assert_eq!(conn, "wh"),
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_write_table_takes_positional_file() {
    let cli = Cli::try_parse_from([
        "octa",
        "--db-write-table",
        "staging.users",
        "--db",
        "wh",
        "--db-write-mode",
        "replace",
        "users.parquet",
    ])
    .unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbWrite {
            conn,
            target,
            mode,
            file,
            ..
        }) => {
            assert_eq!(conn, "wh");
            assert_eq!(target, "staging.users");
            assert_eq!(mode, octa::db::DbWriteMode::Replace);
            assert_eq!(file, std::path::PathBuf::from("users.parquet"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_actions_require_db_flag() {
    for args in [
        vec!["octa", "--db-query", "SELECT 1"],
        vec!["octa", "--db-tables"],
        vec!["octa", "--db-write-table", "s.t", "f.csv"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.detect_action().is_err(), "--db missing must error");
    }
}

#[test]
fn db_action_flags_conflict_with_other_actions() {
    assert!(Cli::try_parse_from(["octa", "--db-tables", "--schema", "f.parquet"]).is_err());
    assert!(Cli::try_parse_from(["octa", "--db-query", "SELECT 1", "--db-tables"]).is_err());
}

#[test]
fn db_catalog_reaches_the_tables_action() {
    let cli = Cli::try_parse_from([
        "octa",
        "--db-tables",
        "--db",
        "wh",
        "--db-catalog",
        "sales_prod",
    ])
    .unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbTables { conn, catalog }) => {
            assert_eq!(conn, "wh");
            assert_eq!(catalog.as_deref(), Some("sales_prod"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_catalog_reaches_the_write_action() {
    let cli = Cli::try_parse_from([
        "octa",
        "--db-write-table",
        "analytics.daily",
        "--db",
        "wh",
        "--db-catalog",
        "sales_prod",
        "rows.parquet",
    ])
    .unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbWrite {
            catalog, target, ..
        }) => {
            assert_eq!(catalog.as_deref(), Some("sales_prod"));
            assert_eq!(target, "analytics.daily");
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_catalog_defaults_to_none() {
    let cli = Cli::try_parse_from(["octa", "--db-tables", "--db", "wh"]).unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbTables { catalog, .. }) => assert!(catalog.is_none()),
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_catalog_requires_db_flag() {
    assert!(Cli::try_parse_from(["octa", "--db-tables", "--db-catalog", "c"]).is_err());
}

#[test]
fn db_copy_parses_into_action() {
    let cli = Cli::try_parse_from([
        "octa",
        "--db-copy",
        "analytics.orders",
        "--db",
        "src_conn",
        "--db-copy-to",
        "tgt_conn",
        "--db-copy-target",
        "reporting.orders",
        "--db-write-mode",
        "replace",
    ])
    .unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbCopy {
            conn,
            source,
            target_conn,
            target,
            mode,
            ..
        }) => {
            assert_eq!(conn, "src_conn");
            assert_eq!(source, "analytics.orders");
            assert_eq!(target_conn, "tgt_conn");
            assert_eq!(target.as_deref(), Some("reporting.orders"));
            assert_eq!(mode, octa::db::DbWriteMode::Replace);
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_copy_target_defaults_to_none() {
    let cli = Cli::try_parse_from([
        "octa",
        "--db-copy",
        "analytics.orders",
        "--db",
        "a",
        "--db-copy-to",
        "b",
    ])
    .unwrap();
    match cli.detect_action().unwrap() {
        Some(Action::DbCopy { target, .. }) => assert!(target.is_none()),
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn db_copy_requires_a_target_connection() {
    let cli = Cli::try_parse_from(["octa", "--db-copy", "s.t", "--db", "a"]).unwrap();
    assert!(
        cli.detect_action().is_err(),
        "--db-copy-to missing must error"
    );
}

#[test]
fn db_copy_conflicts_with_other_actions() {
    assert!(Cli::try_parse_from(["octa", "--db-copy", "s.t", "--db-tables"]).is_err());
}
