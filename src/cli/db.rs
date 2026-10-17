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
/// Public wrapper so sibling CLI actions (`--sync-sql`) can resolve a saved
/// connection by name without duplicating the lookup and its error text.
pub fn find_connection_pub(name: &str) -> Result<(DbConnection, AppSettings)> {
    find_connection(name)
}

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
    let ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&conn.id, settings);
    db::connect(conn, secret.as_deref(), ssh_secret.as_deref())
}

/// `--db-query SQL --db NAME`: run one statement server-side. SELECTs print
/// through the normal output formats; mutations (refused unless the
/// connection allows writes) report rows affected on stderr.
pub fn run_query(
    conn_name: String,
    sql: String,
    explain: bool,
    format: OutputFormat,
) -> Result<()> {
    let (conn, settings) = find_connection(&conn_name)?;
    let mut c = connect(&conn, &settings)?;
    if explain {
        let plan = octa::sql::explain::explain_db(&mut *c, &sql)?;
        return super::sql::print_plan(&plan, format);
    }
    db::ensure_write_allowed(&conn, Some(&sql))?;
    if octa::sql::is_mutation(&sql) {
        let affected = db::with_identifier_fix(conn.engine, &sql, |s| c.execute(s)).0?;
        eprintln!("{affected} row(s) affected");
        Ok(())
    } else {
        let table = db::with_identifier_fix(conn.engine, &sql, |s| c.query(s)).0?;
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
    let src_ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&src_conn.id, &settings);
    let tgt_ssh_secret = octa::ui::settings::db_secrets::get_ssh_secret(&tgt_conn.id, &settings);

    // A server-to-server copy can run for minutes with nothing on screen. The
    // universal lane reports rows as it writes them; the fast lane is one SQL
    // statement and reports nothing, which is why the count is not a total.
    let bar = std::cell::RefCell::new(super::progress::Progress::start(None));
    let report = db::copy::copy_table(
        &db::copy::DbCopyEnd {
            ssh_secret: src_ssh_secret,
            conn: src_conn,
            catalog,
            schema: src_schema.clone(),
            table: src_table.clone(),
        },
        src_secret.as_deref(),
        &db::copy::DbCopyEnd {
            ssh_secret: tgt_ssh_secret,
            conn: tgt_conn,
            catalog: target_catalog,
            schema: tgt_schema.clone(),
            table: tgt_table.clone(),
        },
        tgt_secret.as_deref(),
        mode,
        &|rows| bar.borrow_mut().rows(rows, "rows copied"),
    );
    bar.borrow_mut().finish();
    let report = report?;
    eprintln!(
        "copied {} row(s) from {src_schema}.{src_table} to {tgt_schema}.{tgt_table}{}",
        report.rows_copied,
        if report.created { " (created)" } else { "" }
    );
    Ok(())
}

/// `--db-export SCHEMA --db NAME --out-dir DIR [--to EXT] [--db-export-tables
/// A,B] [--overwrite]`: one file per table, streamed. An empty table list means
/// every table of the schema. Exit code 1 when any table failed.
pub fn run_export(
    conn_name: String,
    mut request: db::export::ExportRequest,
) -> Result<std::process::ExitCode> {
    let (conn, settings) = find_connection(&conn_name)?;
    db::reject_catalog(conn.engine, request.catalog.as_deref())?;
    let mut c = connect(&conn, &settings)?;
    if request.tables.is_empty() {
        request.tables = c.list_tables(request.catalog.as_deref(), &request.schema)?;
        if request.tables.is_empty() {
            bail!("schema '{}' has no tables", request.schema);
        }
    }
    let bar = std::cell::RefCell::new(super::progress::Progress::start(None));
    let outcomes = db::export::export_tables(
        c.as_mut(),
        &request,
        &|p| {
            bar.borrow_mut().rows(
                p.rows,
                &format!("rows of {} ({}/{})", p.table, p.index + 1, p.count),
            )
        },
        &std::sync::atomic::AtomicBool::new(false),
    );
    bar.borrow_mut().finish();
    let mut failed = false;
    for outcome in outcomes? {
        match outcome {
            db::export::TableOutcome::Written { table, path, rows } => {
                eprintln!("{table}: {rows} row(s) -> {}", path.display())
            }
            db::export::TableOutcome::Skipped { table, path } => eprintln!(
                "{table}: skipped, {} is already there (--overwrite replaces it)",
                path.display()
            ),
            db::export::TableOutcome::Failed { table, error } => {
                failed = true;
                eprintln!("{table}: failed: {error}");
            }
        }
    }
    Ok(if failed {
        std::process::ExitCode::from(1)
    } else {
        std::process::ExitCode::SUCCESS
    })
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
