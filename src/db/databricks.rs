//! Databricks connector over the Statement Execution API
//! (`/api/2.0/sql/statements`). Like the Snowflake connector: a small REST
//! client that submits a statement, polls while it runs, and maps the JSON
//! result into a [`DataTable`]. Bearer auth is a PAT, an Azure AD token, or an
//! OAuth M2M token.
//!
//! The connection's `database` field carries the **SQL warehouse id** (the
//! Statement API targets a warehouse, for which the connection model has no
//! dedicated field).
//!
//! **Two dispositions.** `INLINE` returns the rows in the statement response
//! but caps the whole result at 25 MiB, which any real table exceeds; that cap
//! is a hard `BAD_REQUEST`, not a truncation. Row-returning queries therefore
//! ask for `EXTERNAL_LINKS`, where the response carries presigned chunk URLs
//! instead and [`DatabricksConnector::read_external_rows`] downloads them. The
//! `SHOW ...` listings keep `INLINE`: they are tiny, the sidebar runs them
//! constantly, and external links would cost an extra round trip each. Both
//! dispositions deliver the same `JSON_ARRAY` shape, so one decoder
//! ([`append_dbx_rows`]) serves both.
//!
//! Live-only: the parser ([`parse_dbx_result`]) is unit-tested; the HTTP flow
//! is covered by the env-gated live test.

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::rest::{
    InFlight, POLL_DELAY, RestClient, databricks_cancel_path, poll, poll_tries, string_cell,
};
use super::{CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport};

/// How long the submit request itself blocks before Databricks answers with
/// a still-running statement. The API caps this at 50s and rejects 0; 30s
/// keeps a fast query to a single round trip.
const DBX_WAIT: &str = "30s";
const DBX_WAIT_SECS: u32 = 30;

pub struct DatabricksConnector {
    client: RestClient,
    bearer: String,
    warehouse_id: String,
    conn_label: String,
    /// Seconds to keep asking the warehouse whether the statement is done.
    timeout_secs: u32,
    cancel: CancelFlag,
    in_flight: InFlight,
}

impl DatabricksConnector {
    pub fn connect(conn: &DbConnection, secret: Option<&str>) -> Result<Self> {
        let host = conn.host.trim().trim_end_matches('/');
        let base = if host.starts_with("http") {
            host.to_string()
        } else {
            format!("https://{host}")
        };
        let warehouse_id = conn.database.trim().to_string();
        if warehouse_id.is_empty() {
            bail!(
                "Databricks needs a SQL warehouse id; put it in the connection's \
                 Database field"
            );
        }
        let bearer = resolve_bearer(conn, secret)?;
        Ok(Self {
            client: RestClient::new(base).with_header("Accept", "application/json"),
            bearer,
            warehouse_id,
            conn_label: conn.name.clone(),
            timeout_secs: conn.query_timeout_secs,
            cancel: CancelFlag::new(),
            in_flight: InFlight::default(),
        })
    }

    /// Submit a statement and return the SUCCEEDED response JSON, polling while
    /// the warehouse runs it. `disposition` is `"INLINE"` or
    /// `"EXTERNAL_LINKS"` (see the module docs for which goes where).
    fn submit(&self, sql: &str, disposition: &str) -> Result<Value> {
        self.in_flight.clear();
        let body = serde_json::json!({
            "statement": sql,
            "warehouse_id": self.warehouse_id,
            "wait_timeout": DBX_WAIT,
            "on_wait_timeout": "CONTINUE",
            "disposition": disposition,
            "format": "JSON_ARRAY",
        });
        let first = self
            .client
            .post_json("api/2.0/sql/statements", &self.bearer, &body)
            .with_context(|| format!("submitting statement on '{}'", self.conn_label))?;
        if dbx_state(&first) == "SUCCEEDED" {
            return Ok(first);
        }
        let id = first["statement_id"]
            .as_str()
            .context("Databricks did not return a statement_id")?
            .to_string();
        self.in_flight.set(&id);
        let path = format!("api/2.0/sql/statements/{id}");
        let cancel = self.cancel.clone();
        let result = poll(
            || self.client.get_json(&path, &self.bearer),
            |v| dbx_state(v) == "SUCCEEDED",
            |v| matches!(dbx_state(v), "FAILED" | "CANCELED" | "CLOSED"),
            move || cancel.is_cancelled(),
            // The POST already held the request open for DBX_WAIT, so only
            // what is left of the connection's budget is polled for.
            poll_tries(self.timeout_secs, DBX_WAIT_SECS),
            POLL_DELAY,
        );
        // Clear on every exit path (success and error alike), so a stale
        // statement id is never cancelled later.
        self.in_flight.clear();
        result
    }

    /// Run a `SHOW ...` and pull the values of the first column matching one of
    /// `candidates` (case-insensitive), else the last column.
    fn show_column(&self, sql: &str, candidates: &[&str]) -> Result<Vec<String>> {
        let t = parse_dbx_result(&self.submit(sql, "INLINE")?)?;
        if t.columns.is_empty() {
            return Ok(Vec::new());
        }
        let col = t
            .columns
            .iter()
            .position(|c| candidates.iter().any(|w| c.name.eq_ignore_ascii_case(w)))
            .unwrap_or(t.columns.len() - 1);
        Ok(t.rows
            .iter()
            .filter_map(|r| r.get(col))
            .map(cell_text)
            .collect())
    }

    /// Download an `EXTERNAL_LINKS` result's chunks onto `out`, stopping at
    /// `cap` rows.
    ///
    /// Each `external_link` is a **presigned** cloud-storage URL: it carries
    /// its own credentials, and Databricks rejects a request that also sets an
    /// `Authorization` header. `get_with` is the one client method that sends
    /// no bearer, which is why it is used here. `next_chunk_internal_link` is
    /// the opposite case - a workspace API path that does need the bearer.
    fn read_external_rows(
        &self,
        first: &Value,
        columns: &[ColumnInfo],
        cap: usize,
        out: &mut Vec<Vec<CellValue>>,
    ) -> Result<()> {
        // The walk is bounded, not `while there is a next link`: a server
        // that pointed back at a chunk already fetched would otherwise spin
        // this worker thread forever, and it has no cancel of its own between
        // statements. `total_chunk_count` is the real answer; the fallback is
        // far past any result the row cap allows, so it can never cut a
        // well-formed response short.
        let max_batches = first
            .pointer("/manifest/total_chunk_count")
            .and_then(Value::as_u64)
            .unwrap_or(100_000)
            .max(1) as usize;
        let mut links = dbx_external_links(first);
        for _ in 0..max_batches {
            if links.is_empty() {
                return Ok(());
            }
            for link in &links {
                if out.len() >= cap {
                    return Ok(());
                }
                if self.cancel.is_cancelled() {
                    bail!("statement cancelled");
                }
                let body = self.client.get_with(&link.url, &[]).with_context(|| {
                    format!(
                        "downloading result chunk {} on '{}'",
                        link.index, self.conn_label
                    )
                })?;
                append_dbx_rows(&body, columns, out);
            }
            // The last link of a batch is the one that names what follows it.
            let Some(path) = links.last().and_then(|l| l.next_path.clone()) else {
                return Ok(());
            };
            if out.len() >= cap {
                return Ok(());
            }
            let v = self.client.get_json(&path, &self.bearer).with_context(|| {
                format!("fetching the next result chunk on '{}'", self.conn_label)
            })?;
            links = dbx_external_links(&v);
        }
        Ok(())
    }
}

impl DbConnector for DatabricksConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Databricks
    }

    fn list_catalogs(&mut self) -> Result<Vec<String>> {
        self.show_column("SHOW CATALOGS", &["catalog", "catalog_name"])
    }

    fn list_schemas(&mut self, catalog: Option<&str>) -> Result<Vec<String>> {
        let sql = match catalog {
            Some(c) => format!("SHOW SCHEMAS IN `{}`", c.replace('`', "``")),
            None => "SHOW SCHEMAS".to_string(),
        };
        self.show_column(&sql, &["databaseName", "schema_name", "namespace"])
    }

    fn list_tables(&mut self, catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let sch = schema.replace('`', "``");
        let sql = match catalog {
            Some(c) => format!("SHOW TABLES IN `{}`.`{sch}`", c.replace('`', "``")),
            None => format!("SHOW TABLES IN `{sch}`"),
        };
        self.show_column(&sql, &["tableName", "table_name", "name"])
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        self.cancel.reset();
        let cap = crate::formats::initial_load_rows();
        let v = self.submit(sql, "EXTERNAL_LINKS")?;
        // `parse_dbx_result` takes the columns off the manifest and finds no
        // inline `data_array`; the rows arrive from the chunk links instead.
        let mut t = parse_dbx_result(&v)?;
        let columns = t.columns.clone();
        self.read_external_rows(&v, &columns, cap, &mut t.rows)?;
        if t.rows.len() > cap {
            t.rows.truncate(cap);
        }
        Ok(t)
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        self.cancel.reset();
        // The Statement API runs one statement per call (no shared transaction),
        // so swallow BEGIN/COMMIT/ROLLBACK from the shared writer (non-atomic).
        let upper = sql.trim_start();
        let head = upper[..upper.len().min(9)].to_ascii_uppercase();
        if head.starts_with("BEGIN") || head.starts_with("COMMIT") || head.starts_with("ROLLBACK") {
            return Ok(0);
        }
        // No rows come back from DDL/DML, so the cheap disposition is right.
        self.submit(sql, "INLINE")?;
        Ok(0)
    }

    fn write_table(
        &mut self,
        catalog: Option<&str>,
        schema: &str,
        table: &str,
        mode: DbWriteMode,
        data: &DataTable,
    ) -> Result<DbWriteReport> {
        // ponytail: literal-INSERT writer; non-atomic on the stateless API.
        // Standard Spark SQL DDL/DML (backtick idents), so all modes work.
        super::write_table_generic(
            self,
            DbEngine::Databricks,
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
            let Some(id) = in_flight.get() else {
                return;
            };
            let path = databricks_cancel_path(&id);
            // Best effort: the statement may already have finished, and a
            // failed cancel must not surface as a query error.
            let _ = client.delete_json(&path, &bearer);
        }))
    }
}

/// The statement `status.state` string (`""` when absent).
fn dbx_state(v: &Value) -> &str {
    v["status"]["state"].as_str().unwrap_or("")
}

/// Resolve the bearer token for the connection's auth mode.
fn resolve_bearer(conn: &DbConnection, secret: Option<&str>) -> Result<String> {
    match &conn.auth {
        // Personal access token: the stored secret is the bearer.
        DbAuth::Token => secret
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .context("Databricks personal access token (secret) is not set"),
        // Azure AD token via the az CLI (Databricks resource).
        DbAuth::AzureAd => super::auth::resolve_password(conn, secret),
        DbAuth::OAuthClientCredentials {
            client_id,
            token_url,
        } => {
            let url = token_url
                .as_deref()
                .filter(|u| !u.is_empty())
                .context("Databricks OAuth needs a token_url")?;
            let secret = secret.context("Databricks OAuth needs a client secret")?;
            let tok = super::auth::oauth_client_credentials_token(url, client_id, secret, None)?;
            Ok(tok.access_token)
        }
        // User-to-machine browser OAuth. A token cached by the Settings
        // sign-in flow wins; otherwise open the browser now (this runs on a
        // worker thread) and cache the result.
        DbAuth::OAuthBrowser => {
            if let Some(t) = super::auth::cached_browser_token(&conn.id) {
                return Ok(t.access_token);
            }
            let cfg = super::auth::browser_oauth_config(conn, None)
                .context("Databricks browser sign-in needs a workspace host")?;
            let tok = crate::auth::oauth_browser::acquire_token(
                &cfg,
                crate::auth::oauth_browser::open_url_in_browser,
            )?;
            super::auth::cache_browser_token(&conn.id, tok.clone());
            Ok(tok.access_token)
        }
        other => bail!(
            "Databricks needs a personal access token, Azure AD, or OAuth M2M; \
             got {:?}",
            other.kind()
        ),
    }
}

/// Map a Databricks (Spark SQL) type name to an Arrow type-name string.
fn dbx_type_to_arrow(ty: &str) -> &'static str {
    // type_name is the base type (e.g. "DECIMAL"); parameters live elsewhere.
    match ty.to_ascii_uppercase().as_str() {
        "INT" | "INTEGER" | "BIGINT" | "LONG" | "SMALLINT" | "SHORT" | "TINYINT" | "BYTE" => {
            "Int64"
        }
        "DOUBLE" | "FLOAT" | "REAL" | "DECIMAL" => "Float64",
        "BOOLEAN" => "Boolean",
        "DATE" => "Date32",
        t if t.starts_with("TIMESTAMP") => "Timestamp(Microsecond, None)",
        // INTERVAL YEAR TO MONTH has no fixed length and stays text.
        t if t.starts_with("INTERVAL") && !t.contains("YEAR") && !t.contains("MONTH") => {
            "Duration(µs)"
        }
        _ => "Utf8", // STRING, BINARY, ARRAY, MAP, STRUCT, VARIANT, ...
    }
}

/// Parse a Databricks Statement result (`manifest.schema.columns` +
/// `result.data_array`) into a [`DataTable`]. Cells arrive as JSON strings.
///
/// An `EXTERNAL_LINKS` response has no `data_array`, so this yields the
/// columns and no rows; [`DatabricksConnector::read_external_rows`] fills them
/// in from the chunk downloads.
pub(crate) fn parse_dbx_result(v: &Value) -> Result<DataTable> {
    let cols = v["manifest"]["schema"]["columns"]
        .as_array()
        .context("Databricks result missing manifest.schema.columns")?;
    let columns: Vec<ColumnInfo> = cols
        .iter()
        .map(|c| ColumnInfo {
            name: c["name"].as_str().unwrap_or("").to_string(),
            // `type_name` is just "INTERVAL"; the qualifier is in `type_text`.
            data_type: dbx_type_to_arrow(match c["type_name"].as_str() {
                Some("INTERVAL") => c["type_text"].as_str().unwrap_or("INTERVAL YEAR"),
                name => name.unwrap_or("STRING"),
            })
            .to_string(),
        })
        .collect();
    let mut table = DataTable::empty();
    append_dbx_rows(&v["result"]["data_array"], &columns, &mut table.rows);
    table.columns = columns;
    Ok(table)
}

/// A day-time interval as the API prints it, the SQL literal itself:
/// `INTERVAL '1 02:03:04' DAY TO SECOND`, `INTERVAL '-90' MINUTE`. The
/// qualifier says which unit each number is. `None` for anything else.
fn dbx_interval(s: &str) -> Option<CellValue> {
    const UNITS: [(&str, i64); 4] = [
        ("DAY", 86_400),
        ("HOUR", 3_600),
        ("MINUTE", 60),
        ("SECOND", 1),
    ];
    let rest = s.trim().strip_prefix("INTERVAL '")?;
    let (value, qualifier) = rest.split_once('\'')?;
    let (first, last) = match qualifier.trim().split_once(" TO ") {
        Some((a, b)) => (a, b),
        None => (qualifier.trim(), qualifier.trim()),
    };
    let from = UNITS.iter().position(|(u, _)| *u == first)?;
    let to = UNITS.iter().position(|(u, _)| *u == last)?;
    let (negative, value) = match value.trim().strip_prefix('-') {
        Some(v) => (true, v),
        None => (false, value.trim()),
    };
    let fields: Vec<&str> = value.split([' ', ':']).collect();
    if to < from || fields.len() != to - from + 1 {
        return None;
    }
    let mut ns = 0i64;
    for (field, (_, secs)) in fields.iter().zip(&UNITS[from..=to]) {
        let (whole, frac) = field.split_once('.').unwrap_or((field, ""));
        let frac: i64 = format!("{frac:0<9}").get(..9)?.parse().ok()?;
        ns = ns
            .checked_add(
                whole
                    .parse::<i64>()
                    .ok()?
                    .checked_mul(secs * 1_000_000_000)?,
            )?
            .checked_add(frac * secs)?;
    }
    Some(CellValue::Duration(if negative { -ns } else { ns }))
}

/// Decode one `JSON_ARRAY` block (an array of row arrays) onto `out`. Inline
/// results carry it at `result.data_array`; an external chunk download *is*
/// one, as the whole response body. Anything else decodes to no rows.
fn append_dbx_rows(v: &Value, columns: &[ColumnInfo], out: &mut Vec<Vec<CellValue>>) {
    let Some(rows) = v.as_array() else {
        return;
    };
    out.reserve(rows.len());
    for row in rows {
        out.push(
            columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    let v = row.get(i).unwrap_or(&Value::Null);
                    v.as_str()
                        .filter(|_| col.data_type.starts_with("Duration"))
                        .and_then(dbx_interval)
                        .unwrap_or_else(|| string_cell(v, &col.data_type))
                })
                .collect(),
        );
    }
}

/// One chunk of an `EXTERNAL_LINKS` result.
#[derive(Debug, PartialEq, Eq)]
struct DbxLink {
    /// Chunk index, for error messages only.
    index: i64,
    /// The presigned download URL. Must be fetched WITHOUT a bearer token.
    url: String,
    /// Workspace API path yielding the link after this one, when there is one.
    next_path: Option<String>,
}

/// Pull the chunk links out of a Databricks response. The statement response
/// nests them under `result`; the response to a `next_chunk_internal_link`
/// GET carries them at the top level, so both spellings are accepted.
fn dbx_external_links(v: &Value) -> Vec<DbxLink> {
    let arr = v
        .pointer("/result/external_links")
        .or_else(|| v.get("external_links"))
        .and_then(Value::as_array);
    let Some(arr) = arr else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|l| {
            Some(DbxLink {
                index: l["chunk_index"].as_i64().unwrap_or(0),
                url: l["external_link"].as_str()?.to_string(),
                next_path: l["next_chunk_internal_link"]
                    .as_str()
                    .map(str::to_string)
                    .filter(|p| !p.is_empty()),
            })
        })
        .collect()
}

/// Plain text of a cell (for `SHOW` name extraction).
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

/// Why a `WITH ...` statement could write on a read-only Databricks
/// connection, or `None` when it is a query. Spark lets the CTE list lead into
/// `INSERT`, `MERGE`, `UPDATE` or `DELETE`, but never hides one inside a CTE,
/// so the statement is whatever follows the CTE list. That is read the way
/// Spark lexes: strings, backtick names and comments skipped, so a word in
/// `WHERE operation = 'MERGE'` no longer refuses a query.
///
/// Where Spark's reading could differ from this one, every write word in the
/// text counts, as before: a backslash (an escape in a string, but not in a
/// raw string, a backtick name or with `escapedStringLiterals` on) or a nested
/// block comment.
pub(crate) fn with_write_risk(sql: &str) -> Option<String> {
    const WRITES: [&str; 4] = ["INSERT", "UPDATE", "DELETE", "MERGE"];
    let blunt = || {
        sql.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| WRITES.contains(&w.to_ascii_uppercase().as_str()))
            .then(|| {
                "On Databricks a WITH clause can lead into INSERT, UPDATE, DELETE or MERGE, and \
                 with a backslash or a nested comment in the text those words are refused \
                 anywhere in it."
                    .to_string()
            })
    };
    // From the WITH on; a statement in brackets leaves a `)` over at the end,
    // which reads as unbalanced and so falls back to the blunt check.
    let Some(tokens) = super::sql_lex::code_only(
        crate::sql::statement_start(sql),
        super::sql_lex::Dialect::Spark,
    )
    .and_then(|code| spark_tokens(&code)) else {
        return blunt();
    };
    match with_main_verb(&tokens) {
        Some(Tok::Word(v)) if matches!(v.as_str(), "SELECT" | "VALUES" | "TABLE") => None,
        Some(Tok::Open { query: true }) => None,
        // `FROM src SELECT ...` reads, `FROM src INSERT INTO ...` writes.
        Some(Tok::Word(v)) if v == "FROM" => tokens
            .iter()
            .any(|t| matches!(t, Tok::Word(w) if WRITES.contains(&w.as_str())))
            .then(|| "On Databricks this WITH clause leads into a write.".to_string()),
        Some(Tok::Word(v)) => Some(format!(
            "On Databricks this WITH clause leads into {v}, not a query."
        )),
        _ => Some("On Databricks Octa cannot tell what this WITH clause leads into.".to_string()),
    }
}

/// A Spark SQL token, as far as [`with_write_risk`] needs one. Words are
/// upper-cased; a string is `Other`, a backtick name `Name`. An opening bracket says whether what it holds starts like a query
/// (`(SELECT ...`), which is what tells `name (a, b) AS (SELECT ...)` from
/// `name (SELECT ...)` followed by a query in brackets.
#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    Name,
    Open { query: bool },
    Close,
    Comma,
    Other,
}

/// Words a query in brackets starts with.
const QUERY_STARTS: [&str; 5] = ["SELECT", "WITH", "VALUES", "TABLE", "FROM"];

/// The tokens of `sql` as [`sql_lex::code_only`](super::sql_lex::code_only) leaves it
/// (every string and name an empty pair of quotes, no comment left), outside
/// parentheses only: what is inside a bracket is a CTE body, a
/// column list or a subquery, and only its edges matter. `None` for
/// unbalanced brackets.
fn spark_tokens(sql: &str) -> Option<Vec<Tok>> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    // Set by an outermost `(` until the first token inside it is seen.
    let mut first_inside = false;
    let mut push = |out: &mut Vec<Tok>, depth: usize, t: Tok| {
        if std::mem::take(&mut first_inside)
            && let Some(Tok::Open { query }) = out.last_mut()
        {
            *query = matches!(&t, Tok::Open { .. })
                || matches!(&t, Tok::Word(w) if QUERY_STARTS.contains(&w.as_str()));
        }
        if depth == 0 || matches!(t, Tok::Open { .. } | Tok::Close) && depth == 1 {
            first_inside = depth == 1 && matches!(t, Tok::Open { .. });
            out.push(t);
        }
    };
    while i < b.len() {
        let c = b[i];
        match c {
            // An emptied string or name: the quote and its partner.
            b'\'' | b'"' | b'`' => {
                push(
                    &mut out,
                    depth,
                    if c == b'`' { Tok::Name } else { Tok::Other },
                );
                i += 2;
            }
            b'(' => {
                depth += 1;
                push(&mut out, depth, Tok::Open { query: false });
                i += 1;
            }
            b')' => {
                push(&mut out, depth, Tok::Close);
                depth = depth.checked_sub(1)?;
                i += 1;
            }
            b',' => {
                push(&mut out, depth, Tok::Comma);
                i += 1;
            }
            _ if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 => {
                let len = sql[i..]
                    .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                    .unwrap_or(sql.len() - i);
                if len == 0 {
                    // A character that is no letter, such as a currency sign.
                    push(&mut out, depth, Tok::Other);
                    i += sql[i..].chars().next().map_or(1, char::len_utf8);
                } else {
                    push(
                        &mut out,
                        depth,
                        Tok::Word(sql[i..i + len].to_ascii_uppercase()),
                    );
                    i += len;
                }
            }
            _ if c.is_ascii_whitespace() || c == b';' => i += 1,
            _ => {
                push(&mut out, depth, Tok::Other);
                i += 1;
            }
        }
    }
    (depth == 0).then_some(out)
}

/// The token that opens the statement after the CTE list of `WITH [RECURSIVE]
/// name [(columns)] [AS] (query), ...`, or `None` when the text does not have
/// that shape.
fn with_main_verb(tokens: &[Tok]) -> Option<&Tok> {
    let mut t = tokens.iter().peekable();
    if t.next()? != &Tok::Word("WITH".into()) {
        return None;
    }
    if t.peek() == Some(&&Tok::Word("RECURSIVE".into())) {
        t.next();
    }
    loop {
        // The CTE's name.
        if !matches!(t.next()?, Tok::Word(_) | Tok::Name) {
            return None;
        }
        // `(columns)`, `AS` and `(query)`, in Spark's order, the first two
        // optional. At depth 0 a bracket is just its Open and Close.
        if matches!(t.peek(), Some(Tok::Open { query: false })) {
            t.next();
            t.next();
        }
        if matches!(t.peek(), Some(Tok::Word(w)) if w == "AS") {
            t.next();
        }
        if !matches!(t.next()?, Tok::Open { query: true }) || t.next()? != &Tok::Close {
            return None;
        }
        if t.peek() == Some(&&Tok::Comma) {
            t.next();
            continue;
        }
        return t.next();
    }
}

#[cfg(test)]
#[path = "databricks_tests.rs"]
mod tests;
