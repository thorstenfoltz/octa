//! The SQL text Octa sends to a live server, spelled per engine: paging,
//! samples and counts, row keys, metadata, table search, literals and the
//! statements of the shared write path. Pure string building, no I/O.

use super::DbEngine;
use crate::data::DataTable;

/// Wrap `base_sql` in a `LIMIT`/`OFFSET` page for the given engine. Used by
/// the universal copy lane to stream a table in bounded batches. Most engines
/// take standard `LIMIT n OFFSET m`; SQL Server needs the `OFFSET ... FETCH`
/// form (which requires an `ORDER BY`, so a stable no-op sort is supplied).
pub fn paged_sql(engine: DbEngine, base_sql: &str, limit: usize, offset: usize) -> String {
    match engine {
        DbEngine::Mssql => format!(
            "SELECT * FROM ({base_sql}) AS _octa_page \
             ORDER BY (SELECT NULL) OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"
        ),
        // Trino, and Athena which is Trino underneath, put OFFSET *before*
        // LIMIT and reject the other order outright.
        DbEngine::Trino | DbEngine::Athena => {
            format!("SELECT * FROM ({base_sql}) AS _octa_page OFFSET {offset} LIMIT {limit}")
        }
        // Oracle takes the same OFFSET/FETCH form (12c+) but rejects `AS`
        // before a table alias, and rejects the leading underscore in the
        // alias every other engine takes (ORA-00911: an unquoted identifier
        // must start with a letter).
        DbEngine::Oracle => format!(
            "SELECT * FROM ({base_sql}) octa_page \
             OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"
        ),
        _ => format!("SELECT * FROM ({base_sql}) AS _octa_page LIMIT {limit} OFFSET {offset}"),
    }
}

/// The statement that stops the work running in `session_id`, or `None` for
/// an engine that cancels some other way.
///
/// The id is validated as digits-only before it reaches a concatenated
/// statement: it arrives from the server as an integer, so anything else is
/// a bug or an injection attempt, never a legitimate id.
pub(crate) fn kill_sql(engine: DbEngine, session_id: &str) -> Option<String> {
    if session_id.is_empty() || !session_id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match engine {
        DbEngine::MySql => Some(format!("KILL QUERY {session_id}")),
        // MSSQL KILL ends the whole session, not just the statement, and
        // needs ALTER ANY CONNECTION. The caller drops the connector after.
        DbEngine::Mssql => Some(format!("KILL {session_id}")),
        DbEngine::Exasol => Some(format!("KILL STATEMENT IN SESSION {session_id}")),
        _ => None,
    }
}

/// The SQL dialect a live engine speaks. The single mapping point for every
/// DDL statement Octa sends to a server, so CREATE TABLE and ALTER TABLE ADD
/// can never disagree about a type name.
pub(crate) fn live_dialect_for(
    engine: DbEngine,
) -> crate::data::schema_export::sql::LiveSqlDialect {
    use crate::data::schema_export::sql::LiveSqlDialect;
    match engine {
        DbEngine::MySql => LiveSqlDialect::Mysql,
        DbEngine::Mssql => LiveSqlDialect::Mssql,
        DbEngine::Oracle => LiveSqlDialect::Oracle,
        DbEngine::Snowflake => LiveSqlDialect::Snowflake,
        DbEngine::Databricks => LiveSqlDialect::Databricks,
        DbEngine::ClickHouse => LiveSqlDialect::ClickHouse,
        DbEngine::Exasol => LiveSqlDialect::Exasol,
        // Athena is Trino under the covers, and its DDL is Trino's.
        DbEngine::Trino | DbEngine::Athena => LiveSqlDialect::Trino,
        DbEngine::BigQuery => LiveSqlDialect::BigQuery,
        // Redshift genuinely speaks the Postgres dialect, so this arm is
        // correct rather than provisional.
        DbEngine::Postgres | DbEngine::Redshift => LiveSqlDialect::Postgres,
    }
}

/// CREATE TABLE DDL for a live write target, reusing the schema-export
/// type mappings (one source of truth per dialect).
pub fn create_table_sql(
    engine: DbEngine,
    schema: &str,
    table: &str,
    columns: &[crate::data::ColumnInfo],
) -> String {
    use crate::data::schema_export::sql::create_table_qualified;
    create_table_qualified(columns, live_dialect_for(engine), schema, table)
}

/// `[<catalog>.]<schema>.<table>` in the engine's own quoting. `catalog` is
/// set only for three-level engines (Snowflake/Databricks/BigQuery).
pub(crate) fn qualified_name(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
) -> String {
    let mut name = String::new();
    if let Some(cat) = catalog {
        name.push_str(&engine.quote_ident(cat));
        name.push('.');
    }
    if !schema.is_empty() {
        name.push_str(&engine.quote_ident(schema));
        name.push('.');
    }
    name.push_str(&engine.quote_ident(table));
    name
}

/// Reject a catalog passed to an engine that has no catalog level, naming the
/// engines that do. A silent ignore would write to the wrong table.
pub fn reject_catalog(engine: DbEngine, catalog: Option<&str>) -> anyhow::Result<()> {
    if let Some(cat) = catalog
        && !engine.has_catalogs()
    {
        anyhow::bail!(
            "{engine:?} has no catalog level, so 'catalog' ({cat}) cannot be used. \
             Catalogs are supported on Snowflake, Databricks and BigQuery."
        );
    }
    Ok(())
}

/// SELECT the first `n` rows of `[catalog.]schema.table` in the engine's own
/// dialect (SQL Server has no LIMIT clause; it spells the cap TOP).
pub fn select_sample_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
    n: usize,
) -> String {
    // `usize::MAX` is the "Unlimited" cap (Settings -> Performance, `--rows
    // all`, MCP `unlimited`), and anything past a signed 64-bit integer cannot
    // be spelled as a limit anyway: Databricks types 18446744073709551615 as
    // DECIMAL(20,0) and refuses the statement, Postgres overflows bigint.
    // Unlimited means no clause, which is exactly `select_all_sql`.
    if n == usize::MAX || n as u64 > i64::MAX as u64 {
        return select_all_sql(engine, catalog, schema, table);
    }
    let name = qualified_name(engine, catalog, schema, table);
    match engine {
        DbEngine::Mssql => format!("SELECT TOP {n} * FROM {name}"),
        DbEngine::Oracle => format!("SELECT * FROM {name} FETCH FIRST {n} ROWS ONLY"),
        _ => format!("SELECT * FROM {name} LIMIT {n}"),
    }
}

/// Rows in a table, as one cell aliased `n`. Snowflake upper-cases the
/// alias, so read it case-insensitively.
pub fn count_sql(engine: DbEngine, catalog: Option<&str>, schema: &str, table: &str) -> String {
    format!(
        "SELECT COUNT(*) AS n FROM {}",
        qualified_name(engine, catalog, schema, table)
    )
}

/// The whole table, unlimited. This is the base query [`paged_sql`] wraps when
/// the GUI fetches the next page of an open database tab; the row cap has to
/// live in the paging clause, not in the body, or every page would be sliced
/// from the same first N rows.
pub fn select_all_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
) -> String {
    format!(
        "SELECT * FROM {}",
        qualified_name(engine, catalog, schema, table)
    )
}

/// SQL returning the primary-key column names of `schema.table` in ordinal
/// order. One `information_schema` query serves all three engines (Postgres,
/// MySQL/MariaDB, SQL Server all expose these views); values are embedded as
/// string literals with `''` doubling. The `engine` parameter is kept in the
/// signature in case a dialect split ever becomes necessary.
/// One row of [`row_key_sql`]'s result, as [`choose_row_key`] wants it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowKeyCandidate {
    /// `PRIMARY KEY` or `UNIQUE`, straight from `information_schema`.
    pub constraint_type: String,
    pub constraint_name: String,
    pub column_name: String,
    /// `true` when `information_schema.columns.is_nullable` is `YES`.
    pub nullable: bool,
}

/// SQL listing every constraint that could address a single row: the primary
/// key and every unique constraint, with each column's nullability.
///
/// Widened from a primary-key-only lookup because **a UNIQUE constraint over
/// NOT NULL columns identifies a row exactly as well as a primary key** - it
/// is the same server-enforced guarantee - and plenty of real tables (views
/// of legacy schemas, ETL output) have one without ever declaring a PK. Those
/// tables used to open read-only for no reason a user could act on.
///
/// One generic `information_schema` query, as before, so Postgres, MySQL and
/// SQL Server all share it. Nullability comes along because `WHERE col = NULL`
/// never matches, so a nullable unique column would silently update no rows;
/// [`choose_row_key`] drops those candidates.
pub fn row_key_sql(engine: DbEngine, catalog: Option<&str>, schema: &str, table: &str) -> String {
    // Two-level engines only reach this; catalog engines skip the lookup
    // (they expose no discoverable key). Kept in the signature for uniformity.
    let _ = catalog;
    let lit = |s: &str| format!("'{}'", s.replace('\'', "''"));
    // Oracle has no information_schema. Its ALL_* catalog views answer the
    // same question, aliased to the same four columns in the same order, so
    // `choose_row_key` reads either shape without knowing which server spoke.
    // `nullable` is Oracle's Y/N, translated to the YES/NO the caller tests.
    if engine == DbEngine::Oracle {
        return format!(
            "SELECT CASE c.constraint_type WHEN 'P' THEN 'PRIMARY KEY' ELSE 'UNIQUE' END \
               AS constraint_type, c.constraint_name, cc.column_name, \
               CASE col.nullable WHEN 'Y' THEN 'YES' ELSE 'NO' END AS is_nullable \
             FROM all_constraints c \
             JOIN all_cons_columns cc \
               ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name \
             JOIN all_tab_columns col \
               ON col.owner = c.owner AND col.table_name = c.table_name \
              AND col.column_name = cc.column_name \
             WHERE c.constraint_type IN ('P', 'U') \
               AND c.owner = {} AND c.table_name = {} \
             ORDER BY c.constraint_type, c.constraint_name, cc.position",
            lit(schema),
            lit(table)
        );
    }
    format!(
        "SELECT tc.constraint_type, tc.constraint_name, kcu.column_name, c.is_nullable \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON kcu.constraint_name = tc.constraint_name \
          AND kcu.table_schema = tc.table_schema \
          AND kcu.table_name = tc.table_name \
         JOIN information_schema.columns c \
           ON c.table_schema = tc.table_schema \
          AND c.table_name = tc.table_name \
          AND c.column_name = kcu.column_name \
         WHERE tc.constraint_type IN ('PRIMARY KEY', 'UNIQUE') \
           AND tc.table_schema = {} AND tc.table_name = {} \
         ORDER BY tc.constraint_type, tc.constraint_name, kcu.ordinal_position",
        lit(schema),
        lit(table)
    )
}

/// Pick the columns that address one server row, from [`row_key_sql`]'s rows.
///
/// A primary key always wins. Failing that, the **narrowest unique constraint
/// whose columns are all NOT NULL** is taken, ties broken by constraint name
/// so the choice is stable: the baseline snapshot taken at load is addressed
/// by this key at save, and a key that changed between the two would write to
/// the wrong rows. An empty result means the tab stays read-only.
pub fn choose_row_key(rows: &[RowKeyCandidate]) -> Vec<String> {
    use std::collections::BTreeMap;
    // BTreeMap so iteration is by constraint name, i.e. deterministic.
    let mut by_constraint: BTreeMap<(&str, &str), Vec<&RowKeyCandidate>> = BTreeMap::new();
    for r in rows {
        by_constraint
            .entry((r.constraint_type.as_str(), r.constraint_name.as_str()))
            .or_default()
            .push(r);
    }

    if let Some(((_, _), cols)) = by_constraint
        .iter()
        .find(|((ty, _), _)| ty.eq_ignore_ascii_case("PRIMARY KEY"))
    {
        return cols.iter().map(|c| c.column_name.clone()).collect();
    }

    by_constraint
        .iter()
        .filter(|((ty, _), cols)| {
            ty.eq_ignore_ascii_case("UNIQUE") && cols.iter().all(|c| !c.nullable)
        })
        // Narrowest first; BTreeMap order settles a tie by constraint name.
        .min_by_key(|(_, cols)| cols.len())
        .map(|(_, cols)| cols.iter().map(|c| c.column_name.clone()).collect())
        .unwrap_or_default()
}

/// SQL returning a table's metadata for the sidebar "Show metadata..." action,
/// shown as-is in a read-only tab. The result shape differs per engine: the
/// engines with a native `DESCRIBE` return columns (and, for Databricks
/// `EXTENDED`, a "Detailed Table Information" section with location / format /
/// owner / properties); the rest read `information_schema.columns`.
///
/// ponytail: table-level detail beyond columns rides along only where the
/// engine's own command carries it (Databricks today). Add per-engine
/// table-stats queries if a user asks for size/rows on the others.
pub fn table_metadata_sql(
    engine: DbEngine,
    catalog: Option<&str>,
    schema: &str,
    table: &str,
) -> String {
    let name = qualified_name(engine, catalog, schema, table);
    let lit = |s: &str| format!("'{}'", s.replace('\'', "''"));
    match engine {
        // Native DESCRIBE dialects. Databricks EXTENDED adds the detailed
        // table-information block on top of the column list.
        DbEngine::Databricks => format!("DESCRIBE TABLE EXTENDED {name}"),
        DbEngine::Snowflake => format!("DESCRIBE TABLE {name}"),
        DbEngine::ClickHouse => format!("DESCRIBE TABLE {name}"),
        DbEngine::Exasol => format!("DESCRIBE {name}"),
        DbEngine::Trino | DbEngine::Athena => format!("DESCRIBE {name}"),
        // BigQuery has no DESCRIBE: read the dataset's INFORMATION_SCHEMA.
        DbEngine::BigQuery => {
            let prefix = match catalog {
                Some(project) => {
                    format!(
                        "{}.{}",
                        engine.quote_ident(project),
                        engine.quote_ident(schema)
                    )
                }
                None => engine.quote_ident(schema),
            };
            format!(
                "SELECT * FROM {prefix}.INFORMATION_SCHEMA.COLUMNS \
                 WHERE table_name = {} ORDER BY ordinal_position",
                lit(table)
            )
        }
        // Oracle has no information_schema and no DESCRIBE outside SQL*Plus;
        // ALL_TAB_COLUMNS is the same list. `data_default` is left out on
        // purpose: it is a LONG, which costs a second round trip per row.
        DbEngine::Oracle => format!(
            "SELECT column_id, column_name, data_type, nullable, data_length, \
             data_precision, data_scale FROM all_tab_columns \
             WHERE owner = {} AND table_name = {} ORDER BY column_id",
            lit(schema),
            lit(table)
        ),
        // Postgres / Redshift / MySQL / MariaDB / SQL Server: one shared
        // information_schema.columns query (all expose these views).
        DbEngine::Postgres | DbEngine::Redshift | DbEngine::MySql | DbEngine::Mssql => format!(
            "SELECT ordinal_position, column_name, data_type, is_nullable, \
             character_maximum_length, numeric_precision, numeric_scale, column_default \
             FROM information_schema.columns \
             WHERE table_schema = {} AND table_name = {} \
             ORDER BY ordinal_position",
            lit(schema),
            lit(table)
        ),
    }
}

/// The column names out of a [`table_metadata_sql`] result, for the SQL
/// editor's autocomplete. The engines disagree on what the name column is
/// called (`column_name`, Databricks' `col_name`, Snowflake's and
/// ClickHouse's `name`), so it is found by name. Stops at the first blank or
/// `#` row: Databricks appends partition and table detail sections below the
/// column list.
pub fn column_names_from_metadata(t: &DataTable) -> Vec<String> {
    let Some(c) = t.columns.iter().position(|c| {
        matches!(
            c.name.to_ascii_lowercase().as_str(),
            "column_name" | "col_name" | "name"
        )
    }) else {
        return Vec::new();
    };
    t.rows
        .iter()
        .map(|r| r.get(c).map(|v| v.to_string()).unwrap_or_default())
        .take_while(|v| !v.trim().is_empty() && !v.trim_start().starts_with('#'))
        .map(|v| v.trim().to_string())
        .collect()
}

/// SQL finding every table in `catalog` whose name contains `needle` (any
/// case) in one round trip, for the sidebar's deep search. Returns two
/// columns, schema then table, over the same schemas the tree lists.
///
/// `None` where the engine has no such catalogue query: Athena and BigQuery
/// list through their APIs, and a BigQuery `INFORMATION_SCHEMA` is per dataset
/// or per region. The caller then walks schema by schema, which is also its
/// fallback when this query fails (a Databricks `hive_metastore` catalog has
/// no `information_schema`).
pub fn table_search_sql(engine: DbEngine, catalog: Option<&str>, needle: &str) -> Option<String> {
    let q = |s: &str| engine.quote_ident(s);
    let (sch, tbl) = (q("octa_schema"), q("octa_table"));
    let base = match engine {
        DbEngine::Postgres => format!(
            "SELECT table_schema AS {sch}, table_name AS {tbl} FROM information_schema.tables \
             WHERE table_schema NOT IN ('pg_catalog', 'information_schema')"
        ),
        DbEngine::Redshift => format!(
            "SELECT schema_name AS {sch}, table_name AS {tbl} FROM svv_redshift_tables \
             WHERE schema_name NOT IN ('pg_catalog', 'information_schema')"
        ),
        DbEngine::MySql => format!(
            "SELECT table_schema AS {sch}, table_name AS {tbl} FROM information_schema.tables \
             WHERE table_schema NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')"
        ),
        DbEngine::Mssql => format!(
            "SELECT s.name AS {sch}, t.name AS {tbl} FROM sys.tables t \
             JOIN sys.schemas s ON s.schema_id = t.schema_id"
        ),
        DbEngine::Oracle => format!(
            "SELECT o.owner AS {sch}, o.object_name AS {tbl} FROM all_objects o \
             JOIN all_users u ON u.username = o.owner \
             WHERE u.oracle_maintained = 'N' AND o.object_type IN ('TABLE', 'VIEW')"
        ),
        DbEngine::ClickHouse => {
            format!("SELECT database AS {sch}, name AS {tbl} FROM system.tables")
        }
        DbEngine::Exasol => {
            format!("SELECT TABLE_SCHEMA AS {sch}, TABLE_NAME AS {tbl} FROM EXA_ALL_TABLES")
        }
        // The catalog warehouses keep one information_schema per catalog.
        DbEngine::Trino | DbEngine::Databricks | DbEngine::Snowflake => {
            let info = match catalog {
                Some(c) => format!("{}.information_schema.tables", q(c)),
                None => "information_schema.tables".to_string(),
            };
            format!(
                "SELECT table_schema AS {sch}, table_name AS {tbl} FROM {info} \
                 WHERE LOWER(table_schema) <> 'information_schema'"
            )
        }
        DbEngine::Athena | DbEngine::BigQuery => return None,
    };
    let pred = engine.contains_predicate(&["octa_table".to_string()], needle, false)?;
    // No `AS` before the derived table's alias: Oracle refuses it.
    Some(format!(
        "SELECT {sch}, {tbl} FROM ({base}) found WHERE {pred}"
    ))
}

/// Render one cell as a SQL literal in the engine's dialect. Strings quote
/// with `''` doubling; booleans are `TRUE`/`FALSE` except SQL Server's BIT
/// (`1`/`0`); NULL for null and non-finite floats.
pub(crate) fn sql_literal(engine: DbEngine, cell: &crate::data::CellValue) -> String {
    use crate::data::CellValue;
    let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
    match cell {
        CellValue::Null => "NULL".to_string(),
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) => {
            if f.is_finite() {
                f.to_string()
            } else {
                "NULL".to_string()
            }
        }
        CellValue::Bool(b) => match engine {
            // SQL Server's BIT and Oracle's NUMBER(1), which is how every
            // schema older than 23c spells a boolean.
            DbEngine::Mssql | DbEngine::Oracle => if *b { "1" } else { "0" }.to_string(),
            _ => if *b { "TRUE" } else { "FALSE" }.to_string(),
        },
        // Oracle reads a bare quoted string into a DATE through NLS_DATE_FORMAT,
        // which defaults to `DD-MON-RR` and would reject every value Octa
        // renders. The ANSI literal prefixes say which format this is.
        CellValue::Date(s) if engine == DbEngine::Oracle => format!("DATE {}", quote(s)),
        CellValue::DateTime(s) if engine == DbEngine::Oracle => {
            format!("TIMESTAMP {}", quote(s))
        }
        // Text in the engine's own literal rules (backslashes, quotes, line
        // breaks, SQL Server's N prefix); see `pushdown::dialect::str_lit`.
        CellValue::String(s) | CellValue::Nested(s) => super::pushdown::dialect::str_lit(engine, s),
        CellValue::Date(s) | CellValue::DateTime(s) => quote(s),
        // Binary in each server's own hex form. The display text in the last
        // arm would be stored as those characters, not as the bytes.
        CellValue::Binary(b)
            if matches!(
                engine,
                DbEngine::Postgres | DbEngine::Mssql | DbEngine::MySql
            ) =>
        {
            let hex: String = b.iter().map(|byte| format!("{byte:02x}")).collect();
            match engine {
                // Whatever `standard_conforming_strings` says.
                DbEngine::Postgres => format!("decode('{hex}', 'hex')"),
                DbEngine::Mssql => format!("0x{hex}"),
                _ => format!("X'{hex}'"),
            }
        }
        CellValue::Binary(_) => quote(&cell.to_string()),
    }
}

/// Drop `target` if it is there, in the engine's own dialect.
///
/// Oracle is the odd one out: it had no `DROP TABLE IF EXISTS` before 23c, and
/// the PL/SQL wrapper below is the standard idiom for it. It re-raises
/// anything that is not ORA-00942 ("table or view does not exist"), so a
/// permission error still stops the write instead of being swallowed.
pub(crate) fn drop_if_exists_sql(engine: DbEngine, target: &str) -> String {
    match engine {
        DbEngine::Oracle => {
            let inner = target.replace('\'', "''");
            format!(
                "BEGIN EXECUTE IMMEDIATE 'DROP TABLE {inner}'; \
                 EXCEPTION WHEN OTHERS THEN IF SQLCODE != -942 THEN RAISE; END IF; END;"
            )
        }
        _ => format!("DROP TABLE IF EXISTS {target}"),
    }
}

/// One statement inserting every already-rendered `(v1, v2)` tuple in
/// `values`.
///
/// Oracle has no multi-row `VALUES` list; `INSERT ALL ... SELECT * FROM dual`
/// is how it spells the same thing, and it matters because the alternative is
/// one round trip per row.
pub(crate) fn insert_rows_sql(
    engine: DbEngine,
    target: &str,
    col_list: &str,
    values: &[String],
) -> String {
    match engine {
        DbEngine::Oracle => {
            let all: Vec<String> = values
                .iter()
                .map(|v| format!("INTO {target} ({col_list}) VALUES {v}"))
                .collect();
            format!("INSERT ALL {} SELECT * FROM dual", all.join(" "))
        }
        _ => format!(
            "INSERT INTO {target} ({col_list}) VALUES {}",
            values.join(", ")
        ),
    }
}

#[cfg(test)]
#[path = "sql_builders_tests.rs"]
mod tests;
