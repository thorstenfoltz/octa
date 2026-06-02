//! CLI dispatch. Flag-style: one of `--schema`, `--head`, `--convert`,
//! `--sql`, `--export-schema`, `--mcp` selects the action; the file
//! argument(s) follow the flag. Mutually exclusive: passing two action
//! flags is a parse error.
//!
//! Adding a new action: define the flag on [`Cli`] with `group = "action"`,
//! add a variant to [`Action`] + an arm to [`Cli::detect_action`], drop a
//! handler file under `src/cli/<verb>.rs`, and add a match arm in
//! [`dispatch`]. `--mcp` is the one exception - it's dispatched in
//! `main.rs` because it needs a tokio runtime, which the GUI path
//! deliberately avoids constructing.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};

use octa::data::schema_export::SchemaTarget;

pub mod compare_schemas;
pub mod convert;
pub mod describe;
pub mod diff;
pub mod export_schema;
pub mod head;
pub mod output;
pub mod sample;
pub mod schema;
pub mod sql;
pub mod tail;
pub mod unique_columns;
pub mod validate_schema;

/// Long help shown after the option list. Includes worked examples for
/// every action and the SQL form the user asked to surface. Plain-text
/// (no ANSI) because clap dims it on capable terminals automatically.
const AFTER_HELP: &str = "\
Examples:
  GUI (no action flag):
    octa data.parquet                  # open one file in the GUI
    octa a.csv b.json                  # open multiple files (one tab each)

  Schema preview:
    octa --schema data.parquet
    octa -f json --schema data.csv

  First N rows:
    octa --head data.csv               # default 20 rows
    octa --head data.csv -n 5
    octa --head data.parquet -n 100 -f json

  Last N rows / random sample:
    octa --tail data.csv -n 5
    octa --sample data.parquet -n 20 --seed 1

  Format conversion:
    octa --convert in.csv out.parquet
    octa --convert data.json data.xlsx

  SQL query:
    octa --sql sales.parquet -q 'SELECT region, SUM(amount) FROM data \
GROUP BY region'
    octa --sql data.csv -q 'SELECT * FROM data WHERE id > 100 LIMIT 10' -f json
    octa --sql data.csv -q 'DESCRIBE data'
    octa --sql huge.parquet -q 'SELECT count(*) FROM data' --rows all
    octa --head huge.parquet -n 100 --rows 10,000,000

  SQL with multi-table JOIN (extras + ATTACH):
    octa --sql sales.parquet \
         --sql-table customers=customers.csv \
         -q 'SELECT c.name, SUM(d.amount) FROM data d \
JOIN customers c ON d.cid = c.cid GROUP BY c.name'
    octa --sql sales.parquet \
         --sql-attach wh=warehouse.duckdb \
         -q 'SELECT count(*) FROM data d JOIN wh.main.products p \
ON d.cid = p.cid'

  SQL write-back to a DuckDB / SQLite warehouse:
    octa --sql sales.parquet -q 'SELECT region, SUM(amount) AS total \
FROM data GROUP BY region' \
         --sql-write-to analytics.duckdb \
         --sql-write-schema reports \
         --sql-write-table q4_summary
    octa --sql data.csv -q 'SELECT * FROM data WHERE active=1' \
         --sql-write-to users.sqlite --sql-write-table active_users \
         --sql-write-mode replace

  Schema export / codegen:
    octa --export-schema data.parquet -t snowflake
    octa -e data.csv --target pydantic
    octa -e schema.parquet -t databricks

  Schema diff between two files:
    octa --compare-schemas v1.parquet v2.parquet
    octa --compare-schemas a.sqlite b.sqlite --table-a users --table-b users -f json

  Row-level data diff between two files:
    octa --diff v1.csv v2.csv
    octa --diff a.parquet b.parquet -f json

  Validate a file against a JSON Schema (CI-pipeable, exit 1 on drift):
    octa --validate-schema sales.parquet --expect-schema sales.schema.json
    octa --validate-schema data.csv --expect-schema schema.json -f json

  One-shot file snapshot (format + size + schema + preview):
    octa --describe data.parquet
    octa --describe data.csv --sample-rows 10 -f json
    octa --describe users.sqlite --table customers

  Find unique columns / primary-key candidates:
    octa --unique-columns users.csv
    octa --unique-columns sales.parquet --max-combo 2 -f json

  MCP server (stdio):
    octa --mcp                         # serve MCP over stdin/stdout

Notes:
  * The file is registered with DuckDB as a table named `data` in --sql.
  * --convert writes the output via FormatRegistry; read-only target
    formats (SAS, R datasets, HDF5, NetCDF) are rejected with a clear
    error.
  * --export-schema / -e renders FILE's column list as SQL DDL (Postgres,
    MySQL, SQLite, Databricks, Snowflake), a Pydantic v2 model, a
    TypeScript interface, JSON Schema, or a Rust struct; pick the target
    with -t / --target (default postgres). Output goes to stdout.
  * --format / -f governs stdout for every action that prints a table.
  * --compare-schemas reads only the column metadata from both files,
    so no row data is touched. The output is a four-column table:
    status / column / type_a / type_b. `status` is one of `common`,
    `only_in_a`, `only_in_b`, `type_mismatch`.
  * --diff compares the two files' rows by whole-row content (every
    column, positionally) and prints the rows unique to each side with
    a leading `status` column (`only_in_a` / `only_in_b`); a summary
    line (shared / only-in-A / only-in-B counts) is written to stderr.
    The files should share the same column order to diff meaningfully.
  * --validate-schema checks FILE's columns against the JSON Schema in
    --expect-schema. Exit code is 0 when every column matches by name
    and type, 1 otherwise. JSON Schema `type` values the parser can't
    recognise default to `Utf8` and are reported on stderr.
  * --describe is the one-call orientation snapshot. The TSV / CSV
    output is a vertical `field / value` table; `-f json` returns the
    same data as a structured JSON object (mirrors the MCP shape).
  * --unique-columns reports per-column distinct counts + uniqueness;
    `is_unique` is true only when no nulls AND every value distinct.
    Use --max-combo to also test column pairs / triples.
  * --mcp starts an MCP (Model Context Protocol) server on stdio. Tools:
    read_table, tail, sample, schema, list_tables, count_rows, run_sql,
    convert, export_schema, profile, find_duplicates, value_frequency,
    search, compare_schemas, diff_tables, validate_against_schema,
    describe_file, unique_columns. Default row + cell caps come from
    Settings -> MCP.
  * Action flags are mutually exclusive - pick one. Without any, Octa
    launches its GUI.
  * --rows overrides the initial-load row cap for this invocation
    (default 5,000,000). Pass `all` to load every row. Useful for
    --sql / --head / --convert against very large Parquet/CSV files.
";

/// Top-level CLI. Action flags (`--schema`, `--head`, `--convert`, `--sql`)
/// share a mutually-exclusive group; positional `FILES` are forwarded to
/// the GUI only when no action flag is set.
#[derive(Parser, Debug)]
#[command(
    name = "octa",
    version,
    about = "Multi-format data viewer and editor",
    long_about = "Octa is a desktop data viewer with an interactive GUI and a \
                  small CLI surface. Without any action flag it launches the GUI \
                  with whatever files you pass; with one of the action flags it \
                  runs that action and exits.",
    after_help = AFTER_HELP,
    after_long_help = AFTER_HELP,
    disable_help_flag = true
)]
pub struct Cli {
    /// Print column schema (name + data type) for FILE to stdout.
    #[arg(long, value_name = "FILE", group = "action")]
    pub schema: Option<PathBuf>,

    /// Print the first N rows of FILE (default 20, override with -n / --lines).
    #[arg(long, value_name = "FILE", group = "action")]
    pub head: Option<PathBuf>,

    /// Print the last N rows of FILE (default 20, override with -n / --lines).
    #[arg(long, value_name = "FILE", group = "action")]
    pub tail: Option<PathBuf>,

    /// Print a random N-row sample of FILE (default 20, override with
    /// -n / --lines). Reproducible for a given --seed.
    #[arg(long, value_name = "FILE", group = "action")]
    pub sample: Option<PathBuf>,

    /// Convert IN to OUT. Format inferred from each path's extension.
    /// The output format must be writable.
    #[arg(
        long,
        value_names = ["IN", "OUT"],
        num_args = 2,
        group = "action",
    )]
    pub convert: Vec<PathBuf>,

    /// Run a SQL query against FILE. Combine with -q / --query.
    /// The file is exposed to DuckDB as a table called `data`. Additional
    /// tables can be loaded via --sql-table / --sql-attach for cross-format
    /// JOINs; the SELECT result can be written back to a DuckDB or SQLite
    /// file via --sql-write-to.
    #[arg(long, value_name = "FILE", group = "action")]
    pub sql: Option<PathBuf>,

    /// Register an extra table in the SQL workspace as `NAME=PATH`.
    /// Repeatable. The file is loaded via the format registry and exposed
    /// in queries as `NAME`. For multi-table sources, use --sql-attach
    /// instead so every inner table is reachable as `alias.schema.tbl`.
    #[arg(long = "sql-table", value_name = "NAME=PATH")]
    pub sql_table: Vec<String>,

    /// ATTACH a DuckDB or SQLite database to the SQL workspace as
    /// `ALIAS=PATH`. Repeatable. After attachment every table inside the
    /// file is queryable as `alias.schema.tbl` (DuckDB) or `alias.tbl`
    /// (SQLite via the DuckDB sqlite extension when present, else
    /// per-table fallback).
    #[arg(long = "sql-attach", value_name = "ALIAS=PATH")]
    pub sql_attach: Vec<String>,

    /// Write the SELECT result to this DuckDB or SQLite file. Requires
    /// --sql-write-table; --sql-write-schema and --sql-write-mode are
    /// optional. The file is created if missing (DuckDB / SQLite both
    /// support this natively).
    #[arg(long = "sql-write-to", value_name = "PATH")]
    pub sql_write_to: Option<PathBuf>,

    /// Target table name for --sql-write-to.
    #[arg(long = "sql-write-table", value_name = "TABLE")]
    pub sql_write_table: Option<String>,

    /// Target schema for --sql-write-to. DuckDB-only; ignored (and must
    /// be `main` or unset) for SQLite. Defaults to `main`.
    #[arg(long = "sql-write-schema", value_name = "SCHEMA")]
    pub sql_write_schema: Option<String>,

    /// Write mode for --sql-write-to: create (default; errors if the
    /// target table exists), replace (drop + recreate), or append
    /// (INSERT into existing).
    #[arg(long = "sql-write-mode", value_enum, default_value_t = SqlWriteModeArg::Create)]
    pub sql_write_mode: SqlWriteModeArg,

    /// Render FILE's column schema as SQL DDL / a model / a struct and
    /// print it to stdout. Pick the dialect with -t / --target.
    #[arg(
        short = 'e',
        long = "export-schema",
        value_name = "FILE",
        group = "action"
    )]
    pub export_schema: Option<PathBuf>,

    /// Diff the column schemas of two files. Prints a four-column table
    /// (status / column / type_a / type_b) where `status` is one of
    /// `common`, `only_in_a`, `only_in_b`, `type_mismatch`.
    #[arg(
        long = "compare-schemas",
        value_names = ["FILE_A", "FILE_B"],
        num_args = 2,
        group = "action"
    )]
    pub compare_schemas: Vec<PathBuf>,

    /// Row-level diff of two files. Prints rows present in only one side
    /// (`status` = `only_in_a` / `only_in_b`) plus a shared-row count.
    /// Columns are compared positionally, so the two files should share the
    /// same column order for a meaningful result.
    #[arg(
        long = "diff",
        value_names = ["FILE_A", "FILE_B"],
        num_args = 2,
        group = "action"
    )]
    pub diff: Vec<PathBuf>,

    /// Validate FILE's column schema against a JSON Schema. Pair with
    /// `--expect-schema SCHEMA.json` to point at the expected schema.
    /// Exit code is 0 on a clean match, 1 otherwise - CI-pipeable.
    #[arg(long = "validate-schema", value_name = "FILE", group = "action")]
    pub validate_schema: Option<PathBuf>,

    /// One-shot orientation snapshot of FILE. Prints format, file
    /// size, row count, schema, and a sample of rows. Use
    /// `--sample-rows N` to change the preview size (default 5,
    /// max 100). The `--table NAME` flag picks a specific table on
    /// multi-table sources.
    #[arg(long = "describe", value_name = "FILE", group = "action")]
    pub describe: Option<PathBuf>,

    /// Find columns (and optional small combinations) whose values
    /// are unique across FILE. Useful for spotting primary-key
    /// candidates. Use `--max-combo N` (default 1; clamped to [1,3])
    /// to also test pairs / triples.
    #[arg(long = "unique-columns", value_name = "FILE", group = "action")]
    pub unique_columns: Option<PathBuf>,

    /// Start the MCP (Model Context Protocol) server on stdin/stdout.
    /// Mutually exclusive with the other action flags. Tools mirror the
    /// CLI surface: read_table, schema, list_tables, count_rows, run_sql,
    /// convert. Defaults (row + cell caps) come from Settings -> MCP.
    #[arg(long, group = "action")]
    pub mcp: bool,

    /// Number of rows for --head / --tail / --sample.
    #[arg(short = 'n', long = "lines", default_value_t = 20, value_name = "N")]
    pub lines: usize,

    /// Seed for --sample, for reproducible output. Default 0.
    #[arg(long = "seed", default_value_t = 0, value_name = "N")]
    pub seed: u64,

    /// SQL query string for --sql.
    #[arg(short = 'q', long = "query", value_name = "QUERY")]
    pub query: Option<String>,

    /// Target dialect / language for --export-schema.
    #[arg(
        short = 't',
        long = "target",
        value_enum,
        default_value_t = SchemaTargetArg::Postgres,
        value_name = "TARGET"
    )]
    pub target: SchemaTargetArg,

    /// For --compare-schemas only: the table name to read from FILE_A
    /// when the source is multi-table (SQLite, DuckDB, GeoPackage).
    #[arg(long = "table-a", value_name = "NAME")]
    pub table_a: Option<String>,

    /// For --compare-schemas only: the table name to read from FILE_B
    /// when the source is multi-table.
    #[arg(long = "table-b", value_name = "NAME")]
    pub table_b: Option<String>,

    /// For --validate-schema only: path to the expected JSON Schema
    /// file (typically one produced by --export-schema -t json-schema).
    #[arg(long = "expect-schema", value_name = "SCHEMA_FILE")]
    pub expect_schema: Option<PathBuf>,

    /// For --validate-schema / --describe / --unique-columns: the
    /// table name to read from FILE when the source is multi-table.
    #[arg(long = "table", value_name = "NAME")]
    pub table: Option<String>,

    /// For --describe only: number of sample rows to preview
    /// (default 5, max 100).
    #[arg(long = "sample-rows", value_name = "N")]
    pub sample_rows: Option<usize>,

    /// For --unique-columns only: maximum combo size to test
    /// (1 = single columns, 2 = + pairs, 3 = + triples).
    /// Clamped to [1, 3]. Default 1.
    #[arg(long = "max-combo", value_name = "N", default_value_t = 1)]
    pub max_combo: usize,

    /// Output format used by every action that prints a table.
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Tsv)]
    pub format: OutputFormat,

    /// Override the initial-load row cap for streaming formats (Parquet, CSV,
    /// TSV) for this single invocation. Accepts a number (commas allowed,
    /// e.g. `5,000,000`) or `all` to load every row. Defaults to the
    /// compiled-in cap (5 million rows).
    #[arg(long, value_name = "N|all")]
    pub rows: Option<String>,

    /// Files to open in the GUI when no action flag is given.
    /// Ignored (with a warning) when an action flag is set.
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// Print this help (same text for -h and --help).
    #[arg(short = 'h', long = "help", action = clap::ArgAction::HelpLong, value_parser = clap::value_parser!(bool))]
    pub help: Option<bool>,
}

/// One of the six action selections, or `None` for "launch the GUI".
/// `Mcp` is dispatched separately from [`dispatch`] because it requires a
/// tokio runtime that the GUI path intentionally never constructs - see
/// `main.rs` for the dispatch site.
#[derive(Debug)]
pub enum Action {
    Schema(PathBuf),
    Head {
        path: PathBuf,
        n: usize,
    },
    Tail {
        path: PathBuf,
        n: usize,
    },
    Sample {
        path: PathBuf,
        n: usize,
        seed: u64,
    },
    Convert {
        input: PathBuf,
        output: PathBuf,
    },
    Sql {
        path: PathBuf,
        query: String,
        extras: Vec<NamedPath>,
        attachments: Vec<NamedPath>,
        write_target: Option<sql::SqlWriteSpec>,
    },
    ExportSchema {
        path: PathBuf,
        target: SchemaTarget,
    },
    CompareSchemas {
        path_a: PathBuf,
        path_b: PathBuf,
        table_a: Option<String>,
        table_b: Option<String>,
    },
    Diff {
        path_a: PathBuf,
        path_b: PathBuf,
    },
    ValidateSchema {
        path: PathBuf,
        schema_file: PathBuf,
        table: Option<String>,
    },
    Describe {
        path: PathBuf,
        table: Option<String>,
        sample_rows: Option<usize>,
    },
    UniqueColumns {
        path: PathBuf,
        table: Option<String>,
        max_combo: usize,
    },
    Mcp,
}

impl Cli {
    /// Resolve the action flag set into a strongly-typed [`Action`].
    /// Returns `None` when none of the action flags were given.
    /// `Err(...)` when an action's required companion is missing (e.g.
    /// `--sql` without `-q`).
    pub fn detect_action(&self) -> Result<Option<Action>, &'static str> {
        if let Some(p) = &self.schema {
            return Ok(Some(Action::Schema(p.clone())));
        }
        if let Some(p) = &self.head {
            return Ok(Some(Action::Head {
                path: p.clone(),
                n: self.lines,
            }));
        }
        if let Some(p) = &self.tail {
            return Ok(Some(Action::Tail {
                path: p.clone(),
                n: self.lines,
            }));
        }
        if let Some(p) = &self.sample {
            return Ok(Some(Action::Sample {
                path: p.clone(),
                n: self.lines,
                seed: self.seed,
            }));
        }
        if !self.convert.is_empty() {
            // Clap's `num_args = 2` enforces the count, but guard
            // defensively for forward-compatibility.
            if self.convert.len() != 2 {
                return Err("--convert needs exactly two paths: --convert IN OUT");
            }
            return Ok(Some(Action::Convert {
                input: self.convert[0].clone(),
                output: self.convert[1].clone(),
            }));
        }
        if let Some(p) = &self.sql {
            let Some(q) = self.query.clone() else {
                return Err("--sql requires -q / --query \"<sql>\"");
            };
            let extras = parse_named_paths(&self.sql_table, "--sql-table")?;
            let attachments = parse_named_paths(&self.sql_attach, "--sql-attach")?;
            let write_target = match &self.sql_write_to {
                Some(path) => {
                    let table = self
                        .sql_write_table
                        .clone()
                        .ok_or("--sql-write-to requires --sql-write-table TABLE")?;
                    Some(sql::SqlWriteSpec {
                        path: path.clone(),
                        schema: self.sql_write_schema.clone(),
                        table,
                        mode: self.sql_write_mode.to_write_mode(),
                    })
                }
                None => None,
            };
            return Ok(Some(Action::Sql {
                path: p.clone(),
                query: q,
                extras,
                attachments,
                write_target,
            }));
        }
        if let Some(p) = &self.export_schema {
            return Ok(Some(Action::ExportSchema {
                path: p.clone(),
                target: self.target.to_schema_target(),
            }));
        }
        if !self.compare_schemas.is_empty() {
            if self.compare_schemas.len() != 2 {
                return Err(
                    "--compare-schemas needs exactly two paths: --compare-schemas FILE_A FILE_B",
                );
            }
            return Ok(Some(Action::CompareSchemas {
                path_a: self.compare_schemas[0].clone(),
                path_b: self.compare_schemas[1].clone(),
                table_a: self.table_a.clone(),
                table_b: self.table_b.clone(),
            }));
        }
        if !self.diff.is_empty() {
            if self.diff.len() != 2 {
                return Err("--diff needs exactly two paths: --diff FILE_A FILE_B");
            }
            return Ok(Some(Action::Diff {
                path_a: self.diff[0].clone(),
                path_b: self.diff[1].clone(),
            }));
        }
        if let Some(p) = &self.validate_schema {
            let Some(schema_file) = self.expect_schema.clone() else {
                return Err("--validate-schema requires --expect-schema SCHEMA_FILE");
            };
            return Ok(Some(Action::ValidateSchema {
                path: p.clone(),
                schema_file,
                table: self.table.clone(),
            }));
        }
        if let Some(p) = &self.describe {
            return Ok(Some(Action::Describe {
                path: p.clone(),
                table: self.table.clone(),
                sample_rows: self.sample_rows,
            }));
        }
        if let Some(p) = &self.unique_columns {
            return Ok(Some(Action::UniqueColumns {
                path: p.clone(),
                table: self.table.clone(),
                max_combo: self.max_combo,
            }));
        }
        if self.mcp {
            return Ok(Some(Action::Mcp));
        }
        Ok(None)
    }
}

/// Output format flag shared across actions.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum OutputFormat {
    /// Tab-separated values (default). One row per line, TAB between fields,
    /// header row first.
    #[default]
    Tsv,
    /// JSON array of row objects, keyed by column name. Two-space indented.
    Json,
    /// CSV per RFC 4180. Fields containing comma/quote/newline are quoted.
    Csv,
}

/// `--target` selector for `--export-schema`. Mirrors the library's
/// [`SchemaTarget`]; kept as a separate clap `ValueEnum` so the library
/// type stays free of a `clap` dependency. Clap derives kebab-case value
/// names (`json-schema`, ...) from the variant identifiers.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum SchemaTargetArg {
    /// SQL DDL - Postgres dialect.
    #[default]
    Postgres,
    /// SQL DDL - MySQL dialect.
    Mysql,
    /// SQL DDL - SQLite dialect.
    Sqlite,
    /// SQL DDL - Databricks (Spark SQL / Delta) dialect.
    Databricks,
    /// SQL DDL - Snowflake dialect.
    Snowflake,
    /// Pydantic v2 `BaseModel`.
    Pydantic,
    /// TypeScript `interface`.
    Typescript,
    /// JSON Schema (draft 2020-12).
    JsonSchema,
    /// Rust `struct` with serde derives.
    Rust,
}

impl SchemaTargetArg {
    /// Map the CLI flag value onto the library's [`SchemaTarget`].
    fn to_schema_target(self) -> SchemaTarget {
        match self {
            Self::Postgres => SchemaTarget::PostgresSqlDdl,
            Self::Mysql => SchemaTarget::MysqlSqlDdl,
            Self::Sqlite => SchemaTarget::SqliteSqlDdl,
            Self::Databricks => SchemaTarget::DatabricksSqlDdl,
            Self::Snowflake => SchemaTarget::SnowflakeSqlDdl,
            Self::Pydantic => SchemaTarget::PydanticV2,
            Self::Typescript => SchemaTarget::TypeScript,
            Self::JsonSchema => SchemaTarget::JsonSchema,
            Self::Rust => SchemaTarget::RustStruct,
        }
    }
}

/// Write-mode enum mirrored from [`octa::sql::WriteMode`] so the CLI keeps
/// the library type free of a `clap` dependency.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum SqlWriteModeArg {
    /// Error if the target table already exists.
    #[default]
    Create,
    /// Drop the target table (if any) and recreate it.
    Replace,
    /// Insert into an existing target table.
    Append,
}

impl SqlWriteModeArg {
    fn to_write_mode(self) -> octa::sql::WriteMode {
        match self {
            Self::Create => octa::sql::WriteMode::Create,
            Self::Replace => octa::sql::WriteMode::Replace,
            Self::Append => octa::sql::WriteMode::Append,
        }
    }
}

/// `NAME=PATH` pair parsed from a repeatable CLI flag. Used by `--sql-table`
/// and `--sql-attach`.
#[derive(Debug, Clone)]
pub struct NamedPath {
    pub name: String,
    pub path: PathBuf,
}

/// Parse a list of `NAME=PATH` strings into a list of [`NamedPath`]s.
/// `flag` is used for the error message so the user knows which flag was
/// malformed.
pub fn parse_named_paths(
    raw: &[String],
    flag: &'static str,
) -> Result<Vec<NamedPath>, &'static str> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let (name, path) = entry.split_once('=').ok_or(missing_eq_message(flag))?;
        if name.trim().is_empty() {
            return Err(missing_name_message(flag));
        }
        if path.trim().is_empty() {
            return Err(missing_path_message(flag));
        }
        out.push(NamedPath {
            name: name.trim().to_string(),
            path: PathBuf::from(path.trim()),
        });
    }
    Ok(out)
}

fn missing_eq_message(flag: &'static str) -> &'static str {
    match flag {
        "--sql-table" => "--sql-table expects NAME=PATH",
        "--sql-attach" => "--sql-attach expects ALIAS=PATH",
        _ => "expected NAME=PATH",
    }
}
fn missing_name_message(flag: &'static str) -> &'static str {
    match flag {
        "--sql-table" => "--sql-table NAME is empty",
        "--sql-attach" => "--sql-attach ALIAS is empty",
        _ => "name half of NAME=PATH is empty",
    }
}
fn missing_path_message(flag: &'static str) -> &'static str {
    match flag {
        "--sql-table" => "--sql-table PATH is empty",
        "--sql-attach" => "--sql-attach PATH is empty",
        _ => "path half of NAME=PATH is empty",
    }
}

/// Parse the `--rows` flag value. Accepts `all` (case-insensitive) or an
/// integer with optional comma thousand separators. Returns `usize::MAX`
/// for `all`. The returned value is meant to be fed into
/// [`octa::formats::InitialLoadRowsGuard::new`].
pub fn parse_rows_flag(s: &str) -> Result<usize, String> {
    let trimmed = s.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        return Ok(usize::MAX);
    }
    let stripped: String = trimmed.chars().filter(|c| *c != ',' && *c != '_').collect();
    stripped
        .parse::<usize>()
        .map_err(|e| format!("invalid --rows value `{s}`: {e} (expected a number or `all`)"))
}

/// Run an action. Returns an `ExitCode` so `main` can exit with the right
/// status; failures map to `ExitCode::FAILURE`. The `Action::Mcp` arm is
/// never reached here - `main.rs` peels it off before calling `dispatch`
/// because it needs to spin up a tokio runtime that the rest of the CLI
/// (and the GUI) deliberately avoid initialising.
///
/// `rows_override`, when `Some(n)`, installs an
/// [`InitialLoadRowsGuard`](octa::formats::InitialLoadRowsGuard) that lifts
/// the process-wide initial-load cap to `n` for the lifetime of the
/// handler. The guard is dropped (and the cap restored) before this function
/// returns.
pub fn dispatch(action: Action, format: OutputFormat, rows_override: Option<usize>) -> ExitCode {
    let _rows_guard = rows_override.map(octa::formats::InitialLoadRowsGuard::new);
    // --validate-schema decides its own exit code (0 = match, 1 = drift),
    // so it's pulled out of the success/failure mapping below.
    if let Action::ValidateSchema {
        path,
        schema_file,
        table,
    } = action
    {
        return match validate_schema::run(path, schema_file, table, format) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let result = match action {
        Action::Schema(path) => schema::run(path, format),
        Action::Head { path, n } => head::run(path, n, format),
        Action::Tail { path, n } => tail::run(path, n, format),
        Action::Sample { path, n, seed } => sample::run(path, n, seed, format),
        Action::Convert { input, output } => convert::run(input, output),
        Action::Sql {
            path,
            query,
            extras,
            attachments,
            write_target,
        } => sql::run(path, query, format, extras, attachments, write_target),
        Action::ExportSchema { path, target } => export_schema::run(path, target),
        Action::CompareSchemas {
            path_a,
            path_b,
            table_a,
            table_b,
        } => compare_schemas::run(path_a, path_b, table_a, table_b, format),
        Action::Diff { path_a, path_b } => diff::run(path_a, path_b, format),
        Action::Describe {
            path,
            table,
            sample_rows,
        } => describe::run(path, table, sample_rows, format),
        Action::UniqueColumns {
            path,
            table,
            max_combo,
        } => unique_columns::run(path, table, max_combo, format),
        Action::ValidateSchema { .. } => unreachable!("handled above"),
        Action::Mcp => {
            eprintln!("error: --mcp must be dispatched from main, not via cli::dispatch");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Helper used by every reading action: resolve a path to a reader and
/// load the table. Centralises the "no reader available" error message
/// so every action surfaces consistent wording.
pub(crate) fn read_table(path: &std::path::Path) -> anyhow::Result<octa::data::DataTable> {
    use octa::formats::FormatRegistry;
    let registry = FormatRegistry::new();
    let reader = registry
        .reader_for_path(path)
        .ok_or_else(|| anyhow::anyhow!("no reader available for {}", path.display()))?;
    reader.read_file(path)
}
