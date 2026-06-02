//! Internal helpers shared by `run_query` and [`SqlWorkspace`].
//!
//! Everything here is `pub(super)` so the two surfaces can compose freely
//! without the helpers leaking out of the `octa::sql` module. The easter-egg
//! tables (`octopuses`, `stars`, `h2o`) live here too because both the
//! one-shot path and the workspace path need to short-circuit them before
//! handing the query to DuckDB.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use duckdb::{Connection, types::ValueRef};

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
    pub table: DataTable,
}

/// Classify `query` by its leading keyword. Mutating statements do not return
/// rows via `query()` in DuckDB's Rust bindings, so they must be run through
/// `execute()` instead.
pub(super) fn is_mutation(query: &str) -> bool {
    let first = query
        .split(|c: char| c.is_whitespace() || c == '(')
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        first.as_str(),
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

/// Create a TEMP TABLE called `name` and append every row of `table` into it
/// via DuckDB's appender API. Shared by both the one-shot `run_query` path
/// (which builds a fresh connection per call) and [`SqlWorkspace`] (which
/// reuses one persistent connection across many calls).
pub(super) fn register_table_into(conn: &Connection, name: &str, table: &DataTable) -> Result<()> {
    if table.columns.is_empty() {
        return Ok(());
    }
    let cols_sql: Vec<String> = table
        .columns
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
            "CREATE TEMP TABLE {} ({})",
            quote_ident(name),
            cols_sql.join(", ")
        ),
        [],
    )?;

    if table.row_count() == 0 {
        return Ok(());
    }

    let mut app = conn
        .appender(name)
        .with_context(|| format!("opening DuckDB appender for `{name}`"))?;
    for row_idx in 0..table.row_count() {
        let row: Vec<duckdb::types::Value> = (0..table.col_count())
            .map(|c| cell_to_value(table.get(row_idx, c).unwrap_or(&CellValue::Null)))
            .collect();
        app.append_row(duckdb::appender_params_from_iter(row))?;
    }
    Ok(())
}

pub(super) fn execute_query(conn: &Connection, query: &str) -> Result<DataTable> {
    let trimmed = query.trim();
    let mut stmt = conn.prepare(trimmed)?;
    let mut q = stmt.query([])?;

    let stmt_ref = q
        .as_ref()
        .ok_or_else(|| anyhow!("Query produced no statement"))?;
    let col_count = stmt_ref.column_count();
    let columns: Vec<ColumnInfo> = (0..col_count)
        .map(|i| ColumnInfo {
            name: stmt_ref
                .column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| format!("col{i}")),
            data_type: "Utf8".to_string(),
        })
        .collect();

    let mut rows: Vec<Vec<CellValue>> = Vec::new();
    while let Some(r) = q.next()? {
        let mut row = Vec::with_capacity(col_count);
        for i in 0..col_count {
            row.push(value_ref_to_cell(r.get_ref(i)?));
        }
        rows.push(row);
    }

    Ok(DataTable {
        columns,
        rows,
        edits: HashMap::new(),
        source_path: None,
        format_name: Some("SQL Result".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}

pub(super) fn arrow_to_duckdb_type(arrow_ty: &str) -> &'static str {
    match arrow_ty {
        "Int64" | "Int32" | "Int16" | "Int8" => "BIGINT",
        "Float64" | "Float32" => "DOUBLE",
        "Boolean" => "BOOLEAN",
        "Date32" => "DATE",
        "Timestamp(Microsecond, None)" | "Timestamp(Millisecond, None)" => "TIMESTAMP",
        "Binary" | "LargeBinary" => "BLOB",
        _ => "VARCHAR",
    }
}

pub(super) fn cell_to_value(v: &CellValue) -> duckdb::types::Value {
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
        CellValue::Binary(b) => Value::Blob(b.clone()),
    }
}

pub(super) fn value_ref_to_cell(v: ValueRef<'_>) -> CellValue {
    use duckdb::types::ValueRef as V;
    match v {
        V::Null => CellValue::Null,
        V::Boolean(b) => CellValue::Bool(b),
        V::TinyInt(i) => CellValue::Int(i as i64),
        V::SmallInt(i) => CellValue::Int(i as i64),
        V::Int(i) => CellValue::Int(i as i64),
        V::BigInt(i) => CellValue::Int(i),
        V::HugeInt(i) => CellValue::String(i.to_string()),
        V::UTinyInt(i) => CellValue::Int(i as i64),
        V::USmallInt(i) => CellValue::Int(i as i64),
        V::UInt(i) => CellValue::Int(i as i64),
        V::UBigInt(i) => CellValue::String(i.to_string()),
        V::Float(f) => CellValue::Float(f as f64),
        V::Double(f) => CellValue::Float(f),
        V::Decimal(d) => CellValue::String(d.to_string()),
        V::Timestamp(unit, ts) => crate::formats::duckdb_reader::duckdb_timestamp_to_cell(unit, ts),
        V::Text(t) => match std::str::from_utf8(t) {
            Ok(s) => CellValue::String(s.to_string()),
            Err(_) => CellValue::Binary(t.to_vec()),
        },
        V::Blob(b) => CellValue::Binary(b.to_vec()),
        V::Date32(d) => crate::formats::duckdb_reader::duckdb_date32_to_cell(d),
        V::Time64(unit, t) => crate::formats::duckdb_reader::duckdb_time_to_cell(unit, t),
        other => CellValue::String(format!("{other:?}")),
    }
}

pub(super) fn quote_ident(name: &str) -> String {
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
        edits: HashMap::new(),
        source_path: None,
        format_name: Some("\u{1f419} Octopuses".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
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
        edits: HashMap::new(),
        source_path: None,
        format_name: Some("\u{2728} Stars".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
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
        edits: HashMap::new(),
        source_path: None,
        format_name: Some("\u{1f30a} H2O".to_string()),
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
        db_meta: None,
    })
}
