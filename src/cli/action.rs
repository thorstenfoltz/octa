//! [`Action`]: the resolved action a parsed [`Cli`](super::Cli) turns into.
//!
//! Split out of `cli/mod.rs`, which had grown to 2,376 lines holding the whole
//! clap surface, the action enum, the detection pass and the dispatcher in one
//! file. Code moved unchanged.

use std::path::PathBuf;

use octa::data::schema_export::SchemaTarget;

use super::args::NamedPath;
use super::sql;

/// One of the six action selections, or `None` for "launch the GUI".
/// `Mcp` is dispatched separately from [`dispatch`] because it requires a
/// tokio runtime that the GUI path intentionally never constructs - see
/// `main.rs` for the dispatch site.
#[derive(Debug)]
pub enum Action {
    /// Print a shell completion script to stdout.
    Completions(clap_complete::Shell),
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
        /// Explicit output format, required when `output` is `-` (stdout).
        to: Option<String>,
        write_options: octa::formats::write_options::WriteOptions,
    },
    Sql {
        path: PathBuf,
        query: String,
        extras: Vec<NamedPath>,
        attachments: Vec<NamedPath>,
        write_target: Option<sql::SqlWriteSpec>,
        /// Register the file as a DuckDB view over the file rather than
        /// loading its rows (`--stream`).
        stream: bool,
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
    CompareDistributions(Box<super::distribution_compare::Args>),
    CheckReferences(Box<super::referential::Args>),
    Diff {
        path_a: PathBuf,
        /// `None` when the B side is a database table rather than a file.
        path_b: Option<PathBuf>,
        /// `(connection name, qualified table)` for a database B side.
        db_b: Option<(String, String)>,
        mode: octa::data::compare::CompareMode,
        on: Vec<String>,
    },
    ValidateSchema {
        path: PathBuf,
        schema_file: PathBuf,
        table: Option<String>,
    },
    /// Join tables on similarity rather than equality. `--fuzzy-join`.
    FuzzyJoin(Box<crate::cli::fuzzy_join::Args>),
    /// Write an HTML profiling report for a file. `--report OUT.html FILE`.
    Report {
        out: PathBuf,
        path: PathBuf,
        table: Option<String>,
        sample: Option<usize>,
        sections: Option<String>,
    },
    /// Report which files in a folder disagree about their columns.
    /// `--schema-drift DIR`. Exits 1 on drift, like `--validate-schema`.
    SchemaDrift {
        dir: PathBuf,
        recursive: bool,
        ignore_case: bool,
    },
    /// Rank likely relationships between the tables in a folder.
    /// `--relationships DIR [--recursive]`. Always exits 0: a report, not a gate.
    Relationships {
        dir: PathBuf,
        recursive: bool,
    },
    /// Check a file's values against a rules file.
    /// `--check FILE --rules FILE`. Exits 1 on a violation or an unrunnable rule.
    Check {
        path: PathBuf,
        rules: PathBuf,
    },
    /// Compare two versions of the same dataset.
    /// `--drift-report A B [--fail-on SPEC]`. Exits 1 only on a breached gate.
    DriftReport {
        path_a: PathBuf,
        path_b: PathBuf,
        fail_on: Option<String>,
    },
    /// Rewrite a folder of files to one common schema.
    /// `--harmonise-schema DIR --out-dir DIR`. Exits 1 if any file was refused.
    Harmonise {
        dir: PathBuf,
        out_dir: PathBuf,
        target_file: Option<PathBuf>,
        recursive: bool,
        ignore_case: bool,
        overwrite: bool,
        write_options: octa::formats::write_options::WriteOptions,
    },
    Describe {
        path: PathBuf,
        table: Option<String>,
        sample_rows: Option<usize>,
        deep: bool,
    },
    UniqueColumns {
        path: PathBuf,
        table: Option<String>,
        max_combo: usize,
    },
    Anonymize {
        spec: PathBuf,
        file: PathBuf,
    },
    Union {
        files: Vec<PathBuf>,
        union_file: Vec<PathBuf>,
        drop: Vec<String>,
        cast: Vec<String>,
        ignore_case: bool,
    },
    Join {
        files: Vec<PathBuf>,
        join_file: Vec<PathBuf>,
        join_on: Vec<String>,
        join_type: Option<String>,
    },
    Dedupe {
        path: PathBuf,
        dedupe_on: Option<String>,
        dedupe_keep: Option<String>,
    },
    Impute {
        path: PathBuf,
        specs: Vec<String>,
    },
    Outliers {
        path: PathBuf,
        method: Option<String>,
        cols: Option<String>,
        k: Option<f64>,
    },
    DetectPii {
        path: PathBuf,
        sample_rows: Option<usize>,
    },
    /// Convert many files into one target format. `--batch-convert`.
    BatchConvert {
        inputs: Vec<PathBuf>,
        out_dir: PathBuf,
        target_ext: String,
        overwrite: bool,
        write_options: octa::formats::write_options::WriteOptions,
    },
    /// Group rows into time buckets and aggregate. `--resample COL`.
    Resample {
        path: PathBuf,
        spec: octa::data::timeseries::ResampleSpec,
    },
    /// Add a rolling aggregate over the previous N rows. `--rolling COL`.
    Rolling {
        path: PathBuf,
        spec: octa::data::timeseries::RollingSpec,
    },
    Partition {
        /// Positional source file.
        path: PathBuf,
        /// Column name to partition on.
        col: String,
        /// Directory to write partition files into.
        out_dir: PathBuf,
        /// Output extension override (e.g. `csv`). `None` = use source extension.
        format: Option<String>,
        layout: octa::data::partition::PartitionLayout,
    },
    DbQuery {
        conn: String,
        sql: String,
    },
    ToWorkbook {
        out: PathBuf,
        inputs: Vec<PathBuf>,
    },
    SyncSql {
        path: PathBuf,
        conn: String,
        table: String,
        on: String,
    },
    DbTables {
        conn: String,
        catalog: Option<String>,
    },
    DbWrite {
        conn: String,
        catalog: Option<String>,
        /// `SCHEMA.TABLE` target.
        target: String,
        mode: octa::db::DbWriteMode,
        /// Positional source file to upload.
        file: PathBuf,
    },
    DbCopy {
        conn: String,
        catalog: Option<String>,
        source: String,
        target_conn: String,
        target: Option<String>,
        target_catalog: Option<String>,
        mode: octa::db::DbWriteMode,
    },
    CloudLs {
        url: String,
        recursive: bool,
    },
    CloudGet {
        url: String,
        out: PathBuf,
    },
    CloudPut {
        file: PathBuf,
        url: String,
    },
    /// Copy (`move_it: false`) or move (`true`) an object or prefix.
    CloudTransfer {
        from: String,
        to: String,
        move_it: bool,
    },
    CloudDelete {
        url: String,
        recursive: bool,
    },
    ListConnections,
    AddConnection {
        spec: String,
        secret_env: Option<String>,
    },
    RemoveConnection(String),
    Mcp,
}

/// Validate the codec name at parse time so the error names the valid set,
/// rather than silently falling back to uncompressed halfway through a write
/// the user already committed to.
pub(super) fn parse_codec(s: &str) -> Result<String, String> {
    let lower = s.to_ascii_lowercase();
    if octa::formats::write_options::PARQUET_CODECS.contains(&lower.as_str()) {
        Ok(lower)
    } else {
        Err(format!(
            "unknown codec '{s}'; expected one of: {}",
            octa::formats::write_options::PARQUET_CODECS.join(", ")
        ))
    }
}
