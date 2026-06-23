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

use crate::data::{CellValue, DataTable};

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
const FAKE_NAMES: &[&str] = &[
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
const FAKE_LOCALPARTS: &[&str] = &[
    "alex", "jordan", "taylor", "morgan", "casey", "riley", "jamie", "avery", "drew", "sydney",
    "cameron", "reese", "skyler", "harper", "rowan", "emerson",
];
const FAKE_DOMAINS: &[&str] = &["example.com", "example.org", "example.net", "mail.example"];
const FAKE_CITIES: &[&str] = &[
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
const FAKE_COMPANIES: &[&str] = &[
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo, DataTable};

    /// One-column table from string literals (empty string -> empty `String`
    /// cell, the literal `"<null>"` -> `Null`).
    fn table(values: &[&str]) -> DataTable {
        let mut t = DataTable::empty();
        t.columns.push(ColumnInfo {
            name: "v".to_string(),
            data_type: "Utf8".to_string(),
        });
        t.rows = values
            .iter()
            .map(|v| {
                let cell = if *v == "<null>" {
                    CellValue::Null
                } else {
                    CellValue::String(v.to_string())
                };
                vec![cell]
            })
            .collect();
        t
    }

    fn run_one(t: &DataTable, strategy: AnonStrategy, salt: &str) -> Vec<CellValue> {
        let spec = AnonSpec {
            rules: vec![AnonRule {
                columns: vec![0],
                strategy,
                new_column: None,
            }],
            salt: salt.to_string(),
        };
        anonymize_table(t, &spec).remove(0).values
    }

    fn run_outputs(t: &DataTable, rules: Vec<AnonRule>, salt: &str) -> Vec<AnonOutput> {
        anonymize_table(
            t,
            &AnonSpec {
                rules,
                salt: salt.to_string(),
            },
        )
    }

    #[test]
    fn hash_is_deterministic_and_truncated() {
        let t = table(&["alice", "alice", "bob"]);
        let out = run_one(
            &t,
            AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: Some(12),
            },
            "s",
        );
        assert_eq!(out[0], out[1]);
        assert_ne!(out[0], out[2]);
        if let CellValue::String(s) = &out[0] {
            assert_eq!(s.len(), 12);
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn hash_salt_changes_output() {
        let t = table(&["alice"]);
        let a = run_one(
            &t,
            AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: Some(16),
            },
            "saltA",
        );
        let b = run_one(
            &t,
            AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: Some(16),
            },
            "saltB",
        );
        assert_ne!(a[0], b[0]);
    }

    #[test]
    fn hash_blake3_differs_from_sha256() {
        let t = table(&["alice"]);
        let sha = run_one(
            &t,
            AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: Some(16),
            },
            "s",
        );
        let bl = run_one(
            &t,
            AnonStrategy::Hash {
                algo: HashAlgo::Blake3,
                length: Some(16),
            },
            "s",
        );
        assert_ne!(sha[0], bl[0]);
    }

    #[test]
    fn redact_fixed_and_null() {
        let t = table(&["secret"]);
        let fixed = run_one(
            &t,
            AnonStrategy::Redact {
                token: RedactToken::Fixed("[X]".to_string()),
            },
            "",
        );
        assert_eq!(fixed[0], CellValue::String("[X]".to_string()));
        let nulled = run_one(
            &t,
            AnonStrategy::Redact {
                token: RedactToken::Null,
            },
            "",
        );
        assert_eq!(nulled[0], CellValue::Null);
    }

    #[test]
    fn null_and_empty_pass_through() {
        let t = table(&["<null>", ""]);
        let out = run_one(
            &t,
            AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: Some(12),
            },
            "s",
        );
        assert_eq!(out[0], CellValue::Null);
        assert_eq!(out[1], CellValue::String(String::new()));
    }

    #[test]
    fn out_of_range_column_skipped() {
        let t = table(&["alice"]);
        let spec = AnonSpec {
            rules: vec![AnonRule {
                columns: vec![9],
                strategy: AnonStrategy::Redact {
                    token: RedactToken::Null,
                },
                new_column: None,
            }],
            salt: String::new(),
        };
        assert!(anonymize_table(&t, &spec).is_empty());
    }

    #[test]
    fn partial_mask_keeps_last_n() {
        let t = table(&["5551234"]);
        let out = run_one(
            &t,
            AnonStrategy::PartialMask {
                keep: KeepEnd::Last,
                count: 4,
                mask_char: '*',
                mask_len: None,
            },
            "",
        );
        assert_eq!(out[0], CellValue::String("***1234".to_string()));
    }

    #[test]
    fn partial_mask_keeps_first_n() {
        let t = table(&["abcdef"]);
        let out = run_one(
            &t,
            AnonStrategy::PartialMask {
                keep: KeepEnd::First,
                count: 2,
                mask_char: '#',
                mask_len: None,
            },
            "",
        );
        assert_eq!(out[0], CellValue::String("ab####".to_string()));
    }

    #[test]
    fn partial_mask_count_ge_len_keeps_whole_value() {
        let t = table(&["ab"]);
        let out = run_one(
            &t,
            AnonStrategy::PartialMask {
                keep: KeepEnd::Last,
                count: 5,
                mask_char: '*',
                mask_len: None,
            },
            "",
        );
        assert_eq!(out[0], CellValue::String("ab".to_string()));
    }

    #[test]
    fn partial_mask_fixed_len_hides_original_length() {
        let t = table(&["5551234", "12"]);
        let out = run_one(
            &t,
            AnonStrategy::PartialMask {
                keep: KeepEnd::Last,
                count: 2,
                mask_char: '*',
                mask_len: Some(5),
            },
            "",
        );
        // Both rows: 5 mask chars + 2 kept = same length regardless of input.
        assert_eq!(out[0], CellValue::String("*****34".to_string()));
        assert_eq!(out[1], CellValue::String("*****12".to_string()));
    }

    #[test]
    fn partial_mask_counts_characters_not_bytes() {
        let t = table(&["aeiou\u{e9}"]); // "aeioué", é is 2 bytes
        let out = run_one(
            &t,
            AnonStrategy::PartialMask {
                keep: KeepEnd::Last,
                count: 1,
                mask_char: '*',
                mask_len: None,
            },
            "",
        );
        assert_eq!(out[0], CellValue::String("*****\u{e9}".to_string()));
    }

    #[test]
    fn fake_is_consistent_for_duplicates() {
        let t = table(&["alice", "alice", "bob"]);
        let out = run_one(
            &t,
            AnonStrategy::Fake {
                kind: FakeKind::Name,
            },
            "s",
        );
        assert_eq!(out[0], out[1]);
        assert_ne!(out[0], out[2]);
    }

    #[test]
    fn fake_email_looks_like_email() {
        let t = table(&["alice"]);
        let out = run_one(
            &t,
            AnonStrategy::Fake {
                kind: FakeKind::Email,
            },
            "s",
        );
        if let CellValue::String(s) = &out[0] {
            assert!(s.contains('@'), "email should contain @: {s}");
            assert!(s.contains('.'), "email should contain a dot: {s}");
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn fake_uuid_has_uuid_shape() {
        let t = table(&["alice"]);
        let out = run_one(
            &t,
            AnonStrategy::Fake {
                kind: FakeKind::Uuid,
            },
            "s",
        );
        if let CellValue::String(s) = &out[0] {
            let parts: Vec<&str> = s.split('-').collect();
            assert_eq!(parts.len(), 5, "uuid has 5 dash-separated groups: {s}");
            assert_eq!(
                parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
                vec![8, 4, 4, 4, 12]
            );
            assert!(s.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn fake_phone_is_digits_and_separators() {
        let t = table(&["alice"]);
        let out = run_one(
            &t,
            AnonStrategy::Fake {
                kind: FakeKind::Phone,
            },
            "s",
        );
        if let CellValue::String(s) = &out[0] {
            assert!(s.chars().any(|c| c.is_ascii_digit()));
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn fake_salt_changes_value() {
        let t = table(&["alice"]);
        let a = run_one(
            &t,
            AnonStrategy::Fake {
                kind: FakeKind::City,
            },
            "a",
        );
        let b = run_one(
            &t,
            AnonStrategy::Fake {
                kind: FakeKind::City,
            },
            "b",
        );
        assert_ne!(a[0], b[0]);
    }

    #[test]
    fn rerun_with_same_salt_is_idempotent() {
        let t = table(&["alice", "bob"]);
        let strat = || AnonStrategy::Hash {
            algo: HashAlgo::Sha256,
            length: Some(12),
        };
        let first = run_one(&t, strat(), "s");
        let again = run_one(&t, strat(), "s");
        assert_eq!(first, again);
    }

    #[test]
    fn multiple_rules_return_one_column_each() {
        let mut t = DataTable::empty();
        for name in ["email", "phone"] {
            t.columns.push(ColumnInfo {
                name: name.to_string(),
                data_type: "Utf8".to_string(),
            });
        }
        t.rows = vec![vec![
            CellValue::String("a@b.com".to_string()),
            CellValue::String("5551234".to_string()),
        ]];
        let spec = AnonSpec {
            rules: vec![
                AnonRule {
                    columns: vec![0],
                    strategy: AnonStrategy::Hash {
                        algo: HashAlgo::Sha256,
                        length: Some(8),
                    },
                    new_column: None,
                },
                AnonRule {
                    columns: vec![1],
                    strategy: AnonStrategy::PartialMask {
                        keep: KeepEnd::Last,
                        count: 4,
                        mask_char: '*',
                        mask_len: None,
                    },
                    new_column: None,
                },
            ],
            salt: String::new(),
        };
        let out = anonymize_table(&t, &spec);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, AnonSource::Column(0));
        assert_eq!(out[1].source, AnonSource::Column(1));
        assert_eq!(out[1].values[0], CellValue::String("***1234".to_string()));
    }

    #[test]
    fn hash_full_by_default() {
        let t = table(&["alice"]);
        let out = run_outputs(
            &t,
            vec![AnonRule {
                columns: vec![0],
                strategy: AnonStrategy::Hash {
                    algo: HashAlgo::Sha256,
                    length: None,
                },
                new_column: None,
            }],
            "s",
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, AnonSource::Column(0));
        if let CellValue::String(s) = &out[0].values[0] {
            assert_eq!(s.len(), 64);
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn multi_column_hash_is_one_derived_column() {
        let mut t = DataTable::empty();
        for n in ["first", "last"] {
            t.columns.push(ColumnInfo {
                name: n.into(),
                data_type: "Utf8".into(),
            });
        }
        t.rows = vec![
            vec![
                CellValue::String("john".into()),
                CellValue::String("smith".into()),
            ],
            vec![
                CellValue::String("john".into()),
                CellValue::String("smith".into()),
            ],
            vec![
                CellValue::String("jane".into()),
                CellValue::String("smith".into()),
            ],
        ];
        let out = run_outputs(
            &t,
            vec![AnonRule {
                columns: vec![0, 1],
                strategy: AnonStrategy::Hash {
                    algo: HashAlgo::Sha256,
                    length: Some(16),
                },
                new_column: Some("person_id".into()),
            }],
            "s",
        );
        assert_eq!(out.len(), 1);
        match &out[0].source {
            AnonSource::Derived { name } => assert_eq!(name, "person_id"),
            _ => panic!("multi-column hash must be a derived new column"),
        }
        assert_eq!(out[0].values[0], out[0].values[1]);
        assert_ne!(out[0].values[0], out[0].values[2]);
    }

    #[test]
    fn mask_with_two_columns_yields_two_column_outputs() {
        let mut t = DataTable::empty();
        for n in ["a", "b"] {
            t.columns.push(ColumnInfo {
                name: n.into(),
                data_type: "Utf8".into(),
            });
        }
        t.rows = vec![vec![
            CellValue::String("5551234".into()),
            CellValue::String("9998888".into()),
        ]];
        let out = run_outputs(
            &t,
            vec![AnonRule {
                columns: vec![0, 1],
                strategy: AnonStrategy::PartialMask {
                    keep: KeepEnd::Last,
                    count: 4,
                    mask_char: '*',
                    mask_len: None,
                },
                new_column: None,
            }],
            "",
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].source, AnonSource::Column(0));
        assert_eq!(out[1].source, AnonSource::Column(1));
        assert_eq!(out[0].values[0], CellValue::String("***1234".into()));
        assert_eq!(out[1].values[0], CellValue::String("***8888".into()));
    }
}
