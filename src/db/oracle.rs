//! Oracle connector (`oracle-rs`, a pure-Rust TNS implementation over tokio).
//! SQL goes to the server verbatim (real Oracle SQL).
//!
//! No Instant Client: every other Oracle crate wraps OCI/ODPI-C, which would
//! make a separate Oracle download a prerequisite of installing Octa. The
//! price is a young driver, so the error paths here surface the driver's own
//! message rather than flattening it.
//!
//! Two Oracle-specific shapes to know about:
//!
//! - **The Database field is the service name** (`FREEPDB1`, `ORCLPDB1`),
//!   not a database in the Postgres sense. A SID-only listener is not
//!   reachable this way; the docs say so.
//! - **Oracle has no `BEGIN` statement.** A transaction starts with the first
//!   DML and ends at `COMMIT`/`ROLLBACK`, so [`OracleConnector::execute`]
//!   answers the shared write-back skeleton's `BEGIN` itself, the same way the
//!   ClickHouse and warehouse connectors do.
//!
//! Auth is password only and the socket is plain TNS: no TLS/TCPS, no
//! wallet, no Kerberos. That puts Autonomous Database on OCI out of reach
//! (it always wants TLS) and leaves the jump host as the way to encrypt the
//! hop to an on-prem server.
//!
//! **Three things `oracle-rs` 0.1.7 does not do**, all verified against an
//! Oracle Free 23ai server, all handled here rather than hidden:
//!
//! 1. It hands every `NUMBER` over as **decimal text**, whatever the column
//!    declares, so [`OracleConnector::cell`] parses it into the bucket
//!    [`arrow_type`] chose. That text carries Oracle's full precision, which
//!    is why parsing it beats waiting for the driver to decode.
//! 2. It does not decode `BINARY_FLOAT` / `BINARY_DOUBLE` at all: the raw
//!    bytes arrive as a lossy string. Those cells say so rather than show
//!    mojibake; `CAST(x AS NUMBER)` in the query is the workaround.
//! 3. **Any statement the server rejects loses the connection**, and the
//!    driver reports its own canned text instead of the `ORA-` reason (its
//!    own source says so). The connection cache reconnects on the next call,
//!    so a typo costs a round trip rather than a wedged session.

use anyhow::{Context, Result, bail};
use oracle_rs::{Config, Connection, LobValue, OracleType, Value};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::{DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport, auth, runtime};

/// Rows per round trip once the driver's first prefetch is exhausted.
const FETCH_ROWS: u32 = 1000;

/// Largest LOB pulled into a cell. Above this the cell says what it is and
/// how big instead, because a grid of 50 MB CLOBs is a memory accident, not a
/// table anyone reads.
const MAX_LOB_BYTES: u64 = 1 << 20;

pub struct OracleConnector {
    conn: Connection,
    conn_label: String,
    /// Set between the write-back skeleton's `BEGIN` and its
    /// `COMMIT`/`ROLLBACK`. Outside one, [`OracleConnector::execute`] commits
    /// each statement itself: the driver never auto-commits, so an INSERT run
    /// from the SQL panel would otherwise be discarded at disconnect.
    in_transaction: bool,
}

impl OracleConnector {
    pub fn connect(conn: &DbConnection, stored: Option<&str>) -> Result<Self> {
        let secret = auth::resolve_password(conn, stored)?;
        let service = conn.database.trim();
        if service.is_empty() {
            bail!(
                "Oracle needs a service name: put it in the Database field of \
                 connection '{}' (for example FREEPDB1)",
                conn.name
            );
        }
        // Behind a jump host the socket goes to the loopback forward; `host`
        // and `port` keep naming the database itself for the error text.
        let (dial_host, dial_port) = conn.dial_target();
        let config = Config::new(dial_host, dial_port, service, &conn.username, secret);
        let client = runtime()
            .block_on(Connection::connect_with_config(config))
            .with_context(|| {
                format!(
                    "connecting to {}@{}:{}/{}",
                    conn.username, conn.host, conn.port, service
                )
            })?;
        Ok(Self {
            conn: client,
            conn_label: conn.name.clone(),
            in_transaction: false,
        })
    }

    /// The first column of every row of `sql`, dropping empties. Used by the
    /// catalog listings, which all select one name column.
    fn string_column(&mut self, sql: &str, what: &str) -> Result<Vec<String>> {
        let table = self
            .query(sql)
            .with_context(|| format!("{what} on '{}'", self.conn_label))?;
        Ok(table
            .rows
            .iter()
            .filter_map(|r| r.first().map(CellValue::to_string))
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// One cell, coerced into the bucket [`arrow_type`] chose for its column
    /// so the column's declared type and its values agree.
    async fn cell(
        &self,
        value: &Value,
        col: &oracle_rs::ColumnInfo,
        arrow: &str,
    ) -> Result<CellValue> {
        // The driver returns BINARY_FLOAT / BINARY_DOUBLE undecoded, as raw
        // bytes in a lossy string. A cell that says so is the only honest
        // rendering; showing the mojibake would look like data.
        if !value.is_null()
            && matches!(
                col.oracle_type,
                OracleType::BinaryFloat | OracleType::BinaryDouble
            )
        {
            let what = match col.oracle_type {
                OracleType::BinaryFloat => "BINARY_FLOAT",
                _ => "BINARY_DOUBLE",
            };
            return Ok(CellValue::String(format!(
                "[{what} not readable by this driver: SELECT CAST({} AS NUMBER)]",
                col.name
            )));
        }
        Ok(match value {
            Value::Null => CellValue::Null,
            Value::Boolean(b) => CellValue::Bool(*b),
            Value::Bytes(b) => CellValue::Binary(b.clone()),
            Value::Lob(lob) => self.lob_cell(lob, arrow == "Binary").await?,
            Value::Json(j) => CellValue::Nested(j.to_string()),
            Value::RowId(r) => r
                .to_string()
                .map(CellValue::String)
                .unwrap_or(CellValue::Null),
            // A NUMBER arrives as decimal text whatever it was declared as,
            // so a numeric column parses it back; a real VARCHAR2 keeps it.
            Value::String(s) => match arrow {
                "Int64" => s
                    .parse::<i64>()
                    .map(CellValue::Int)
                    .or_else(|_| s.parse::<f64>().map(CellValue::Float))
                    .unwrap_or_else(|_| CellValue::String(s.clone())),
                "Float64" => s
                    .parse::<f64>()
                    .map(CellValue::Float)
                    .unwrap_or_else(|_| CellValue::String(s.clone())),
                _ => CellValue::String(s.clone()),
            },
            Value::Date(d) => CellValue::DateTime(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                d.year, d.month, d.day, d.hour, d.minute, d.second
            )),
            Value::Timestamp(t) => {
                let mut s = format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    t.year, t.month, t.day, t.hour, t.minute, t.second
                );
                if t.microsecond > 0 {
                    s.push_str(&format!(".{:06}", t.microsecond));
                }
                if t.tz_hour_offset != 0 || t.tz_minute_offset != 0 {
                    let sign = if t.tz_hour_offset < 0 { '-' } else { '+' };
                    s.push_str(&format!(
                        " {sign}{:02}:{:02}",
                        t.tz_hour_offset.abs(),
                        t.tz_minute_offset.abs()
                    ));
                }
                CellValue::DateTime(s)
            }
            // NUMBER arrives as an integer whenever it is whole and fits; the
            // rest keep Oracle's full-precision decimal text.
            Value::Integer(i) => {
                if arrow == "Float64" {
                    CellValue::Float(*i as f64)
                } else {
                    CellValue::Int(*i)
                }
            }
            Value::Float(f) => CellValue::Float(*f),
            Value::Number(n) => {
                if arrow == "Int64" {
                    // ponytail: a NUMBER(38,0) wider than i64 degrades to a
                    // float here. Carry the decimal text instead if someone
                    // hits it on real data.
                    match n.to_i64() {
                        Ok(i) => CellValue::Int(i),
                        Err(_) => n.to_f64().map(CellValue::Float).unwrap_or(CellValue::Null),
                    }
                } else {
                    n.to_f64().map(CellValue::Float).unwrap_or(CellValue::Null)
                }
            }
            // VECTOR (23ai), REF CURSOR and object collections have no flat
            // rendering. The debug form at least shows what is in the cell.
            other => CellValue::Nested(format!("{other:?}")),
        })
    }

    /// A LOB cell. Oracle prefetches small LOBs inline; the rest arrive as a
    /// locator that costs one round trip to read.
    ///
    /// ponytail: one round trip per locator cell, so a page of large CLOBs is
    /// slow. Batch them only if someone reads such a table often.
    async fn lob_cell(&self, lob: &LobValue, binary: bool) -> Result<CellValue> {
        Ok(match lob {
            LobValue::Null => CellValue::Null,
            LobValue::Empty => {
                if binary {
                    CellValue::Binary(Vec::new())
                } else {
                    CellValue::String(String::new())
                }
            }
            LobValue::Inline(data) => {
                if binary {
                    CellValue::Binary(data.to_vec())
                } else {
                    CellValue::String(String::from_utf8_lossy(data).into_owned())
                }
            }
            LobValue::Locator(loc) => {
                let size = lob.size().unwrap_or(0);
                if size > MAX_LOB_BYTES {
                    // Deliberately a text marker even in a Binary column: an
                    // empty blob would claim the value is empty, which is the
                    // one thing it is not.
                    // Oracle reports a CLOB's size in characters and a BLOB's
                    // in bytes, so the marker names the unit it is quoting.
                    let (what, unit) = if binary {
                        ("BLOB", "bytes")
                    } else {
                        ("CLOB", "characters")
                    };
                    return Ok(CellValue::String(format!(
                        "[{what} of {size} {unit}, too large to read]"
                    )));
                }
                if binary {
                    CellValue::Binary(self.conn.read_blob(loc).await?.to_vec())
                } else {
                    CellValue::String(self.conn.read_clob(loc).await?)
                }
            }
        })
    }
}

/// Whether a NUMBER value is not a whole number. The driver sends NUMBER as
/// text, so most of the evidence is in the digits.
fn is_fractional(v: &Value) -> bool {
    match v {
        Value::Float(_) | Value::Number(_) => true,
        Value::String(s) => s.contains(['.', 'e', 'E']),
        _ => false,
    }
}

/// Map an Oracle column to the Arrow-name strings Octa uses.
///
/// `sample` is that column's own values. It is read only for a NUMBER with no
/// declared precision, which is what Oracle reports for a literal or a
/// computed column: `SELECT 1 FROM dual` has no precision to go on, and
/// reading it as `1.0` is not what anybody meant. Everything else is decided
/// by the declaration alone.
fn arrow_type<'a>(
    c: &oracle_rs::ColumnInfo,
    sample: impl Iterator<Item = &'a Value>,
) -> &'static str {
    match c.oracle_type {
        OracleType::Number | OracleType::BinaryInteger => {
            if c.precision > 0 {
                // NUMBER(p, 0) is a whole number; any other scale is not.
                // (An unconstrained scale reads as 129, the -127 byte the
                // driver never sign-extends, so it lands here as "not 0".)
                if c.scale == 0 { "Int64" } else { "Float64" }
            } else if sample.into_iter().any(is_fractional) {
                "Float64"
            } else {
                "Int64"
            }
        }
        // Not "Float64": the driver never decodes these, so the cell carries
        // a marker and the column type has to agree with the cell.
        OracleType::BinaryFloat | OracleType::BinaryDouble => "Utf8",
        // Oracle had no native boolean before 23c, so pre-23c schemas spell it
        // NUMBER(1) or CHAR(1) and land in those buckets instead.
        OracleType::Boolean => "Boolean",
        // An Oracle DATE always carries a time, so it is a timestamp here.
        OracleType::Date
        | OracleType::Timestamp
        | OracleType::TimestampTz
        | OracleType::TimestampLtz => "Timestamp(Microsecond, None)",
        OracleType::Raw | OracleType::LongRaw | OracleType::Blob | OracleType::Bfile => "Binary",
        _ => "Utf8",
    }
}

impl DbConnector for OracleConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Oracle
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        // `oracle_maintained` (12.1+, which the driver requires anyway) is what
        // keeps the ~30 shipped schemas out of the sidebar for a DBA account.
        self.string_column(
            "SELECT username FROM all_users WHERE oracle_maintained = 'N' ORDER BY username",
            "listing schemas",
        )
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let owner = schema.replace('\'', "''");
        self.string_column(
            &format!(
                "SELECT table_name FROM all_tables WHERE owner = '{owner}' \
                 UNION SELECT view_name FROM all_views WHERE owner = '{owner}' \
                 ORDER BY 1"
            ),
            "listing tables",
        )
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        // Stop at the initial-load row cap so an unbounded SELECT cannot
        // exhaust memory; the server-side cursor is left for Oracle to close.
        let cap = crate::formats::initial_load_rows();
        let label = self.conn_label.clone();
        runtime()
            .block_on(async {
                let first = self.conn.query(sql, &[]).await?;
                let columns = first.columns;
                let cursor_id = first.cursor_id;
                let mut rows = first.rows;
                let mut more = first.has_more_rows;
                while more && rows.len() < cap {
                    let next = self
                        .conn
                        .fetch_more(cursor_id, &columns, FETCH_ROWS)
                        .await?;
                    more = next.has_more_rows && !next.rows.is_empty();
                    rows.extend(next.rows);
                }
                rows.truncate(cap);

                // ponytail: the first 200 rows decide an undeclared NUMBER's
                // bucket. Scanning every row of a 5M-row page to type one literal
                // column is not worth it; widen this if a real table ever hides
                // its first decimal past row 200.
                let types: Vec<&'static str> = columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| arrow_type(c, rows.iter().take(200).filter_map(|r| r.get(i))))
                    .collect();
                let mut table = DataTable::empty();
                table.columns = columns
                    .iter()
                    .zip(&types)
                    .map(|(c, t)| ColumnInfo {
                        name: c.name.clone(),
                        data_type: (*t).to_string(),
                    })
                    .collect();
                for row in &rows {
                    let mut cells = Vec::with_capacity(types.len());
                    for (i, ty) in types.iter().enumerate() {
                        cells.push(
                            self.cell(row.get(i).unwrap_or(&Value::Null), &columns[i], ty)
                                .await?,
                        );
                    }
                    table.rows.push(cells);
                }
                Ok::<_, anyhow::Error>(table)
            })
            .with_context(|| format!("querying '{label}'"))
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        // Exact matches only: `BEGIN` also opens a PL/SQL block, and a user
        // running one from the SQL panel must not have it swallowed.
        let stmt = sql.trim().trim_end_matches(';').trim();
        if stmt.eq_ignore_ascii_case("BEGIN") {
            self.in_transaction = true;
            return Ok(0);
        }
        if stmt.eq_ignore_ascii_case("COMMIT") {
            self.in_transaction = false;
            runtime()
                .block_on(self.conn.commit())
                .with_context(|| format!("committing on '{}'", self.conn_label))?;
            return Ok(0);
        }
        if stmt.eq_ignore_ascii_case("ROLLBACK") {
            self.in_transaction = false;
            runtime()
                .block_on(self.conn.rollback())
                .with_context(|| format!("rolling back on '{}'", self.conn_label))?;
            return Ok(0);
        }
        let in_transaction = self.in_transaction;
        let label = self.conn_label.clone();
        runtime()
            .block_on(async {
                let result = self.conn.execute(sql, &[]).await?;
                if !in_transaction {
                    self.conn.commit().await?;
                }
                Ok::<_, oracle_rs::Error>(result.rows_affected)
            })
            .with_context(|| format!("executing on '{label}'"))
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        super::reject_catalog(self.engine(), catalog)?;
        super::write_table_generic(self, DbEngine::Oracle, None, schema, table, mode, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(oracle_type: OracleType, precision: i16, scale: i16) -> oracle_rs::ColumnInfo {
        oracle_rs::ColumnInfo {
            name: "C".to_string(),
            oracle_type,
            data_size: 22,
            buffer_size: 22,
            precision,
            scale,
            nullable: true,
            csfrm: 1,
            type_schema: None,
            type_name: None,
            domain_schema: None,
            domain_name: None,
            is_json: false,
            is_oson: false,
            vector_dimensions: None,
            vector_format: None,
            element_type: None,
        }
    }

    /// NUMBER is Oracle's only numeric type, so the integer/decimal split has
    /// to come out of the precision and scale or every id column reads as a
    /// float.
    #[test]
    fn number_scale_decides_int_or_float() {
        let none = || [].iter();
        assert_eq!(arrow_type(&col(OracleType::Number, 38, 0), none()), "Int64");
        assert_eq!(
            arrow_type(&col(OracleType::Number, 10, 2), none()),
            "Float64"
        );
    }

    /// A literal or a computed column carries no precision, so its values are
    /// the only evidence there is.
    #[test]
    fn undeclared_number_follows_its_values() {
        let whole = [Value::Null, Value::Integer(1)];
        let fractional = [Value::Integer(1), Value::Float(1.5)];
        assert_eq!(
            arrow_type(&col(OracleType::Number, 0, -127), whole.iter()),
            "Int64"
        );
        assert_eq!(
            arrow_type(&col(OracleType::Number, 0, -127), fractional.iter()),
            "Float64"
        );
    }

    /// An Oracle DATE carries a time of day, unlike a SQL DATE.
    #[test]
    fn date_is_a_timestamp() {
        let none = || [].iter();
        assert_eq!(
            arrow_type(&col(OracleType::Date, 0, 0), none()),
            "Timestamp(Microsecond, None)"
        );
        assert_eq!(arrow_type(&col(OracleType::Blob, 0, 0), none()), "Binary");
        assert_eq!(arrow_type(&col(OracleType::Varchar, 0, 0), none()), "Utf8");
    }
}
