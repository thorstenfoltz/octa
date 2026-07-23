//! Live database connection actions: `--db-query`, `--db-tables`,
//! `--db-write-table`, all against a connection saved in
//! Settings -> Databases (`--db NAME`). Queries run server-side in the
//! engine's native SQL dialect; writes are gated on the connection's
//! allow-writes switch.

use std::path::PathBuf;

use anyhow::{Result, bail};

use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::db::{self, DbConnection, DbWriteMode};
use octa::ui::settings::AppSettings;

use super::OutputFormat;
use super::output::write_table;

/// Look a saved connection up by name (case-insensitive) or id. The error
/// lists what is available so a typo is a one-step fix.
fn find_connection(name: &str) -> Result<(DbConnection, AppSettings)> {
    let settings = AppSettings::load();
    let found = settings
        .db_connections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name) || c.id == name)
        .cloned();
    match found {
        Some(c) => Ok((c, settings)),
        None => {
            let names: Vec<&str> = settings
                .db_connections
                .iter()
                .map(|c| c.name.as_str())
                .collect();
            if names.is_empty() {
                bail!(
                    "no saved database connection named '{name}' \
                     (none exist yet - add one in Settings -> Databases)"
                );
            }
            bail!(
                "no saved database connection named '{name}'. Available: {}",
                names.join(", ")
            );
        }
    }
}

fn connect(conn: &DbConnection, settings: &AppSettings) -> Result<Box<dyn db::DbConnector>> {
    let secret = octa::ui::settings::db_secrets::get_db_secret(&conn.id, settings);
    db::connect(conn, secret.as_deref())
}

/// `--db-query SQL --db NAME`: run one statement server-side. SELECTs print
/// through the normal output formats; mutations (refused unless the
/// connection allows writes) report rows affected on stderr.
pub fn run_query(conn_name: String, sql: String, format: OutputFormat) -> Result<()> {
    let (conn, settings) = find_connection(&conn_name)?;
    let mut c = connect(&conn, &settings)?;
    if octa::sql::is_mutation(&sql) {
        db::ensure_write_allowed(&conn, Some(&sql))?;
        let affected = c.execute(&sql)?;
        eprintln!("{affected} row(s) affected");
        Ok(())
    } else {
        let table = c.query(&sql)?;
        write_table(&table, format)
    }
}

/// `--db-tables --db NAME [--db-catalog CAT]`: list the server's namespace.
///
/// On a three-level engine with no `--db-catalog`, the catalogs themselves
/// are listed: recursing into every schema of every catalog by default would
/// be a very slow accident, and the catalog list is the first drill-down step
/// anyway.
pub fn run_tables(conn_name: String, catalog: Option<String>, format: OutputFormat) -> Result<()> {
    let (conn, settings) = find_connection(&conn_name)?;
    db::reject_catalog(conn.engine, catalog.as_deref())?;
    let mut c = connect(&conn, &settings)?;
    let mut out = DataTable::empty();

    if conn.engine.has_catalogs() && catalog.is_none() {
        out.columns = vec![ColumnInfo {
            name: "catalog".to_string(),
            data_type: "Utf8".to_string(),
        }];
        for cat in c.list_catalogs()? {
            out.rows.push(vec![CellValue::String(cat)]);
        }
        return write_table(&out, format);
    }

    let mut columns = Vec::new();
    if catalog.is_some() {
        columns.push(ColumnInfo {
            name: "catalog".to_string(),
            data_type: "Utf8".to_string(),
        });
    }
    columns.push(ColumnInfo {
        name: "schema".to_string(),
        data_type: "Utf8".to_string(),
    });
    columns.push(ColumnInfo {
        name: "table".to_string(),
        data_type: "Utf8".to_string(),
    });
    out.columns = columns;

    let cat = catalog.as_deref();
    for schema in c.list_schemas(cat)? {
        for table in c.list_tables(cat, &schema)? {
            let mut row = Vec::new();
            if let Some(cat) = cat {
                row.push(CellValue::String(cat.to_string()));
            }
            row.push(CellValue::String(schema.clone()));
            row.push(CellValue::String(table));
            out.rows.push(row);
        }
    }
    write_table(&out, format)
}

/// `--db-write-table SCHEMA.TABLE --db NAME FILE`: write a local file into a
/// server table. The file reads through the registry (any supported format,
/// compressed inputs included).
pub fn run_write(
    conn_name: String,
    catalog: Option<String>,
    target: String,
    mode: DbWriteMode,
    file: PathBuf,
) -> Result<()> {
    let Some((schema, table)) = target.split_once('.') else {
        bail!("--db-write-table expects SCHEMA.TABLE, got '{target}'");
    };
    let (conn, settings) = find_connection(&conn_name)?;
    db::reject_catalog(conn.engine, catalog.as_deref())?;
    db::ensure_write_allowed(&conn, None)?;
    let data = super::read_table(&file)?;
    let mut c = connect(&conn, &settings)?;
    let report = c.write_table(catalog.as_deref(), schema, table, mode, &data)?;
    eprintln!(
        "wrote {} row(s) to {}{schema}.{table}{}",
        report.rows_written,
        catalog.map(|c| format!("{c}.")).unwrap_or_default(),
        if report.created { " (created)" } else { "" }
    );
    Ok(())
}

/// Split `SCHEMA.TABLE`, naming the flag in the error so a typo is a
/// one-step fix.
fn split_qualified(flag: &str, value: &str) -> Result<(String, String)> {
    match value.split_once('.') {
        Some((s, t)) if !s.is_empty() && !t.is_empty() => Ok((s.to_string(), t.to_string())),
        _ => bail!("{flag} expects SCHEMA.TABLE, got '{value}'"),
    }
}

/// `--db-copy SCHEMA.TABLE --db SRC --db-copy-to TGT`: server-to-server table
/// copy between two saved connections. Fast lane (DuckDB ATTACH both ends)
/// when both engines are attachable, universal lane otherwise; `copy_table`
/// picks. Target schema/table default to the source's.
pub fn run_copy(
    conn_name: String,
    catalog: Option<String>,
    source: String,
    target_conn_name: String,
    target: Option<String>,
    target_catalog: Option<String>,
    mode: DbWriteMode,
) -> Result<()> {
    let (src_schema, src_table) = split_qualified("--db-copy", &source)?;
    let (src_conn, settings) = find_connection(&conn_name)?;
    let (tgt_conn, _) = find_connection(&target_conn_name)?;
    db::reject_catalog(src_conn.engine, catalog.as_deref())?;
    db::reject_catalog(tgt_conn.engine, target_catalog.as_deref())?;

    let (tgt_schema, tgt_table) = match &target {
        Some(t) => split_qualified("--db-copy-target", t)?,
        None => (src_schema.clone(), src_table.clone()),
    };

    let src_secret = octa::ui::settings::db_secrets::get_db_secret(&src_conn.id, &settings);
    let tgt_secret = octa::ui::settings::db_secrets::get_db_secret(&tgt_conn.id, &settings);

    let report = db::copy::copy_table(
        &db::copy::DbCopyEnd {
            conn: src_conn,
            catalog,
            schema: src_schema.clone(),
            table: src_table.clone(),
        },
        src_secret.as_deref(),
        &db::copy::DbCopyEnd {
            conn: tgt_conn,
            catalog: target_catalog,
            schema: tgt_schema.clone(),
            table: tgt_table.clone(),
        },
        tgt_secret.as_deref(),
        mode,
    )?;
    eprintln!(
        "copied {} row(s) from {src_schema}.{src_table} to {tgt_schema}.{tgt_table}{}",
        report.rows_copied,
        if report.created { " (created)" } else { "" }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cli::{Action, Cli};
    use clap::Parser;

    #[test]
    fn db_query_parses_into_action() {
        let cli =
            Cli::try_parse_from(["octa", "--db-query", "SELECT 1", "--db", "warehouse"]).unwrap();
        match cli.detect_action().unwrap() {
            Some(Action::DbQuery { conn, sql }) => {
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
}
