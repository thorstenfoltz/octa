//! Hash columns on the server: the digest `data::transform::hash_columns`
//! computes, spelled in the engine's SQL, so a partly loaded database table
//! gets the column for every row, present and future. Each value is the
//! database's own text of the cell, so dates and decimals can hash
//! differently from the same row in a file; the docs say so.

use std::sync::atomic::AtomicBool;

use crate::data::transform::hash_columns::{HashColumnsAlgo, HashColumnsSpec};
use crate::db::{DbConnector, DbEngine};

use super::dialect::{limit_clause, str_lit, trim};
use super::{ServerSource, check_cancel};

/// One hash column, by column name, so it survives a column moving.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerHash {
    /// The new column's name.
    pub name: String,
    /// The hashed columns, in the order they are joined.
    pub columns: Vec<String>,
    pub algo: HashColumnsAlgo,
    pub delimiter: String,
    pub null_text: String,
    pub trim: bool,
    pub upper: bool,
}

impl ServerHash {
    /// `spec` as a server hash named `name`; `names` are the tab's columns,
    /// which `spec.columns` index into.
    pub fn of(spec: &HashColumnsSpec, names: &[String], name: &str) -> Self {
        Self {
            name: name.to_string(),
            columns: spec
                .columns
                .iter()
                .filter_map(|&c| names.get(c).cloned())
                .collect(),
            algo: spec.algo,
            delimiter: spec.delimiter.clone(),
            null_text: spec.null_text.clone(),
            trim: spec.trim,
            upper: spec.upper,
        }
    }
}

fn upper(e: DbEngine, t: &str) -> String {
    match e {
        // ClickHouse's `upper` is ASCII only.
        DbEngine::ClickHouse => format!("upperUTF8({t})"),
        _ => format!("UPPER({t})"),
    }
}

/// The text one row hashes, as `hash_columns::row_input` builds it: each
/// value as text, NULL as the NULL text, then trimmed, then upper-cased,
/// joined with the delimiter. `TRIM` strips spaces only, where the local
/// engine strips any whitespace. ponytail: Oracle and Exasol read `''` as
/// NULL, so an empty NULL text there relies on `||` skipping a NULL (Oracle
/// does); unverified on Exasol.
pub fn input_sql(e: DbEngine, h: &ServerHash) -> String {
    let delimiter = str_lit(e, &h.delimiter);
    let mut parts: Vec<String> = Vec::new();
    for (i, col) in h.columns.iter().enumerate() {
        if i > 0 && !h.delimiter.is_empty() {
            parts.push(delimiter.clone());
        }
        let mut part = format!(
            "COALESCE({}, {})",
            e.as_text(&e.quote_ident(col)),
            str_lit(e, &h.null_text)
        );
        if h.trim {
            part = trim(e, &part);
        }
        if h.upper {
            part = upper(e, &part);
        }
        parts.push(part);
    }
    match (e, parts.len()) {
        (_, 0) => str_lit(e, ""),
        (_, 1) => parts.remove(0),
        // MySQL's `||` is OR; SQL Server has no `||`.
        (DbEngine::MySql | DbEngine::Mssql, _) => format!("CONCAT({})", parts.join(", ")),
        _ => format!("({})", parts.join(" || ")),
    }
}

/// The lowercase hex digest of [`input_sql`], over its UTF-8 bytes.
pub fn hash_sql(e: DbEngine, h: &ServerHash) -> String {
    use HashColumnsAlgo::*;
    let x = input_sql(e, h);
    let (md, sha) = match h.algo {
        Md5 => (true, 0),
        Sha256 => (false, 256),
        Sha512 => (false, 512),
    };
    match e {
        DbEngine::Postgres => match md {
            true => format!("md5({x})"),
            false => format!("encode(sha{sha}(convert_to({x}, 'UTF8')), 'hex')"),
        },
        // DuckDB has md5 and sha256 but no sha512, so a SHA-512 hash column
        // fails with DuckDB's own message (documented). SQLite shares the arm
        // only to keep the match exhaustive: it never pushes down.
        DbEngine::DuckDb | DbEngine::Sqlite => match md {
            true => format!("md5({x})"),
            false => format!("sha{sha}({x})"),
        },
        DbEngine::Redshift | DbEngine::MySql | DbEngine::Snowflake | DbEngine::Databricks => {
            match md {
                true => format!("MD5({x})"),
                false => format!("SHA2({x}, {sha})"),
            }
        }
        // HASHBYTES hashes the bytes of the type it gets: VARCHAR under a
        // UTF-8 collation (SQL Server 2019 and later; older servers refuse).
        DbEngine::Mssql => {
            let algo = match md {
                true => "MD5".to_string(),
                false => format!("SHA2_{sha}"),
            };
            format!(
                "LOWER(CONVERT(VARCHAR(128), HASHBYTES('{algo}', \
                 CONVERT(VARCHAR(MAX), {x} COLLATE Latin1_General_100_CI_AS_SC_UTF8)), 2))"
            )
        }
        DbEngine::Oracle => {
            let algo = match md {
                true => "MD5".to_string(),
                false => format!("SHA{sha}"),
            };
            format!("LOWER(RAWTOHEX(STANDARD_HASH({x}, '{algo}')))")
        }
        DbEngine::BigQuery => match md {
            true => format!("TO_HEX(MD5({x}))"),
            false => format!("TO_HEX(SHA{sha}({x}))"),
        },
        DbEngine::ClickHouse => match md {
            true => format!("lower(hex(MD5({x})))"),
            false => format!("lower(hex(SHA{sha}({x})))"),
        },
        DbEngine::Exasol => match md {
            true => format!("HASH_MD5({x})"),
            false => format!("HASH_SHA{sha}({x})"),
        },
        DbEngine::Trino | DbEngine::Athena => match md {
            true => format!("lower(to_hex(md5(to_utf8({x}))))"),
            false => format!("lower(to_hex(sha{sha}(to_utf8({x}))))"),
        },
    }
}

/// The table with `hashes` added after its own columns. The table alias has
/// no `AS` (Oracle), and `octa_t.*` because Oracle refuses a bare `*` beside
/// other select items.
pub fn select_with(e: DbEngine, table_sql: &str, hashes: &[ServerHash]) -> String {
    let added: String = hashes
        .iter()
        .map(|h| format!(", {} AS {}", hash_sql(e, h), e.quote_ident(&h.name)))
        .collect();
    format!("SELECT octa_t.*{added} FROM {table_sql} octa_t")
}

/// The hashed text and the digest of the first `n` rows the database
/// returns from `src`, for the dialog's preview.
pub fn preview(
    c: &mut dyn DbConnector,
    src: &ServerSource,
    h: &ServerHash,
    n: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<(String, String)>> {
    let e = src.engine();
    // SQL Server's OFFSET ... FETCH needs an ORDER BY.
    let order = match e {
        DbEngine::Mssql => " ORDER BY (SELECT NULL)",
        _ => "",
    };
    check_cancel(cancel)?;
    let t = c.query(&format!(
        "SELECT {}, {} FROM {}{order}{}",
        input_sql(e, h),
        hash_sql(e, h),
        src.from_sql(),
        limit_clause(e, n)
    ))?;
    let text = |r: usize, col: usize| t.get(r, col).map(|v| v.to_string()).unwrap_or_default();
    Ok((0..t.row_count())
        .map(|r| (text(r, 0), text(r, 1)))
        .collect())
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
