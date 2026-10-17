//! A file too big to hold in memory, read a page at a time.
//!
//! Every other reader in Octa produces a whole [`DataTable`]; this one produces
//! a **handle**. The rows stay on disk and DuckDB fetches the slice the table
//! view is currently showing, which is what makes a 40 GB CSV viewable at all.
//! Only the formats DuckDB can scan in place qualify ([`ScanKind`]); anything
//! else has to be read the ordinary way.
//!
//! `page` and `filtered_count` build their SQL from typed inputs: the sort
//! column is addressed by **name**, taken from `self.columns` and quoted, never
//! by interpolating what a caller typed. `filter` is the one place a caller
//! supplies a SQL fragment, and its only caller builds that fragment from the
//! table view's own typed filter inputs.
//!
//! ponytail: one connection per open large tab, no pooling. If concurrent
//! paging on a single tab ever matters, give `LargeTable` an internal mutex or
//! a small pool.

use std::path::Path;

use anyhow::{Context, Result, bail};
use duckdb::Connection;

use crate::data::conditional_format::CondOp;
use crate::data::value_frequency::{ValueFrequency, ValueFrequencyRow};
use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::duckdb_reader::{
    column_zones, duckdb_session_timezone, duckdb_type_to_arrow, duckdb_value_to_cell_tz,
};

/// The formats DuckDB can scan straight off disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanKind {
    Parquet,
    Csv,
    Json,
}

impl ScanKind {
    pub fn for_path(path: &Path) -> Option<ScanKind> {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "parquet" | "pq" => Some(ScanKind::Parquet),
            "csv" | "tsv" | "txt" => Some(ScanKind::Csv),
            "json" | "jsonl" | "ndjson" => Some(ScanKind::Json),
            _ => None,
        }
    }

    /// The scan expression for one path. Public because the SQL workspace
    /// builds a view from it for `--stream`.
    pub fn scan_expr(self, path: &Path) -> String {
        // Single-quoted SQL string literal, so an embedded quote is doubled.
        let quoted = path.to_string_lossy().replace('\'', "''");
        match self {
            ScanKind::Parquet => format!("read_parquet('{quoted}')"),
            ScanKind::Csv => format!("read_csv_auto('{quoted}')"),
            ScanKind::Json => format!("read_json_auto('{quoted}')"),
        }
    }
}

/// An open scan over a file whose rows were never loaded.
pub struct LargeTable {
    conn: Connection,
    scan: String,
    columns: Vec<ColumnInfo>,
    /// Each column's type as DuckDB names it, which decides how a displayed
    /// value can be matched in a query (see [`LargeTable::values_filter`]).
    duck_types: Vec<String>,
    row_count: usize,
}

/// Open `path` as a paged scan.
pub fn open(path: &Path) -> Result<LargeTable> {
    let kind = ScanKind::for_path(path)
        .ok_or_else(|| anyhow::anyhow!("{} cannot be scanned in place", path.display()))?;
    let scan = kind.scan_expr(path);
    let conn = Connection::open_in_memory().context("opening an in-memory DuckDB connection")?;

    let session_tz = duckdb_session_timezone(&conn);
    let mut stmt = conn
        .prepare(&format!("DESCRIBE SELECT * FROM {scan}"))
        .with_context(|| format!("describing {}", path.display()))?;
    let described: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
    let columns: Vec<ColumnInfo> = described
        .iter()
        .map(|(name, ty)| ColumnInfo {
            name: name.clone(),
            data_type: duckdb_type_to_arrow(ty, session_tz.as_deref()),
        })
        .collect();
    let duck_types = described.into_iter().map(|(_, ty)| ty).collect();
    if columns.is_empty() {
        bail!("{} has no columns", path.display());
    }

    let row_count: usize = conn
        .query_row(&format!("SELECT count(*) FROM {scan}"), [], |r| {
            r.get::<_, i64>(0)
        })
        .with_context(|| format!("counting rows in {}", path.display()))?
        .max(0) as usize;

    Ok(LargeTable {
        conn,
        scan,
        columns,
        duck_types,
        row_count,
    })
}

/// How a column's cells read in Octa, by the DuckDB type they come from
/// (`duckdb_value_to_cell_tz`), and so how a displayed value is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Text,
    Integer,
    Float,
    Decimal,
    Bool,
    Date,
    Time,
    Timestamp,
    /// Lists, structs, enums, intervals, blobs: their text is Rust's debug
    /// spelling or hex, which no query can reproduce.
    Unmatched,
}

fn cell_kind(duck_type: &str) -> CellKind {
    let t = duck_type.trim().to_uppercase();
    match t.as_str() {
        "BOOLEAN" => CellKind::Bool,
        "TINYINT" | "SMALLINT" | "INTEGER" | "BIGINT" | "HUGEINT" | "UTINYINT" | "USMALLINT"
        | "UINTEGER" | "UBIGINT" => CellKind::Integer,
        "FLOAT" | "DOUBLE" => CellKind::Float,
        "DATE" => CellKind::Date,
        "TIME" => CellKind::Time,
        "TIMESTAMP"
        | "TIMESTAMP WITH TIME ZONE"
        | "TIMESTAMP_S"
        | "TIMESTAMP_MS"
        | "TIMESTAMP_NS" => CellKind::Timestamp,
        _ if t == "VARCHAR" || t.starts_with("VARCHAR(") => CellKind::Text,
        _ if t.starts_with("DECIMAL") => CellKind::Decimal,
        _ => CellKind::Unmatched,
    }
}

fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The text Octa displays for a cell of a column of `kind`, spelled in SQL
/// over the column `q` (`''` for an empty cell): `duckdb_value_to_cell_tz`
/// and `CellValue`'s `Display`, rebuilt so a text comparison in a query sees
/// what the in-memory one sees. `None` when no query can spell it.
// ponytail: a float DuckDB prints in exponent form (1e-07, 1.5e+20) is
// spelled that way here, where Octa writes the digits out; values that
// small or that large are rare in tabular data.
fn display_sql(kind: CellKind, q: &str, zone: Option<&str>) -> Option<String> {
    // chrono's `%.f`: nothing for whole seconds, else 3 or 6 digits.
    let fraction = |t: &str| {
        format!(
            "CASE WHEN microsecond({t}) % 1000000 = 0 THEN '' \
             WHEN microsecond({t}) % 1000 = 0 THEN '.' || \
             lpad(CAST((microsecond({t}) % 1000000) // 1000 AS VARCHAR), 3, '0') \
             ELSE '.' || lpad(CAST(microsecond({t}) % 1000000 AS VARCHAR), 6, '0') END"
        )
    };
    let text = match kind {
        CellKind::Text => q.to_string(),
        CellKind::Integer | CellKind::Bool | CellKind::Decimal => format!("CAST({q} AS VARCHAR)"),
        CellKind::Date => format!("strftime({q}, '%Y-%m-%d')"),
        CellKind::Time => {
            let t = format!("(DATE '1970-01-01' + {q})");
            format!("strftime({t}, '%H:%M:%S') || {}", fraction(&t))
        }
        CellKind::Timestamp => {
            let t = match zone {
                Some(z) => format!("timezone({}, {q})", sql_str(z)),
                None => format!("CAST({q} AS TIMESTAMP)"),
            };
            format!("strftime({t}, '%Y-%m-%d %H:%M:%S') || {}", fraction(&t))
        }
        CellKind::Float => {
            let d = format!("CAST({q} AS DOUBLE)");
            format!(
                "CASE WHEN isnan({d}) THEN 'NaN' \
                 WHEN isinf({d}) THEN CASE WHEN {d} > 0 THEN 'inf' ELSE '-inf' END \
                 WHEN {d} = trunc({d}) AND abs({d}) < 1e15 THEN printf('%.1f', {d}) \
                 ELSE CAST({d} AS VARCHAR) END"
            )
        }
        CellKind::Unmatched => return None,
    };
    Some(format!("COALESCE({text}, '')"))
}

/// The SQL literal for a cell that Octa displays as `v` in a column of
/// `kind`, or `None` when no cell of that column can display as `v` (which
/// then keeps nothing, exactly as the in-memory test would). Each guard
/// re-renders the parsed value and requires the same text back, so `2`
/// never matches a float cell, which Octa shows as `2.0`.
fn cell_literal(kind: CellKind, v: &str) -> Option<String> {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    match kind {
        CellKind::Text => Some(sql_str(v)),
        CellKind::Integer => v.parse::<i128>().ok().map(|n| n.to_string()),
        CellKind::Float => {
            let x = v.parse::<f64>().ok()?;
            (CellValue::Float(x).to_string() == v).then(|| format!("CAST('{x:?}' AS DOUBLE)"))
        }
        CellKind::Decimal => v.parse::<f64>().is_ok().then(|| sql_str(v)),
        CellKind::Bool => match v {
            "true" | "false" => Some(v.to_string()),
            _ => None,
        },
        CellKind::Date => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .ok()
            .filter(|d| d.format("%Y-%m-%d").to_string() == v)
            .map(|_| format!("DATE {}", sql_str(v))),
        CellKind::Time => NaiveTime::parse_from_str(v, "%H:%M:%S%.f")
            .ok()
            .filter(|t| t.format("%H:%M:%S%.f").to_string() == v)
            .map(|_| format!("TIME {}", sql_str(v))),
        CellKind::Timestamp => NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S%.f")
            .ok()
            .filter(|t| t.format("%Y-%m-%d %H:%M:%S%.f").to_string() == v)
            .map(|_| format!("TIMESTAMP {}", sql_str(v))),
        CellKind::Unmatched => None,
    }
}

impl LargeTable {
    pub fn columns(&self) -> &[ColumnInfo] {
        &self.columns
    }

    /// Rows in the whole file, unfiltered.
    pub fn row_count(&self) -> usize {
        self.row_count
    }

    /// The `WHERE` condition keeping the rows whose cell in column `col`
    /// reads as one of `values` the way Octa displays cells, `""` standing
    /// for an empty one: the in-memory column filter, run over the whole file.
    /// Values are compared in the column's own type, so a float, a date or a
    /// zoned timestamp matches exactly what an ordinary tab would keep.
    /// `None` when the column's cells cannot be matched in a query at all.
    pub fn values_filter(&self, col: usize, values: &[&str]) -> Option<String> {
        let q = self.quoted_column(col)?;
        let kind = cell_kind(self.duck_types.get(col)?);
        if kind == CellKind::Unmatched {
            return None;
        }
        let zone = column_zones(&self.columns).into_iter().nth(col).flatten();
        let expr = match kind {
            CellKind::Float => format!("CAST({q} AS DOUBLE)"),
            CellKind::Decimal => format!("CAST({q} AS VARCHAR)"),
            // Shown on the zone's wall clock (or UTC without one), and the
            // displayed text is that wall clock with no offset.
            CellKind::Timestamp => match zone {
                Some(z) => format!("timezone({}, {q})", sql_str(&z)),
                None => format!("CAST({q} AS TIMESTAMP)"),
            },
            _ => q.clone(),
        };
        let literals: Vec<String> = values
            .iter()
            .filter(|v| !v.is_empty())
            .filter_map(|v| cell_literal(kind, v))
            .collect();
        let mut parts = Vec::new();
        if !literals.is_empty() {
            parts.push(format!("{expr} IN ({})", literals.join(", ")));
        }
        if values.contains(&"") {
            parts.push(match kind {
                CellKind::Text => format!("{q} IS NULL OR {q} = ''"),
                _ => format!("{q} IS NULL"),
            });
        }
        Some(if parts.is_empty() {
            "1 = 0".to_string()
        } else {
            parts.join(" OR ")
        })
    }

    /// The `WHERE` condition for a comparison filter (the Ask box's chips,
    /// `amount > 1000`): `predicate_filter::matches` over the whole file. As
    /// in memory, it tests the cell's displayed text: a number comparison
    /// when both the text and `value` read as numbers, otherwise text,
    /// folded to lower case unless `case_sensitive`. `None` when the
    /// column's cells cannot be spelled in a query.
    pub fn predicate_filter(
        &self,
        col: usize,
        op: CondOp,
        value: &str,
        case_sensitive: bool,
    ) -> Option<String> {
        let q = self.quoted_column(col)?;
        let kind = cell_kind(self.duck_types.get(col)?);
        let zone = column_zones(&self.columns).into_iter().nth(col).flatten();
        let d = display_sql(kind, &q, zone.as_deref())?;
        let folded = if case_sensitive {
            d.clone()
        } else {
            format!("lower({d})")
        };
        let lit = sql_str(&if case_sensitive {
            value.to_string()
        } else {
            value.to_lowercase()
        });
        let cmp = |sym: &str| {
            let text = format!("{folded} {sym} {lit}");
            let Ok(b) = value.trim().parse::<f64>() else {
                return text;
            };
            // A cell whose text reads as a number compares as one. NaN
            // compares false, as in Rust; DuckDB would put it above all.
            let n = format!("TRY_CAST(trim({d}) AS DOUBLE)");
            let number = if b.is_nan() {
                "false".to_string()
            } else {
                format!("NOT isnan({n}) AND {n} {sym} CAST('{b:?}' AS DOUBLE)")
            };
            format!("CASE WHEN {n} IS NOT NULL THEN {number} ELSE {text} END")
        };
        Some(match op {
            CondOp::Empty => format!("{d} = ''"),
            CondOp::NotEmpty => format!("{d} <> ''"),
            CondOp::Eq => format!("{folded} = {lit}"),
            CondOp::Ne => format!("{folded} <> {lit}"),
            CondOp::Contains => format!("contains({folded}, {lit})"),
            CondOp::NotContains => format!("NOT contains({folded}, {lit})"),
            CondOp::Gt => cmp(">"),
            CondOp::Lt => cmp("<"),
            CondOp::Ge => cmp(">="),
            CondOp::Le => cmp("<="),
        })
    }

    /// A job counting column `col`'s values over the whole file, for the
    /// Filter by value popup and the Column Filter window: the `top_n` most
    /// common, only those whose text contains `search` when it is not empty,
    /// labelled exactly as cells are displayed, plus the column's distinct,
    /// empty and filled counts. It opens a connection of its own, so it runs
    /// on a worker while the tab keeps paging.
    // ponytail: the search runs on DuckDB's text of the value, which can
    // differ from Octa's for a zoned timestamp; the filter itself is exact.
    pub fn value_frequency_job(
        &self,
        col: usize,
        search: &str,
        top_n: usize,
    ) -> Option<impl FnOnce() -> Result<ValueFrequency> + Send + 'static> {
        let q = self.quoted_column(col)?;
        let name = self.columns.get(col)?.name.clone();
        let zone = column_zones(&self.columns).into_iter().nth(col).flatten();
        // An empty text counts as missing, as `compute_value_frequency` has it.
        let filled_sql = if cell_kind(self.duck_types.get(col)?) == CellKind::Text {
            format!("NULLIF({q}, '')")
        } else {
            q.clone()
        };
        let scan = self.scan.clone();
        let needle = search.trim().to_lowercase();
        Some(move || {
            let conn = Connection::open_in_memory().context("opening a DuckDB connection")?;
            let f = &filled_sql;
            let (unique, nulls, filled): (i64, i64, i64) = conn.query_row(
                &format!(
                    "SELECT count(DISTINCT {f}), count(*) - count({f}), count({f}) FROM {scan}"
                ),
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let search_sql = if needle.is_empty() {
                String::new()
            } else {
                format!(
                    " AND contains(lower(CAST({q} AS VARCHAR)), {})",
                    sql_str(&needle)
                )
            };
            let mut stmt = conn.prepare(&format!(
                "SELECT {q}, count(*) AS n FROM {scan} WHERE {f} IS NOT NULL{search_sql} \
                 GROUP BY 1 ORDER BY n DESC, CAST({q} AS VARCHAR) LIMIT {top_n}"
            ))?;
            let mut rows = Vec::new();
            let mut found = stmt.query([])?;
            while let Some(r) = found.next()? {
                let label = duckdb_value_to_cell_tz(r.get_ref(0)?, zone.as_deref()).to_string();
                let count: i64 = r.get(1)?;
                rows.push(ValueFrequencyRow {
                    label,
                    count: count.max(0) as usize,
                });
            }
            Ok(ValueFrequency {
                column_name: name,
                rows,
                nulls: nulls.max(0) as usize,
                total_non_null: filled.max(0) as usize,
                unique_count: unique.max(0) as usize,
                binned: false,
            })
        })
    }

    /// Quote a column for SQL by index, so no caller string reaches the query.
    fn quoted_column(&self, idx: usize) -> Option<String> {
        self.columns
            .get(idx)
            .map(|c| format!("\"{}\"", c.name.replace('"', "\"\"")))
    }

    /// One page of rows. `order` is `(column index, ascending)`.
    pub fn page(
        &self,
        offset: usize,
        len: usize,
        order: Option<(usize, bool)>,
        filter: Option<&str>,
    ) -> Result<DataTable> {
        self.reader().page(&self.conn, offset, len, order, filter)
    }

    /// How many rows the filter keeps. `None` filter is the whole file, which
    /// is already known and costs no query.
    pub fn filtered_count(&self, filter: Option<&str>) -> Result<usize> {
        self.reader().count(&self.conn, filter)
    }

    /// [`Self::page`] as a job for a worker, on a clone of the connection, so
    /// a deep page of a big CSV or a new filter's count never holds up the
    /// window. With `count`, the job also counts the rows `filter` keeps and
    /// returns that total; without, the caller still has it from before.
    pub fn page_job(
        &self,
        offset: usize,
        len: usize,
        order: Option<(usize, bool)>,
        filter: String,
        count: bool,
    ) -> Result<impl FnOnce() -> Result<(DataTable, Option<usize>)> + Send + 'static> {
        let conn = self
            .conn
            .try_clone()
            .context("opening a second connection to the file")?;
        let reader = self.reader();
        Ok(move || {
            let filter = Some(filter.as_str()).filter(|f| !f.trim().is_empty());
            let total = if count {
                Some(reader.count(&conn, filter)?)
            } else {
                None
            };
            Ok((reader.page(&conn, offset, len, order, filter)?, total))
        })
    }

    /// Write the whole file, or the rows `filter` keeps, in `order`, to
    /// `target` as a job for a worker: DuckDB streams it from the scan to
    /// the new file, so no row passes through Octa and no copy of the table
    /// is held anywhere. Written beside `target` and renamed over it once
    /// complete. The job returns how many rows it wrote. The handle stops it
    /// (`InterruptHandle::interrupt`), leaving `target` as it was.
    pub fn export_job(
        &self,
        target: &Path,
        format: ExportFormat,
        order: Option<(usize, bool)>,
        filter: String,
    ) -> Result<(
        impl FnOnce() -> Result<usize> + Send + 'static,
        std::sync::Arc<duckdb::InterruptHandle>,
    )> {
        let conn = self
            .conn
            .try_clone()
            .context("opening a second connection to the file")?;
        let interrupt = conn.interrupt_handle();
        // DuckDB writes no decimal comma (`DECIMAL_SEPARATOR` is read-only),
        // so fractional columns are spelled with one here. Text is never
        // rewritten, as in Octa's own CSV writer.
        let projection = if matches!(
            format,
            ExportFormat::Delimited {
                decimal_comma: true,
                ..
            }
        ) {
            (0..self.columns.len())
                .filter_map(|i| {
                    let c = self.quoted_column(i)?;
                    Some(match cell_kind(&self.duck_types[i]) {
                        CellKind::Float | CellKind::Decimal => {
                            format!("replace(CAST({c} AS VARCHAR), '.', ',') AS {c}")
                        }
                        _ => c,
                    })
                })
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            "*".to_string()
        };
        let mut select = format!("SELECT {projection} FROM {}", self.scan);
        if !filter.trim().is_empty() {
            select.push_str(&format!(" WHERE {filter}"));
        }
        if let Some((col, asc)) = order
            && let Some(c) = self.quoted_column(col)
        {
            select.push_str(&format!(
                " ORDER BY {c} {}",
                if asc { "ASC" } else { "DESC" }
            ));
        }
        let target = target.to_path_buf();
        Ok((
            move || {
                crate::formats::write_atomically(&target, |tmp| format.copy(&conn, &select, tmp))
            },
            interrupt,
        ))
    }

    /// What reading a page needs besides a connection, owned so a worker can
    /// take it.
    fn reader(&self) -> PageReader {
        PageReader {
            scan: self.scan.clone(),
            columns: self.columns.clone(),
            row_count: self.row_count,
        }
    }
}

/// The file formats [`LargeTable::export_job`] writes, the ones DuckDB's
/// `COPY` writes itself.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    /// CSV or TSV.
    Delimited {
        delimiter: u8,
        decimal_comma: bool,
        bom: bool,
    },
    /// `codec` is one of `write_options::PARQUET_CODECS`, which DuckDB
    /// spells the same way.
    Parquet {
        codec: String,
        row_group_size: Option<usize>,
    },
    /// One JSON object per line, or with `array` one JSON array.
    Json { array: bool },
}

impl ExportFormat {
    /// What `path`'s extension asks for, `delimited` being how a CSV or TSV
    /// is written. `None` for a format `COPY` cannot write.
    pub fn for_path(
        path: &Path,
        delimited: Option<(u8, bool, bool)>,
        parquet: &crate::formats::write_options::ParquetOptions,
    ) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "csv" | "tsv" => {
                let (delimiter, decimal_comma, bom) =
                    delimited.unwrap_or((if ext == "tsv" { b'\t' } else { b',' }, false, false));
                Some(Self::Delimited {
                    delimiter,
                    decimal_comma,
                    bom,
                })
            }
            "parquet" | "pq" => Some(Self::Parquet {
                codec: parquet.compression.clone(),
                row_group_size: parquet.row_group_size,
            }),
            "json" => Some(Self::Json { array: true }),
            "jsonl" | "ndjson" => Some(Self::Json { array: false }),
            _ => None,
        }
    }

    /// `COPY (select) TO tmp`, returning the rows written.
    fn copy(&self, conn: &Connection, select: &str, tmp: &Path) -> Result<usize> {
        let quoted = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let options = match self {
            Self::Delimited { delimiter, .. } => {
                let delim = (*delimiter as char).to_string().replace('\'', "''");
                format!("FORMAT csv, HEADER true, DELIMITER '{delim}'")
            }
            Self::Parquet {
                codec,
                row_group_size,
            } => {
                // Only a known codec name reaches the SQL text.
                let codec =
                    if crate::formats::write_options::PARQUET_CODECS.contains(&codec.as_str()) {
                        codec.as_str()
                    } else {
                        "zstd"
                    };
                let mut o = format!("FORMAT parquet, COMPRESSION '{codec}'");
                if let Some(n) = row_group_size {
                    o.push_str(&format!(", ROW_GROUP_SIZE {n}"));
                }
                o
            }
            Self::Json { array } => format!("FORMAT json, ARRAY {array}"),
        };
        let bom = matches!(self, Self::Delimited { bom: true, .. });
        // DuckDB writes no byte order mark: the rows go to a second temp
        // file and are appended behind one.
        let body = if bom {
            let mut b = tmp.as_os_str().to_os_string();
            b.push(".body");
            std::path::PathBuf::from(b)
        } else {
            tmp.to_path_buf()
        };
        let written = conn
            .execute(
                &format!("COPY ({select}) TO '{}' ({options})", quoted(&body)),
                [],
            )
            .context("writing the file")?;
        if bom {
            let result = (|| -> Result<()> {
                let mut out = std::fs::File::create(tmp)?;
                std::io::Write::write_all(&mut out, b"\xEF\xBB\xBF")?;
                std::io::copy(&mut std::fs::File::open(&body)?, &mut out)?;
                Ok(())
            })();
            let _ = std::fs::remove_file(&body);
            result?;
        }
        Ok(written)
    }
}

/// The scan, the schema and the row count: everything [`LargeTable::page`]
/// and [`LargeTable::filtered_count`] read, apart from the connection.
struct PageReader {
    scan: String,
    columns: Vec<ColumnInfo>,
    row_count: usize,
}

impl PageReader {
    fn page(
        &self,
        conn: &Connection,
        offset: usize,
        len: usize,
        order: Option<(usize, bool)>,
        filter: Option<&str>,
    ) -> Result<DataTable> {
        let mut sql = format!("SELECT * FROM {}", self.scan);
        if let Some(f) = filter.filter(|f| !f.trim().is_empty()) {
            sql.push_str(&format!(" WHERE {f}"));
        }
        if let Some((col, asc)) = order
            && let Some(c) = self.columns.get(col)
        {
            sql.push_str(&format!(
                " ORDER BY \"{}\" {}",
                c.name.replace('"', "\"\""),
                if asc { "ASC" } else { "DESC" }
            ));
        }
        sql.push_str(&format!(" LIMIT {len} OFFSET {offset}"));

        let mut stmt = conn.prepare(&sql)?;
        let col_count = self.columns.len();
        let zones = column_zones(&self.columns);
        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let mut row = Vec::with_capacity(col_count);
            for (i, tz) in zones.iter().enumerate() {
                row.push(duckdb_value_to_cell_tz(r.get_ref(i)?, tz.as_deref()));
            }
            rows.push(row);
        }

        let mut table = DataTable::empty();
        table.columns = self.columns.clone();
        table.rows = rows;
        // The page is a window onto a bigger file, so the view can say
        // "rows 500 to 600 of 40,000,000" rather than "100 rows".
        table.total_rows = Some(self.row_count);
        table.row_offset = offset;
        Ok(table)
    }

    fn count(&self, conn: &Connection, filter: Option<&str>) -> Result<usize> {
        let Some(f) = filter.filter(|f| !f.trim().is_empty()) else {
            return Ok(self.row_count);
        };
        let n: i64 = conn.query_row(
            &format!("SELECT count(*) FROM {} WHERE {f}", self.scan),
            [],
            |r| r.get(0),
        )?;
        Ok(n.max(0) as usize)
    }
}

/// Rewrite a file DuckDB cannot scan in place as a temporary Parquet file, so
/// large-file mode can read it after all.
///
/// This is the one path that does hold the whole table in memory, once, which
/// is why the notice warns about it: it costs about what opening the file
/// normally costs, plus the disk space for the copy. `cancel` is checked
/// before the read and before the write, the only two points where stopping
/// means anything.
pub fn convert_to_temp_parquet(
    path: &Path,
    progress: &dyn Fn(usize),
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<std::path::PathBuf> {
    use std::sync::atomic::Ordering;

    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    // The point of the conversion is to reach every row, so the streaming cap
    // is lifted for the read.
    let table = {
        let _g = crate::formats::InitialLoadRowsGuard::new(usize::MAX);
        crate::formats::read_table_auto(
            path,
            None,
            crate::formats::compression::DEFAULT_MAX_DECOMPRESSED_BYTES,
        )?
    };
    progress(table.row_count());
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }

    let tmp = tempfile::Builder::new()
        .suffix(".parquet")
        .tempfile()
        .context("creating a temporary Parquet file")?;
    let out = tmp.path().to_path_buf();
    crate::formats::parquet_reader::write_parquet_with(
        &out,
        &table,
        &crate::formats::write_options::ParquetOptions::default(),
    )
    .with_context(|| format!("writing {}", out.display()))?;
    // Kept past the handle's lifetime on purpose: the tab reads from it for as
    // long as it is open, and the OS temp cleaner reclaims it afterwards.
    let _ = tmp.keep();
    progress(table.row_count());
    Ok(out)
}

#[cfg(test)]
#[path = "large_tests.rs"]
mod tests;
