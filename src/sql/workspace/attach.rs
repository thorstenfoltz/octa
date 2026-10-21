//! Attaching databases to the workspace: DuckDB and SQLite files, and live
//! connections (natively through DuckDB's `postgres` / `mysql` extensions, or
//! by importing the picked tables for every other engine).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::{RegisteredTable, SqlWorkspace, TableOrigin, dedupe_sql_name};
use crate::formats::FormatRegistry;
use crate::sql::engine::quote_ident;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachKind {
    DuckDb,
    Sqlite,
    /// Live PostgreSQL server via the DuckDB `postgres` extension.
    Postgres,
    /// Live MySQL / MariaDB server via the DuckDB `mysql` extension.
    MySql,
    /// Live SQL Server; no DuckDB extension exists, so its tables are
    /// imported as workspace tables (never a native ATTACH).
    Mssql,
}

impl AttachKind {
    pub fn from_path(path: &Path) -> AttachKind {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "duckdb" | "ddb" => AttachKind::DuckDb,
            _ => AttachKind::Sqlite,
        }
    }
}

/// How a live-database connection is attached to the workspace.
pub(super) enum AttachStrategy {
    /// A native DuckDB `ATTACH` via the named extension.
    Native { ext: &'static str, kind: AttachKind },
    /// No DuckDB extension: import the tables as workspace tables.
    Import,
    /// A saved DuckDB or SQLite file: the same attach as **Attach
    /// database...** with a picked file.
    File,
}

/// Pick the attach strategy for an engine. Mirrors
/// [`crate::db::DbEngine::duckdb_attachable`]: Postgres/Redshift/MySQL attach
/// natively, everything else (SQL Server + the warehouses) is imported.
pub(super) fn attach_kind_for(engine: crate::db::DbEngine) -> AttachStrategy {
    use crate::db::DbEngine;
    match engine {
        DbEngine::Postgres | DbEngine::Redshift => AttachStrategy::Native {
            ext: "postgres",
            kind: AttachKind::Postgres,
        },
        DbEngine::MySql => AttachStrategy::Native {
            ext: "mysql",
            kind: AttachKind::MySql,
        },
        DbEngine::DuckDb | DbEngine::Sqlite => AttachStrategy::File,
        _ => AttachStrategy::Import,
    }
}

/// Which part of a live server an attach should import.
///
/// `catalog` names the top level of a three-level engine
/// ([`crate::db::DbEngine::has_catalogs`]) and is required there; `schema`
/// and `table` narrow the import further. A Databricks catalog routinely
/// holds hundreds of tables, which is far too many to import, so the attach
/// menu drills down to the single table and passes what the user picked.
#[derive(Debug, Clone, Default)]
pub struct AttachScope {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub table: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub alias: String,
    pub path: PathBuf,
    pub kind: AttachKind,
    /// `true` when the attachment is a real DuckDB `ATTACH`; `false` when the
    /// workspace fell back to per-table loading for a SQLite file because the
    /// DuckDB `sqlite` extension wasn't available.
    pub native: bool,
}

/// What an attach did. A DuckDB `ATTACH` leaves the server queryable as
/// `alias.schema.table` and shows up as an attachment; an engine DuckDB
/// cannot attach has its picked tables **copied in** as ordinary workspace
/// tables, which is not an attachment at all - listing one as well made a
/// single import look like it had attached the connection twice.
#[derive(Debug, Clone)]
pub enum AttachOutcome {
    Attached(Attachment),
    /// Workspace table names the import registered, in the order it took them.
    Imported(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct AttachedTable {
    pub schema: String,
    pub table: String,
    pub row_count: Option<usize>,
}

impl SqlWorkspace {
    /// ATTACH a DuckDB or SQLite database under `alias` so its tables are
    /// addressable as `alias.schema.table`. For SQLite the workspace tries
    /// the DuckDB `sqlite` extension first; if `INSTALL sqlite; LOAD sqlite;`
    /// fails the workspace falls back to enumerating the file's tables via
    /// `rusqlite` and registering each as a normal workspace table under the
    /// name `alias__table`. The boolean `native` field on the returned
    /// [`Attachment`] tells the caller which path was taken.
    pub fn attach(&mut self, path: &Path, alias: &str, kind: AttachKind) -> Result<Attachment> {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let done = self.file_attach_job(path, alias, kind)?(&stop)?;
        Ok(self.note_file_attach(done))
    }

    /// [`Self::attach`] as a job for a worker, on the runner connection
    /// (DuckDB keeps an ATTACH per database, so the window's connection sees
    /// it). A SQLite file whose extension cannot be loaded is copied table by
    /// table instead, as `alias__table`; raising `stop` ends that between
    /// tables and takes back what it made. Once it worked, the caller calls
    /// [`Self::note_file_attach`].
    pub fn file_attach_job(
        &self,
        path: &Path,
        alias: &str,
        kind: AttachKind,
    ) -> Result<impl FnOnce(&std::sync::atomic::AtomicBool) -> Result<FileAttached> + Send + 'static>
    {
        if self.attachments.contains_key(alias) {
            bail!("alias '{alias}' is already attached");
        }
        if matches!(
            kind,
            AttachKind::Postgres | AttachKind::MySql | AttachKind::Mssql
        ) {
            bail!("live database connections attach via attach_db, not a file path");
        }
        let path = path.to_path_buf();
        let alias = alias.to_string();
        let runner = self.runner.clone();
        let mut sink = self.import_sink();
        let sqlite_extension = self.sqlite_extension;
        Ok(move |stop: &std::sync::atomic::AtomicBool| {
            let path_str = path
                .canonicalize()
                .unwrap_or_else(|_| path.clone())
                .to_string_lossy()
                .replace('\'', "''");
            let attachment = |native| Attachment {
                alias: alias.clone(),
                path: path.clone(),
                kind,
                native,
            };
            let conn = runner
                .lock()
                .map_err(|_| anyhow!("the SQL worker stopped"))?;
            if kind == AttachKind::DuckDb {
                conn.execute(
                    &format!("ATTACH '{path_str}' AS {} (READ_ONLY)", quote_ident(&alias)),
                    [],
                )
                .with_context(|| format!("ATTACHing DuckDB at {}", path.display()))?;
                return Ok(FileAttached {
                    attachment: attachment(true),
                    imported: Vec::new(),
                    sqlite_extension,
                });
            }
            // SQLite: the extension once, remembered by the workspace.
            let loaded = sqlite_extension
                .unwrap_or_else(|| conn.execute_batch("INSTALL sqlite; LOAD sqlite;").is_ok());
            if loaded
                && conn
                    .execute(
                        &format!(
                            "ATTACH '{path_str}' AS {} (TYPE SQLITE, READ_ONLY)",
                            quote_ident(&alias)
                        ),
                        [],
                    )
                    .is_ok()
            {
                return Ok(FileAttached {
                    attachment: attachment(true),
                    imported: Vec::new(),
                    sqlite_extension: Some(true),
                });
            }
            drop(conn);
            // Fallback: read each table through rusqlite and register it as a
            // workspace table under `alias__table`. The user loses the
            // schema-qualified addressing but gets the same JOIN capability
            // on every install.
            let registry = FormatRegistry::new();
            let reader = registry
                .reader_for_path(&path)
                .ok_or_else(|| anyhow!("no SQLite reader available"))?;
            let listing = reader.list_tables(&path)?.unwrap_or_default();
            let mut imported = Vec::new();
            for info in &listing {
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    sink.roll_back();
                    bail!(
                        "attaching {} was stopped; nothing was attached",
                        path.display()
                    );
                }
                // Best-effort: a table that fails to load is skipped rather
                // than failing the whole attach.
                let Ok(t) = reader.read_table(&path, &info.name) else {
                    continue;
                };
                if let Ok(name) = sink.add(&format!("{alias}__{}", info.name), &t) {
                    imported.push(ImportedTable {
                        name,
                        origin: TableOrigin::File {
                            path: path.clone(),
                            inner_table: Some(info.name.clone()),
                        },
                        rows: t.row_count(),
                    });
                }
            }
            Ok(FileAttached {
                attachment: attachment(false),
                imported,
                sqlite_extension: Some(loaded),
            })
        })
    }

    /// List what a [`Self::file_attach_job`] attached; returns the attachment.
    pub fn note_file_attach(&mut self, done: FileAttached) -> Attachment {
        if done.sqlite_extension.is_some() {
            self.sqlite_extension = done.sqlite_extension;
        }
        self.note_imported(done.imported);
        self.note_attachment(done.attachment.clone());
        done.attachment
    }

    /// Attach a saved live-database connection read-only. PostgreSQL, Redshift
    /// and MySQL go through the DuckDB `postgres` / `mysql` extensions (which
    /// install over the network on first use, like the lakehouse readers);
    /// SQL Server, Oracle and the warehouse engines (ClickHouse, Exasol,
    /// Snowflake, Databricks, BigQuery) have no extension, so the tables
    /// `scope` covers are imported as workspace tables (mirrors the SQLite
    /// fallback). `secret` is the stored password; IAM/AD auth modes mint their
    /// own token inside `db::connect` / `duckdb_attach_sql`'s password.
    ///
    /// `scope` says which part of the server to import. A three-level engine
    /// **requires** a catalog: without one the enumeration below runs against
    /// whatever catalog the server happens to default to, which is how an
    /// attached Databricks connection used to import nothing at all while the
    /// sidebar listed every table. A schema (and a table) narrow it further,
    /// which is the only way a catalog of several hundred tables can be
    /// attached at all. Natively attachable engines ignore the scope: DuckDB
    /// ATTACH is lazy, so the whole server stays queryable for free.
    pub fn attach_db(
        &mut self,
        db_conn: &crate::db::DbConnection,
        secret: Option<&str>,
        ssh_secret: Option<&str>,
        alias: &str,
        scope: &AttachScope,
    ) -> Result<AttachOutcome> {
        match attach_kind_for(db_conn.engine) {
            AttachStrategy::Native { .. } => {
                let attachment = self.native_attach_job(db_conn, secret, ssh_secret, alias)?()?;
                self.note_attachment(attachment.clone());
                Ok(AttachOutcome::Attached(attachment))
            }
            AttachStrategy::File => {
                let path = crate::db::file_db::file_path(db_conn)?;
                let kind = match db_conn.engine {
                    crate::db::DbEngine::DuckDb => AttachKind::DuckDb,
                    _ => AttachKind::Sqlite,
                };
                Ok(AttachOutcome::Attached(self.attach(&path, alias, kind)?))
            }
            // SQL Server and the warehouse engines have no DuckDB extension, so
            // their tables are imported as workspace tables.
            AttachStrategy::Import => {
                // Behind a jump host the imported connection has to name the
                // loopback forward.
                let tunnelled = crate::db::ssh_tunnel::with_tunnel(db_conn, ssh_secret)?;
                Ok(AttachOutcome::Imported(
                    self.attach_import(&tunnelled, secret, ssh_secret, scope)?,
                ))
            }
        }
    }

    /// The native ATTACH of a PostgreSQL, Redshift or MySQL connection as a
    /// job for a worker: the token command an IAM/AD connection runs, the
    /// extension install (network on first use) and the handshake all happen
    /// on the runner connection. DuckDB keeps an ATTACH per database, so the
    /// window's connection sees it too. Once it worked, the caller calls
    /// [`Self::note_attachment`].
    pub fn native_attach_job(
        &self,
        db_conn: &crate::db::DbConnection,
        secret: Option<&str>,
        ssh_secret: Option<&str>,
        alias: &str,
    ) -> Result<impl FnOnce() -> Result<Attachment> + Send + 'static> {
        if self.attachments.contains_key(alias) {
            bail!("alias '{alias}' is already attached");
        }
        let AttachStrategy::Native { ext, kind } = attach_kind_for(db_conn.engine) else {
            bail!("{} has no native ATTACH", db_conn.engine.label());
        };
        let runner = self.runner.clone();
        let db_conn = db_conn.clone();
        let secret = secret.map(str::to_string);
        let ssh_secret = ssh_secret.map(str::to_string);
        let alias = alias.to_string();
        Ok(move || {
            // Behind a jump host the ATTACH string has to name the loopback
            // forward.
            let db_conn = crate::db::ssh_tunnel::with_tunnel(&db_conn, ssh_secret.as_deref())?;
            let display = PathBuf::from(format!(
                "{}:{}/{}",
                db_conn.host, db_conn.port, db_conn.database
            ));
            let password = crate::db::auth::resolve_password(&db_conn, secret.as_deref())?;
            let conn = runner
                .lock()
                .map_err(|_| anyhow!("the SQL worker stopped"))?;
            conn.execute_batch(&format!("INSTALL {ext}; LOAD {ext};"))
                .with_context(|| {
                    format!("installing the DuckDB {ext} extension (network on first use)")
                })?;
            duckdb_attach(&conn, &db_conn, &password, &alias, true)
                .with_context(|| format!("ATTACHing {ext} server {}", display.display()))?;
            Ok(Attachment {
                alias,
                path: display,
                kind,
                native: true,
            })
        })
    }

    /// List an attachment a [`Self::native_attach_job`] made.
    pub fn note_attachment(&mut self, attachment: Attachment) {
        self.attachments
            .insert(attachment.alias.clone(), attachment);
    }

    /// Import the scoped tables of a non-attachable connection (SQL Server,
    /// Oracle or a warehouse) as workspace tables, each capped at the
    /// streaming initial-load row count. Returns the names it registered.
    /// The CLI and MCP use this; the GUI runs [`import_scope`] on a worker
    /// with progress and Cancel instead, then calls [`Self::note_imported`].
    fn attach_import(
        &mut self,
        db_conn: &crate::db::DbConnection,
        secret: Option<&str>,
        ssh_secret: Option<&str>,
        scope: &AttachScope,
    ) -> Result<Vec<String>> {
        check_import_scope(db_conn, scope)?;
        let mut c = crate::db::connect(db_conn, secret, ssh_secret)?;
        let mut sink = self.import_sink();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let imported = import_scope(c.as_mut(), db_conn, scope, &mut sink, &stop, |_, _| {})?;
        Ok(self.note_imported(imported))
    }

    /// Where a worker registers imported tables: the runner connection, so
    /// the window keeps the workspace's own. Names are deduped against the
    /// tables the workspace has now.
    pub fn import_sink(&self) -> ImportSink {
        ImportSink {
            runner: self.runner.clone(),
            taken: self.tables.keys().cloned().collect(),
            made: Vec::new(),
        }
    }

    /// List the tables an [`import_scope`] registered, after it finished.
    /// Returns their names.
    pub fn note_imported(&mut self, imported: Vec<ImportedTable>) -> Vec<String> {
        imported
            .into_iter()
            .map(|t| {
                self.tables.insert(
                    t.name.clone(),
                    RegisteredTable {
                        sql_name: t.name.clone(),
                        origin: t.origin,
                        row_count: t.rows,
                    },
                );
                t.name
            })
            .collect()
    }

    pub fn detach(&mut self, alias: &str) -> Result<()> {
        self.attached_tables_cache.borrow_mut().remove(alias);
        let attachment = self
            .attachments
            .remove(alias)
            .ok_or_else(|| anyhow!("no attachment '{alias}'"))?;
        if attachment.native {
            self.conn
                .execute(&format!("DETACH {}", quote_ident(alias)), [])
                .with_context(|| format!("DETACHing {alias}"))?;
        } else {
            // Fallback registrations carried the alias prefix; drop them all.
            let prefix = format!("{alias}__");
            let keys: Vec<String> = self
                .tables
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            for k in keys {
                let _ = self.remove_table(&k);
            }
        }
        Ok(())
    }

    /// Enumerate tables inside an attached database via the live DuckDB
    /// connection's `information_schema`. Returns an empty Vec for fallback
    /// attachments (the user already sees those tables as regular workspace
    /// entries).
    ///
    /// **Cached per alias**: the SQL panel rebuilds its workspace snapshot
    /// every frame, and for a Postgres/MySQL attachment this listing is a
    /// network round trip - uncached, two attached servers made the whole
    /// editor unusable. The cache is dropped on detach and by
    /// [`Self::invalidate_attached_cache`] (the panel's refresh button).
    pub fn list_attached_tables(&self, alias: &str) -> Result<Vec<AttachedTable>> {
        if let Some(cached) = self.attached_tables_cache.borrow().get(alias) {
            return Ok(cached.clone());
        }
        let attachment = self
            .attachments
            .get(alias)
            .ok_or_else(|| anyhow!("no attachment '{alias}'"))?;
        if !attachment.native {
            return Ok(Vec::new());
        }
        // Row counts only for local file attachments: on a remote server
        // COUNT(*) is a per-table scan (InnoDB counts by walking the rows),
        // so listing a big database would hammer it just to draw the tree.
        let count_rows = matches!(attachment.kind, AttachKind::DuckDb | AttachKind::Sqlite);
        let mut stmt = self.conn.prepare(
            "SELECT table_schema, table_name FROM information_schema.tables \
             WHERE table_catalog = ? AND table_type = 'BASE TABLE' \
             ORDER BY table_schema, table_name",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map([alias], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(rows.len());
        for (schema, table) in rows {
            let row_count: Option<usize> = if count_rows {
                let count_sql = format!(
                    "SELECT COUNT(*) FROM {}.{}.{}",
                    quote_ident(alias),
                    quote_ident(&schema),
                    quote_ident(&table)
                );
                self.conn
                    .query_row(&count_sql, [], |r| r.get::<_, i64>(0))
                    .ok()
                    .map(|n| n as usize)
            } else {
                None
            };
            out.push(AttachedTable {
                schema,
                table,
                row_count,
            });
        }
        self.attached_tables_cache
            .borrow_mut()
            .insert(alias.to_string(), out.clone());
        Ok(out)
    }

    /// Drop the cached attached-table listings so the next
    /// [`Self::list_attached_tables`] re-queries the servers (the panel's
    /// refresh action calls this; remote table lists can drift).
    pub fn invalidate_attached_cache(&self) {
        self.attached_tables_cache.borrow_mut().clear();
    }
}

/// One connection-string value as `engine`'s extension reads it. libpq
/// quotes with `'` and backslash escapes; DuckDB's MySQL extension knows only
/// `"` with `\"` and `\\`, and ends an unquoted value at `=` (a password with
/// a space or `=` failed to attach, 2026-10-21).
fn conn_value(engine: crate::db::DbEngine, v: &str) -> String {
    let (quote, special): (char, &[char]) = if engine == crate::db::DbEngine::MySql {
        ('"', &['"', '\\', '='])
    } else {
        ('\'', &['\'', '\\'])
    };
    if v.is_empty() || v.contains(|c: char| c.is_whitespace() || special.contains(&c)) {
        let escaped = v
            .replace('\\', "\\\\")
            .replace(quote, &format!("\\{quote}"));
        format!("{quote}{escaped}{quote}")
    } else {
        v.to_string()
    }
}

/// Run [`duckdb_attach_sql`] on `conn`. Both extensions repeat the whole
/// connection string in a failed ATTACH's error, password included (seen
/// against servers, 2026-10-21), so it is masked before the error goes
/// anywhere.
pub fn duckdb_attach(
    conn: &duckdb::Connection,
    db_conn: &crate::db::DbConnection,
    password: &str,
    alias: &str,
    read_only: bool,
) -> Result<()> {
    let sql = duckdb_attach_sql(db_conn, password, alias, read_only)?;
    conn.execute(&sql, [])
        .map(drop)
        .map_err(|e| anyhow!(mask_password(&e.to_string(), db_conn.engine, password)))
}

/// `password=<value>` as [`duckdb_attach_sql`] wrote it, either as the
/// extension saw it or still `''`-doubled inside the statement.
pub(super) fn mask_password(msg: &str, engine: crate::db::DbEngine, password: &str) -> String {
    let inner = format!("password={}", conn_value(engine, password));
    msg.replace(&inner.replace('\'', "''"), "password=***")
        .replace(&inner, "password=***")
}

/// Build the full `ATTACH ... (TYPE postgres|mysql[, READ_ONLY])` statement
/// for a live connection. Pure so the escaping is unit-testable. Only
/// meaningful for Postgres/MySQL (SQL Server has no DuckDB extension).
/// `read_only: false` attaches writable (the server-to-server copy's target);
/// everything else attaches READ_ONLY.
///
/// The inner connection string is `key=value` pairs, each value quoted the
/// way its extension reads it ([`conn_value`]); the assembled string is then
/// embedded in the SQL literal with `''` doubling.
///
/// The connection's SSL mode goes along, so Attach and the fast copy encrypt
/// and check exactly as the connection's own connector does. Refused where
/// the extension cannot honour it rather than attached with a weaker check:
/// libpq's verify-ca needs a CA file (`sslrootcert=system` is verify-full
/// only), and DuckDB's MySQL extension checks a certificate only against a
/// CA file and never its name (both verified against servers, 2026-10-20).
pub fn duckdb_attach_sql(
    db_conn: &crate::db::DbConnection,
    password: &str,
    alias: &str,
    read_only: bool,
) -> Result<String> {
    use crate::db::tls::TlsMode;
    let val = |v: &str| conn_value(db_conn.engine, v);
    let mode = crate::db::tls::effective_mode(db_conn);
    // libpq and the MySQL client take the path literally, so `~/` is
    // expanded here as for the native connectors.
    let ca = db_conn
        .tls
        .ca_path()
        .map(|p| p.to_string_lossy().into_owned());
    let ca = ca.as_deref();
    // Through a jump host or Connect via this is not the database's own
    // address.
    let (dial_host, dial_port) = db_conn.dial_target();
    let inner = match db_conn.engine {
        crate::db::DbEngine::MySql => {
            let mut s = format!(
                "host={} port={} database={} user={} password={} ssl_mode={}",
                val(&dial_host),
                dial_port,
                val(&db_conn.database),
                val(&db_conn.username),
                val(password),
                mode.label(crate::db::DbEngine::MySql)
            );
            if mode.checks_certificate() {
                let Some(ca) = ca else {
                    bail!(
                        "Attach cannot check this MySQL server's certificate without a CA file: \
                         DuckDB's MySQL extension checks only against one. Add the CA file to \
                         the connection, or choose required"
                    );
                };
                s.push_str(&format!(" ssl_ca={}", val(ca)));
            }
            s
        }
        _ => {
            // libpq checks the certificate against `host` and dials
            // `hostaddr`, the same split the native connector makes.
            let host = if dial_host == db_conn.host {
                val(&dial_host)
            } else {
                format!("{} hostaddr={}", val(&db_conn.host), val(&dial_host))
            };
            let mut s = format!(
                "host={host} port={} dbname={} user={} password={} sslmode={}",
                dial_port,
                val(&db_conn.database),
                val(&db_conn.username),
                val(password),
                mode.label(crate::db::DbEngine::Postgres)
            );
            // Only for the verify modes: libpq treats `require` plus a root
            // file as verify-ca.
            match (mode, ca) {
                (TlsMode::VerifyCa | TlsMode::VerifyFull, Some(ca)) => {
                    s.push_str(&format!(" sslrootcert={}", val(ca)));
                }
                (TlsMode::VerifyFull, None) => s.push_str(" sslrootcert=system"),
                (TlsMode::VerifyCa, None) => bail!(
                    "Attach cannot use verify-ca without a CA file: libpq checks the system's \
                     roots only under verify-full. Add the CA file to the connection, or \
                     choose verify-full"
                ),
                _ => {}
            }
            s
        }
    };
    let ty = match db_conn.engine {
        crate::db::DbEngine::MySql => "mysql",
        _ => "postgres",
    };
    let ro = if read_only { ", READ_ONLY" } else { "" };
    Ok(format!(
        "ATTACH '{}' AS {} (TYPE {ty}{ro})",
        inner.replace('\'', "''"),
        quote_ident(alias)
    ))
}

/// One table an import registered: its workspace name, where it came from,
/// and the rows it holds.
pub struct ImportedTable {
    pub name: String,
    pub origin: TableOrigin,
    pub rows: usize,
}

/// Registers imported tables on a workspace's runner connection (see
/// [`SqlWorkspace::import_sink`]). Remembers what it made, so a failed or
/// cancelled import can take every table back out.
pub struct ImportSink {
    runner: std::sync::Arc<std::sync::Mutex<duckdb::Connection>>,
    taken: std::collections::HashSet<String>,
    made: Vec<String>,
}

impl ImportSink {
    /// Register `table` under `base` (deduped, `_2` and on) and return the
    /// name it got.
    pub fn add(&mut self, base: &str, table: &crate::data::DataTable) -> Result<String> {
        let name = dedupe_sql_name(base, |s| self.taken.contains(s));
        let conn = self
            .runner
            .lock()
            .map_err(|_| anyhow!("the SQL worker stopped"))?;
        super::Session(&conn).drop_relation(&name)?;
        crate::sql::engine::register_table_into(&conn, &name, table)?;
        self.taken.insert(name.clone());
        self.made.push(name.clone());
        Ok(name)
    }

    /// Drop every table this sink made: a stopped or failed import leaves
    /// nothing behind.
    pub fn roll_back(&mut self) {
        if let Ok(conn) = self.runner.lock() {
            for name in self.made.drain(..) {
                let _ = super::Session(&conn).drop_relation(&name);
            }
        }
    }
}

/// The refusals an import makes before it connects.
pub fn check_import_scope(db_conn: &crate::db::DbConnection, scope: &AttachScope) -> Result<()> {
    // A three-level engine enumerated with no catalog asks the server for
    // "the schemas", which it answers from whatever catalog the session
    // happens to sit in - usually none of the ones the user can see in the
    // sidebar. Refuse instead of importing the wrong thing, or nothing.
    if db_conn.engine.has_catalogs() && scope.catalog.is_none() {
        bail!(
            "'{}' has a catalog level: pick which catalog to attach. \
             The sidebar's Databases section lists them.",
            db_conn.name
        );
    }
    // A table names nothing on its own: `schema.table` is the address the
    // import selects by.
    if scope.schema.is_none() && scope.table.is_some() {
        bail!("a table can only be attached together with its schema");
    }
    Ok(())
}

/// Import every table `scope` covers on `c` into `sink`: one SELECT each,
/// capped at the streaming initial-load row count. A table is named after
/// **itself** (`orders`), because the name exists to be typed in a `FROM`
/// clause; where it is from is on the row as its origin, and a name the
/// workspace already has takes the usual `_2` suffix.
///
/// All or nothing: raising `stop` ends it between tables, and a stop or any
/// failure takes the tables already made back out, so an attach never
/// leaves part of a schema behind. `progress(done, total)` reports each
/// table.
pub fn import_scope(
    c: &mut dyn crate::db::DbConnector,
    db_conn: &crate::db::DbConnection,
    scope: &AttachScope,
    sink: &mut ImportSink,
    stop: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Vec<ImportedTable>> {
    check_import_scope(db_conn, scope)?;
    let catalog = scope.catalog.as_deref();
    let mut pairs: Vec<(String, String)> = Vec::new();
    match (&scope.schema, &scope.table) {
        // One table: no enumeration at all.
        (Some(schema), Some(t)) => pairs.push((schema.clone(), t.clone())),
        (Some(schema), None) => {
            for t in c.list_tables(catalog, schema)? {
                pairs.push((schema.clone(), t));
            }
        }
        (None, _) => {
            for schema in c.list_schemas(catalog)? {
                for t in c.list_tables(catalog, &schema)? {
                    pairs.push((schema.clone(), t));
                }
            }
        }
    }
    if pairs.is_empty() {
        let mut where_ = format!("'{}'", db_conn.name);
        if let Some(cat) = catalog {
            where_.push_str(&format!(" catalog '{cat}'"));
        }
        if let Some(schema) = &scope.schema {
            where_.push_str(&format!(" schema '{schema}'"));
        }
        bail!("{where_} reported no tables to import");
    }
    let total = pairs.len();
    progress(0, total);
    let cap = crate::formats::initial_load_rows();
    let mut imported: Vec<ImportedTable> = Vec::new();
    let result = (|| -> Result<()> {
        for (done, (schema, t)) in pairs.into_iter().enumerate() {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                bail!("the import was stopped; nothing was attached");
            }
            let sql = crate::db::select_sample_sql(db_conn.engine, catalog, &schema, &t, cap);
            let table = c
                .query(&sql)
                .with_context(|| format!("importing {schema}.{t}"))?;
            let name = sink.add(&t, &table)?;
            let origin = match catalog {
                Some(cat) => format!("{} {cat}.{schema}.{t}", db_conn.name),
                None => format!("{} {schema}.{t}", db_conn.name),
            };
            imported.push(ImportedTable {
                name,
                origin: TableOrigin::Db(origin),
                rows: table.row_count(),
            });
            progress(done + 1, total);
        }
        Ok(())
    })();
    if let Err(e) = result {
        sink.roll_back();
        return Err(e);
    }
    Ok(imported)
}

/// What a [`SqlWorkspace::file_attach_job`] did: the attachment, the tables
/// a SQLite fallback copied, and whether the sqlite extension loaded (so the
/// next attach does not try the network again).
pub struct FileAttached {
    pub attachment: Attachment,
    pub imported: Vec<ImportedTable>,
    pub sqlite_extension: Option<bool>,
}
