//! Internal helpers shared by `run_query` and [`SqlWorkspace`].
//!
//! Everything here is `pub(super)` so the two surfaces can compose freely
//! without the helpers leaking out of the `octa::sql` module. The easter-egg
//! tables (`octopuses`, `stars`, `h2o`) live here too because both the
//! one-shot path and the workspace path need to short-circuit them before
//! handing the query to DuckDB.

use anyhow::{Context, Result, anyhow};
use duckdb::Connection;

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Classification of a SQL statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// A read-only query (SELECT etc.). `table` holds the result rows.
    Select,
    /// A mutation (INSERT / UPDATE / DELETE / ...). `table` holds the full
    /// contents of `data` after the statement ran, suitable for replacing the
    /// base table in the caller's UI.
    Mutation,
}

/// Result of executing a user query.
#[derive(Debug, Clone)]
pub struct QueryOutcome {
    pub kind: QueryKind,
    /// Number of rows reported affected by a mutation (None for SELECT).
    pub affected: Option<usize>,
    /// For SELECT: the query result. For mutations: the post-mutation contents
    /// of `data`, rebuilt with the original table's column schema preserved.
    /// When `created` is set: the full contents of the table that statement
    /// created, with its declared column types.
    pub table: DataTable,
    /// Name of the table or view a `CREATE ...` statement added to the
    /// workspace's own catalog. The workspace drops it again after reading
    /// it into `table`: the caller owns it now (the GUI opens it as a tab).
    /// A CREATE inside an attached database leaves this `None`.
    pub created: Option<String>,
    /// The row cap left rows of the result behind, so `table` is not all of it.
    pub cut: bool,
}

/// Classify `query` by its leading keyword. Mutating statements do not return
/// rows via `query()` in DuckDB's Rust bindings, so they must be run through
/// `execute()` instead. Also the write-classification the live-DB read-only
/// gate uses (`crate::db::ensure_write_allowed`), hence `pub`.
pub fn is_mutation(query: &str) -> bool {
    matches!(
        first_keyword(query).as_str(),
        "INSERT"
            | "UPDATE"
            | "DELETE"
            | "REPLACE"
            | "MERGE"
            | "CREATE"
            | "DROP"
            | "ALTER"
            | "TRUNCATE"
            | "ATTACH"
            | "DETACH"
            | "COPY"
            | "SET"
            | "PRAGMA"
    )
}

/// Whether `query` holds a semicolon anywhere but at its end, strings and
/// comments included. Blunt on purpose: engines lex those differently, so
/// only this reading cannot be fooled into missing a second statement.
pub fn has_inner_semicolon(query: &str) -> bool {
    query
        .trim()
        .trim_end_matches(|c: char| c == ';' || c.is_whitespace())
        .contains(';')
}

/// Whether `query` is a `CREATE ...` statement (table, view, or anything
/// else DuckDB accepts after the keyword). `SqlWorkspace::execute` diffs the
/// catalog around these to hand a freshly created table to the caller.
pub(super) fn is_create(query: &str) -> bool {
    first_keyword(query) == "CREATE"
}

/// `query` from its first keyword on: leading whitespace, `(`, `;` and
/// comments skipped, so `-- note\nDELETE ...` still reads as a DELETE (it read
/// as `--` once, which no check took for a write). A block comment ends at its
/// first `*/`; one that holds another `/*` is left in place, because
/// PostgreSQL, SQL Server and Spark nest them and the others do not, so no
/// single reading of what follows is safe.
pub fn statement_start(query: &str) -> &str {
    let mut rest = query;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '(' || c == ';');
        if let Some(r) = rest.strip_prefix("--").or_else(|| rest.strip_prefix('#')) {
            rest = r.split_once('\n').map_or("", |(_, after)| after);
        } else if let Some(r) = rest.strip_prefix("/*")
            && let Some((body, after)) = r.split_once("*/")
            && !body.contains("/*")
        {
            rest = after;
        } else {
            return rest;
        }
    }
}

/// Leading keyword of `query`, upper-cased; empty for a blank query or one
/// that does not start with a word.
pub fn first_keyword(query: &str) -> String {
    statement_start(query)
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
}

/// Create a table called `name` and append every row of `table` into it
/// via DuckDB's appender API, in one go. The SQL panel's copy of `data`
/// uses the two halves itself ([`create_table_for`], then [`append_rows`]
/// per slice).
pub(super) fn register_table_into(conn: &Connection, name: &str, table: &DataTable) -> Result<()> {
    register_table_as(conn, name, table, &table.columns)
}

/// [`register_table_into`] with `columns` as the SQL types instead of the
/// table's own; same names and order.
pub(super) fn register_table_as(
    conn: &Connection,
    name: &str,
    table: &DataTable,
    columns: &[ColumnInfo],
) -> Result<()> {
    if columns.is_empty() {
        return Ok(());
    }
    create_table_for(conn, name, columns)?;
    append_rows(conn, name, table, columns)
}

/// The empty table [`register_table_into`] fills, typed from `columns`.
pub(super) fn create_table_for(
    conn: &Connection,
    name: &str,
    columns: &[ColumnInfo],
) -> Result<()> {
    let cols_sql: Vec<String> = columns
        .iter()
        .map(|c| {
            format!(
                "{} {}",
                quote_ident(&c.name),
                arrow_to_duckdb_type(&c.data_type)
            )
        })
        .collect();
    conn.execute(
        &format!(
            "CREATE TABLE {} ({})",
            quote_ident(name),
            cols_sql.join(", ")
        ),
        [],
    )?;
    Ok(())
}

/// Append every row of `table` (pending edits read through) to `name`, whose
/// SQL types came from `columns`.
pub(super) fn append_rows(
    conn: &Connection,
    name: &str,
    table: &DataTable,
    columns: &[ColumnInfo],
) -> Result<()> {
    if table.row_count() == 0 {
        return Ok(());
    }
    // A float goes into a DECIMAL as the digits it shows: DuckDB's own
    // double-to-decimal cast scales the binary value and can land one off in
    // the last digit past about 15 digits.
    let decimal: Vec<bool> = columns
        .iter()
        .map(|c| crate::data::decimal::decimal_parts(&c.data_type).is_some())
        .collect();

    let mut app = conn
        .appender(name)
        .with_context(|| format!("opening DuckDB appender for `{name}`"))?;
    for row_idx in 0..table.row_count() {
        let row: Vec<duckdb::types::Value> = (0..table.col_count())
            .map(
                |c| match table.get(row_idx, c).unwrap_or(&CellValue::Null) {
                    CellValue::Float(f) if decimal.get(c) == Some(&true) && f.is_finite() => {
                        duckdb::types::Value::Text(f.to_string())
                    }
                    cell => cell_to_value(cell),
                },
            )
            .collect();
        app.append_row(duckdb::appender_params_from_iter(row))?;
    }
    Ok(())
}

pub(super) fn execute_query(conn: &Connection, query: &str) -> Result<DataTable> {
    query_table(conn, query, false)
}

/// [`execute_query`], and whether the row cap left rows of the result behind.
pub(super) fn execute_query_cut(conn: &Connection, query: &str) -> Result<(DataTable, bool)> {
    query_table_cut(conn, query, false)
}

/// [`execute_query`]; with `typed`, each column carries the result's Arrow
/// type (`Int32`, `Float64`, `Date32`, ...) instead of `Utf8`. The live
/// DuckDB file connection wants that: its tab sorts, filters and writes back
/// by type, like every other database tab.
pub(crate) fn query_table(conn: &Connection, query: &str, typed: bool) -> Result<DataTable> {
    query_table_cut(conn, query, typed).map(|(table, _)| table)
}

/// [`query_table`], and whether the row cap left rows behind: it looks one row
/// past the cap, so a result that only just fits is not called cut.
fn query_table_cut(conn: &Connection, query: &str, typed: bool) -> Result<(DataTable, bool)> {
    let trimmed = query.trim();
    let mut stmt = conn.prepare(trimmed)?;
    let mut q = stmt.query([])?;

    let stmt_ref = q
        .as_ref()
        .ok_or_else(|| anyhow!("Query produced no statement"))?;
    let col_count = stmt_ref.column_count();
    // The driver panics on a nanosecond time (`unreachable!`, row.rs), which took
    // the whole process down. This SQL is the user's, so it cannot be rewritten to
    // fetch the column as text; they can.
    for i in 0..col_count {
        if format!("{}", stmt_ref.column_type(i)) == "Time64(ns)" {
            let name = stmt_ref.column_name(i).map_or("?", |s| s.as_str());
            return Err(anyhow!(
                "Octa cannot read column {name}, a TIME_NS. Select it as text instead: \
                 CAST({} AS VARCHAR)",
                quote_ident(name)
            ));
        }
    }
    // DuckDB already bakes the session timezone into a TIMESTAMPTZ result
    // column's Arrow type (`Timestamp(us, "Europe/Berlin")`), so the zone the
    // values should be read on comes straight off the statement.
    let zones: Vec<Option<String>> = (0..col_count)
        .map(|i| {
            let ty = format!("{}", stmt_ref.column_type(i));
            crate::formats::parquet_reader::timestamp_tz_from_type_name(&ty).map(str::to_string)
        })
        .collect();
    let columns: Vec<ColumnInfo> = (0..col_count)
        .map(|i| ColumnInfo {
            name: stmt_ref
                .column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{i}")),
            data_type: {
                let ty = format!("{}", stmt_ref.column_type(i));
                // DuckDB's INTERVAL is Arrow's month-day-nano; Octa types it Duration.
                let ty = if ty.starts_with("Interval") {
                    "Duration(Microsecond)".to_string()
                } else {
                    ty
                };
                if typed || crate::data::times::TimeKind::of_type(&ty).is_some() {
                    ty
                } else {
                    "Utf8".to_string()
                }
            },
        })
        .collect();

    // Results honour the same initial-load row cap as file opens: an
    // unbounded SELECT must not materialise the whole result in memory.
    let cap = crate::formats::initial_load_rows();
    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    let mut cut = false;
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(col_count);
        for (i, tz) in zones.iter().enumerate() {
            row.push(crate::formats::duckdb_reader::duckdb_value_to_cell_tz(
                r.get_ref(i)?,
                tz.as_deref(),
            ));
        }
        rows.push(row);
        if rows.len() >= cap {
            cut = q.next()?.is_some();
            break;
        }
    }

    let mut table = DataTable {
        columns,
        rows,
        format_name: Some("SQL Result".to_string()),
        ..DataTable::empty()
    };
    crate::data::times::settle_interval_columns(&mut table);
    Ok((table, cut))
}

pub(super) fn arrow_to_duckdb_type(arrow_ty: &str) -> String {
    // A real decimal, so SUM and AVG stay exact in the SQL panel.
    if let Some(decimal) = crate::data::decimal::duckdb_type(arrow_ty) {
        return decimal;
    }
    match crate::data::times::TimeKind::of_type(arrow_ty) {
        Some(crate::data::times::TimeKind::Time) => return "TIME".to_string(),
        Some(crate::data::times::TimeKind::Duration) => return "INTERVAL".to_string(),
        None => {}
    }
    match arrow_ty {
        "Int64" | "Int32" | "Int16" | "Int8" => "BIGINT",
        "Float64" | "Float32" => "DOUBLE",
        "Boolean" => "BOOLEAN",
        "Date32" => "DATE",
        "Timestamp(Microsecond, None)" | "Timestamp(Millisecond, None)" => "TIMESTAMP",
        "Binary" | "LargeBinary" => "BLOB",
        _ => "VARCHAR",
    }
    .to_string()
}

pub(crate) fn cell_to_value(v: &CellValue) -> duckdb::types::Value {
    use duckdb::types::Value;
    match v {
        CellValue::Null => Value::Null,
        CellValue::Bool(b) => Value::Boolean(*b),
        CellValue::Int(n) => Value::BigInt(*n),
        CellValue::Float(f) => Value::Double(*f),
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => Value::Text(s.clone()),
        CellValue::Time(ns) => {
            Value::Time64(duckdb::types::TimeUnit::Microsecond, ns.div_euclid(1_000))
        }
        CellValue::Duration(ns) => Value::Interval {
            months: 0,
            days: 0,
            nanos: *ns,
        },
        CellValue::Binary(b) => Value::Blob(b.clone()),
    }
}

/// Quote a DuckDB (or SQLite) identifier: double quotes, inner quotes doubled.
pub fn quote_ident(name: &str) -> String {
    let escaped = name.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

/// Easter egg: `SELECT * FROM octopuses` (case-insensitive, optional trailing
/// semicolon) returns a hand-crafted little aquarium. Anything more elaborate
/// (extra clauses, joins, projections) falls through to DuckDB unchanged.
pub(super) fn octopuses_easter_egg(query: &str) -> Option<DataTable> {
    let normalized = query
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized != "select * from octopuses" {
        return None;
    }
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "species".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "tentacles".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "favorite_snack".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "iq".into(),
            data_type: "Int64".into(),
        },
    ];
    let rows = vec![
        vec![
            CellValue::Int(1),
            CellValue::String("Inky".into()),
            CellValue::String("Common octopus".into()),
            CellValue::Int(8),
            CellValue::String("Crab".into()),
            CellValue::Int(140),
        ],
        vec![
            CellValue::Int(2),
            CellValue::String("Otto".into()),
            CellValue::String("Giant Pacific octopus".into()),
            CellValue::Int(8),
            CellValue::String("Lego brick".into()),
            CellValue::Int(155),
        ],
        vec![
            CellValue::Int(3),
            CellValue::String("Paul".into()),
            CellValue::String("Common octopus".into()),
            CellValue::Int(8),
            CellValue::String("Mussel (predicted)".into()),
            CellValue::Int(200),
        ],
        vec![
            CellValue::Int(4),
            CellValue::String("Mimi".into()),
            CellValue::String("Mimic octopus".into()),
            CellValue::Int(8),
            CellValue::String("Whatever the neighbors brought".into()),
            CellValue::Int(160),
        ],
        vec![
            CellValue::Int(5),
            CellValue::String("Blue".into()),
            CellValue::String("Blue-ringed octopus".into()),
            CellValue::Int(8),
            CellValue::String("Tiny shrimp (do not pet)".into()),
            CellValue::Int(120),
        ],
    ];
    Some(DataTable {
        columns,
        rows,
        format_name: Some("\u{1f419} Octopuses".to_string()),
        ..DataTable::empty()
    })
}

/// Easter egg: `SELECT * FROM stars` returns ten of the brightest known stars.
pub(super) fn stars_easter_egg(query: &str) -> Option<DataTable> {
    let normalized = query
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized != "select * from stars" {
        return None;
    }
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "constellation".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "apparent_magnitude".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "distance_ly".into(),
            data_type: "Float64".into(),
        },
    ];
    let entries: &[(i64, &str, &str, f64, f64)] = &[
        (1, "Sirius", "Canis Major", -1.46, 8.6),
        (2, "Canopus", "Carina", -0.74, 310.0),
        (3, "Arcturus", "Boötes", -0.05, 36.7),
        (4, "Vega", "Lyra", 0.03, 25.0),
        (5, "Rigel", "Orion", 0.13, 860.0),
        (6, "Procyon", "Canis Minor", 0.34, 11.46),
        (7, "Betelgeuse", "Orion", 0.50, 642.5),
        (8, "Altair", "Aquila", 0.77, 16.73),
        (9, "Aldebaran", "Taurus", 0.85, 65.3),
        (10, "Antares", "Scorpius", 1.06, 555.0),
    ];
    let rows = entries
        .iter()
        .map(|(id, name, con, mag, ly)| {
            vec![
                CellValue::Int(*id),
                CellValue::String((*name).to_string()),
                CellValue::String((*con).to_string()),
                CellValue::Float(*mag),
                CellValue::Float(*ly),
            ]
        })
        .collect();
    Some(DataTable {
        columns,
        rows,
        format_name: Some("\u{2728} Stars".to_string()),
        ..DataTable::empty()
    })
}

/// Easter egg: `SELECT * FROM h2o` returns a hand-crafted table of ocean zones.
pub(super) fn h2o_easter_egg(query: &str) -> Option<DataTable> {
    let normalized = query
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized != "select * from h2o" {
        return None;
    }
    let columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "zone".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "depth_m".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "temperature_c".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "salinity_psu".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "pressure_atm".into(),
            data_type: "Float64".into(),
        },
        ColumnInfo {
            name: "fact".into(),
            data_type: "Utf8".into(),
        },
    ];
    let entries: &[(i64, &str, i64, f64, f64, f64, &str)] = &[
        (
            1,
            "Sunlight (Epipelagic)",
            100,
            20.0,
            35.0,
            10.0,
            "Where photosynthesis happens; most marine life lives here.",
        ),
        (
            2,
            "Twilight (Mesopelagic)",
            500,
            10.0,
            34.9,
            50.0,
            "Bioluminescence becomes the primary light source.",
        ),
        (
            3,
            "Midnight (Bathypelagic)",
            2000,
            4.0,
            34.8,
            200.0,
            "Pitch dark, near-freezing, home of the giant squid.",
        ),
        (
            4,
            "Abyss (Abyssopelagic)",
            5000,
            2.5,
            34.7,
            500.0,
            "Vast plains of fine sediment; pressure crushes most submarines.",
        ),
        (
            5,
            "Hadal (Hadalpelagic)",
            10000,
            1.5,
            34.7,
            1000.0,
            "Ocean trenches: Mariana, Tonga, Kermadec.",
        ),
        (
            6,
            "Surface mixed layer",
            20,
            22.0,
            35.2,
            3.0,
            "Wind and waves keep this layer thoroughly stirred.",
        ),
        (
            7,
            "Thermocline",
            300,
            14.0,
            35.1,
            30.0,
            "Sharp temperature drop; sound waves bend through it.",
        ),
        (
            8,
            "Halocline",
            150,
            16.0,
            36.5,
            15.0,
            "Salinity gradient: freshwater plumes float on saltwater.",
        ),
        (
            9,
            "Hydrothermal vent",
            2500,
            350.0,
            34.6,
            250.0,
            "Superheated water hosts entire chemosynthetic ecosystems.",
        ),
        (
            10,
            "Polar ice cap base",
            5,
            -1.8,
            32.0,
            1.5,
            "Saltwater stays liquid below the freshwater freezing point.",
        ),
    ];
    let rows = entries
        .iter()
        .map(|(id, zone, depth, temp, sal, pres, fact)| {
            vec![
                CellValue::Int(*id),
                CellValue::String((*zone).to_string()),
                CellValue::Int(*depth),
                CellValue::Float(*temp),
                CellValue::Float(*sal),
                CellValue::Float(*pres),
                CellValue::String((*fact).to_string()),
            ]
        })
        .collect();
    Some(DataTable {
        columns,
        rows,
        format_name: Some("\u{1f30a} H2O".to_string()),
        ..DataTable::empty()
    })
}
