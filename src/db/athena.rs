//! Amazon Athena connector over the JSON API (`AmazonAthena.*` targets on
//! `athena.<region>.amazonaws.com`), signed with [`super::sigv4`].
//!
//! A query is three calls: `StartQueryExecution`, then `GetQueryExecution`
//! until the state settles, then `GetQueryResults` page by page. That is why
//! the signing is native rather than a shell-out to the `aws` CLI: three
//! process spawns per query would dominate the latency of a query that
//! Athena itself answers in a second.
//!
//! Two Athena facts shape the connection form. Every query needs a
//! **workgroup** (`primary` unless the account says otherwise), and unless
//! that workgroup enforces its own output location, a query also needs an
//! **S3 output location** to write its result to. Both are per-connection
//! fields; Athena refuses the query outright when neither the workgroup nor
//! the connection supplies a location, and its own message says so.
//!
//! Credentials come from the same chain as the rest of Octa's AWS support:
//! the IAM Identity Center role when the connection is configured for it,
//! then the environment, then whatever the `aws` CLI is configured with.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::data::{CellValue, ColumnInfo, DataTable};

use super::rest::{InFlight, POLL_DELAY, RestClient, poll, poll_tries};
use super::sigv4::{self, Credentials};
use super::{
    CancelFlag, DbAuth, DbConnection, DbConnector, DbEngine, DbWriteMode, DbWriteReport, auth,
};

/// Athena's default data catalogue. Federated catalogues exist, but the
/// sidebar stays two-level: a Glue database is what people browse.
const DEFAULT_CATALOG: &str = "AwsDataCatalog";

pub struct AthenaConnector {
    client: RestClient,
    creds: Credentials,
    region: String,
    host: String,
    database: String,
    workgroup: String,
    output_location: Option<String>,
    conn_label: String,
    /// Seconds to keep asking Athena whether the query execution is done.
    timeout_secs: u32,
    cancel: CancelFlag,
    in_flight: InFlight,
}

impl AthenaConnector {
    pub fn connect(conn: &DbConnection, _secret: Option<&str>) -> Result<Self> {
        let region = region_of(conn)
            .with_context(|| format!("connection '{}' needs an AWS region", conn.name))?;
        let host = if conn.host.trim().is_empty() {
            format!("athena.{region}.amazonaws.com")
        } else {
            conn.host.trim().trim_start_matches("https://").to_string()
        };
        let creds = credentials(conn)?;
        Ok(Self {
            client: RestClient::new(format!("https://{host}")),
            creds,
            region,
            host,
            database: conn.database.trim().to_string(),
            workgroup: conn
                .athena_workgroup
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("primary")
                .to_string(),
            output_location: conn
                .athena_output_location
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            conn_label: conn.name.clone(),
            timeout_secs: conn.query_timeout_secs,
            cancel: CancelFlag::new(),
            in_flight: InFlight::default(),
        })
    }

    /// One signed `AmazonAthena.<action>` call.
    fn call(&self, action: &str, body: &Value) -> Result<Value> {
        let payload = serde_json::to_vec(body)?;
        let target = format!("AmazonAthena.{action}");
        let extra = [("x-amz-target", target.clone())];
        let req = sigv4::Request {
            method: "POST",
            path: "/",
            host: &self.host,
            headers: &extra,
            payload: &payload,
        };
        let mut headers = sigv4::sign(
            &self.creds,
            &self.region,
            "athena",
            &req,
            &sigv4::amz_date_now(),
        );
        headers.push(("X-Amz-Target".to_string(), target));
        self.client
            .post_raw("/", &payload, "application/x-amz-json-1.1", &headers)
            .with_context(|| format!("{action} on '{}'", self.conn_label))
    }

    /// Start a query, wait for it to settle, and return its execution id.
    fn run_to_completion(&self, sql: &str) -> Result<String> {
        self.cancel.reset();
        let mut ctx = json!({ "Catalog": DEFAULT_CATALOG });
        if !self.database.is_empty() {
            ctx["Database"] = json!(self.database);
        }
        let mut body = json!({
            "QueryString": sql,
            "QueryExecutionContext": ctx,
            "WorkGroup": self.workgroup,
        });
        if let Some(loc) = &self.output_location {
            body["ResultConfiguration"] = json!({ "OutputLocation": loc });
        }
        let started = self.call("StartQueryExecution", &body)?;
        let id = started["QueryExecutionId"]
            .as_str()
            .context("Athena did not return a QueryExecutionId")?
            .to_string();
        self.in_flight.set(&id);

        let cancel = self.cancel.clone();
        let done = poll(
            || self.call("GetQueryExecution", &json!({ "QueryExecutionId": id })),
            |v| state_of(v) == "SUCCEEDED",
            |v| matches!(state_of(v), "FAILED" | "CANCELLED"),
            move || cancel.is_cancelled(),
            // StartQueryExecution returns immediately, so the whole budget is
            // available for polling. This used to be a fixed 600 attempts
            // (five minutes); a connection that needs that long now says so
            // in its own query timeout.
            poll_tries(self.timeout_secs, 0),
            POLL_DELAY,
        );
        self.in_flight.clear();
        match done {
            Ok(_) => Ok(id),
            Err(e) => {
                // The state-change reason is where Athena puts the actual
                // complaint (a missing output location, a syntax error, a
                // table that is not in the catalogue).
                let reason = self
                    .call("GetQueryExecution", &json!({ "QueryExecutionId": id }))
                    .ok()
                    .and_then(|v| {
                        v.pointer("/QueryExecution/Status/StateChangeReason")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                match reason {
                    Some(r) => bail!("{r}"),
                    None => Err(e),
                }
            }
        }
    }

    /// Read a finished query's rows, following `NextToken` up to `cap`.
    fn results(&self, id: &str, cap: usize) -> Result<DataTable> {
        let mut table = DataTable::empty();
        let mut token: Option<String> = None;
        // Athena repeats the column names as the first row of the first page
        // of a SELECT result; every later page is data only.
        let mut first_page = true;
        loop {
            let mut body = json!({ "QueryExecutionId": id, "MaxResults": 1000 });
            if let Some(t) = &token {
                body["NextToken"] = json!(t);
            }
            let page = self.call("GetQueryResults", &body)?;
            if table.columns.is_empty() {
                table.columns = page
                    .pointer("/ResultSet/ResultSetMetadata/ColumnInfo")
                    .and_then(Value::as_array)
                    .map(|cols| {
                        cols.iter()
                            .map(|c| ColumnInfo {
                                name: c["Name"].as_str().unwrap_or("").to_string(),
                                data_type: super::trino::trino_type_to_arrow(
                                    c["Type"].as_str().unwrap_or("varchar"),
                                )
                                .to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            let types: Vec<String> = table.columns.iter().map(|c| c.data_type.clone()).collect();
            if let Some(rows) = page.pointer("/ResultSet/Rows").and_then(Value::as_array) {
                for (i, row) in rows.iter().enumerate() {
                    if first_page && i == 0 && is_header_row(row, &table.columns) {
                        continue;
                    }
                    if table.rows.len() >= cap {
                        return Ok(table);
                    }
                    let data = row["Data"].as_array();
                    table.rows.push(
                        types
                            .iter()
                            .enumerate()
                            .map(|(c, ty)| {
                                athena_cell(data.and_then(|d| d.get(c)).unwrap_or(&Value::Null), ty)
                            })
                            .collect(),
                    );
                }
            }
            first_page = false;
            match page["NextToken"].as_str() {
                Some(t) => token = Some(t.to_string()),
                None => return Ok(table),
            }
        }
    }

    /// The first column of every row of `sql`, for catalogue listings that go
    /// through a query rather than a list API.
    fn names(&self, list: Value, key: &str) -> Vec<String> {
        list.as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i[key].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// `Status.State` of a `GetQueryExecution` response.
fn state_of(v: &Value) -> &str {
    v.pointer("/QueryExecution/Status/State")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// Whether this first row is Athena's repeat of the column names rather than
/// data. Compared against the header names, because a data row can legally
/// hold the same strings and only the first row of the first page is a
/// candidate.
fn is_header_row(row: &Value, columns: &[ColumnInfo]) -> bool {
    let Some(data) = row["Data"].as_array() else {
        return false;
    };
    data.len() == columns.len()
        && data
            .iter()
            .zip(columns)
            .all(|(d, c)| d["VarCharValue"].as_str() == Some(c.name.as_str()))
}

/// One Athena cell. Every value arrives as text in `VarCharValue`, and a
/// missing `VarCharValue` is NULL.
fn athena_cell(v: &Value, arrow_type: &str) -> CellValue {
    let Some(s) = v.get("VarCharValue").and_then(Value::as_str) else {
        return CellValue::Null;
    };
    match arrow_type {
        "Int64" => s
            .parse::<i64>()
            .map(CellValue::Int)
            .unwrap_or_else(|_| CellValue::String(s.to_string())),
        "Float64" => s
            .parse::<f64>()
            .map(CellValue::Float)
            .unwrap_or_else(|_| CellValue::String(s.to_string())),
        "Boolean" => CellValue::Bool(s.eq_ignore_ascii_case("true")),
        "Date32" => CellValue::Date(s.to_string()),
        "Timestamp(Microsecond, None)" => CellValue::DateTime(s.to_string()),
        _ => CellValue::String(s.to_string()),
    }
}

/// The region for this connection: the auth mode's own, else the one in the
/// endpoint host, else nothing.
fn region_of(conn: &DbConnection) -> Option<String> {
    if let DbAuth::AwsIam { region, .. } = &conn.auth
        && let Some(r) = region.as_deref().map(str::trim).filter(|r| !r.is_empty())
    {
        return Some(r.to_string());
    }
    // athena.eu-central-1.amazonaws.com -> eu-central-1
    conn.host
        .trim()
        .split('.')
        .nth(1)
        .filter(|r| r.contains('-'))
        .map(str::to_string)
}

/// Resolve signing credentials: the Identity Center role Octa already mints
/// when the connection is configured for it, then the environment, then the
/// `aws` CLI's own resolved credentials (which covers profiles, instance
/// roles and `aws sso login`).
fn credentials(conn: &DbConnection) -> Result<Credentials> {
    if auth::aws_sso_config(conn).is_some() {
        let token = auth::cached_browser_token(&conn.id).with_context(|| {
            format!(
                "sign in to AWS IAM Identity Center in Settings -> Databases for '{}', then retry",
                conn.name
            )
        })?;
        let c = auth::aws_role_credentials(conn, &token.access_token)?;
        return Ok(Credentials {
            access_key_id: c.access_key_id,
            secret_access_key: c.secret_access_key,
            session_token: c.session_token,
        });
    }
    if let (Ok(id), Ok(secret)) = (
        std::env::var("AWS_ACCESS_KEY_ID"),
        std::env::var("AWS_SECRET_ACCESS_KEY"),
    ) && !id.is_empty()
    {
        return Ok(Credentials {
            access_key_id: id,
            secret_access_key: secret,
            session_token: std::env::var("AWS_SESSION_TOKEN").unwrap_or_default(),
        });
    }
    cli_credentials()
}

/// `aws configure export-credentials`, which resolves whatever the CLI is
/// configured with (a profile, an SSO session, an instance role). One process
/// per connection, not per query.
fn cli_credentials() -> Result<Credentials> {
    let out = std::process::Command::new("aws")
        .args(["configure", "export-credentials", "--format", "process"])
        .output()
        .context(
            "no AWS credentials: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, or install \
             the aws CLI and run `aws sso login`",
        )?;
    if !out.status.success() {
        bail!(
            "the aws CLI could not provide credentials: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let v: Value = serde_json::from_slice(&out.stdout)
        .context("the aws CLI returned something that is not credentials JSON")?;
    let field = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    Ok(Credentials {
        access_key_id: field("AccessKeyId"),
        secret_access_key: field("SecretAccessKey"),
        session_token: field("SessionToken"),
    })
}

impl DbConnector for AthenaConnector {
    fn engine(&self) -> DbEngine {
        DbEngine::Athena
    }

    fn list_schemas(&mut self, _catalog: Option<&str>) -> Result<Vec<String>> {
        let v = self.call("ListDatabases", &json!({ "CatalogName": DEFAULT_CATALOG }))?;
        Ok(self.names(v["DatabaseList"].clone(), "Name"))
    }

    fn list_tables(&mut self, _catalog: Option<&str>, schema: &str) -> Result<Vec<String>> {
        let v = self.call(
            "ListTableMetadata",
            &json!({ "CatalogName": DEFAULT_CATALOG, "DatabaseName": schema }),
        )?;
        Ok(self.names(v["TableMetadataList"].clone(), "Name"))
    }

    fn query(&mut self, sql: &str) -> Result<DataTable> {
        let id = self.run_to_completion(sql)?;
        self.results(&id, crate::formats::initial_load_rows())
    }

    fn execute(&mut self, sql: &str) -> Result<u64> {
        // Athena has no client transactions: the shared write skeleton's
        // BEGIN / COMMIT are no-ops, as on the other query services.
        let head = sql.trim_start().to_ascii_uppercase();
        if head.starts_with("BEGIN") || head.starts_with("COMMIT") || head.starts_with("ROLLBACK") {
            return Ok(0);
        }
        self.run_to_completion(sql)?;
        // Athena reports no affected-row count, and inventing one would be a
        // number nobody could trust.
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
        super::reject_catalog(self.engine(), catalog)?;
        super::write_table_generic(self, DbEngine::Athena, None, schema, table, mode, data)
    }

    fn cancel_handle(&self) -> Option<Box<dyn Fn() + Send>> {
        let flag = self.cancel.clone();
        Some(Box::new(move || flag.cancel()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_come_out_of_varcharvalue() {
        assert_eq!(athena_cell(&json!({}), "Int64"), CellValue::Null);
        assert_eq!(
            athena_cell(&json!({"VarCharValue": "42"}), "Int64"),
            CellValue::Int(42)
        );
        assert_eq!(
            athena_cell(&json!({"VarCharValue": "1.5"}), "Float64"),
            CellValue::Float(1.5)
        );
        assert_eq!(
            athena_cell(&json!({"VarCharValue": "true"}), "Boolean"),
            CellValue::Bool(true)
        );
    }

    /// Athena repeats the column names as the first row of a SELECT result,
    /// and counting that as data would put a header row in every table.
    #[test]
    fn the_repeated_header_row_is_recognised() {
        let columns = vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
        ];
        let header = json!({"Data": [{"VarCharValue": "id"}, {"VarCharValue": "name"}]});
        let data = json!({"Data": [{"VarCharValue": "1"}, {"VarCharValue": "ada"}]});
        assert!(is_header_row(&header, &columns));
        assert!(!is_header_row(&data, &columns));
    }

    #[test]
    fn region_comes_from_auth_then_host() {
        let mut c = DbConnection {
            id: "x".into(),
            name: "a".into(),
            engine: DbEngine::Athena,
            host: "athena.eu-central-1.amazonaws.com".into(),
            port: 443,
            database: "default".into(),
            username: String::new(),
            auth: DbAuth::Password,
            allow_writes: false,
            oauth_client_id: None,
            oauth_tenant: None,
            athena_workgroup: None,
            athena_output_location: None,
            ssh: None,
            query_timeout_secs: super::super::DEFAULT_QUERY_TIMEOUT_SECS,
            tunnel_port: None,
        };
        assert_eq!(region_of(&c).as_deref(), Some("eu-central-1"));
        c.auth = DbAuth::AwsIam {
            region: Some("us-east-1".into()),
            sso_start_url: None,
            sso_region: None,
            sso_account_id: None,
            sso_role: None,
        };
        assert_eq!(region_of(&c).as_deref(), Some("us-east-1"));
    }
}
