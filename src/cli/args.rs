//! The clap surface: the `Cli` struct, its long help, the value enums and
//! the shared argument parsers.
//!
//! Split out of `cli/mod.rs`, which had grown to 2,376 lines holding the whole
//! clap surface, the action enum, the detection pass and the dispatcher in one
//! file. Code moved unchanged.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use octa::data::schema_export::SchemaTarget;

use super::action::parse_codec;

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
    octa --diff v1.csv v2.csv                       # whole-row set diff (default)
    octa --diff v1.csv v2.csv --diff-mode ordered   # positional, cell-level
    octa --diff v1.csv v2.csv --diff-mode join --diff-on id
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

  Anonymise / mask columns for sharing:
    octa --anonymize spec.json data.csv
    octa --anonymize spec.json data.parquet -f json

  Union (vertical stack of multiple files):
    octa --union jan.csv --union-file feb.csv --union-file mar.csv
    octa --union a.parquet --union-file b.parquet --union-drop internal_id
    octa --union a.csv --union-file b.csv --union-cast amount=Float64 -f json

  Batch conversion (many files, one target format):
    octa --batch-convert --to parquet --out-dir ./out a.csv b.csv c.csv
    octa --batch-convert --to json --out-dir ./out --overwrite data/*.csv

  Time buckets (resample) and rolling windows:
    octa --resample day --interval month --value-cols amount sales.csv
    octa --resample ts --interval week --agg mean --value-cols amount,qty \\
         --group-by region sales.parquet
    octa --rolling amount --order-by day --window 7 --agg mean sales.csv
    octa --rolling amount --order-by day --window 7 \\
         --partition-by-cols region sales.csv

  MCP server (stdio):
    octa --mcp                         # serve MCP over stdin/stdout

Notes:
  * The file is registered with DuckDB as a table named `data` in --sql.
  * --convert writes the output via FormatRegistry; read-only target
    formats (SAS, R datasets, HDF5, NetCDF) are rejected with a clear
    error.
  * --export-schema / -e renders FILE's column list as SQL DDL (Postgres,
    MySQL, SQLite, Databricks, Snowflake, MS SQL Server), a Pydantic v2 model, a
    TypeScript interface, JSON Schema, or a Rust struct; pick the target
    with -t / --target (default postgres). Output goes to stdout.
  * --format / -f governs stdout for every action that prints a table.
  * --compare-schemas reads only the column metadata from both files,
    so no row data is touched. The output is a four-column table:
    status / column / type_a / type_b. `status` is one of `common`,
    `only_in_a`, `only_in_b`, `type_mismatch`.
  * --diff compares two files. --diff-mode picks the strategy:
      set     (default) whole-row membership; prints rows unique to each
              side tagged `only_in_a` / `only_in_b`.
      ordered positional row-by-row; prints unique trailing rows plus
              paired `changed_a` / `changed_b` rows (with a
              `changed_columns` column naming the differing fields).
      join    matches rows on --diff-on KEY[,KEY...] and prints
              added / removed / changed rows.
    A summary line (per-mode counts) is written to stderr. For set and
    ordered the files should share the same column order; join matches
    columns by name.
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
  * --union stacks two or more files vertically into one output table.
    Positional FILE is the first source; add further sources with
    --union-file (repeatable). Schemas are reconciled automatically:
    columns present in only some sources are filled with null; differing
    numeric types are widened (all int -> Int64; int + float -> Float64;
    anything else -> Utf8 text). --union-drop COL excludes a column
    entirely; --union-cast COL=TYPE overrides the resolved Arrow type for
    a column. A summary line (source count, output columns, output rows)
    goes to stderr; the result table goes to stdout in --format.
  * Database connections (saved in Settings -> Databases; secrets stay in
    the system keyring):
      octa --db-tables --db warehouse
      octa --db-query \"SELECT * FROM public.users LIMIT 10\" --db warehouse
      octa --db-write-table staging.users --db warehouse users.parquet
    Queries run on the server in its native SQL dialect (PostgreSQL,
    MySQL/MariaDB, SQL Server). Mutations and --db-write-table are refused
    unless the connection's \"Allow writes\" switch is on.
  * --mcp starts an MCP (Model Context Protocol) server on stdio. Tools:
    read_table, tail, sample, schema, list_tables, count_rows, run_sql,
    convert, export_schema, profile, find_duplicates, fuzzy_duplicates,
    value_frequency, search, compare_schemas, diff_tables, union_tables,
    validate_against_schema, describe_file, unique_columns, pivot,
    correlation, grep_files, list_objects, write_table, edit_table,
    transform_columns, anonymize, list_db_connections, list_db_tables,
    query_db, write_db_table, copy_db_table. The file-writing tools (convert,
    write_table, edit_table, transform_columns, anonymize) and the database
    write tools (write_db_table, copy_db_table) are dropped under
    --mcp-read-only. Read tools also accept cloud URLs (s3://, az://, gs://)
    in their `path`, and `list_objects` browses a bucket; both use ambient
    cloud credentials. Default row + cell caps come from Settings -> MCP.
  * Action flags are mutually exclusive - pick one. Without any, Octa
    launches its GUI.
  * --rows overrides the initial-load row cap for this invocation
    (default 5,000,000). Pass `all` to load every row. Useful for
    --sql / --head / --convert against very large Parquet/CSV files.
";

/// Footer under the plain `--help` flag list: where to find the rest, rather
/// than the rest itself.
const SHORT_AFTER_HELP: &str = "\
Action flags are mutually exclusive - pick one. Without any, Octa launches
its GUI with the files you passed.

  octa --help-all    worked examples for every action
  man octa           the manual page
  https://thorstenfoltz.github.io/octa/    full documentation
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
    // `--help` stays a flag list. The ~160 lines of worked examples are the
    // long help, behind `--help-all`, because printing them by default buried
    // the flag list under something the size of the man page.
    after_help = SHORT_AFTER_HELP,
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

    /// Print a random N-row sample of FILE (default 20, override with -n / --lines).
    ///
    /// Reproducible for a given --seed.
    #[arg(long, value_name = "FILE", group = "action")]
    pub sample: Option<PathBuf>,

    /// Convert IN to OUT. Format inferred from each path's extension.
    ///
    /// The output format must be writable.
    #[arg(
        long,
        value_names = ["IN", "OUT"],
        num_args = 2,
        group = "action",
    )]
    pub convert: Vec<PathBuf>,

    /// Run a SQL query against FILE.
    ///
    /// Combine with -q / --query. The file is exposed to DuckDB as a table called
    /// `data`. Additional tables can be loaded via --sql-table / --sql-attach for
    /// cross-format JOINs; the SELECT result can be written back to a DuckDB or
    /// SQLite file via --sql-write-to.
    #[arg(long, value_name = "FILE", group = "action")]
    pub sql: Option<PathBuf>,

    /// Register an extra table in the SQL workspace as `NAME=PATH`.
    ///
    /// Repeatable. The file is loaded via the format registry and exposed in
    /// queries as `NAME`. For multi-table sources, use --sql-attach instead so
    /// every inner table is reachable as `alias.schema.tbl`.
    #[arg(long = "sql-table", value_name = "NAME=PATH")]
    pub sql_table: Vec<String>,

    /// ATTACH a DuckDB or SQLite database to the SQL workspace as `ALIAS=PATH`.
    ///
    /// Repeatable. After attachment every table inside the file is queryable as
    /// `alias.schema.tbl` (DuckDB) or `alias.tbl` (SQLite via the DuckDB sqlite
    /// extension when present, else per-table fallback).
    #[arg(long = "sql-attach", value_name = "ALIAS=PATH")]
    pub sql_attach: Vec<String>,

    /// Write the SELECT result to this DuckDB or SQLite file.
    ///
    /// Requires --sql-write-table; --sql-write-schema and --sql-write-mode are
    /// optional. The file is created if missing (DuckDB / SQLite both support
    /// this natively).
    #[arg(long = "sql-write-to", value_name = "PATH")]
    pub sql_write_to: Option<PathBuf>,

    /// Target table name for --sql-write-to.
    #[arg(long = "sql-write-table", value_name = "TABLE")]
    pub sql_write_table: Option<String>,

    /// Target schema for --sql-write-to.
    ///
    /// DuckDB-only; ignored (and must be `main` or unset) for SQLite. Defaults to
    /// `main`.
    #[arg(long = "sql-write-schema", value_name = "SCHEMA")]
    pub sql_write_schema: Option<String>,

    /// Write mode for --sql-write-to.
    ///
    /// create (default) errors if the target table exists, replace drops and
    /// recreates it, append INSERTs into the existing one.
    #[arg(long = "sql-write-mode", value_enum, default_value_t = SqlWriteModeArg::Create)]
    pub sql_write_mode: SqlWriteModeArg,

    /// Render FILE's column schema as SQL DDL / a model / a struct and print it to stdout.
    ///
    /// Pick the dialect with -t / --target.
    #[arg(
        short = 'e',
        long = "export-schema",
        value_name = "FILE",
        group = "action"
    )]
    pub export_schema: Option<PathBuf>,

    /// Diff the column schemas of two files.
    ///
    /// Prints a four-column table (status / column / type_a / type_b) where
    /// `status` is one of `common`, `only_in_a`, `only_in_b`, `type_mismatch`.
    #[arg(
        long = "compare-schemas",
        value_names = ["FILE_A", "FILE_B"],
        num_args = 2,
        group = "action"
    )]
    pub compare_schemas: Vec<PathBuf>,

    /// Compare the distributions of two columns.
    ///
    /// Numeric columns are compared with a two-sample Kolmogorov-Smirnov test,
    /// anything else as categories with a chi-square test. Prints a
    /// plain-language headline in front of the statistic. Pass
    /// `--dist-file-b` to compare against a second file, or leave it off to
    /// compare two columns of the same one.
    #[arg(long = "compare-distributions", value_name = "FILE", group = "action")]
    pub compare_distributions: Option<PathBuf>,

    /// Column to compare, for `--compare-distributions`.
    #[arg(long = "dist-column", value_name = "COL")]
    pub dist_column: Option<String>,

    /// Second file for `--compare-distributions`. Omit to use the first.
    #[arg(long = "dist-file-b", value_name = "FILE")]
    pub dist_file_b: Option<PathBuf>,

    /// Second column for `--compare-distributions`. Defaults to `--dist-column`.
    #[arg(long = "dist-column-b", value_name = "COL")]
    pub dist_column_b: Option<String>,

    /// List child rows whose foreign key has no parent.
    ///
    /// The argument is the PARENT file. Exits 1 when orphans exist, so it
    /// works as a CI gate. A null key is not an orphan: in every relational
    /// database it means "no parent".
    #[arg(long = "check-references", value_name = "PARENT", group = "action")]
    pub check_references: Option<PathBuf>,

    /// The parent's key column, for `--check-references`.
    #[arg(long = "parent-column", value_name = "COL")]
    pub parent_column: Option<String>,

    /// The child file, for `--check-references`. Omit for a self-reference.
    #[arg(long = "child-file", value_name = "FILE")]
    pub child_file: Option<PathBuf>,

    /// The child's foreign-key column, for `--check-references`.
    #[arg(long = "child-column", value_name = "COL")]
    pub child_column: Option<String>,

    /// Row-level diff of two files.
    ///
    /// Prints rows present in only one side (`status` = `only_in_a` /
    /// `only_in_b`) plus a shared-row count. Columns are compared positionally,
    /// so the two files should share the same column order for a meaningful
    /// result.
    #[arg(
        long = "diff",
        value_names = ["FILE_A", "FILE_B"],
        // One or two: the B side is a file normally, but `--diff-db` replaces
        // it with a live table, and then a single file is the whole input.
        // `detect_action` enforces which count each form needs.
        num_args = 1..=2,
        group = "action"
    )]
    pub diff: Vec<PathBuf>,

    /// Comparison strategy for `--diff`.
    ///
    /// `set` (default) reports whole rows unique to each side. `ordered` compares
    /// row-by-row in order and reports the differing cells. `join` matches rows
    /// on the `--diff-on` key column(s) and reports added / removed / changed
    /// rows.
    #[arg(long = "diff-mode", value_name = "MODE", default_value = "set")]
    pub diff_mode: String,

    /// Key column(s) for `--diff-mode join` (comma-separated or repeated).
    #[arg(long = "diff-on", value_name = "COLS", value_delimiter = ',')]
    pub diff_on: Vec<String>,

    /// Validate FILE's column schema against a JSON Schema.
    ///
    /// Pair with `--expect-schema SCHEMA.json` to point at the expected schema.
    /// Exit code is 0 on a clean match, 1 otherwise - CI-pipeable.
    #[arg(long = "validate-schema", value_name = "FILE", group = "action")]
    pub validate_schema: Option<PathBuf>,

    /// One-shot orientation snapshot of FILE.
    ///
    /// Prints format, file size, row count, schema, and a sample of rows. Use
    /// `--sample-rows N` to change the preview size (default 5, max 100). The
    /// `--table NAME` flag picks a specific table on multi-table sources.
    #[arg(long = "describe", value_name = "FILE", group = "action")]
    pub describe: Option<PathBuf>,

    /// Find columns (and optional small combinations) whose values are unique across FILE.
    ///
    /// Useful for spotting primary-key candidates. Use `--max-combo N` (default
    /// 1; clamped to [1,3]) to also test pairs / triples.
    #[arg(long = "unique-columns", value_name = "FILE", group = "action")]
    pub unique_columns: Option<PathBuf>,

    /// Anonymise / mask columns of FILE per a JSON SPEC file, printing the sanitised table to stdout (the input file is never modified).
    ///
    /// The spec lists per-column rules (`hash` / `partial_mask` / `redact` /
    /// `fake`) plus an optional shared `salt`; columns are named.
    #[arg(
        long = "anonymize",
        value_names = ["SPEC", "FILE"],
        num_args = 2,
        group = "action"
    )]
    pub anonymize: Vec<PathBuf>,

    /// Stack two or more tabular files into one output table, reconciling differing schemas.
    ///
    /// Combine with `--union-file` to add further sources; use `--union-drop` to
    /// omit columns and `--union-cast COL=TYPE` to override a column's target
    /// Arrow type.
    #[arg(long, group = "action")]
    pub union: bool,

    /// Additional file(s) to include in the `--union` stack.
    ///
    /// Repeatable. The positional file plus all `--union-file` values form the
    /// full input list (minimum two files total).
    #[arg(long = "union-file", value_name = "FILE")]
    pub union_file: Vec<PathBuf>,

    /// Column name to exclude from the `--union` output. Repeatable.
    #[arg(long = "union-drop", value_name = "COL")]
    pub union_drop: Vec<String>,

    /// Override a column's target Arrow type in the `--union` output.
    ///
    /// Syntax: `COL=TYPE` (e.g. `amount=Float64`). Repeatable.
    #[arg(long = "union-cast", value_name = "COL=TYPE")]
    pub union_cast: Vec<String>,

    /// Join tables on similarity rather than equality.
    #[arg(long = "fuzzy-join", group = "action")]
    pub fuzzy_join: bool,

    /// Additional table to join, repeatable. One per join step.
    #[arg(long = "fuzzy-join-file", value_name = "PATH")]
    pub fuzzy_join_file: Vec<PathBuf>,

    /// Column pair to compare: LEFT=RIGHT. Repeat for several columns; their
    /// scores are averaged.
    #[arg(long = "fuzzy-on", value_name = "LEFT=RIGHT")]
    pub fuzzy_on: Vec<String>,

    /// Similarity measure: edit_ratio, jaro_winkler or token_set.
    #[arg(
        long = "fuzzy-method",
        value_name = "NAME",
        default_value = "edit_ratio"
    )]
    pub fuzzy_method: String,

    /// Match threshold in 0.0..=1.0. Default 0.85.
    #[arg(long = "fuzzy-threshold", value_name = "N", default_value = "0.85")]
    pub fuzzy_threshold: f64,

    /// Exact-match blocking columns: LEFT=RIGHT. Only rows agreeing here are
    /// compared, which is what makes a large join feasible.
    #[arg(long = "fuzzy-block", value_name = "LEFT=RIGHT")]
    pub fuzzy_block: Option<String>,

    /// inner, left, right or full. Default left.
    #[arg(long = "fuzzy-join-type", value_name = "TYPE", default_value = "left")]
    pub fuzzy_join_type: String,

    /// Rows considered per side. Default 20000.
    #[arg(long = "fuzzy-max-rows", value_name = "N", default_value = "20000")]
    pub fuzzy_max_rows: usize,

    /// Write an HTML profiling report for FILE to this path.
    #[arg(long = "report", value_name = "OUT.html", group = "action")]
    pub report: Option<PathBuf>,

    /// Profile a random sample of N rows instead of every row (--report).
    #[arg(long = "report-sample", value_name = "N", requires = "report")]
    pub report_sample: Option<usize>,

    /// Comma-separated report sections: stats, distributions, top_values,
    /// correlation. Default: all four (--report).
    #[arg(long = "report-sections", value_name = "LIST", requires = "report")]
    pub report_sections: Option<String>,

    /// Treat column names differing only in case as one column when unioning.
    /// Off by default: differing case is a real difference to some tools.
    #[arg(long = "union-ignore-case")]
    pub union_ignore_case: bool,

    /// Join two or more tabular files on shared key column(s).
    ///
    /// Combine with `--join-file` to add sources beyond the positional file(s);
    /// use `--join-on` to name the key columns and `--join-type` to pick the join
    /// strategy (default `left`).
    #[arg(long, group = "action")]
    pub join: bool,

    /// Additional file(s) to include in the `--join` operation.
    ///
    /// Repeatable. The positional file(s) plus all `--join-file` values form the
    /// full input list (minimum two files total).
    #[arg(long = "join-file", value_name = "FILE")]
    pub join_file: Vec<PathBuf>,

    /// Key column(s) for `--join`. Comma-separated or repeated. Required.
    #[arg(long = "join-on", value_name = "COL[,COL,...]")]
    pub join_on: Option<String>,

    /// Join strategy for `--join`: `left` (default), `inner`, `right`, or
    /// `full`.
    #[arg(long = "join-type", value_name = "TYPE")]
    pub join_type: Option<String>,

    /// Remove duplicate rows from FILE.
    ///
    /// By default the whole row is the duplicate key; use `--dedupe-on` to
    /// restrict to named columns.
    #[arg(long, value_name = "FILE", group = "action")]
    pub dedupe: Option<PathBuf>,

    /// Key column(s) for `--dedupe` (comma-separated). Absent = whole row.
    #[arg(long = "dedupe-on", value_name = "COL[,COL,...]")]
    pub dedupe_on: Option<String>,

    /// Which occurrence to keep when deduplicating: `first` (default) or
    /// `last`.
    #[arg(long = "dedupe-keep", value_name = "first|last")]
    pub dedupe_keep: Option<String>,

    /// Fill missing/empty cells in one or more columns of FILE.
    ///
    /// Each flag takes a `COL=STRATEGY` pair. Strategies: `mean`, `median`,
    /// `mode`, `ffill`, `bfill`, `const:VALUE`. Repeatable.
    #[arg(long = "impute", value_name = "COL=STRATEGY")]
    pub impute: Vec<String>,

    /// Flag numeric outlier cells in FILE per column using IQR or z-score.
    ///
    /// Combine with `--outlier-method`, `--outlier-cols`, and `--outlier-k`.
    #[arg(long, group = "action")]
    pub outliers: bool,

    /// Detection method for `--outliers`: `iqr` (default) or `zscore`.
    #[arg(long = "outlier-method", value_name = "iqr|zscore")]
    pub outlier_method: Option<String>,

    /// Comma-separated column names to check with `--outliers`. Defaults to
    /// all columns.
    #[arg(long = "outlier-cols", value_name = "COL[,COL,...]")]
    pub outlier_cols: Option<String>,

    /// Threshold multiplier for `--outliers`. Default 1.5 for IQR, 3.0 for
    /// z-score.
    #[arg(long = "outlier-k", value_name = "K")]
    pub outlier_k: Option<f64>,

    /// Scan FILE for likely PII columns (email, phone, IBAN, credit card, SSN).
    ///
    /// Combine with `--pii-sample` to control how many rows are sampled.
    #[arg(long = "detect-pii", value_name = "FILE", group = "action")]
    pub detect_pii: Option<PathBuf>,

    /// Number of rows to sample per column when running `--detect-pii`
    /// (default 500).
    #[arg(long = "pii-sample", value_name = "N")]
    pub pii_sample: Option<usize>,

    /// Split FILE into one output file per distinct value of COL and write each group into --out-dir.
    ///
    /// Output format defaults to the source extension; override with
    /// --partition-format.
    #[arg(long = "partition-by", value_name = "COL", group = "action")]
    pub partition_by: Option<String>,

    /// Output directory for --partition-by. Created if absent.
    #[arg(long = "out-dir", value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Output file extension for --partition-by (without the leading dot, e.g. `csv`, `parquet`).
    ///
    /// Defaults to the source file's extension.
    #[arg(long = "partition-format", value_name = "EXT")]
    pub partition_format: Option<String>,

    /// Naming for --partition-by. `flat` writes `value.ext` side by side;
    /// `folder` writes `value/part-0001.ext`; `hive` writes
    /// `col=value/data.ext`; `hive-parts` writes `col=value/part-0001.ext`.
    /// All four hold the same rows and all four reopen as one table - the
    /// choice is only about the names. Default `flat`.
    #[arg(long, value_name = "LAYOUT", requires = "partition_by")]
    pub partition_layout: Option<String>,

    /// Convert every positional FILE into --out-dir as --to EXT.
    ///
    /// One failed file does not stop the run; the exit code is 1 if any failed.
    #[arg(long = "batch-convert", group = "action")]
    pub batch_convert: bool,

    /// Overwrite existing outputs in --batch-convert. Default: skip them.
    #[arg(long = "overwrite")]
    pub overwrite: bool,

    /// Group rows into time buckets: one row per --interval of COL.
    ///
    /// Needs --value-cols. Optional --agg (default sum), --group-by.
    #[arg(long = "resample", value_name = "COL", group = "action")]
    pub resample: Option<String>,

    /// Bucket size for --resample: minute|hour|day|week|month|quarter|year (default day).
    #[arg(long = "interval", value_name = "UNIT")]
    pub interval: Option<String>,

    /// Columns aggregated by --resample (comma-separated).
    #[arg(long = "value-cols", value_name = "COLS")]
    pub value_cols: Option<String>,

    /// Extra grouping columns for --resample: one series per combination.
    #[arg(long = "group-by", value_name = "COLS")]
    pub group_by: Option<String>,

    /// Add a rolling aggregate of COL over the previous --window rows.
    ///
    /// Needs --order-by and --window. Optional --agg (default mean),
    /// --partition-by-cols.
    #[arg(long = "rolling", value_name = "COL", group = "action")]
    pub rolling: Option<String>,

    /// Rows in the --rolling frame, including the current row.
    #[arg(long = "window", value_name = "N")]
    pub window: Option<usize>,

    /// Column that orders the --rolling frame. Required: a rolling aggregate
    /// over unordered rows is meaningless.
    #[arg(long = "order-by", value_name = "COL")]
    pub order_by: Option<String>,

    /// Columns that restart the --rolling frame.
    ///
    /// Spelled `--partition-by-cols` because `--partition-by` is the
    /// split-into-files action.
    #[arg(long = "partition-by-cols", value_name = "COLS")]
    pub partition_by_cols: Option<String>,

    /// Aggregate for --resample / --rolling: sum|mean|min|max|count|first|last.
    #[arg(long = "agg", value_name = "FN")]
    pub agg: Option<String>,

    /// Run SQL on a saved database connection (server-side, in the engine's native dialect).
    ///
    /// Needs --db NAME. Mutations require the connection's "Allow writes" switch.
    #[arg(long = "db-query", value_name = "SQL", group = "action")]
    pub db_query: Option<String>,
    /// Print the SQL that would make a database table match FILE.
    /// Reads the table, writes nothing.
    #[arg(long, value_name = "FILE", group = "action")]
    pub sync_sql: Option<PathBuf>,
    /// Write the positional FILEs into one .xlsx, one worksheet per file.
    #[arg(long, value_name = "OUT.xlsx", group = "action")]
    pub to_workbook: Option<PathBuf>,
    /// Target table for --sync-sql, as SCHEMA.TABLE.
    #[arg(long, value_name = "SCHEMA.TABLE", requires = "sync_sql")]
    pub sync_table: Option<String>,
    /// Key columns matching file rows to table rows (comma separated).
    #[arg(long, value_name = "COLS", requires = "sync_sql")]
    pub sync_on: Option<String>,

    /// List schemas and tables of a saved database connection. Needs --db.
    #[arg(long = "db-tables", group = "action")]
    pub db_tables: bool,

    /// Write the positional FILE into a database table (SCHEMA.TABLE).
    ///
    /// Needs --db; refused unless the connection allows writes.
    #[arg(long = "db-write-table", value_name = "SCHEMA.TABLE", group = "action")]
    pub db_write_table: Option<String>,

    /// Saved connection name or id (Settings -> Databases) for the --db-*
    /// actions.
    #[arg(long = "db", value_name = "CONNECTION")]
    pub db: Option<String>,

    /// Write mode for --db-write-table.
    #[arg(long = "db-write-mode", value_enum, default_value_t = SqlWriteModeArg::Create)]
    pub db_write_mode: SqlWriteModeArg,

    /// Catalog (top namespace level) for a three-level engine.
    ///
    /// Only Snowflake, Databricks and BigQuery have one; passing it to any other
    /// engine is an error. With --db-tables and no catalog, the catalogs
    /// themselves are listed.
    #[arg(long = "db-catalog", value_name = "NAME", requires = "db")]
    pub db_catalog: Option<String>,

    /// Copy SCHEMA.TABLE from --db to another saved connection, server to server.
    ///
    /// Requires --db-copy-to.
    #[arg(
        long = "db-copy",
        value_name = "SCHEMA.TABLE",
        group = "action",
        requires = "db"
    )]
    pub db_copy: Option<String>,

    /// Target connection for --db-copy (a saved connection name or id).
    #[arg(long = "db-copy-to", value_name = "CONNECTION")]
    pub db_copy_to: Option<String>,

    /// Target SCHEMA.TABLE for --db-copy. Default: the source's.
    #[arg(long = "db-copy-target", value_name = "SCHEMA.TABLE")]
    pub db_copy_target: Option<String>,

    /// Target catalog for --db-copy on a three-level engine.
    #[arg(long = "db-copy-target-catalog", value_name = "NAME")]
    pub db_copy_target_catalog: Option<String>,

    /// Start the MCP (Model Context Protocol) server on stdin/stdout.
    ///
    /// Mutually exclusive with the other action flags. Tools mirror the CLI
    /// surface: read_table, schema, list_tables, count_rows, run_sql, convert.
    /// Defaults (row + cell caps) come from Settings -> MCP.
    #[arg(long, group = "action")]
    pub mcp: bool,

    /// Print a shell completion script for SHELL to stdout.
    ///
    /// `eval "$(octa --completions zsh)"` wires completions up for the current
    /// shell; `install.sh` writes the files for every shell it can find.
    /// Generated from this very argument list, so a new flag is completed the
    /// moment it exists.
    #[arg(long, value_name = "SHELL", group = "action")]
    pub completions: Option<clap_complete::Shell>,

    /// With `--mcp`, omit the file-writing tools (`write_table`, `edit_table`,
    /// `convert`) so the server exposes a read-only surface.
    #[arg(long, requires = "mcp")]
    pub mcp_read_only: bool,

    /// With `--mcp`, advertise ONLY these tools. Takes group names
    /// (core, quality, compare, combine, reshape, databases, cloud, write) and
    /// individual tool names, comma-separated: `--mcp-tools core,databases` or
    /// `--mcp-tools read_table,run_sql`. Without it every tool is advertised.
    ///
    /// This is how you keep an agent's context small: the client reads the
    /// whole tool list once and carries it in every request to its model.
    #[arg(long, requires = "mcp", value_name = "LIST", value_delimiter = ',')]
    pub mcp_tools: Vec<String>,

    /// With `--mcp`, advertise everything EXCEPT these, in the same spelling as
    /// `--mcp-tools`. Applied after it, so the two combine:
    /// `--mcp-tools core,write --mcp-without convert`.
    #[arg(long, requires = "mcp", value_name = "LIST", value_delimiter = ',')]
    pub mcp_without: Vec<String>,

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

    /// For --describe only: also report the file's physical layout - row
    /// groups, compression, encodings and column statistics. Parquet only
    /// for now; other formats report their size and nothing more.
    #[arg(long = "deep", requires = "describe")]
    pub deep: bool,

    /// Compare against a live database table instead of a second file.
    /// Names a saved connection (see --db-tables); pair with
    /// --diff-db-table. With this set, --diff takes a single file.
    #[arg(long = "diff-db", value_name = "CONN")]
    pub diff_db: Option<String>,

    /// Table on the --diff-db connection, as SCHEMA.TABLE or
    /// CATALOG.SCHEMA.TABLE. An unqualified name uses the connection's
    /// own database.
    #[arg(long = "diff-db-table", value_name = "TABLE", requires = "diff_db")]
    pub diff_db_table: Option<String>,

    /// Compression codec for the written file. Parquet targets only;
    /// ignored by other formats. One of: uncompressed, snappy, zstd,
    /// gzip, lz4. Applies to --convert and --batch-convert.
    #[arg(long = "compression", value_name = "CODEC", value_parser = parse_codec)]
    pub compression: Option<String>,

    /// Rows per Parquet row group. Larger groups scan faster, smaller
    /// groups let readers skip more precisely. Applies to --convert and
    /// --batch-convert.
    #[arg(long = "row-group-size", value_name = "N")]
    pub row_group_size: Option<usize>,

    /// For --unique-columns only: maximum combo size to test (1 = single columns, 2 = + pairs, 3 = + triples).
    ///
    /// Clamped to [1, 3]. Default 1.
    #[arg(long = "max-combo", value_name = "N", default_value_t = 1)]
    pub max_combo: usize,

    /// Output format used by every action that prints a table.
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::Tsv)]
    pub format: OutputFormat,

    /// Override the initial-load row cap for streaming formats (Parquet, CSV, TSV) for this single invocation.
    ///
    /// Accepts a number (commas allowed, e.g. `5,000,000`) or `all` to load every
    /// row. Defaults to the compiled-in cap (5 million rows).
    #[arg(long, value_name = "N|all")]
    pub rows: Option<String>,

    /// Files to open in the GUI when no action flag is given.
    ///
    /// Ignored (with a warning) when an action flag is set.
    #[arg(value_name = "FILE")]
    pub files: Vec<PathBuf>,

    /// List a cloud bucket or prefix: `s3://bucket/prefix/`, `az://...`, `gs://...`.
    ///
    /// One folder level by default; add --recursive to flatten everything
    /// under the prefix. Credentials come from a saved connection covering the
    /// URL, else the ambient chain (AWS_* env, cached SSO, az login, gcloud
    /// ADC).
    #[arg(long = "cloud-ls", value_name = "URL", group = "action")]
    pub cloud_ls: Option<String>,

    /// Download one cloud object to a local file. Needs --out.
    #[arg(long = "cloud-get", value_name = "URL", group = "action")]
    pub cloud_get: Option<String>,

    /// Upload a local file to a cloud object. Needs --to.
    #[arg(long = "cloud-put", value_name = "FILE", group = "action")]
    pub cloud_put: Option<PathBuf>,

    /// Copy a cloud object, or a whole prefix, to --to.
    ///
    /// A source ending in `/` copies the folder recursively. Within one bucket
    /// the backend copies server-side; across buckets, accounts or providers
    /// the object is streamed in blocks, so size does not drive memory.
    #[arg(long = "cloud-copy", value_name = "URL", group = "action")]
    pub cloud_copy: Option<String>,

    /// Move a cloud object, or a whole prefix, to --to (copy, then delete).
    ///
    /// Object stores have no rename. The delete only runs once every copy has
    /// succeeded, so an interrupted move leaves the source intact.
    #[arg(long = "cloud-move", value_name = "URL", group = "action")]
    pub cloud_move: Option<String>,

    /// Delete a cloud object, or a whole prefix with --recursive.
    ///
    /// Cannot be undone unless the bucket has versioning enabled.
    #[arg(long = "cloud-delete", value_name = "URL", group = "action")]
    pub cloud_delete: Option<String>,

    /// Destination: a cloud URL for --cloud-put / --cloud-copy / --cloud-move,
    /// or the target extension for --batch-convert (no leading dot, e.g.
    /// `csv`, `parquet`, `json`).
    ///
    /// One flag for both because the actions are mutually exclusive, the same
    /// way --out-dir is shared with --partition-by.
    #[arg(long = "to", value_name = "URL|EXT")]
    pub to: Option<String>,

    /// Output file for --cloud-get.
    #[arg(long = "out", value_name = "FILE")]
    pub out: Option<PathBuf>,

    /// Recurse into every object under the prefix.
    ///
    /// For --cloud-ls this flattens the listing; for --cloud-delete it is the
    /// required confirmation that a folder delete is meant; for
    /// --schema-drift it walks subdirectories.
    #[arg(long = "recursive")]
    pub recursive: bool,

    /// Scan a folder and report which files disagree about their columns.
    /// Exits 1 when they do, so a CI step can gate on it.
    #[arg(long = "schema-drift", value_name = "DIR", group = "action")]
    pub schema_drift: Option<PathBuf>,

    /// Let DuckDB scan the file in place instead of loading it into memory.
    /// Applies to --sql over a Parquet, CSV or JSON file; every other action
    /// needs the rows themselves and says so rather than ignoring the flag.
    #[arg(long = "stream")]
    pub stream: bool,

    /// Rank likely relationships between the tables in DIR.
    #[arg(long = "relationships", value_name = "DIR", group = "action")]
    pub relationships: Option<PathBuf>,

    /// Check FILE's values against a rules file. Exits 1 on any violation.
    #[arg(long = "check", value_name = "FILE", group = "action")]
    pub check: Option<PathBuf>,

    /// Rules file (TOML) listing the checks to run (--check).
    #[arg(long = "rules", value_name = "FILE", requires = "check")]
    pub rules: Option<PathBuf>,

    /// Compare two versions of the same dataset (values, not just columns).
    #[arg(
        long = "drift-report",
        value_name = "FILE",
        num_args = 2,
        group = "action"
    )]
    pub drift_report: Option<Vec<PathBuf>>,

    /// Fail (exit 1) when a metric changed more than the given fraction,
    /// e.g. --fail-on null_rate:0.05,rows:0.1 (--drift-report).
    #[arg(long = "fail-on", value_name = "SPEC", requires = "drift_report")]
    pub fail_on: Option<String>,

    /// Rewrite every file in a folder to one common schema, writing copies
    /// into --out-dir. The originals are never modified. Exits 1 if any file
    /// was refused.
    #[arg(long = "harmonise-schema", value_name = "DIR", group = "action")]
    pub harmonise_schema: Option<PathBuf>,

    /// Take the target schema from this file instead of the shape most files
    /// in the folder already have (--harmonise-schema).
    #[arg(long = "target-file", value_name = "FILE")]
    pub target_file: Option<PathBuf>,

    /// Treat column names differing only in case as the same column
    /// (--schema-drift, --harmonise-schema).
    #[arg(long = "ignore-case")]
    pub ignore_case: bool,

    /// List the saved cloud and database connections (names and targets only,
    /// never secrets).
    #[arg(long = "list-connections", group = "action")]
    pub list_connections: bool,

    /// Add or update a saved connection from a `key=value,key=value` spec.
    ///
    /// `kind=` and `name=` are always required. A connection with the same
    /// name is replaced wholesale, keeping its id and therefore its stored
    /// secret, so re-running a provisioning script is idempotent; keys you
    /// leave out go back to their defaults. Unknown keys are an error rather
    /// than ignored, so a typo cannot produce a connection pointing nowhere.
    ///
    /// Cloud (kind=s3|azure|gcs): bucket, region, endpoint, prefix, account,
    /// profile, account_level, anonymous, allow_writes, force_path_style,
    /// allow_http.
    /// Database (kind=postgres|mysql|mssql|oracle|redshift|clickhouse|exasol|
    /// trino|athena|snowflake|databricks|bigquery): host, port, database,
    /// user, allow_writes.
    ///
    /// Pass the password / access key via --secret-env, never in the spec.
    #[arg(long = "add-connection", value_name = "SPEC", group = "action")]
    pub add_connection: Option<String>,

    /// Remove a saved connection by name or id, and drop its stored secret.
    #[arg(long = "remove-connection", value_name = "NAME", group = "action")]
    pub remove_connection: Option<String>,

    /// Name of an environment variable holding the secret for
    /// --add-connection.
    ///
    /// Read from the environment rather than the command line so it stays out
    /// of `ps` output and shell history. Database: the password. S3:
    /// `ACCESS_KEY_ID:SECRET_ACCESS_KEY[:TOKEN]`. Azure: the account key, or a
    /// SAS token. GCS uses application-default credentials and takes none.
    #[arg(long = "secret-env", value_name = "VAR")]
    pub secret_env: Option<String>,

    /// Print the flag list (same text for -h and --help).
    #[arg(short = 'h', long = "help", action = clap::ArgAction::HelpShort, value_parser = clap::value_parser!(bool))]
    pub help: Option<bool>,

    /// Print the flag list plus worked examples for every action.
    #[arg(long = "help-all", action = clap::ArgAction::HelpLong, value_parser = clap::value_parser!(bool))]
    pub help_all: Option<bool>,
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
    /// SQL DDL - Microsoft SQL Server (T-SQL) dialect.
    Mssql,
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
    pub(super) fn to_schema_target(self) -> SchemaTarget {
        match self {
            Self::Postgres => SchemaTarget::PostgresSqlDdl,
            Self::Mysql => SchemaTarget::MysqlSqlDdl,
            Self::Sqlite => SchemaTarget::SqliteSqlDdl,
            Self::Databricks => SchemaTarget::DatabricksSqlDdl,
            Self::Snowflake => SchemaTarget::SnowflakeSqlDdl,
            Self::Mssql => SchemaTarget::MssqlSqlDdl,
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
    pub(super) fn to_write_mode(self) -> octa::sql::WriteMode {
        match self {
            Self::Create => octa::sql::WriteMode::Create,
            Self::Replace => octa::sql::WriteMode::Replace,
            Self::Append => octa::sql::WriteMode::Append,
        }
    }

    /// The same three modes, as the live-database write enum.
    pub(super) fn to_db_write_mode(self) -> octa::db::DbWriteMode {
        match self {
            Self::Create => octa::db::DbWriteMode::Create,
            Self::Replace => octa::db::DbWriteMode::Replace,
            Self::Append => octa::db::DbWriteMode::Append,
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
