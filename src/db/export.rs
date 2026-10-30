//! Export database tables to files: the tables of one schema (all, or a
//! chosen few) into a folder, one file per table. Every table is streamed page
//! by page through [`DbConnector::fetch_batches`] into a writer that takes one
//! batch at a time, so a table of any size never sits in memory whole. Hence
//! the formats on offer: the ones that can be written that way.
//!
//! The GUI dialog, `--db-export` and the MCP tool `export_db_tables` all call
//! [`export_tables`].

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::bail;

use super::{DbConnector, paged_sql};
use crate::data::DataTable;
use crate::formats::write_options::WriteOptions;

/// Rows per page fetched from the server.
const BATCH_ROWS: usize = 50_000;

/// What the files are written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Parquet,
    Csv,
    Tsv,
    Jsonl,
}

impl ExportFormat {
    /// In picker order.
    pub const ALL: [ExportFormat; 4] = [Self::Parquet, Self::Csv, Self::Tsv, Self::Jsonl];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Parquet => "parquet",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Jsonl => "jsonl",
        }
    }

    /// What the picker shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Parquet => "Parquet",
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Jsonl => "JSON Lines",
        }
    }

    /// From an extension or name as typed on the command line or over MCP
    /// (`parquet`, `.csv`, `ndjson`, ...).
    pub fn parse(name: &str) -> Option<Self> {
        match name
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "parquet" | "pq" => Some(Self::Parquet),
            "csv" => Some(Self::Csv),
            "tsv" | "tab" => Some(Self::Tsv),
            "jsonl" | "ndjson" | "json lines" | "jsonlines" => Some(Self::Jsonl),
            _ => None,
        }
    }
}

/// One export run.
#[derive(Debug, Clone)]
pub struct ExportRequest {
    /// Catalog for a three-level engine (Snowflake, Databricks, BigQuery).
    pub catalog: Option<String>,
    pub schema: String,
    /// The tables to export, in order.
    pub tables: Vec<String>,
    /// Created when missing.
    pub dir: PathBuf,
    pub format: ExportFormat,
    /// Replace files that are already there. Off: such a table is skipped.
    pub overwrite: bool,
    /// Parquet codec and CSV knobs, as Settings or the command line say.
    pub options: WriteOptions,
}

/// What happened to one table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableOutcome {
    Written {
        table: String,
        path: PathBuf,
        rows: usize,
    },
    /// The file was there and `overwrite` was off.
    Skipped {
        table: String,
        path: PathBuf,
    },
    Failed {
        table: String,
        error: String,
    },
}

impl TableOutcome {
    pub fn table(&self) -> &str {
        match self {
            Self::Written { table, .. }
            | Self::Skipped { table, .. }
            | Self::Failed { table, .. } => table,
        }
    }
}

/// Where a run is, for a progress line: table `index` of `count`, and the rows
/// written of it so far.
#[derive(Debug, Clone, Copy)]
pub struct Progress<'a> {
    pub index: usize,
    pub count: usize,
    pub table: &'a str,
    pub rows: usize,
}

/// Export `req.tables` through `conn`. One table failing does not stop the
/// others; `cancel` does (checked between pages), and the table it caught
/// mid-way is reported as failed with nothing left on disk.
pub fn export_tables(
    conn: &mut dyn DbConnector,
    req: &ExportRequest,
    progress: &dyn Fn(Progress),
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<TableOutcome>> {
    std::fs::create_dir_all(&req.dir)?;
    let mut used = HashSet::new();
    let mut out = Vec::with_capacity(req.tables.len());
    for (index, table) in req.tables.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let path = req.dir.join(file_name(table, req.format, &mut used));
        if path.exists() && !req.overwrite {
            out.push(TableOutcome::Skipped {
                table: table.clone(),
                path,
            });
            continue;
        }
        let report = |rows| {
            progress(Progress {
                index,
                count: req.tables.len(),
                table,
                rows,
            })
        };
        report(0);
        out.push(match export_one(conn, req, table, &path, &report, cancel) {
            Ok(rows) => TableOutcome::Written {
                table: table.clone(),
                path,
                rows,
            },
            Err(e) => TableOutcome::Failed {
                table: table.clone(),
                error: format!("{e:#}"),
            },
        });
    }
    Ok(out)
}

/// Stream one table into `path` (through a temp, so a failure leaves
/// nothing half-written under the real name). Returns the rows written.
fn export_one(
    conn: &mut dyn DbConnector,
    req: &ExportRequest,
    table: &str,
    path: &Path,
    progress: &dyn Fn(usize),
    cancel: &AtomicBool,
) -> anyhow::Result<usize> {
    let engine = conn.engine();
    let q = |s: &str| engine.quote_ident(s);
    let name = match &req.catalog {
        Some(c) => format!("{}.{}.{}", q(c), q(&req.schema), q(table)),
        None => format!("{}.{}", q(&req.schema), q(table)),
    };
    let select = format!("SELECT * FROM {name}");
    crate::formats::write_atomically(path, |tmp| {
        let mut sink: Option<Sink> = None;
        let mut rows = 0usize;
        conn.fetch_batches(&select, BATCH_ROWS, &mut |batch| {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            if sink.is_none() {
                sink = Some(Sink::create(tmp, req, &batch)?);
            }
            if let Some(sink) = sink.as_mut() {
                sink.write(&batch)?;
            }
            rows += batch.row_count();
            progress(rows);
            Ok(())
        })?;
        // An empty table hands over no page at all, so the file's header and
        // schema have to come from somewhere else.
        let sink = match sink {
            Some(sink) => sink,
            None => {
                let mut empty = conn.query(&paged_sql(engine, &select, 1, 0))?;
                if empty.columns.is_empty() {
                    empty.columns = catalog_columns(conn, req, table)?;
                }
                if empty.columns.is_empty() {
                    bail!("the table is empty and the server does not list its columns");
                }
                let mut sink = Sink::create(tmp, req, &empty)?;
                sink.write(&empty)?;
                rows = empty.row_count();
                sink
            }
        };
        sink.finish()?;
        Ok(rows)
    })
}

/// The columns of an empty table, from the server's catalog: the drivers name
/// a result's columns from the rows it returns, and an empty table returns
/// none. Their types are unknown there, so they are written as text.
fn catalog_columns(
    conn: &mut dyn DbConnector,
    req: &ExportRequest,
    table: &str,
) -> anyhow::Result<Vec<crate::data::ColumnInfo>> {
    let all = super::relationships::scan_columns(
        conn,
        req.catalog.as_deref(),
        std::slice::from_ref(&req.schema),
        Some(table),
    )?;
    let names = super::relationships::table_columns(&all, table)
        .map(|(_, columns)| columns)
        .unwrap_or_default();
    Ok(names
        .into_iter()
        .map(|name| crate::data::ColumnInfo {
            name,
            data_type: "Utf8".into(),
        })
        .collect())
}

/// The file a table is written to: its name with anything a file system would
/// refuse replaced, and a number added when two tables would land on the same
/// file (`Users` and `users` on a case-insensitive disk).
fn file_name(table: &str, format: ExportFormat, used: &mut HashSet<String>) -> String {
    let mut stem: String = table
        .chars()
        .map(|c| {
            if c.is_control() || r#"/\:*?"<>|"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    stem = stem.trim().trim_matches('.').to_string();
    if stem.is_empty() {
        stem = "table".to_string();
    }
    let candidate = crate::data::unique_name(&stem, |c| used.contains(&c.to_lowercase()));
    used.insert(candidate.to_lowercase());
    format!("{candidate}.{}", format.extension())
}

/// A file being written one batch at a time.
enum Sink {
    Delimited {
        writer: csv::Writer<std::io::BufWriter<std::fs::File>>,
        decimal_comma: bool,
    },
    Parquet(crate::formats::parquet_reader::ParquetStreamWriter),
    Jsonl(std::io::BufWriter<std::fs::File>),
}

impl Sink {
    /// Open `path`, taking the columns (and for CSV the header) from `first`.
    fn create(path: &Path, req: &ExportRequest, first: &DataTable) -> anyhow::Result<Self> {
        let file =
            || -> anyhow::Result<_> { Ok(std::io::BufWriter::new(std::fs::File::create(path)?)) };
        Ok(match req.format {
            ExportFormat::Parquet => {
                Self::Parquet(crate::formats::parquet_reader::ParquetStreamWriter::create(
                    path,
                    &first.columns,
                    &req.options.parquet,
                )?)
            }
            ExportFormat::Csv | ExportFormat::Tsv => {
                let csv = &req.options.csv;
                let delimiter = if req.format == ExportFormat::Tsv {
                    b'\t'
                } else {
                    csv.delimiter
                };
                let mut writer =
                    crate::formats::csv_reader::delimited_writer(file()?, csv, delimiter)?;
                if csv.write_header {
                    crate::formats::csv_reader::write_delimited_header(
                        &mut writer,
                        &first.columns,
                    )?;
                }
                Self::Delimited {
                    writer,
                    decimal_comma: csv.decimal_comma,
                }
            }
            ExportFormat::Jsonl => Self::Jsonl(file()?),
        })
    }

    fn write(&mut self, batch: &DataTable) -> anyhow::Result<()> {
        match self {
            Self::Delimited {
                writer,
                decimal_comma,
            } => crate::formats::csv_reader::write_delimited_rows(writer, batch, *decimal_comma),
            Self::Parquet(writer) => writer.write(batch),
            Self::Jsonl(out) => {
                let rows = crate::formats::json_reader::table_to_json_array(batch);
                for row in rows.as_array().into_iter().flatten() {
                    serde_json::to_writer(&mut *out, row)?;
                    out.write_all(b"\n")?;
                }
                Ok(())
            }
        }
    }

    fn finish(self) -> anyhow::Result<()> {
        match self {
            Self::Delimited { mut writer, .. } => writer.flush()?,
            Self::Parquet(writer) => writer.finish()?,
            Self::Jsonl(mut out) => out.flush()?,
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
