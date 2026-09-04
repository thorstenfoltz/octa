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

pub mod action;
pub mod anonymize;
pub mod args;
pub mod batch_convert;
pub mod check;
pub mod cloud;
pub mod compare_schemas;
pub mod completions;
pub mod connections;
pub mod convert;
pub mod db;
pub mod dedupe;
pub mod describe;
pub mod detect;
pub mod diff;
pub mod dispatch;
pub mod distribution_compare;
pub mod drift;
pub mod export_schema;
pub mod fuzzy_join;
pub mod harmonise;
pub mod head;
pub mod impute;
pub mod join;
pub mod outliers;
pub mod output;
pub mod partition;
pub mod pii;
pub mod progress;
pub mod referential;
pub mod relationships;
pub mod report;
pub mod sample;
pub mod schema;
pub mod schema_drift;
pub mod sql;
pub mod sync_sql;
pub mod tail;
pub mod timeseries;
pub mod union;
pub mod unique_columns;
pub mod validate_schema;
pub mod workbook;

pub use action::Action;
pub use args::{Cli, NamedPath, OutputFormat, parse_named_paths, parse_rows_flag};
pub use dispatch::dispatch;
// The per-action handlers under `src/cli/*.rs` call this as `super::read_table`,
// so it keeps its old path even though the body moved into `dispatch`.
pub(crate) use dispatch::read_table;
