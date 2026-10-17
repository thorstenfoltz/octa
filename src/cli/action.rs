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
        sort: SortArgs,
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
        sort: SortArgs,
        /// Environment variable holding the zip passphrase (never the value).
        password_env: Option<String>,
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
        /// Print the plan instead of running the query (`--explain`).
        explain: bool,
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
    /// Replay a recipe on a file. `--recipe`.
    Recipe {
        recipe: PathBuf,
        input: PathBuf,
        out: Option<PathBuf>,
    },
    /// Overlapping time spans; exits 1 when any. `--overlaps`.
    Overlaps(Box<super::overlaps::Args>),
    /// Generate test data shaped like the inputs. `--test-data`.
    TestData(Box<super::test_data::Args>),
    /// Merge versions of a table. `--merge`.
    Merge(Box<super::merge::Args>),
    Diff {
        path_a: PathBuf,
        /// `None` when the B side is a database table rather than a file.
        path_b: Option<PathBuf>,
        /// `(connection name, qualified table)` for a database B side.
        db_b: Option<(String, String)>,
        mode: octa::data::compare::CompareMode,
        on: Vec<String>,
        /// Tolerance and ignore options (exact by default).
        options: octa::data::compare::CompareOptions,
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
    /// Compare N folders file by file, local or cloud.
    /// `--compare-folders A B [C ...] [--no-recursive]`. Exits 1 when
    /// anything differs, is missing, or a listing hit the file limit.
    CompareFolders {
        roots: Vec<String>,
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
        /// Where the output goes: a folder of harmonised copies, or, with
        /// `combine`, the single file they are folded into.
        out_dir: PathBuf,
        /// Fold every file into one table with a provenance column instead
        /// of writing one harmonised copy per input.
        combine: bool,
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
    HashColumns {
        path: PathBuf,
        columns: String,
        algo: Option<String>,
        delimiter: Option<String>,
        null_text: Option<String>,
        trim: bool,
        upper: bool,
        name: Option<String>,
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
        /// Environment variable holding the zip passphrase (never the value).
        password_env: Option<String>,
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
        /// Column name to partition on; `None` splits the whole table.
        col: Option<String>,
        /// Directory to write partition files into.
        out_dir: PathBuf,
        /// Output extension override (e.g. `csv`). `None` = use source extension.
        format: Option<String>,
        layout: octa::data::partition::PartitionLayout,
        /// `--split-rows`.
        max_rows: Option<usize>,
        /// `--split-size`, in bytes.
        max_bytes: Option<u64>,
    },
    DbQuery {
        conn: String,
        sql: String,
        explain: bool,
    },
    ApiFetch {
        conn: String,
        path: Option<String>,
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
    DbExport {
        conn: String,
        catalog: Option<String>,
        schema: String,
        /// Empty = every table of the schema.
        tables: Vec<String>,
        out_dir: PathBuf,
        format: octa::db::export::ExportFormat,
        overwrite: bool,
        write_options: octa::formats::write_options::WriteOptions,
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
    /// What a column's values look like. `--shapes FILE --shapes-column COL`.
    Shapes(Box<super::shapes::Args>),
    /// Columns that always follow another column. `--lookups FILE`.
    Lookups(Box<super::lookups::Args>),
    /// The commits that changed one cell. `--cell-history FILE`.
    CellHistory(Box<super::cell_history::Args>),
    /// Join by location. `--spatial-join POINTS --spatial-layer FILE...`.
    SpatialJoin(Box<super::spatial_join::Args>),
    /// Holt-Winters forecast of one column. `--forecast FILE`.
    Forecast(Box<super::forecast::Args>),
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

/// `--sort-by`, `--sort-desc` and `--natural` as typed. Checked when the
/// action runs, so the error can name the column at fault.
#[derive(Debug, Clone, Default)]
pub struct SortArgs {
    pub by: Vec<String>,
    pub desc: Vec<String>,
    pub natural: bool,
}

impl SortArgs {
    /// The recipe sort step these flags describe; `None` without --sort-by.
    pub fn step(&self) -> anyhow::Result<Option<octa::data::recipe::Sort>> {
        if self.by.is_empty() {
            if !self.desc.is_empty() || self.natural {
                anyhow::bail!("--sort-desc and --natural need --sort-by COL[,COL...]");
            }
            return Ok(None);
        }
        if let Some(stray) = self.desc.iter().find(|d| !self.by.contains(d)) {
            anyhow::bail!("--sort-desc names `{stray}`, which --sort-by does not list");
        }
        Ok(Some(octa::data::recipe::Sort {
            by: self
                .by
                .iter()
                .map(|c| octa::data::recipe::SortKey {
                    column: c.clone(),
                    descending: self.desc.contains(c),
                })
                .collect(),
            natural: self.natural,
        }))
    }
}

/// Say so when a sort saw only the rows the loader kept: a "top 10" taken from
/// the first 2 million rows looks complete and is not.
pub fn note_partial_sort(table: &octa::data::DataTable) {
    let cap = octa::formats::initial_load_rows();
    if table.row_count() >= cap {
        eprintln!(
            "note: only the first {cap} rows were loaded and sorted; pass --rows all to sort the whole file"
        );
    }
}
