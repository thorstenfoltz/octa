//! Heuristic PII (personal-data) column detection.
//!
//! For each column we combine two signals:
//!
//! 1. **Column name** - does the header look like a known PII field
//!    (`email`, `first_name`, `gender`, `country`, ...)?
//! 2. **Cell values** - for kinds with a recognisable shape (email, IP,
//!    credit card, IBAN, SSN, phone, date, postal code) we measure the
//!    fraction of sampled non-empty cells that match.
//!
//! ## Confidence
//!
//! `value_match` is that fraction (0.0 when the kind has no value pattern, e.g.
//! a person's name). `by_name` is whether the header matched. Confidence is:
//!
//! ```text
//! if value_match >= 0.6 : value_match (+0.2 if the name also matches, capped 1.0)
//! else if by_name       : 0.6 + 0.4 * value_match
//! else                  : value_match
//! ```
//!
//! A column is reported when its best kind scores >= 0.5. So a strong value
//! pattern alone is enough (`0.6+`), and a header match alone is enough
//! (`0.6`); the two together score highest. Some kinds (name, country,
//! birthdate, postal code, address) have an ambiguous or no value pattern, so
//! they are reported **only when the header matches** (`require_name`).

use crate::data::CellValue;
use crate::data::DataTable;
use crate::data::transform::{AnonRule, AnonStrategy, HashAlgo};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiiKind {
    Email,
    Phone,
    Ip,
    CreditCard,
    Iban,
    Ssn,
    Name,
    Gender,
    Country,
    BirthDate,
    PostalCode,
    Address,
}

impl PiiKind {
    /// Stable snake_case identifier (used by the CLI / MCP JSON output).
    pub fn id(self) -> &'static str {
        match self {
            PiiKind::Email => "email",
            PiiKind::Phone => "phone",
            PiiKind::Ip => "ip_address",
            PiiKind::CreditCard => "credit_card",
            PiiKind::Iban => "iban",
            PiiKind::Ssn => "ssn",
            PiiKind::Name => "name",
            PiiKind::Gender => "gender",
            PiiKind::Country => "country",
            PiiKind::BirthDate => "birth_date",
            PiiKind::PostalCode => "postal_code",
            PiiKind::Address => "address",
        }
    }
}

const ALL_KINDS: [PiiKind; 12] = [
    PiiKind::Email,
    PiiKind::Phone,
    PiiKind::Ip,
    PiiKind::CreditCard,
    PiiKind::Iban,
    PiiKind::Ssn,
    PiiKind::Name,
    PiiKind::Gender,
    PiiKind::Country,
    PiiKind::BirthDate,
    PiiKind::PostalCode,
    PiiKind::Address,
];

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnPii {
    pub column: usize,
    pub kind: PiiKind,
    pub confidence: f64,
    /// The column header matched this kind's keywords.
    pub by_name: bool,
    /// Fraction of sampled non-empty cells matching the kind's value pattern
    /// (0.0 for kinds without one, e.g. a person's name).
    pub value_match: f64,
}

/// Per-kind detection descriptor.
struct KindSpec {
    /// Header keywords. Keywords of length <= 3 must match a whole token (so
    /// `ip` does not fire inside `recipient`); longer ones may match anywhere
    /// in the squashed header.
    keywords: &'static [&'static str],
    /// Only report this kind when the header matches (its value pattern is
    /// absent or too ambiguous to stand alone).
    require_name: bool,
    /// Whether the kind has a recognisable value pattern at all.
    has_value: bool,
}

fn spec(kind: PiiKind) -> KindSpec {
    let (keywords, require_name, has_value): (&[&str], bool, bool) = match kind {
        PiiKind::Email => (&["email", "mail", "e_mail"], false, true),
        PiiKind::Phone => (
            &["phone", "tel", "mobile", "cell", "fax", "telephone"],
            false,
            true,
        ),
        PiiKind::Ip => (&["ip", "ipaddr", "ipaddress", "ip_address"], false, true),
        PiiKind::CreditCard => (&["cc", "card", "creditcard", "pan", "ccnum"], false, true),
        PiiKind::Iban => (&["iban", "account", "bank", "acct"], false, true),
        PiiKind::Ssn => (
            &["ssn", "social", "nino", "national_id", "nationalid"],
            false,
            true,
        ),
        PiiKind::Name => (
            &[
                "name",
                "firstname",
                "lastname",
                "surname",
                "givenname",
                "familyname",
                "fullname",
                "fname",
                "lname",
            ],
            true,
            false,
        ),
        PiiKind::Gender => (&["gender", "sex"], false, true),
        PiiKind::Country => (&["country", "nation", "nationality"], true, false),
        PiiKind::BirthDate => (
            &["birth", "birthday", "birthdate", "dob", "dateofbirth"],
            true,
            true,
        ),
        PiiKind::PostalCode => (&["zip", "zipcode", "postal", "postcode", "plz"], true, true),
        PiiKind::Address => (&["address", "street", "addr", "strasse"], true, false),
    };
    KindSpec {
        keywords,
        require_name,
        has_value,
    }
}

struct Patterns {
    email: Regex,
    ip: Regex,
    cc: Regex,
    iban: Regex,
    ssn: Regex,
    date: Regex,
    postal: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        // Unanchored so a value embedded in free text (e.g. a comment) still
        // counts as a match.
        email: Regex::new(r"(?i)\b[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}\b").unwrap(),
        ip: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
        cc: Regex::new(r"\b\d{4}[ -]?\d{4}[ -]?\d{4}[ -]?\d{1,4}\b").unwrap(),
        iban: Regex::new(r"\b[A-Z]{2}\d{2}[A-Z0-9]{10,30}\b").unwrap(),
        ssn: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
        date: Regex::new(
            r"^\d{4}-\d{2}-\d{2}$|^\d{1,2}[./]\d{1,2}[./]\d{2,4}$|^\d{4}/\d{2}/\d{2}$",
        )
        .unwrap(),
        postal: Regex::new(r"^[A-Za-z0-9][A-Za-z0-9 -]{2,9}$").unwrap(),
    })
}

/// German + English gender tokens (the audience includes non-English locales).
const GENDER_TOKENS: &[&str] = &[
    "m",
    "f",
    "w",
    "d",
    "male",
    "female",
    "man",
    "woman",
    "männlich",
    "maennlich",
    "weiblich",
    "divers",
    "nonbinary",
    "non-binary",
    "nb",
    "other",
    "genderqueer",
    "trans",
];

/// Whether one cell value matches the kind's value pattern.
fn value_matches(kind: PiiKind, raw: &str) -> bool {
    let s = raw.trim();
    if s.is_empty() {
        return false;
    }
    let p = patterns();
    match kind {
        PiiKind::Email => p.email.is_match(s),
        PiiKind::Ip => p.ip.is_match(s),
        PiiKind::CreditCard => p.cc.is_match(s),
        PiiKind::Iban => p.iban.is_match(s),
        PiiKind::Ssn => p.ssn.is_match(s),
        PiiKind::BirthDate => p.date.is_match(s),
        PiiKind::PostalCode => p.postal.is_match(s) && s.chars().any(|c| c.is_ascii_digit()),
        PiiKind::Phone => is_phone(s),
        PiiKind::Gender => GENDER_TOKENS.contains(&s.to_ascii_lowercase().as_str()),
        // No value pattern.
        PiiKind::Name | PiiKind::Country | PiiKind::Address => false,
    }
}

/// Phone heuristic: a run of digits with phone punctuation, requiring either a
/// separator/`+` or a plausible phone length (10-15 digits). This keeps bare
/// numeric columns like salaries or 8-digit IDs from registering as phones.
fn is_phone(s: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^\+?[\d][\d\s().-]{6,}\d$").unwrap());
    if !re.is_match(s) {
        return false;
    }
    // A date like "1990-04-12" also matches the digit/separator shape; don't
    // claim it as a phone number.
    if patterns().date.is_match(s) {
        return false;
    }
    let digits = s.chars().filter(|c| c.is_ascii_digit()).count();
    let has_sep = s
        .chars()
        .any(|c| matches!(c, '+' | '-' | '(' | ')' | ' ' | '.'));
    has_sep || (10..=15).contains(&digits)
}

/// Whether the column header matches any of the kind's keywords.
fn name_hit(keywords: &[&str], header: &str) -> bool {
    let lower = header.to_ascii_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let squashed: String = lower
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    keywords.iter().any(|kw| {
        if kw.len() <= 3 {
            tokens.contains(kw)
        } else {
            tokens.contains(kw) || squashed.contains(kw)
        }
    })
}

fn confidence(by_name: bool, value_match: f64) -> f64 {
    if value_match >= 0.6 {
        (value_match + if by_name { 0.2 } else { 0.0 }).min(1.0)
    } else if by_name {
        0.6 + 0.4 * value_match
    } else {
        value_match
    }
}

/// Scan every column; report the best-scoring PII kind (confidence >= 0.5) per
/// column. `sample_rows` caps how many non-empty cells are inspected per column.
pub fn scan_pii(table: &DataTable, sample_rows: usize) -> Vec<ColumnPii> {
    let mut out = Vec::new();
    for col in 0..table.col_count() {
        let header = table
            .columns
            .get(col)
            .map(|c| c.name.as_str())
            .unwrap_or("");
        let cells: Vec<String> = (0..table.row_count())
            .filter_map(|r| match table.get(r, col) {
                Some(CellValue::Null) | None => None,
                Some(v) => {
                    let s = v.to_string();
                    if s.trim().is_empty() { None } else { Some(s) }
                }
            })
            .take(sample_rows)
            .collect();

        let mut best: Option<ColumnPii> = None;
        for kind in ALL_KINDS {
            let sp = spec(kind);
            let by_name = name_hit(sp.keywords, header);
            if sp.require_name && !by_name {
                continue;
            }
            let value_match = if sp.has_value && !cells.is_empty() {
                cells.iter().filter(|c| value_matches(kind, c)).count() as f64 / cells.len() as f64
            } else {
                0.0
            };
            let conf = confidence(by_name, value_match);
            if conf >= 0.5 && best.as_ref().map(|b| conf > b.confidence).unwrap_or(true) {
                best = Some(ColumnPii {
                    column: col,
                    kind,
                    confidence: conf,
                    by_name,
                    value_match,
                });
            }
        }
        if let Some(b) = best {
            out.push(b);
        }
    }
    out
}

/// Map findings to default anonymise rules (all kinds -> full Hash).
pub fn suggested_anon_rules(findings: &[ColumnPii]) -> Vec<AnonRule> {
    findings
        .iter()
        .map(|f| AnonRule {
            columns: vec![f.column],
            strategy: AnonStrategy::Hash {
                algo: HashAlgo::default(),
                length: None,
            },
            new_column: None,
        })
        .collect()
}

#[cfg(test)]
#[path = "pii_tests.rs"]
mod tests;
