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

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::duckdb_reader::{duckdb_type_to_arrow, duckdb_value_to_cell};

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
    row_count: usize,
}

/// Open `path` as a paged scan.
pub fn open(path: &Path) -> Result<LargeTable> {
    let kind = ScanKind::for_path(path)
        .ok_or_else(|| anyhow::anyhow!("{} cannot be scanned in place", path.display()))?;
    let scan = kind.scan_expr(path);
    let conn = Connection::open_in_memory().context("opening an in-memory DuckDB connection")?;

    let mut stmt = conn
        .prepare(&format!("DESCRIBE SELECT * FROM {scan}"))
        .with_context(|| format!("describing {}", path.display()))?;
    let columns: Vec<ColumnInfo> = stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let ty: String = r.get(1)?;
            Ok(ColumnInfo {
                name,
                data_type: duckdb_type_to_arrow(&ty),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
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
        row_count,
    })
}

impl LargeTable {
    pub fn columns(&self) -> &[ColumnInfo] {
        &self.columns
    }

    /// Rows in the whole file, unfiltered.
    pub fn row_count(&self) -> usize {
        self.row_count
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
        let mut sql = format!("SELECT * FROM {}", self.scan);
        if let Some(f) = filter.filter(|f| !f.trim().is_empty()) {
            sql.push_str(&format!(" WHERE {f}"));
        }
        if let Some((col, asc)) = order
            && let Some(name) = self.quoted_column(col)
        {
            sql.push_str(&format!(
                " ORDER BY {name} {}",
                if asc { "ASC" } else { "DESC" }
            ));
        }
        sql.push_str(&format!(" LIMIT {len} OFFSET {offset}"));

        let mut stmt = self.conn.prepare(&sql)?;
        let col_count = self.columns.len();
        let mut rows: Vec<Vec<CellValue>> = Vec::new();
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let mut row = Vec::with_capacity(col_count);
            for i in 0..col_count {
                row.push(duckdb_value_to_cell(r.get_ref(i)?));
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

    /// How many rows the filter keeps. `None` filter is the whole file, which
    /// is already known and costs no query.
    pub fn filtered_count(&self, filter: Option<&str>) -> Result<usize> {
        let Some(f) = filter.filter(|f| !f.trim().is_empty()) else {
            return Ok(self.row_count);
        };
        let n: i64 = self.conn.query_row(
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
mod tests {
    use super::*;
    use std::io::Write;

    fn big_csv(rows: usize) -> tempfile::NamedTempFile {
        let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
        writeln!(f, "id,name").unwrap();
        for i in 0..rows {
            writeln!(f, "{i},row{i}").unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn reports_columns_and_row_count_without_loading() {
        let f = big_csv(1000);
        let t = open(f.path()).unwrap();
        assert_eq!(t.row_count(), 1000);
        assert_eq!(t.columns().len(), 2);
        assert_eq!(t.columns()[0].name, "id");
    }

    #[test]
    fn pages_from_the_middle() {
        let f = big_csv(1000);
        let t = open(f.path()).unwrap();
        // Ordered, because DuckDB parallelises a CSV scan: OFFSET without an
        // ORDER BY is a slice of an unspecified order, so asserting a value
        // there would be asserting an accident.
        let page = t.page(500, 3, Some((0, true)), None).unwrap();
        assert_eq!(page.row_count(), 3);
        assert_eq!(page.get(0, 0).unwrap().to_string(), "500");
        assert_eq!(page.total_rows, Some(1000), "the page knows its whole");
        assert_eq!(page.row_offset, 500);
        // Unordered still pages, it just does not promise which rows.
        assert_eq!(t.page(500, 3, None, None).unwrap().row_count(), 3);
    }

    #[test]
    fn sorts_descending_on_a_column() {
        let f = big_csv(100);
        let t = open(f.path()).unwrap();
        let page = t.page(0, 1, Some((0, false)), None).unwrap();
        assert_eq!(page.get(0, 0).unwrap().to_string(), "99");
    }

    #[test]
    fn filters_and_counts() {
        let f = big_csv(100);
        let t = open(f.path()).unwrap();
        let filter = "\"name\" = 'row7'";
        assert_eq!(t.filtered_count(Some(filter)).unwrap(), 1);
        let page = t.page(0, 10, None, Some(filter)).unwrap();
        assert_eq!(page.row_count(), 1);
    }

    #[test]
    fn refuses_a_format_duckdb_cannot_scan() {
        let f = tempfile::Builder::new().suffix(".sav").tempfile().unwrap();
        assert!(ScanKind::for_path(f.path()).is_none());
    }
}
