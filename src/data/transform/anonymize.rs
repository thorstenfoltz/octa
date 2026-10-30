//! Anonymise / mask sensitive columns - a "prepare for sharing" transform.
//!
//! Pure and testable (no UI/IO), one drop-in op like the other
//! `src/data/transform/` modules. The caller picks columns and a per-column
//! [`AnonStrategy`]; [`anonymize_table`] returns one replacement column per
//! rule (column index + new cell values), mirroring the in-place transforms.
//!
//! Every strategy keys off one deterministic digest:
//! `digest = HASH(salt_bytes ++ value_bytes)` for the chosen [`HashAlgo`].
//! The same input value + salt always yields the same output, so duplicate
//! values stay linked and a re-run with the same salt re-joins to an earlier
//! export. Null / empty input cells pass through unchanged.

use serde::{Deserialize, Serialize};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Digest algorithm backing every strategy.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgo {
    #[default]
    Sha256,
    Blake3,
}

/// Which end of the string [`AnonStrategy::PartialMask`] keeps in clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeepEnd {
    First,
    Last,
}

/// What [`AnonStrategy::Redact`] replaces the whole value with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RedactToken {
    /// A fixed string (e.g. `[REDACTED]`).
    Fixed(String),
    /// Replace with a `Null` cell.
    Null,
}

/// Kind of synthetic data produced by [`AnonStrategy::Fake`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FakeKind {
    #[default]
    Name,
    Email,
    City,
    Company,
    Phone,
    Uuid,
}

/// How one column should be scrambled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnonStrategy {
    /// Replace with a hex digest (optionally truncated). Stable and join-able.
    Hash {
        #[serde(default)]
        algo: HashAlgo,
        /// Truncate the hex digest to this many chars. `None` = full digest.
        #[serde(default)]
        length: Option<usize>,
    },
    /// Keep first/last N characters, mask the rest (preserves shape).
    PartialMask {
        #[serde(default = "default_keep_end")]
        keep: KeepEnd,
        #[serde(default = "default_mask_count")]
        count: usize,
        #[serde(default = "default_mask_char")]
        mask_char: char,
        /// Fixed number of mask characters, so every output has the same length
        /// (`keep` count + this) and the original length stops leaking. `None`
        /// masks exactly the hidden characters (length-revealing, the default).
        #[serde(default)]
        mask_len: Option<usize>,
    },
    /// Replace the whole value with a fixed token or `Null`.
    Redact {
        #[serde(default = "default_redact_token")]
        token: RedactToken,
    },
    /// Substitute realistic synthetic data of a chosen kind.
    Fake {
        #[serde(default)]
        kind: FakeKind,
    },
}

fn default_keep_end() -> KeepEnd {
    KeepEnd::Last
}
fn default_mask_count() -> usize {
    4
}
fn default_mask_char() -> char {
    '*'
}
fn default_redact_token() -> RedactToken {
    RedactToken::Fixed("[REDACTED]".to_string())
}

/// One column-anonymisation rule. `columns` holds one or more source columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnonRule {
    pub columns: Vec<usize>,
    pub strategy: AnonStrategy,
    /// Name for the derived column when this is a multi-source hash. Ignored
    /// otherwise; defaults to `hash_<c0>_<c1>...`.
    #[serde(default)]
    pub new_column: Option<String>,
}

/// Where an [`AnonOutput`] should land.
#[derive(Debug, Clone, PartialEq)]
pub enum AnonSource {
    /// Bound to an existing column: the caller may replace it or append a copy.
    Column(usize),
    /// A derived value that must become a brand-new column (multi-source hash).
    Derived { name: String },
}

/// One produced column of values plus where it belongs.
#[derive(Debug, Clone, PartialEq)]
pub struct AnonOutput {
    pub source: AnonSource,
    pub values: Vec<CellValue>,
}

/// A full anonymisation request: per-column rules + one shared salt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnonSpec {
    pub rules: Vec<AnonRule>,
    #[serde(default)]
    pub salt: String,
}

// Small built-in pools so no fake-data crate dependency is added. Email,
// phone, and UUID are generated from the digest rather than drawn from a list.
pub(crate) const FAKE_NAMES: &[&str] = &[
    "Alex Carter",
    "Jordan Lee",
    "Taylor Brooks",
    "Morgan Hayes",
    "Casey Reed",
    "Riley Quinn",
    "Jamie Ellis",
    "Avery Stone",
    "Drew Parker",
    "Sydney Marsh",
    "Cameron Blake",
    "Reese Holt",
    "Skyler Dunn",
    "Harper Vance",
    "Rowan Frost",
    "Emerson Pike",
];
pub(crate) const FAKE_LOCALPARTS: &[&str] = &[
    "alex", "jordan", "taylor", "morgan", "casey", "riley", "jamie", "avery", "drew", "sydney",
    "cameron", "reese", "skyler", "harper", "rowan", "emerson",
];
pub(crate) const FAKE_DOMAINS: &[&str] =
    &["example.com", "example.org", "example.net", "mail.example"];
pub(crate) const FAKE_CITIES: &[&str] = &[
    "Springfield",
    "Riverton",
    "Fairview",
    "Lakeside",
    "Greenville",
    "Ashford",
    "Brookfield",
    "Maplewood",
    "Oakdale",
    "Cedar Falls",
    "Westbrook",
    "Kingsport",
    "Bridgeport",
    "Sunnyvale",
    "Elmwood",
    "Harborview",
];
pub(crate) const FAKE_COMPANIES: &[&str] = &[
    "Acme Industries",
    "Globex Corp",
    "Initech LLC",
    "Umbrella Group",
    "Soylent Co",
    "Hooli Inc",
    "Vandelay Ltd",
    "Stark Solutions",
    "Wayne Holdings",
    "Wonka Works",
    "Tyrell Systems",
    "Cyberdyne Labs",
    "Massive Dynamic",
    "Pied Piper",
    "Aperture Co",
    "Nakatomi Trading",
];

/// Lowercase hex of `salt ++ value` under the chosen algorithm. This is the
/// single deterministic source every strategy derives from.
fn digest_hex(algo: HashAlgo, salt: &str, value: &str) -> String {
    match algo {
        HashAlgo::Sha256 => {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(salt.as_bytes());
            h.update(value.as_bytes());
            hex_lower(&h.finalize())
        }
        HashAlgo::Blake3 => {
            let mut h = blake3::Hasher::new();
            h.update(salt.as_bytes());
            h.update(value.as_bytes());
            hex_lower(h.finalize().as_bytes())
        }
    }
}

/// Format bytes as lowercase hex.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// First 8 bytes of `salt ++ value`'s SHA-256 digest as a `u64` (big-endian).
/// Used by Fake to index word pools deterministically. Always SHA-256 so the
/// fake mapping does not depend on the user's hash-algo choice.
fn digest_u64(salt: &str, value: &str) -> u64 {
    let hex = digest_hex(HashAlgo::Sha256, salt, value);
    let raw = hex.as_bytes();
    let mut bytes = [0u8; 8];
    // Each hex pair -> one byte; we only need the first 8 bytes (16 hex chars).
    for (i, b) in bytes.iter_mut().enumerate() {
        let hi = (raw[i * 2] as char).to_digit(16).unwrap_or(0) as u8;
        let lo = (raw[i * 2 + 1] as char).to_digit(16).unwrap_or(0) as u8;
        *b = (hi << 4) | lo;
    }
    u64::from_be_bytes(bytes)
}

/// Pick from a pool by the digest `u64`.
fn pick<'a>(pool: &'a [&'a str], n: u64) -> &'a str {
    pool[(n as usize) % pool.len()]
}

/// Produce deterministic synthetic data of `kind` for `value` under `salt`.
fn fake_value(kind: FakeKind, salt: &str, value: &str) -> CellValue {
    let n = digest_u64(salt, value);
    let hex = digest_hex(HashAlgo::Sha256, salt, value);
    let s = match kind {
        FakeKind::Name => pick(FAKE_NAMES, n).to_string(),
        FakeKind::City => pick(FAKE_CITIES, n).to_string(),
        FakeKind::Company => pick(FAKE_COMPANIES, n).to_string(),
        FakeKind::Email => {
            let local = pick(FAKE_LOCALPARTS, n);
            let domain = pick(FAKE_DOMAINS, n >> 8);
            // A short numeric suffix keeps low-cardinality pools from colliding.
            format!("{local}{}@{domain}", n % 1000)
        }
        FakeKind::Phone => {
            // +1-AAA-BBB-CCCC from successive digest bytes.
            let area = 200 + (n % 800);
            let mid = 100 + ((n >> 16) % 900);
            let last = (n >> 32) % 10000;
            format!("+1-{area:03}-{mid:03}-{last:04}")
        }
        FakeKind::Uuid => {
            // 32 hex chars from the digest, grouped 8-4-4-4-12.
            let h = &hex[..32];
            format!(
                "{}-{}-{}-{}-{}",
                &h[0..8],
                &h[8..12],
                &h[12..16],
                &h[16..20],
                &h[20..32]
            )
        }
    };
    CellValue::String(s)
}

/// Apply `strategy` to one already-non-empty string value.
fn apply_strategy(strategy: &AnonStrategy, salt: &str, value: &str) -> CellValue {
    match strategy {
        AnonStrategy::Hash { algo, length } => {
            let hex = digest_hex(*algo, salt, value);
            let n = length.unwrap_or(hex.len()).min(hex.len());
            CellValue::String(hex[..n].to_string())
        }
        AnonStrategy::PartialMask {
            keep,
            count,
            mask_char,
            mask_len,
        } => {
            let chars: Vec<char> = value.chars().collect();
            let len = chars.len();
            let keep_n = (*count).min(len);
            // None -> mask exactly the hidden chars (reveals length); Some(n) ->
            // a fixed run so every output is the same length.
            let mask_n = mask_len.unwrap_or(len - keep_n);
            let mask: String = std::iter::repeat_n(*mask_char, mask_n).collect();
            let masked = match keep {
                KeepEnd::Last => {
                    let kept: String = chars[len - keep_n..].iter().collect();
                    format!("{mask}{kept}")
                }
                KeepEnd::First => {
                    let kept: String = chars[..keep_n].iter().collect();
                    format!("{kept}{mask}")
                }
            };
            CellValue::String(masked)
        }
        AnonStrategy::Redact { token } => match token {
            RedactToken::Null => CellValue::Null,
            RedactToken::Fixed(s) => CellValue::String(s.clone()),
        },
        AnonStrategy::Fake { kind } => fake_value(*kind, salt, value),
    }
}

/// Anonymise the chosen columns. Returns one [`AnonOutput`] per produced
/// column. mask/redact/fake produce one `Column` output per source column; a
/// hash rule with one source column produces one `Column` output, with two or
/// more it concatenates them (unit-separated, in order) and produces a single
/// `Derived` new column. Null / empty cells pass through unchanged. Out-of-range
/// column indices are skipped silently (the caller validates against live
/// columns).
pub fn anonymize_table(table: &DataTable, spec: &AnonSpec) -> Vec<AnonOutput> {
    let n = table.row_count();
    let col_count = table.col_count();
    let mut out = Vec::new();
    for rule in &spec.rules {
        let cols: Vec<usize> = rule
            .columns
            .iter()
            .copied()
            .filter(|&c| c < col_count)
            .collect();
        if cols.is_empty() {
            continue;
        }
        let is_combined_hash =
            matches!(rule.strategy, AnonStrategy::Hash { .. }) && cols.len() >= 2;
        if is_combined_hash {
            let name = rule.new_column.clone().unwrap_or_else(|| {
                let parts: Vec<String> = cols.iter().map(|c| c.to_string()).collect();
                format!("hash_{}", parts.join("_"))
            });
            let values: Vec<CellValue> = (0..n)
                .map(|r| {
                    let mut parts: Vec<String> = Vec::with_capacity(cols.len());
                    let mut all_empty = true;
                    for &c in &cols {
                        match table.get(r, c) {
                            Some(CellValue::Null) | None => parts.push(String::new()),
                            Some(v) => {
                                let s = v.to_string();
                                if !s.is_empty() {
                                    all_empty = false;
                                }
                                parts.push(s);
                            }
                        }
                    }
                    if all_empty {
                        return CellValue::Null;
                    }
                    apply_strategy(&rule.strategy, &spec.salt, &parts.join("\u{1f}"))
                })
                .collect();
            out.push(AnonOutput {
                source: AnonSource::Derived { name },
                values,
            });
        } else {
            for &c in &cols {
                let values: Vec<CellValue> = (0..n)
                    .map(|r| {
                        let cur = table.get(r, c).cloned().unwrap_or(CellValue::Null);
                        let text = match &cur {
                            CellValue::Null => return CellValue::Null,
                            other => other.to_string(),
                        };
                        if text.is_empty() {
                            return cur;
                        }
                        apply_strategy(&rule.strategy, &spec.salt, &text)
                    })
                    .collect();
                out.push(AnonOutput {
                    source: AnonSource::Column(c),
                    values,
                });
            }
        }
    }
    out
}

// One column name or several, as the CLI spec file and the MCP tool take them.
// No doc comments on these two types: schemars would add them to the MCP
// tool's input schema.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum ColumnsField {
    One(String),
    Many(Vec<String>),
}

impl ColumnsField {
    pub fn names(&self) -> Vec<String> {
        match self {
            ColumnsField::One(s) => vec![s.clone()],
            ColumnsField::Many(v) => v.clone(),
        }
    }
}

// An [`AnonRule`] that names its columns instead of indexing them.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "RuleParam")]
pub struct NamedAnonRule {
    /// One column name or an array of names. Two or more names with a hash
    /// strategy combine them into one new column.
    pub columns: ColumnsField,
    /// Name for the combined-hash new column (multi-column hash only).
    #[serde(default)]
    pub new_column: Option<String>,
    /// How to scramble it (see the tool description for the shapes).
    pub strategy: AnonStrategy,
}

/// Resolve `rules` by column name, anonymise, and write the result into
/// `table`: over the source columns, or with `new_columns` beside them as
/// `<name>_anon`. A multi-column hash always lands in a new column. Returns
/// how many columns were produced. Shared by the CLI and the MCP tool.
pub fn anonymize_by_name(
    table: &mut DataTable,
    rules: &[NamedAnonRule],
    salt: &str,
    new_columns: bool,
) -> anyhow::Result<usize> {
    let rules = rules
        .iter()
        .map(|r| {
            let columns = r
                .columns
                .names()
                .iter()
                .map(|n| table.column_index(n))
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(AnonRule {
                columns,
                strategy: r.strategy.clone(),
                new_column: r.new_column.clone(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let outputs = anonymize_table(
        table,
        &AnonSpec {
            rules,
            salt: salt.to_string(),
        },
    );
    let produced = outputs.len();
    for o in outputs {
        match o.source {
            AnonSource::Column(c) if !new_columns => {
                for (row, v) in o.values.into_iter().enumerate() {
                    table.set(row, c, v);
                }
            }
            AnonSource::Column(c) => {
                let base = table
                    .columns
                    .get(c)
                    .map(|x| x.name.clone())
                    .unwrap_or_default();
                push_text_column(table, &format!("{base}_anon"), o.values);
            }
            AnonSource::Derived { name } => push_text_column(table, &name, o.values),
        }
    }
    table.apply_edits();
    Ok(produced)
}

/// Append a Utf8 column under a free name. Not undoable: the GUI dialog has
/// its own undoable path, the headless surfaces have no undo.
fn push_text_column(table: &mut DataTable, name: &str, values: Vec<CellValue>) {
    let name = table.unique_column_name(name);
    table.columns.push(ColumnInfo {
        name,
        data_type: "Utf8".into(),
    });
    for (row, v) in values.into_iter().enumerate() {
        if let Some(r) = table.rows.get_mut(row) {
            r.push(v);
        }
    }
}

#[cfg(test)]
#[path = "anonymize_tests.rs"]
mod tests;
