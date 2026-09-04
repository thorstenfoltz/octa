//! SQL dumps (`mysqldump`, `pg_dump`, `sqlite3 .dump`) opened as their tables.
//!
//! **Never reached by extension.** A `.sql` file still opens as text, which is
//! what it is most of the time. This reader is reached only by name, through
//! **File -> Open as -> SQL dump** and **View -> Reopen as -> SQL dump**, so
//! turning a dump into tables is something you ask for rather than something
//! that happens to you.
//!
//! **No SQL parser was written.** The statements are split apart, the
//! dialect-only spellings are scrubbed off, and the result is replayed into an
//! in-memory SQLite database, which is then read exactly the way a `.sqlite`
//! file is. SQLite is already in the binary and already understands the
//! ninety-five per cent of dump syntax that is just SQL.
//!
//! What that buys, and what it costs:
//!
//! - **A statement that will not replay is skipped, not fatal.** Dumps are
//!   full of statements no other engine can run (`LOCK TABLES`, `SET`,
//!   `ALTER ... OWNER TO`), and failing the whole file over them would be
//!   useless. Only failures of statements that carry schema or data
//!   (`CREATE TABLE`, `INSERT`, `COPY`) are counted and reported, because only
//!   those mean something is missing from what you are looking at.
//! - **`COPY ... FROM stdin` blocks are replayed as inserts.** That is what a
//!   default `pg_dump` writes, so without it a Postgres dump would open as a
//!   set of empty tables, which is worse than an error.
//! - **MySQL backslash escapes are rewritten first.** `\'` inside a string is
//!   MySQL-only and SQLite would end the string on it. The rewrite runs only
//!   when the file looks like a `mysqldump`, since in Postgres and SQLite a
//!   backslash is an ordinary character.
//!
//! Read-only: this reader turns a dump into tables to look at, and Save As is
//! how you keep them.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use regex::Regex;
use rusqlite::Connection;

use crate::data::DataTable;

use super::{FormatReader, TableInfo};
use crate::formats::sqlite_reader;

pub struct SqlDumpReader;

/// Dumps are text, and this one is read whole before a line of it runs. The
/// cap is the archive reader's, for the same reason: a 10 GB file must fail
/// with a sentence rather than by exhausting the machine.
const MAX_DUMP_BYTES: u64 = 512 * 1024 * 1024;

/// How much of a failing statement to quote back at the user. Long enough to
/// recognise which one it was, short enough not to paste a 4 MB `INSERT` into
/// a banner.
const REPORT_EXCERPT: usize = 160;

/// What the replay did, so the app can say when something did not make it.
#[derive(Debug, Clone, Default)]
pub struct DumpReport {
    /// Statements that ran without error.
    pub replayed: usize,
    /// Statements carrying schema or data that did not run, with their reason.
    pub skipped: Vec<String>,
    /// How many statements carrying schema or data were seen in total.
    pub data_statements: usize,
}

impl DumpReport {
    /// One line for the load banner, or `None` when nothing was lost.
    pub fn banner(&self) -> Option<String> {
        if self.skipped.is_empty() {
            return None;
        }
        Some(
            crate::i18n::t("sqldump.skipped")
                .replace("{n}", &self.skipped.len().to_string())
                .replace("{total}", &self.data_statements.to_string())
                .replace(
                    "{first}",
                    self.skipped.first().map(String::as_str).unwrap_or(""),
                ),
        )
    }
}

/// How many dumps' reports to remember. A report is a handful of strings, so
/// this is nothing; the replayed database beside it is the expensive half and
/// gets one slot.
const REPORT_SLOTS: usize = 8;

/// What the last replays left behind.
///
/// The two halves are kept apart on purpose. The **database** is the whole
/// dump in memory, so exactly one is held and opening a second dump drops the
/// first - which is fine, because the only reason to keep it is that listing a
/// dump's tables and then opening one of them would otherwise parse the file
/// twice in a row. The **report** is a few strings, and losing it means losing
/// the banner that says rows are missing, so several are kept: with one slot,
/// opening a second dump between the read and the banner silently swallowed
/// the first one's warning.
///
/// ponytail: a `Vec` scanned linearly, not an LRU. Eight entries.
#[derive(Default)]
struct Cache {
    conn: Option<((PathBuf, u64), Connection)>,
    reports: Vec<((PathBuf, u64), DumpReport)>,
}

fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Cache::default()))
}

/// Identity of a file for the cache: path plus length. Cheap, and a dump that
/// changed length is a different dump.
fn cache_key(path: &Path) -> (PathBuf, u64) {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    (path.to_path_buf(), len)
}

/// Replay `path` if it is not already the cached dump, then hand `f` the
/// database it produced.
fn with_replay<T>(path: &Path, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let key = cache_key(path);
    let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    if cache.conn.as_ref().map(|(k, _)| k) != Some(&key) {
        let (conn, report) = replay(path)?;
        cache.conn = Some((key.clone(), conn));
        cache.reports.retain(|(k, _)| k != &key);
        cache.reports.push((key, report));
        if cache.reports.len() > REPORT_SLOTS {
            cache.reports.remove(0);
        }
    }
    let (_, conn) = cache.conn.as_ref().expect("just filled");
    f(conn)
}

/// What the last replay of `path` skipped, for the load banner. `None` when
/// that file is not the one in the cache (nothing to say beats guessing).
pub fn report_for(path: &Path) -> Option<DumpReport> {
    let key = cache_key(path);
    let cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    cache
        .reports
        .iter()
        .find(|(k, _)| k == &key)
        .map(|(_, r)| r.clone())
}

impl FormatReader for SqlDumpReader {
    fn name(&self) -> &str {
        "SQL dump"
    }

    /// Deliberately empty: `.sql` belongs to the text reader, and this one is
    /// asked for by name. See the module note.
    fn extensions(&self) -> &[&str] {
        &[]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        let first = with_replay(path, |conn| {
            Ok(sqlite_reader::list_user_tables_conn(conn)?
                .into_iter()
                .next()
                .map(|t| t.name))
        })?;
        match first {
            Some(name) => self.read_table(path, &name),
            None => {
                let lost = report_for(path).map(|r| r.skipped.len()).unwrap_or(0);
                bail!(
                    "{} produced no tables. {lost} statement(s) carrying schema or data could \
                     not be replayed; reopen it as Text (View -> Reopen as) to read the file \
                     itself.",
                    path.display()
                )
            }
        }
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let tables = with_replay(path, sqlite_reader::list_user_tables_conn)?;
        if tables.is_empty() {
            return Ok(None);
        }
        Ok(Some(tables))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        with_replay(path, |conn| {
            let mut t = sqlite_reader::read_table_conn(conn, table)?;
            // The rows came out of a scratch database, not out of the file on
            // disk, so nothing here is diff-saveable back to it.
            t.db_meta = None;
            t.format_name = Some("SQL dump".to_string());
            t.source_path = Some(path.to_string_lossy().to_string());
            Ok(t)
        })
    }
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

fn replay(path: &Path) -> Result<(Connection, DumpReport)> {
    let len = std::fs::metadata(path)
        .with_context(|| format!("reading {}", path.display()))?
        .len();
    if len > MAX_DUMP_BYTES {
        bail!(
            "{} is {} bytes, past the {MAX_DUMP_BYTES} byte limit for replaying a dump into \
             memory. Load it into a real database and open that instead.",
            path.display(),
            len
        );
    }
    let text = crate::data::encoding::read_to_string_detected(path)
        .with_context(|| format!("reading {}", path.display()))?;

    let text = if looks_like_mysqldump(&text) {
        rewrite_mysql_escapes(&text)
    } else {
        text
    };

    let conn = Connection::open_in_memory().context("opening the scratch database for the dump")?;
    // A dump's foreign keys point at tables that may not exist yet, or at all.
    // We are reading data, not enforcing a schema.
    let _ = conn.execute_batch("PRAGMA foreign_keys = OFF");

    let mut report = DumpReport::default();
    for_each_statement(&text, |stmt| {
        let carries_data = stmt.carries_data();
        if carries_data {
            report.data_statements += 1;
        }
        match execute(&conn, &stmt) {
            Ok(()) => report.replayed += 1,
            Err(e) if carries_data => report.skipped.push(format!("{}: {e}", stmt.excerpt())),
            // Everything else a dump contains that SQLite cannot run - `SET`,
            // `LOCK TABLES`, `ALTER ... OWNER TO`, `CREATE INDEX` on a MySQL
            // prefix - loses nothing that is visible in a table.
            Err(_) => {}
        }
    });
    Ok((conn, report))
}

/// One statement out of the dump.
enum Stmt {
    /// SQL to hand to SQLite as-is (after scrubbing).
    Sql(String),
    /// A `COPY ... FROM stdin` block: the target and its tab-separated rows.
    Copy {
        table: String,
        columns: Vec<String>,
        rows: Vec<String>,
    },
}

impl Stmt {
    /// Whether losing this statement loses schema or data. Only these are
    /// worth reporting: the rest of a dump is bookkeeping for the engine it
    /// came from.
    fn carries_data(&self) -> bool {
        match self {
            Stmt::Copy { .. } => true,
            Stmt::Sql(s) => {
                let head = leading_keywords(s);
                head.starts_with("CREATE TABLE") || head.starts_with("INSERT")
            }
        }
    }

    fn excerpt(&self) -> String {
        let s = match self {
            Stmt::Sql(s) => s.trim(),
            Stmt::Copy { table, .. } => return format!("COPY {table}"),
        };
        let flat = s[..char_boundary(s, 8192)]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if flat.chars().count() <= REPORT_EXCERPT {
            flat
        } else {
            let cut: String = flat.chars().take(REPORT_EXCERPT).collect();
            format!("{cut}...")
        }
    }
}

fn execute(conn: &Connection, stmt: &Stmt) -> Result<()> {
    match stmt {
        Stmt::Sql(sql) => {
            conn.execute_batch(sql)?;
            Ok(())
        }
        Stmt::Copy {
            table,
            columns,
            rows,
        } => {
            let cols = columns
                .iter()
                .map(|c| quote_ident(c))
                .collect::<Vec<_>>()
                .join(", ");
            let holes = vec!["?"; columns.len()].join(", ");
            let sql = if columns.is_empty() {
                bail!("COPY without a column list")
            } else {
                format!(
                    "INSERT INTO {} ({cols}) VALUES ({holes})",
                    quote_ident(table)
                )
            };
            // `COPY` writes booleans as `t` / `f`, which SQLite would store
            // as those two letters under a column the schema calls boolean.
            // Everything else is left as text and converted by SQLite's own
            // column affinity, the way `12.50` into a numeric column is.
            let booleans = boolean_columns(conn, table, columns);
            let mut prepared = conn.prepare(&sql)?;
            for row in rows {
                let mut values: Vec<Option<String>> = split_copy_row(row, columns.len());
                for (i, is_bool) in booleans.iter().enumerate() {
                    if *is_bool && let Some(v) = values[i].as_deref() {
                        values[i] = match v {
                            "t" => Some("1".to_string()),
                            "f" => Some("0".to_string()),
                            _ => continue,
                        };
                    }
                }
                prepared.execute(rusqlite::params_from_iter(values.iter()))?;
            }
            Ok(())
        }
    }
}

/// Which of `columns` the table declares as boolean, in the order given.
fn boolean_columns(conn: &Connection, table: &str, columns: &[String]) -> Vec<bool> {
    let declared: Vec<(String, String)> = conn
        .prepare(&format!("PRAGMA table_info({})", quote_ident(table)))
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
                .collect()
        })
        .unwrap_or_default();
    columns
        .iter()
        .map(|c| {
            declared.iter().any(|(name, ty)| {
                name.eq_ignore_ascii_case(c) && ty.to_ascii_uppercase().starts_with("BOOL")
            })
        })
        .collect()
}

/// One `COPY` data line into its fields, decoded. `\N` is NULL; the rest of
/// the escapes are Postgres' text-format ones.
fn split_copy_row(row: &str, want: usize) -> Vec<Option<String>> {
    let mut out: Vec<Option<String>> = row
        .split('\t')
        .map(|f| {
            if f == "\\N" {
                None
            } else {
                Some(decode_copy_field(f))
            }
        })
        .collect();
    out.resize(want, None);
    out.truncate(want);
    out
}

fn decode_copy_field(field: &str) -> String {
    if !field.contains('\\') {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('v') => out.push('\u{b}'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Splitting
// ---------------------------------------------------------------------------

/// Walk the dump statement by statement, pulling `COPY ... FROM stdin` blocks
/// out whole (their rows are data, not SQL, and carry no terminator).
///
/// Each one is handed over and dropped rather than collected: a 500 MB dump
/// would otherwise be held twice, once as text and once as a vector of the
/// same bytes cut up.
fn for_each_statement(text: &str, mut f: impl FnMut(Stmt)) {
    let b = text.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if let Some(next) = skip_atom(b, i) {
            i = next;
            continue;
        }
        if b[i] == b';' {
            let raw = &text[start..i];
            i += 1;
            start = i;
            if let Some(header) = copy_header(strip_leading_noise(raw)) {
                // The rows run from here to a line holding exactly `\.`.
                let (rows, after) = take_copy_rows(text, start);
                start = after;
                i = after;
                f(Stmt::Copy {
                    table: header.0,
                    columns: header.1,
                    rows,
                });
            } else if let Some(sql) = scrub(raw) {
                f(Stmt::Sql(sql));
            }
            continue;
        }
        i += 1;
    }
    if let Some(sql) = scrub(&text[start..]) {
        f(Stmt::Sql(sql));
    }
}

/// If `b[i]` opens a quoted run or a comment, return the index just past it.
/// This is the one place that knows dump syntax, and both the statement
/// splitter and the column-list splitter go through it.
fn skip_atom(b: &[u8], i: usize) -> Option<usize> {
    match b[i] {
        b'\'' => Some(skip_quoted(b, i, b'\'')),
        b'"' => Some(skip_quoted(b, i, b'"')),
        b'`' => Some(skip_quoted(b, i, b'`')),
        b'-' if b.get(i + 1) == Some(&b'-') => Some(skip_line(b, i)),
        b'#' => Some(skip_line(b, i)),
        b'/' if b.get(i + 1) == Some(&b'*') => {
            let mut j = i + 2;
            while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                j += 1;
            }
            Some((j + 2).min(b.len()))
        }
        b'$' => skip_dollar_quoted(b, i),
        _ => None,
    }
}

/// A quoted run ending at the next unescaped `q`. A doubled quote (`''`,
/// `""`) is a literal one and does not end it. Backslashes are not special:
/// [`rewrite_mysql_escapes`] has already removed them where they were.
fn skip_quoted(b: &[u8], i: usize, q: u8) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        if b[j] == q {
            if b.get(j + 1) == Some(&q) {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

fn skip_line(b: &[u8], i: usize) -> usize {
    let mut j = i;
    while j < b.len() && b[j] != b'\n' {
        j += 1;
    }
    (j + 1).min(b.len())
}

/// Postgres dollar quoting (`$$ ... $$`, `$body$ ... $body$`), which is how a
/// function body full of semicolons survives being split.
fn skip_dollar_quoted(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    if b.get(j) != Some(&b'$') {
        return None;
    }
    let tag = &b[i..=j];
    let mut k = j + 1;
    while k + tag.len() <= b.len() {
        if &b[k..k + tag.len()] == tag {
            return Some(k + tag.len());
        }
        k += 1;
    }
    Some(b.len())
}

/// `COPY schema.table (a, b) FROM stdin` -> the table and its columns.
fn copy_header(raw: &str) -> Option<(String, Vec<String>)> {
    let re = regexes();
    let caps = re.copy.captures(raw)?;
    let table = bare_name(caps.get(1)?.as_str());
    let columns = caps
        .get(2)
        .map(|m| {
            split_top_level(m.as_str())
                .into_iter()
                .map(|c| bare_name(c.trim()))
                .filter(|c| !c.is_empty())
                .collect()
        })
        .unwrap_or_default();
    Some((table, columns))
}

/// The data lines of a `COPY` block, and the offset just past its `\.`
/// terminator.
fn take_copy_rows(text: &str, from: usize) -> (Vec<String>, usize) {
    let mut rows = Vec::new();
    let mut pos = from;
    while pos < text.len() {
        let rest = &text[pos..];
        let (line, next) = match rest.find('\n') {
            Some(nl) => (&rest[..nl], pos + nl + 1),
            None => (rest, text.len()),
        };
        let trimmed = line.strip_suffix('\r').unwrap_or(line);
        if trimmed == "\\." {
            return (rows, next);
        }
        // A leading blank line before the data is formatting, not a row.
        if !(rows.is_empty() && trimmed.trim().is_empty()) {
            rows.push(trimmed.to_string());
        }
        pos = next;
    }
    (rows, text.len())
}

// ---------------------------------------------------------------------------
// Scrubbing
// ---------------------------------------------------------------------------

struct Patterns {
    copy: Regex,
    create: Regex,
    insert: Regex,
    tz: Regex,
    enum_set: Regex,
    trailing_junk: Regex,
    now_call: Regex,
    fn_default: Regex,
}

fn regexes() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        copy: Regex::new(r"(?is)^\s*COPY\s+([^\s(]+)\s*(?:\(([^)]*)\))?\s+FROM\s+stdin").unwrap(),
        // Group 1 is everything up to the table name, group 2 the name itself.
        create: Regex::new(r"(?is)^(\s*CREATE\s+(?:TEMP\s+|TEMPORARY\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?)(`[^`]+`|\x22[^\x22]+\x22|[A-Za-z0-9_.$]+)").unwrap(),
        insert: Regex::new(r"(?is)^(\s*(?:INSERT|REPLACE)\s+(?:IGNORE\s+)?INTO\s+)(`[^`]+`|\x22[^\x22]+\x22|[A-Za-z0-9_.$]+)").unwrap(),
        // `timestamp with time zone` and friends: SQLite's type-name grammar
        // stops at the keyword.
        tz: Regex::new(r"(?i)\b(with|without)\s+time\s+zone\b").unwrap(),
        // MySQL's `enum('a','b')` / `set('a','b')` are not type names SQLite
        // can parse; the values are text either way.
        enum_set: Regex::new(r"(?is)\b(enum|set)\s*\([^)]*\)").unwrap(),
        // Column attributes SQLite has no notion of.
        trailing_junk: Regex::new(
            r"(?is)\bAUTO_INCREMENT\b|\bZEROFILL\b|\bCOLLATE\s+[A-Za-z0-9_]+|\bCHARACTER\s+SET\s+[A-Za-z0-9_]+|\bCOMMENT\s+'(?:[^']|'')*'|\bON\s+UPDATE\s+CURRENT_TIMESTAMP(?:\s*\(\s*\d*\s*\))?",
        )
        .unwrap(),
        // MySQL writes `current_timestamp()`; SQLite spells the same thing as
        // a bare keyword and rejects the call form.
        now_call: Regex::new(r"(?i)\bCURRENT_TIMESTAMP\s*\(\s*\d*\s*\)").unwrap(),
        // Any other function default has to be parenthesised for SQLite. The
        // dump's own INSERTs name every column, so the expression is never
        // evaluated - it only has to parse.
        fn_default: Regex::new(r"(?i)\bDEFAULT\s+([A-Za-z_][A-Za-z0-9_]*\s*\([^)]*\))")
            .unwrap(),
    })
}

/// Turn one raw statement into something SQLite will accept, or `None` when
/// there is nothing left to run.
fn scrub(raw: &str) -> Option<String> {
    // `strip_leading_noise` already walked past every leading comment, so an
    // empty remainder means the statement was nothing but comments.
    let trimmed = strip_leading_noise(raw).trim();
    if trimmed.is_empty() {
        return None;
    }
    let head = leading_keywords(trimmed);
    if head.starts_with("CREATE TABLE")
        || head.starts_with("CREATE TEMP TABLE")
        || head.starts_with("CREATE TEMPORARY TABLE")
    {
        return Some(scrub_create_table(trimmed));
    }
    if head.starts_with("INSERT") || head.starts_with("REPLACE INTO") {
        let re = regexes();
        return Some(
            re.insert
                .replace(trimmed, |c: &regex::Captures| {
                    format!("{}{}", &c[1], quote_ident(&bare_name(&c[2])))
                })
                .into_owned(),
        );
    }
    Some(trimmed.to_string())
}

/// A `CREATE TABLE` with the dialect filed off: no schema qualifier, no
/// MySQL-only index clauses, no table options, no column attributes SQLite
/// does not have.
fn scrub_create_table(sql: &str) -> String {
    let re = regexes();
    // Head + name, with the schema qualifier dropped: SQLite would read
    // `public.users` as a table in an attached database called `public`.
    let renamed = re.create.replace(sql, |c: &regex::Captures| {
        format!("{}{}", &c[1], quote_ident(&bare_name(&c[2])))
    });

    let Some(open) = renamed.find('(') else {
        return renamed.into_owned();
    };
    let Some(close) = matching_paren(&renamed, open) else {
        return renamed.into_owned();
    };
    let head = &renamed[..=open];
    let body = &renamed[open + 1..close];

    let items: Vec<String> = split_top_level(body)
        .into_iter()
        .filter(|item| !is_index_clause(item))
        .map(scrub_column)
        .filter(|item| !item.trim().is_empty())
        .collect();

    // Everything after the closing paren is `ENGINE=InnoDB DEFAULT
    // CHARSET=...`, which belongs to MySQL and to nothing else.
    format!("{head}{})", items.join(", "))
}

/// A table-level index clause MySQL allows and SQLite does not. `PRIMARY KEY`
/// and `CONSTRAINT`/`FOREIGN KEY`/`CHECK`/`UNIQUE (...)` are real SQL and stay.
fn is_index_clause(item: &str) -> bool {
    let head = leading_keywords(item);
    head.starts_with("KEY")
        || head.starts_with("INDEX")
        || head.starts_with("UNIQUE KEY")
        || head.starts_with("UNIQUE INDEX")
        || head.starts_with("FULLTEXT")
        || head.starts_with("SPATIAL")
}

fn scrub_column(item: String) -> String {
    let re = regexes();
    let s = re.enum_set.replace_all(&item, "text");
    let s = re.tz.replace_all(&s, "");
    let s = re.now_call.replace_all(&s, "CURRENT_TIMESTAMP");
    let s = re.fn_default.replace_all(&s, "DEFAULT ($1)");
    // Trimmed, never re-joined on whitespace: collapsing runs of spaces
    // would also rewrite them inside a DEFAULT string literal.
    re.trailing_junk.replace_all(&s, "").trim().to_string()
}

/// Split a parenthesised list on its top-level commas, ignoring commas inside
/// nested parens, strings and quoted identifiers.
fn split_top_level(body: &str) -> Vec<String> {
    let b = body.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < b.len() {
        if let Some(next) = skip_atom(b, i) {
            i = next;
            continue;
        }
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                out.push(body[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last = body[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}

/// Index of the `)` closing the `(` at `open`, respecting strings and
/// comments.
fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < b.len() {
        if let Some(next) = skip_atom(b, i) {
            i = next;
            continue;
        }
        match b[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Skip past the leading whitespace, comments and psql meta-commands of a
/// statement, so the regexes below can anchor on the first real keyword.
///
/// `pg_dump` puts a three-line `--` banner in front of every statement and
/// (since 17.6) a `\restrict` line at the top of the file. Without this both
/// the schema-stripping and the `COPY` detection quietly failed to match, and
/// a real Postgres dump opened as nothing at all.
fn strip_leading_noise(raw: &str) -> &str {
    let b = raw.as_bytes();
    let mut i = 0usize;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return &raw[i..];
        }
        let comment = b[i] == b'#'
            || (b[i] == b'-' && b.get(i + 1) == Some(&b'-'))
            || (b[i] == b'/' && b.get(i + 1) == Some(&b'*'));
        if comment {
            i = skip_atom(b, i).unwrap_or(b.len());
            continue;
        }
        // A psql meta-command (`\restrict`, `\connect`) is a whole line and
        // is not SQL; left in place it would glue itself to the next
        // statement.
        if b[i] == b'\\' {
            i = skip_line(b, i);
            continue;
        }
        return &raw[i..];
    }
}

/// The first few words of a statement, upper-cased and with comments and
/// runs of whitespace flattened, so `carries_data` and the scrub dispatcher
/// can match on a prefix.
fn leading_keywords(s: &str) -> String {
    // Four words live well inside this; scanning a multi-megabyte `INSERT`
    // to find them would cost a copy of it per statement.
    let head = &s[..char_boundary(s, 4096)];
    strip_comments(head)
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// The largest index <= `at` that lands on a character boundary.
fn char_boundary(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Drop comments so a statement that opens with `/*!40000 ... */` is still
/// recognised by its first real keyword.
fn strip_comments(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < b.len() {
        let is_comment = matches!(b[i], b'#')
            || (b[i] == b'-' && b.get(i + 1) == Some(&b'-'))
            || (b[i] == b'/' && b.get(i + 1) == Some(&b'*'));
        if is_comment {
            i = skip_atom(b, i).unwrap_or(b.len());
            out.push(' ');
            continue;
        }
        if let Some(next) = skip_atom(b, i) {
            out.push_str(&s[i..next]);
            i = next;
            continue;
        }
        let ch_len = utf8_len(b[i]);
        out.push_str(&s[i..(i + ch_len).min(s.len())]);
        i += ch_len;
    }
    out
}

/// `public."My Table"` / `` `users` `` -> `My Table` / `users`: the last
/// dotted component, unquoted.
fn bare_name(raw: &str) -> String {
    let last = split_qualified(raw).pop().unwrap_or_default();
    let t = last.trim();
    let unquoted = t
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .or_else(|| t.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .or_else(|| t.strip_prefix('[').and_then(|s| s.strip_suffix(']')));
    match unquoted {
        Some(inner) => inner.replace("\"\"", "\""),
        None => t.to_string(),
    }
}

/// Split on dots that are not inside quotes, so `"a.b".c` is two parts.
fn split_qualified(raw: &str) -> Vec<String> {
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < b.len() {
        if let Some(next) = skip_atom(b, i) {
            i = next;
            continue;
        }
        if b[i] == b'.' {
            out.push(raw[start..i].to_string());
            start = i + 1;
        }
        i += 1;
    }
    out.push(raw[start..].to_string());
    out
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

// ---------------------------------------------------------------------------
// MySQL string escapes
// ---------------------------------------------------------------------------

/// Whether the file is a `mysqldump`. Only then is a backslash inside a
/// string an escape; in Postgres and SQLite it is an ordinary character, and
/// rewriting one there would corrupt the data.
fn looks_like_mysqldump(text: &str) -> bool {
    let head: String = text.chars().take(4096).collect();
    head.contains("/*!") || head.to_ascii_uppercase().contains("MYSQL DUMP")
}

/// Rewrite MySQL's backslash escapes inside single-quoted strings into SQLite
/// spelling, so the splitter and SQLite both see plain SQL strings.
fn rewrite_mysql_escapes(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'\'' {
            // Comments and other quoting styles pass through untouched, but
            // must be stepped over so a `'` inside them opens nothing.
            if let Some(next) = skip_atom(b, i) {
                out.push_str(&text[i..next]);
                i = next;
                continue;
            }
            let ch_len = utf8_len(b[i]);
            out.push_str(&text[i..(i + ch_len).min(text.len())]);
            i += ch_len;
            continue;
        }
        out.push('\'');
        i += 1;
        while i < b.len() {
            match b[i] {
                b'\\' if i + 1 < b.len() => {
                    match b[i + 1] {
                        b'\'' => out.push_str("''"),
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'Z' => out.push('\u{1a}'),
                        // `\0` cannot travel inside a SQL string literal.
                        b'0' => {}
                        // `\%` and `\_` stay as they are: MySQL keeps the
                        // backslash outside a LIKE pattern.
                        other => {
                            out.push('\\');
                            out.push(other as char);
                        }
                    }
                    i += 2;
                }
                b'\'' => {
                    if b.get(i + 1) == Some(&b'\'') {
                        out.push_str("''");
                        i += 2;
                        continue;
                    }
                    out.push('\'');
                    i += 1;
                    break;
                }
                _ => {
                    let ch_len = utf8_len(b[i]);
                    out.push_str(&text[i..(i + ch_len).min(text.len())]);
                    i += ch_len;
                }
            }
        }
    }
    out
}

/// Byte length of the UTF-8 character starting with `first`.
fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}
