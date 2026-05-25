//! `octa --sql <FILE> --query '<SQL>'` — run a SQL query against a file
//! and print the result. The file is loaded as the in-memory DuckDB table
//! named `data`; the query can reference any DuckDB SQL feature.

use std::path::PathBuf;

use octa::sql::{QueryKind, run_query};

use super::OutputFormat;
use super::output::write_table;

pub fn run(path: PathBuf, query: String, format: OutputFormat) -> anyhow::Result<()> {
    let table = super::read_table(&path)?;
    let outcome = run_query(&table, &query)?;
    match outcome.kind {
        QueryKind::Select => {
            write_table(&outcome.table, format)?;
        }
        QueryKind::Mutation => {
            if let Some(n) = outcome.affected {
                eprintln!("{n} rows affected");
            } else {
                eprintln!("mutation completed");
            }
            // Mutations against an in-memory snapshot of the file aren't
            // persisted back to disk by `--sql` — that's a deliberate
            // safety boundary. Surface the post-mutation `data` table so
            // the user can pipe it into `--convert` if they want it.
            write_table(&outcome.table, format)?;
        }
    }
    Ok(())
}
