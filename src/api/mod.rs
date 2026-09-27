//! REST / JSON endpoints as a data source.
//!
//! A saved [`ApiConnection`] is a base URL, an auth mode, a pointer at the
//! array in the response that holds the rows, and a pagination rule. Fetching
//! one produces a `DataTable` exactly as a database table does - so this
//! module deliberately does **not** go through `FormatRegistry`, whose every
//! method takes a `&Path`.
//!
//! ## Layout
//! - `mod`     - the connection model and its auth / pagination enums.
//! - `client`  - the fetch-and-paginate loop.
//! - `records` - finding the rows inside a response envelope.
//!
//! The model is shaped after [`octa::db::DbConnection`] rather than
//! `CloudConnection`, because what varies between two endpoints is mostly
//! their auth, and the database side already has the tagged-auth-enum plus
//! fieldless-mirror pattern that a settings form needs.

pub mod client;
pub mod records;

use serde::{Deserialize, Serialize};

/// How to authenticate against an endpoint.
///
/// The credential itself is never in here: it lives in the OS keyring under
/// `api.<id>.secret`, the same one-opaque-string-per-connection shape
/// `db_secrets` uses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ApiAuth {
    /// A public endpoint.
    #[default]
    None,
    /// `Authorization: Bearer <secret>`.
    Bearer,
    /// The secret in a header of your choosing (`X-API-Key`, ...).
    HeaderKey { name: String },
    /// The secret as a query parameter (`?api_key=...`).
    QueryKey { name: String },
    /// `Authorization: Basic base64(username:secret)`.
    Basic { username: String },
}

/// [`ApiAuth`] without its fields, for the settings combo box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiAuthKind {
    #[default]
    None,
    Bearer,
    HeaderKey,
    QueryKey,
    Basic,
}

impl ApiAuthKind {
    pub const ALL: &'static [ApiAuthKind] = &[
        ApiAuthKind::None,
        ApiAuthKind::Bearer,
        ApiAuthKind::HeaderKey,
        ApiAuthKind::QueryKey,
        ApiAuthKind::Basic,
    ];

    /// i18n key for the combo entry.
    pub fn i18n_key(self) -> &'static str {
        match self {
            ApiAuthKind::None => "api.auth_none",
            ApiAuthKind::Bearer => "api.auth_bearer",
            ApiAuthKind::HeaderKey => "api.auth_header_key",
            ApiAuthKind::QueryKey => "api.auth_query_key",
            ApiAuthKind::Basic => "api.auth_basic",
        }
    }

    /// Whether this mode needs a stored secret at all.
    pub fn needs_secret(self) -> bool {
        !matches!(self, ApiAuthKind::None)
    }

    pub fn of(auth: &ApiAuth) -> Self {
        match auth {
            ApiAuth::None => ApiAuthKind::None,
            ApiAuth::Bearer => ApiAuthKind::Bearer,
            ApiAuth::HeaderKey { .. } => ApiAuthKind::HeaderKey,
            ApiAuth::QueryKey { .. } => ApiAuthKind::QueryKey,
            ApiAuth::Basic { .. } => ApiAuthKind::Basic,
        }
    }
}

/// How to walk past the first page.
///
/// Four shapes cover most of what real APIs do. Anything stranger is a reason
/// to add a variant, not to invent a scripting language for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ApiPaging {
    /// One request, whatever comes back.
    #[default]
    None,
    /// `?page=1`, `?page=2`, ... stopping on the first empty page.
    PageNumber { param: String, start: u32 },
    /// `?offset=0&limit=100`, `?offset=100&limit=100`, ...
    OffsetLimit {
        offset_param: String,
        limit_param: String,
    },
    /// A cursor somewhere in the response body, fed back as a parameter.
    /// `pointer` is a JSON pointer such as `/meta/next_cursor`.
    Cursor { pointer: String, param: String },
    /// `Link: <...>; rel="next"`, the GitHub-style header.
    LinkHeader,
}

/// [`ApiPaging`] without its fields, for the settings combo box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiPagingKind {
    #[default]
    None,
    PageNumber,
    OffsetLimit,
    Cursor,
    LinkHeader,
}

impl ApiPagingKind {
    pub const ALL: &'static [ApiPagingKind] = &[
        ApiPagingKind::None,
        ApiPagingKind::PageNumber,
        ApiPagingKind::OffsetLimit,
        ApiPagingKind::Cursor,
        ApiPagingKind::LinkHeader,
    ];

    pub fn i18n_key(self) -> &'static str {
        match self {
            ApiPagingKind::None => "api.paging_none",
            ApiPagingKind::PageNumber => "api.paging_page",
            ApiPagingKind::OffsetLimit => "api.paging_offset",
            ApiPagingKind::Cursor => "api.paging_cursor",
            ApiPagingKind::LinkHeader => "api.paging_link",
        }
    }

    pub fn of(p: &ApiPaging) -> Self {
        match p {
            ApiPaging::None => ApiPagingKind::None,
            ApiPaging::PageNumber { .. } => ApiPagingKind::PageNumber,
            ApiPaging::OffsetLimit { .. } => ApiPagingKind::OffsetLimit,
            ApiPaging::Cursor { .. } => ApiPagingKind::Cursor,
            ApiPaging::LinkHeader => ApiPagingKind::LinkHeader,
        }
    }
}

fn default_timeout() -> u32 {
    30
}

/// A saved endpoint, persisted in `settings.toml` as `api_connections`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiConnection {
    /// Stable id, frozen once created: it names the keyring entry.
    pub id: String,
    /// What the user calls it. The CLI and MCP address it by this.
    pub name: String,
    /// Scheme and host, plus any common prefix (`https://api.example.com/v1`).
    pub base_url: String,
    /// Default path under the base URL (`/orders`). May be overridden per call.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub auth: ApiAuth,
    /// Extra headers sent with every request (`Accept`, a tenant id, ...).
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// JSON pointer to the array holding the rows (`/data/items`). Empty means
    /// "find it", which is what the JSON reader already does for files.
    #[serde(default)]
    pub records_pointer: String,
    #[serde(default)]
    pub paging: ApiPaging,
    /// Page size to ask for, where the pagination mode takes one.
    #[serde(default)]
    pub page_size: Option<u32>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,
}

impl Default for ApiConnection {
    fn default() -> Self {
        ApiConnection {
            id: String::new(),
            name: String::new(),
            base_url: String::new(),
            path: String::new(),
            auth: ApiAuth::None,
            headers: Vec::new(),
            records_pointer: String::new(),
            paging: ApiPaging::None,
            page_size: None,
            timeout_secs: default_timeout(),
        }
    }
}

impl ApiConnection {
    /// A fresh id. Frozen for the connection's life, because it names the
    /// keyring entry that holds its secret.
    pub fn fresh_id() -> String {
        format!(
            "api-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    }

    /// Full URL for `path`, or the connection's own path when `None`.
    ///
    /// The result is **always** under [`Self::base_url`]. A path that looks
    /// like an absolute URL is treated as a path anyway, so neither a CLI
    /// argument nor an agent's tool call can move the request to another host:
    /// the human who saved the connection chose the host, and that is the only
    /// place it is chosen. See [`same_origin`].
    pub fn url_for(&self, path: Option<&str>) -> String {
        let base = self.base_url.trim_end_matches('/');
        let p = path.unwrap_or(&self.path).trim();
        if p.is_empty() {
            return base.to_string();
        }
        format!("{base}/{}", p.trim_start_matches('/'))
    }

    /// A human label for the tab this connection opens.
    pub fn tab_label(&self, path: Option<&str>) -> String {
        let p = path.unwrap_or(&self.path);
        let p = p.trim().trim_start_matches('/');
        if p.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.name, p)
        }
    }
}

/// Scheme-and-authority of a URL, for [`same_origin`].
fn origin_of(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // Userinfo is not part of the origin, and comparing it would let
    // `https://evil@good.example` differ from `https://good.example`.
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    Some((scheme.to_ascii_lowercase(), hostport.to_ascii_lowercase()))
}

/// Whether `candidate` points at the same scheme and host:port as `base`.
///
/// Pagination follows URLs the *server* chose - a `Link: rel="next"` header,
/// or a cursor the response body carried. Those are data, so a hostile or
/// compromised endpoint could otherwise walk the fetch loop onto a host the
/// user never approved. The page loop refuses anything that leaves the origin.
pub fn same_origin(base: &str, candidate: &str) -> bool {
    match (origin_of(base), origin_of(candidate)) {
        (Some(a), Some(b)) => a == b,
        // A relative next-link never leaves the origin.
        (Some(_), None) => !candidate.contains("://"),
        _ => false,
    }
}

/// Look a saved connection up by name (case-insensitive) or id. The error
/// lists what exists, so a CLI user or an agent can self-correct.
pub fn find_connection(connections: &[ApiConnection], name: &str) -> anyhow::Result<ApiConnection> {
    if let Some(c) = connections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name) || c.id == name)
    {
        return Ok(c.clone());
    }
    let names: Vec<&str> = connections.iter().map(|c| c.name.as_str()).collect();
    if names.is_empty() {
        anyhow::bail!(
            "no saved API connection named '{name}' \
             (none exist - add one in Settings -> API endpoints)"
        );
    }
    anyhow::bail!(
        "no saved API connection named '{name}' (have: {})",
        names.join(", ")
    )
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
