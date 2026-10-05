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
mod tests {
    use super::super::test_support::{DuckConn, source};
    use super::*;
    use crate::data::transform::hash_columns::{hash_columns_row, row_input};
    use crate::data::{CellValue, ColumnInfo, DataTable};

    fn table() -> DataTable {
        let mut t = DataTable::empty();
        t.columns = ["a", "b"]
            .iter()
            .map(|n| ColumnInfo {
                name: (*n).into(),
                data_type: "Utf8".into(),
            })
            .collect();
        let cell = |s: Option<&str>| s.map_or(CellValue::Null, |s| CellValue::String(s.into()));
        t.rows = [
            (Some("x"), Some("y")),
            (Some("  Mixed Case "), None),
            (None, Some("tail ")),
            (Some(""), Some("o'quote")),
        ]
        .into_iter()
        .map(|(a, b)| vec![cell(a), cell(b)])
        .collect();
        t
    }

    fn spec(delimiter: &str, trim: bool, upper: bool) -> HashColumnsSpec {
        HashColumnsSpec {
            columns: vec![0, 1],
            algo: HashColumnsAlgo::Md5,
            delimiter: delimiter.into(),
            null_text: "-".into(),
            trim,
            upper,
        }
    }

    /// The Postgres spelling, run by DuckDB, hashes exactly what the local
    /// engine hashes, digest and text alike.
    #[test]
    fn the_database_hash_is_the_local_one() {
        let t = table();
        let names = vec!["a".to_string(), "b".to_string()];
        for (d, tr, up) in [
            ("|", false, false),
            ("", false, false),
            ("|", true, false),
            (";", true, true),
        ] {
            let s = spec(d, tr, up);
            let h = ServerHash::of(&s, &names, "h");
            let mut c = DuckConn::new(t.clone());
            let got = preview(&mut c, &source(), &h, 10, &AtomicBool::new(false)).unwrap();
            let want: Vec<(String, String)> = (0..t.row_count())
                .map(|r| (row_input(&t, r, &s), hash_columns_row(&t, r, &s)))
                .collect();
            assert_eq!(got, want, "delimiter {d:?} trim {tr} upper {up}");
        }
    }

    #[test]
    fn one_column_needs_no_concatenation() {
        let h = ServerHash::of(&spec("|", false, false), &["a".into()], "h");
        let h = ServerHash {
            columns: vec!["a".into()],
            ..h
        };
        assert_eq!(
            input_sql(DbEngine::Postgres, &h),
            "COALESCE(CAST(\"a\" AS VARCHAR), '-')"
        );
    }

    #[test]
    fn every_engine_spells_its_hash() {
        use DbEngine::*;
        let names = vec!["a".to_string(), "b".to_string()];
        let of = |algo| ServerHash {
            algo,
            ..ServerHash::of(&spec("|", false, false), &names, "h")
        };
        let (md5, s256, s512) = (
            of(HashColumnsAlgo::Md5),
            of(HashColumnsAlgo::Sha256),
            of(HashColumnsAlgo::Sha512),
        );
        let cases: [(DbEngine, [&str; 3]); 12] = [
            (
                Postgres,
                ["md5(", "encode(sha256(convert_to(", "sha512(convert_to("],
            ),
            (Redshift, ["MD5(", "SHA2(", ", 512)"]),
            (MySql, ["MD5(CONCAT(", "SHA2(CONCAT(", ", 512)"]),
            (Snowflake, ["MD5(", "SHA2(", ", 512)"]),
            (Databricks, ["MD5(", "SHA2(", ", 512)"]),
            (Mssql, ["HASHBYTES('MD5'", "'SHA2_256'", "'SHA2_512'"]),
            (Oracle, ["STANDARD_HASH(", "'SHA256'", "'SHA512'"]),
            (
                BigQuery,
                ["TO_HEX(MD5(", "TO_HEX(SHA256(", "TO_HEX(SHA512("],
            ),
            (ClickHouse, ["lower(hex(MD5(", "SHA256(", "SHA512("]),
            (Exasol, ["HASH_MD5(", "HASH_SHA256(", "HASH_SHA512("]),
            (
                Trino,
                ["md5(to_utf8(", "sha256(to_utf8(", "sha512(to_utf8("],
            ),
            (
                Athena,
                ["md5(to_utf8(", "sha256(to_utf8(", "sha512(to_utf8("],
            ),
        ];
        for (e, want) in cases {
            for (h, w) in [&md5, &s256, &s512].into_iter().zip(want) {
                let got = hash_sql(e, h);
                assert!(got.contains(w), "{e:?}: {got}");
            }
        }
        let mssql = hash_sql(Mssql, &md5);
        assert!(
            mssql.contains("CONCAT(") && mssql.contains("_UTF8"),
            "{mssql}"
        );
        assert!(!hash_sql(Postgres, &md5).contains("CONCAT("));
    }

    fn hashed() -> (DataTable, ServerHash, HashColumnsSpec) {
        let t = table();
        let s = spec("|", true, false);
        let h = ServerHash::of(&s, &["a".into(), "b".into()], "h");
        (t, h, s)
    }

    /// A page of a view with a hash column: the column comes back under its
    /// name, and a filter on it runs on the database.
    #[test]
    fn a_page_filters_on_the_hash_column() {
        use super::super::view::{ServerView, ViewFilter, page_sql};
        let (t, h, s) = hashed();
        let want = hash_columns_row(&t, 1, &s);
        let view = ServerView {
            filters: vec![ViewFilter::values("h", [want.clone()])],
            derived: vec![h],
            ..Default::default()
        };
        let e = DbEngine::Postgres;
        let sql = page_sql(e, &view, &view.from_item(e, "\"data\""), &[], 10, 0);
        let got = DuckConn::new(t).query(&sql).unwrap();
        let col = got.columns.iter().position(|c| c.name == "h").unwrap();
        assert_eq!(got.row_count(), 1, "{sql}");
        assert_eq!(got.get(0, col).unwrap().to_string(), want);
    }

    /// Every analysis reads `from_sql()`: with hash columns, and with a
    /// filter beside them, the hash is there to count.
    #[test]
    fn analyses_read_the_table_with_its_hashes() {
        let (t, h, s) = hashed();
        let mut src = source();
        src.derived = vec![h];
        assert!(!src.is_plain());
        let distinct = |src: &ServerSource| {
            let sql = format!("SELECT COUNT(DISTINCT \"h\") FROM {}", src.from_sql());
            let out = DuckConn::new(t.clone()).query(&sql).unwrap();
            out.get(0, 0).unwrap().to_string()
        };
        assert_eq!(distinct(&src), t.row_count().to_string());
        src.filter = Some(format!("\"h\" = '{}'", hash_columns_row(&t, 0, &s)));
        assert_eq!(distinct(&src), "1");
    }

    #[test]
    fn a_hashed_table_samples_exactly() {
        let mut src = source();
        src.derived = vec![hashed().1];
        assert_eq!(super::super::sample::fast_sql(&src, 10, 1_000_000), None);
        assert!(super::super::sample::exact_sql(&src, 10).contains("octa_t.*"));
    }

    #[test]
    fn oracle_aliases_the_table_without_as() {
        let h = ServerHash::of(&spec("|", false, false), &["a".into(), "b".into()], "h");
        let sql = select_with(DbEngine::Oracle, "\"T\"", &[h]);
        assert!(sql.starts_with("SELECT octa_t.*, "), "{sql}");
        assert!(sql.ends_with(" AS \"h\" FROM \"T\" octa_t"), "{sql}");
    }
}
