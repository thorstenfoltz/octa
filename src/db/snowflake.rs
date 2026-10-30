//! Snowflake connector over the SQL API v2 (`/api/v2/statements`). No native
//! driver crate; this is a small REST client (see [`super::rest`]) that submits
//! a statement, polls when Snowflake answers 202/async, and maps the JSON
//! result set into a [`DataTable`]. Bearer auth is resolved per connection from
//! the Phase-4 auth machinery (key-pair JWT, OAuth, or external-browser SSO).
//!
//! Live-only: the parser ([`parse_sf_result`]) is unit-tested; the HTTP flow is
//! covered by the env-gated live test.

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::rest::{
    InFlight, POLL_DELAY, RestClient, poll, poll_tries, snowflake_cancel_path, string_cell,
};
use super::{CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};

pub struct SnowflakeConnector {
    client: RestClient,
    bearer: String,
    database: String,
    conn_label: String,
    /// Seconds to keep asking Snowflake whether the statement is done. Also
    /// the statement timeout sent to the server, so the two agree: a server
    /// still running a statement Octa gave up on is a query nobody is
    /// watching and a warehouse nobody meant to keep warm.
    timeout_secs: u32,
    cancel: CancelFlag,
    in_flight: InFlight,
}

impl SnowflakeConnector {
    /// Connect: derive the account host, resolve a bearer token for the
    /// connection's auth mode, and tag the REST client with the matching
    /// Snowflake token type.
    pub fn connect(conn: &DbConnection, secret: Option<&str>) -> Result<Self> {
        let host = conn.host.trim();
        let base = if host.contains("snowflakecomputing.com") {
            format!("https://{host}")
        } else {
            format!("https://{host}.snowflakecomputing.com")
        };
        let account = account(conn);

        let (bearer, token_type) = resolve_bearer(conn, account, secret)?;
        let client = RestClient::new(base)
            .with_header("X-Snowflake-Authorization-Token-Type", token_type)
            .with_header("Accept", "application/json");
        Ok(Self {
            client,
            bearer,
            database: conn.database.clone(),
            conn_label: conn.name.clone(),
            timeout_secs: conn.query_timeout_secs,
            cancel: CancelFlag::new(),
            in_flight: InFlight::default(),
        })
    }

    /// Submit one statement and return the completed result JSON, polling while
    /// Snowflake reports the statement still running.
    fn submit(&self, sql: &str) -> Result<Value> {
        self.in_flight.clear();
        let body = serde_json::json!({
            "statement": sql,
            "timeout": self.timeout_secs,
            "database": self.database,
        });
        let first = self
            .client
            .post_json("api/v2/statements", &self.bearer, &body)
            .with_context(|| format!("submitting statement on '{}'", self.conn_label))?;
        if first.get("resultSetMetaData").is_some() {
            return Ok(first);
        }
        // Async: poll the status endpoint until the result set appears.
        let handle = first["statementHandle"]
            .as_str()
            .context("Snowflake did not return a statement handle")?
            .to_string();
        self.in_flight.set(&handle);
        let path = format!("api/v2/statements/{handle}");
        let cancel = self.cancel.clone();
        let result = poll(
            || self.client.get_json(&path, &self.bearer),
            |v| v.get("resultSetMetaData").is_some(),
            |v| {
                v.get("message").is_some()
                    && v.get("resultSetMetaData").is_none()
                    && is_error_code(v)
            },
            move || cancel.is_cancelled(),
            // The POST returns 202 straight away, so the whole budget is
            // available for polling.
            poll_tries(self.timeout_secs, 0),
            POLL_DELAY,
        );
        // Clear on every exit path (success and error alike), so a stale
        // handle is never cancelled later.
        self.in_flight.clear();
        result
    }

    /// Run a `SHOW ...` and pull the values of its `name` column.
    fn show_names(&self, sql: &str) -> Result<Vec<String>> {
        let t = parse_sf_result(&self.submit(sql)?)?;
        let Some(col) = t
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case("name"))
        else {
            return Ok(Vec::new());
        };
        Ok(t.rows
            .iter()
            .filter_map(|r| r.get(col))
            .map(cell_text)
            .collect())
    }

    /// Append partitions 1..n of a result set, stopping at `cap` rows.
    ///
    /// The SQL API returns **partition 0 only** and lists the rest in
    /// `resultSetMetaData.partitionInfo`; each further one is a GET on the
    /// same statement with `?partition=N`. Without this walk a large table
    /// opened with however many rows happened to fit in the first partition
    /// and said nothing about the rest, which is worse than an error.
    fn read_partitions(&self, first: &Value, table: &mut DataTable, cap: usize) -> Result<()> {
        let count = sf_partition_count(first);
        if count < 2 {
            return Ok(());
        }
        let Some(handle) = first["statementHandle"].as_str() else {
            return Ok(());
        };
        let columns = table.columns.clone();
        for n in 1..count {
            if table.rows.len() >= cap {
                return Ok(());
            }
            if self.cancel.is_cancelled() {
                bail!("statement cancelled");
            }
            let v = self
                .client
                .get_json(&sf_partition_path(handle, n), &self.bearer)
                .with_context(|| {
                    format!("fetching result partition {n} on '{}'", self.conn_label)
                })?;
            append_sf_rows(&v["data"], &columns, &mut table.rows);
        }
        Ok(())
    }
}

/// How many partitions a result set has (1 when the API says nothing).
fn sf_partition_count(v: &Value) -> usize {
    v["resultSetMetaData"]["partitionInfo"]
        .as_array()
        .map_or(1, Vec::len)
}

/// The GET path for one further partition of a finished statement.
fn sf_partition_path(handle: &str, partition: usize) -> String {
    format!("api/v2/statements/{handle}?partition={partition}")
}

impl DbConnector for SnowflakeConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Snowflake
    }

    fn list_catalogs(&mut self) -> Result<Vec<String>> {
        // Snowflake's top level is the database.
        self.show_names("SHOW DATABASES")
    }

    fn list_schemas(&mut self, catalog: Option<&str>) -> Result<Vec<String>> {
        // Snowflake identifiers: double-quote and double embedded quotes.
        let sql = match catalog {
            Some(c) => format!("SHOW SCHEMAS IN DATABASE \"{}\"", c.replace('"', "\"\"")),
            None => "SHOW SCHEMAS".to_string(),
        };
        self.show_names(&sql)
    }

    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let sch = schema.replace('"', "\"\"");
        let sql = match catalog {
            Some(c) => format!(
                "SHOW TABLES IN SCHEMA \"{}\".\"{sch}\"",
                c.replace('"', "\"\"")
            ),
            None => format!("SHOW TABLES IN SCHEMA \"{sch}\""),
        };
        self.show_names(&sql)
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.cancel.reset();
        let cap = crate::formats::initial_load_rows();
        let v = self.submit(sql)?;
        let mut t = parse_sf_result(&v)?;
        self.read_partitions(&v, &mut t, cap)?;
        if t.rows.len() > cap {
            t.rows.truncate(cap);
        }
        Ok(t)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        self.cancel.reset();
        // The SQL API auto-commits each call, so a BEGIN/COMMIT/ROLLBACK from
        // the shared writer can't form one transaction; swallow them (writes
        // are therefore non-atomic).
        let head = sql.trim_start();
        let upper = head[..head.len().min(9)].to_ascii_uppercase();
        if upper.starts_with("BEGIN")
            || upper.starts_with("COMMIT")
            || upper.starts_with("ROLLBACK")
        {
            return Ok(0);
        }
        let v = self.submit(sql)?;
        Ok(v["stats"]["numRowsInserted"].as_u64().unwrap_or(0))
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        // ponytail: literal-INSERT writer; non-atomic on the SQL API (each call
        // auto-commits). Standard Snowflake DDL/DML, so Create/Append/Replace
        // all work.
        super::write_table_generic(
            self,
            DbEngine::Snowflake,
            catalog,
            schema,
            table,
            mode,
            data,
        )
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let cancel = self.cancel.clone();
        let in_flight = self.in_flight.clone();
        let client = self.client.clone();
        let bearer = self.bearer.clone();
        Some(Box::new(move || {
            // Stop the client-side wait first: that always works, and the
            // vendor call below is best-effort.
            cancel.cancel();
            let Some(handle) = in_flight.get() else {
                return;
            };
            let path = snowflake_cancel_path(&handle);
            // Best effort: the statement may already have finished, and a
            // failed cancel must not surface as a query error.
            let _ = client.post_json(&path, &bearer, &serde_json::json!({}));
        }))
    }
}

/// Whether an error `code` is present and non-success (`"000000"` is success).
fn is_error_code(v: &Value) -> bool {
    match v.get("code").and_then(Value::as_str) {
        Some(c) => c != "000000" && c != "090001",
        None => false,
    }
}

/// The account identifier for JWT/SSO: the host label without domain.
pub fn account(conn: &DbConnection) -> &str {
    let host = conn.host.trim();
    host.split('.').next().unwrap_or(host)
}

/// Resolve the bearer token and the Snowflake token-type header for the auth
/// mode.
fn resolve_bearer(
    conn: &DbConnection,
    account: &str,
    secret: Option<&str>,
) -> Result<(String, &'static str)> {
    match &conn.auth {
        DbAuth::KeyPairJwt { private_key_path } => {
            let pem = std::fs::read(private_key_path)
                .with_context(|| format!("reading Snowflake key at {private_key_path}"))?;
            let jwt = super::auth::snowflake_jwt(account, &conn.username, &pem, secret)?;
            Ok((jwt, "KEYPAIR_JWT"))
        }
        DbAuth::OAuthClientCredentials {
            client_id,
            token_url,
        } => {
            let url = token_url
                .as_deref()
                .filter(|u| !u.is_empty())
                .context("Snowflake OAuth needs a token_url")?;
            let secret = secret.context("Snowflake OAuth needs a client secret")?;
            let tok = super::auth::oauth_client_credentials_token(url, client_id, secret, None)?;
            Ok((tok.access_token, "OAUTH"))
        }
        // External browser: a token cached by a sign-in (Settings, the
        // sidebar, an earlier connect) wins; otherwise open the browser now
        // (this runs on a worker thread) and cache the result.
        DbAuth::OAuthBrowser => {
            if let Some(t) = super::auth::cached_browser_token(&conn.id) {
                return Ok((t.access_token, "OAUTH"));
            }
            let tok = super::auth::snowflake_sso_token(account, &conn.username, open_browser)?;
            super::auth::cache_browser_token(&conn.id, tok.clone());
            Ok((tok.access_token, "OAUTH"))
        }
        other => bail!(
            "Snowflake needs key-pair (JWT), OAuth, or external-browser auth for \
             the SQL API; got {:?}",
            other.kind()
        ),
    }
}

/// Best-effort OS browser open for the SSO flow.
fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let (bin, args): (&str, &[&str]) = ("xdg-open", &[]);
    #[cfg(target_os = "macos")]
    let (bin, args): (&str, &[&str]) = ("open", &[]);
    #[cfg(target_os = "windows")]
    let (bin, args): (&str, &[&str]) = ("cmd", &["/C", "start", ""]);
    let _ = std::process::Command::new(bin).args(args).arg(url).spawn();
}

/// Map a Snowflake column type name to an Arrow type-name string.
fn sf_type_to_arrow(ty: &str, scale: Option<i64>) -> &'static str {
    match ty.to_ascii_lowercase().as_str() {
        "fixed" => {
            if scale.unwrap_or(0) == 0 {
                "Int64"
            } else {
                "Float64"
            }
        }
        "real" | "float" | "double" => "Float64",
        "boolean" => "Boolean",
        "date" => "Date32",
        t if t.starts_with("timestamp") => "Timestamp(Microsecond, None)",
        "time" => "Time64(ns)",
        _ => "Utf8", // text, variant, object, array, binary, ...
    }
}

/// Parse a Snowflake SQL API result (`resultSetMetaData.rowType` + `data`) into
/// a [`DataTable`]. Snowflake returns every cell as a JSON string (or null), so
/// each is re-parsed by its column type.
pub(crate) fn parse_sf_result(v: &Value) -> Result<DataTable> {
    let row_type = v["resultSetMetaData"]["rowType"]
        .as_array()
        .context("Snowflake result missing resultSetMetaData.rowType")?;
    let columns: Vec<ColumnInfo> = row_type
        .iter()
        .map(|c| ColumnInfo {
            name: c["name"].as_str().unwrap_or("").to_string(),
            data_type: sf_type_to_arrow(c["type"].as_str().unwrap_or("text"), c["scale"].as_i64())
                .to_string(),
        })
        .collect();
    let mut table = DataTable::empty();
    append_sf_rows(&v["data"], &columns, &mut table.rows);
    table.columns = columns;
    Ok(table)
}

/// Decode one partition's `data` block (an array of row arrays) onto `out`.
/// Partition 0 arrives inside the statement response, the rest as their own
/// GETs, and both are this same shape.
fn append_sf_rows(v: &Value, columns: &[ColumnInfo], out: &mut Vec<Vec<CellValue>>) {
    let Some(rows) = v.as_array() else {
        return;
    };
    out.reserve(rows.len());
    for row in rows {
        out.push(
            columns
                .iter()
                .enumerate()
                .map(|(i, col)| sf_cell(row.get(i).unwrap_or(&Value::Null), &col.data_type))
                .collect(),
        );
    }
}

/// The SQL API sends a DATE as days since the epoch (`"18262"`) and every
/// TIMESTAMP as seconds since it with nine decimals, a TIMESTAMP_TZ followed by
/// its offset (`"1616173619.000000000 1500"`). Those numbers used to land in
/// Date / DateTime cells verbatim. Shown in UTC, like the other connectors'
/// zoned types; anything else goes through `string_cell` unchanged.
fn sf_cell(v: &Value, arrow_type: &str) -> CellValue {
    let decoded = v.as_str().and_then(|s| match arrow_type {
        "Date32" => {
            let days = i32::try_from(s.parse::<i64>().ok()?).ok()?;
            chrono::NaiveDate::from_num_days_from_ce_opt(days.checked_add(719_163)?)
                .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string()))
        }
        "Timestamp(Microsecond, None)" => sf_epoch(s.split(' ').next()?)
            .map(|dt| CellValue::DateTime(dt.format("%Y-%m-%d %H:%M:%S%.f").to_string())),
        // Seconds since midnight, nine decimals.
        "Time64(ns)" => i64::try_from(sf_nanos(s)?)
            .ok()
            .filter(|n| (0..crate::data::times::NANOS_PER_DAY).contains(n))
            .map(CellValue::Time),
        _ => None,
    });
    decoded.unwrap_or_else(|| string_cell(v, arrow_type))
}

/// `"-1.500000000"` as whole nanoseconds, exactly.
fn sf_nanos(s: &str) -> Option<i128> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let frac: i64 = format!("{frac:0<9}").get(..9)?.parse().ok()?;
    Some(
        i128::from(whole.parse::<i64>().ok()?) * 1_000_000_000
            + if s.starts_with('-') {
                -i128::from(frac)
            } else {
                i128::from(frac)
            },
    )
}

/// `"-1.500000000"` read exactly, digit by digit (an f64 would blur the
/// nanoseconds): the fraction carries the sign of the whole part.
fn sf_epoch(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let nanos = sf_nanos(s)?;
    chrono::DateTime::from_timestamp(
        i64::try_from(nanos.div_euclid(1_000_000_000)).ok()?,
        nanos.rem_euclid(1_000_000_000) as u32,
    )
}

/// Plain text of a cell (for the `SHOW ... name` extraction).
fn cell_text(c: &CellValue) -> String {
    match c {
        CellValue::String(s)
        | CellValue::Date(s)
        | CellValue::DateTime(s)
        | CellValue::Nested(s) => s.clone(),
        CellValue::Int(i) => i.to_string(),
        CellValue::Float(f) => f.to_string(),
        CellValue::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
#[path = "snowflake_tests.rs"]
mod tests;
