//! CLI dispatch. Flag-style: one of `--schema`, `--head`, `--convert`,
//! `--sql`, `--export-schema`, `--mcp` selects the action; the file
//! argument(s) follow the flag. Mutually exclusive: passing two action
//! flags is a parse error.
//!
//! Adding a new action: define the flag on [`Cli`] with `group = "action"`,
//! add a variant to [`Action`] + an arm to [`Cli::detect_action`], drop a
//! handler file under `src/cli/<verb>.rs`, and add a match arm in
//! [`dispatch`]. `--mcp` is the one exception — it's dispatched in
//! `main.rs` because it needs a tokio runtime, which the GUI path
//! deliberately avoids constructing.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};

use octa::data::schema_export::SchemaTarget;

pub mod convert;
pub mod export_schema;
pub mod head;
pub mod output;
pub mod schema;
pub mod sql;

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

  Schema export / codegen:
    octa --export-schema data.parquet -t snowflake
    octa -e data.csv --target pydantic
    octa -e schema.parquet -t databricks

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
  * --mcp starts an MCP (Model Context Protocol) server on stdio. Tools:
    read_table, schema, list_tables, count_rows, run_sql, convert,
    export_schema, profile, find_duplicates, value_frequency, search.
    Default row + cell caps come from Octa's Settings → MCP.
  * Action flags are mutually exclusive — pick one. Without any, Octa
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
    /// The file is exposed to DuckDB as a table called `data`.
    #[arg(long, value_name = "FILE", group = "action")]
    pub sql: Option<PathBuf>,

    /// Render FILE's column schema as SQL DDL / a model / a struct and
    /// print it to stdout. Pick the dialect with -t / --target.
    #[arg(
        short = 'e',
        long = "export-schema",
        value_name = "FILE",
        group = "action"
    )]
    pub export_schema: Option<PathBuf>,

    /// Start the MCP (Model Context Protocol) server on stdin/stdout.
    /// Mutually exclusive with the other action flags. Tools mirror the
    /// CLI surface: read_table, schema, list_tables, count_rows, run_sql,
    /// convert. Defaults (row + cell caps) come from Settings → MCP.
    #[arg(long, group = "action")]
    pub mcp: bool,

    /// Number of rows for --head.
    #[arg(short = 'n', long = "lines", default_value_t = 20, value_name = "N")]
    pub lines: usize,

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
/// tokio runtime that the GUI path intentionally never constructs — see
/// `main.rs` for the dispatch site.
#[derive(Debug)]
pub enum Action {
    Schema(PathBuf),
    Head { path: PathBuf, n: usize },
    Convert { input: PathBuf, output: PathBuf },
    Sql { path: PathBuf, query: String },
    ExportSchema { path: PathBuf, target: SchemaTarget },
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
            return Ok(Some(Action::Sql {
                path: p.clone(),
                query: q,
            }));
        }
        if let Some(p) = &self.export_schema {
            return Ok(Some(Action::ExportSchema {
                path: p.clone(),
                target: self.target.to_schema_target(),
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
/// names (`json-schema`, …) from the variant identifiers.
#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum SchemaTargetArg {
    /// SQL DDL — Postgres dialect.
    #[default]
    Postgres,
    /// SQL DDL — MySQL dialect.
    Mysql,
    /// SQL DDL — SQLite dialect.
    Sqlite,
    /// SQL DDL — Databricks (Spark SQL / Delta) dialect.
    Databricks,
    /// SQL DDL — Snowflake dialect.
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
/// never reached here — `main.rs` peels it off before calling `dispatch`
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
    let result = match action {
        Action::Schema(path) => schema::run(path, format),
        Action::Head { path, n } => head::run(path, n, format),
        Action::Convert { input, output } => convert::run(input, output),
        Action::Sql { path, query } => sql::run(path, query, format),
        Action::ExportSchema { path, target } => export_schema::run(path, target),
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
