//! Test data: new rows shaped like a real table, so the shape can be shared
//! without the data.
//!
//! [`profile_table`] looks at each column once and picks a [`Generator`]; the
//! user can override any of them; [`generate`] then draws as many rows as
//! asked, deterministically for a given seed. Nothing here copies a real row:
//!
//! - numbers, dates and datetimes follow the real **spread** (drawn from the
//!   real percentiles, so a skewed column stays skewed), not only the range;
//! - personal data the PII scanner recognises becomes fake data of the same
//!   kind (a generated IBAN or card number even passes its check digit);
//! - a low-cardinality column keeps its real values at their real
//!   frequencies, the one place real values appear, and the dialog offers
//!   to rename them;
//! - code-like text keeps its shape (`ORD-00123` -> `ORD-48213`);
//! - empty cells come back at the real rate.
//!
//! Tables that link (an `orders.customer_id` pointing at `customers.id`)
//! keep linking: [`suggest_links`] finds the pairs with the Join key finder,
//! and a child column then draws only from the parent's **generated** keys.
//!
//! ponytail: every column is drawn on its own, so correlations between
//! columns (big orders from big customers) are not kept. Upgrade path:
//! sample whole rows per category.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::data::id_checks::IdKind;
use crate::data::pii::{PiiKind, scan_pii};
use crate::data::transform::anonymize::{
    FAKE_CITIES, FAKE_COMPANIES, FAKE_DOMAINS, FAKE_LOCALPARTS, FAKE_NAMES,
};
use crate::data::{CellValue, ColumnInfo, DataTable, is_numeric_data_type};

/// A column with at most this many distinct values, each seen at least
/// twice, is treated as a category.
pub const CATEGORY_MAX: usize = 50;
/// Rows looked at per column when profiling.
const SAMPLE: usize = 100_000;
/// Percentiles kept per numeric / date column (0, 1, ..., 100).
const QUANTILES: usize = 101;

/// Kinds of fake value, for columns the PII scanner (or the column name)
/// recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fake {
    Name,
    Email,
    City,
    Company,
    Address,
    Phone,
    Iban,
    CardNumber,
    Ip,
    Uuid,
}

impl Fake {
    pub const ALL: [Fake; 10] = [
        Fake::Name,
        Fake::Email,
        Fake::City,
        Fake::Company,
        Fake::Address,
        Fake::Phone,
        Fake::Iban,
        Fake::CardNumber,
        Fake::Ip,
        Fake::Uuid,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Fake::Name => "fake_name",
            Fake::Email => "fake_email",
            Fake::City => "fake_city",
            Fake::Company => "fake_company",
            Fake::Address => "fake_address",
            Fake::Phone => "fake_phone",
            Fake::Iban => "fake_iban",
            Fake::CardNumber => "fake_card_number",
            Fake::Ip => "fake_ip",
            Fake::Uuid => "fake_uuid",
        }
    }
}

/// How one column's values are made.
#[derive(Debug, Clone, PartialEq)]
pub enum Generator {
    /// 1, 2, 3, ... (unique whole numbers: IDs).
    RunningNumber,
    /// Drawn from the real percentiles. `whole` writes integers.
    Number {
        quantiles: Vec<f64>,
        decimals: u32,
        whole: bool,
    },
    /// Days since 0001-01-01, drawn from the real percentiles.
    Date {
        quantiles: Vec<f64>,
    },
    /// Seconds since 1970, drawn from the real percentiles.
    DateTime {
        quantiles: Vec<f64>,
    },
    Bool {
        true_share: f64,
    },
    /// These values at these relative frequencies.
    Category {
        values: Vec<(String, usize)>,
    },
    Fake(Fake),
    /// The real values' shape: `prefix` kept, then `9` a digit, `A` / `a` a
    /// letter of that case, anything else literal. One shape per entry with
    /// its frequency.
    Pattern {
        prefix: String,
        shapes: Vec<(String, usize)>,
    },
    /// Neutral words, `min_len..=max_len` characters.
    Text {
        min_len: usize,
        max_len: usize,
    },
    /// Values of another generated column (`table`, `column` index into the
    /// plans passed to [`generate`]).
    Link {
        table: usize,
        column: usize,
    },
    /// Every cell empty.
    Empty,
}

impl Generator {
    /// Stable snake_case name (CLI, MCP, i18n `testdata.gen_<id>`).
    pub fn id(&self) -> &'static str {
        match self {
            Generator::RunningNumber => "running_number",
            Generator::Number { .. } => "number",
            Generator::Date { .. } => "date",
            Generator::DateTime { .. } => "datetime",
            Generator::Bool { .. } => "bool",
            Generator::Category { .. } => "category",
            Generator::Fake(f) => f.id(),
            Generator::Pattern { .. } => "pattern",
            Generator::Text { .. } => "text",
            Generator::Link { .. } => "link",
            Generator::Empty => "empty",
        }
    }

    /// Whether any real value goes into the output as it is. Only real
    /// categories do; the dialog flags them.
    pub fn keeps_real_values(&self) -> bool {
        matches!(self, Generator::Category { values } if !is_renamed(values))
    }
}

/// One column of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnPlan {
    pub name: String,
    pub data_type: String,
    pub generator: Generator,
    /// Share of empty cells, 0.0 to 1.0.
    pub null_share: f64,
    /// Every non-empty value different in the real column; kept so.
    pub unique: bool,
    /// Other generators that make sense for this column, detected one first.
    pub alternatives: Vec<Generator>,
}

/// One table to generate.
#[derive(Debug, Clone, PartialEq)]
pub struct TablePlan {
    pub name: String,
    pub rows: usize,
    pub columns: Vec<ColumnPlan>,
}

/// A child column whose values must exist in a parent column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub parent: (usize, usize),
    pub child: (usize, usize),
}

// ------------------------------------------------------------------ profile

fn is_blank(v: &CellValue) -> bool {
    matches!(v, CellValue::Null) || v.to_string().trim().is_empty()
}

fn quantiles(mut xs: Vec<f64>) -> Vec<f64> {
    xs.sort_by(f64::total_cmp);
    (0..QUANTILES)
        .map(|i| {
            let pos = i as f64 / (QUANTILES - 1) as f64 * (xs.len() - 1) as f64;
            let lo = pos.floor() as usize;
            let hi = pos.ceil() as usize;
            xs[lo] + (xs[hi] - xs[lo]) * (pos - lo as f64)
        })
        .collect()
}

fn as_f64(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(f) if f.is_finite() => Some(*f),
        CellValue::Bool(_) => None,
        other => other.to_string().trim().parse::<f64>().ok(),
    }
}

fn decimals_of(v: &CellValue) -> u32 {
    let s = v.to_string();
    s.split_once('.')
        .map(|(_, f)| f.trim_end_matches('0').len().min(6) as u32)
        .unwrap_or(0)
}

const DATETIME_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%dT%H:%M",
];

pub(crate) fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

pub(crate) fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    DATETIME_FORMATS
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
        .or_else(|| parse_date(s).and_then(|d| d.and_hms_opt(0, 0, 0)))
}

/// Shape of a value: digits -> `9`, letters -> `A` / `a`, rest literal.
fn shape(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                '9'
            } else if c.is_uppercase() {
                'A'
            } else if c.is_lowercase() {
                'a'
            } else {
                c
            }
        })
        .collect()
}

fn common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    let mut n = first.chars().count();
    for v in &values[1..] {
        n = n.min(
            first
                .chars()
                .zip(v.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    // Keep a literal prefix only up to its last non-digit, so `ORD-001`
    // and `ORD-002` share `ORD-`, not `ORD-00` (which would cap the range).
    let p: String = first.chars().take(n.min(8)).collect();
    let keep = p
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    p[..keep].to_string()
}

fn by_name(name: &str) -> Option<Fake> {
    let n = name.to_lowercase();
    if n.contains("city") || n.contains("town") || n.contains("stadt") {
        Some(Fake::City)
    } else if n.contains("company") || n.contains("firm") || n.contains("supplier") {
        Some(Fake::Company)
    } else if n.contains("uuid") || n.contains("guid") {
        Some(Fake::Uuid)
    } else {
        None
    }
}

fn fake_for(kind: PiiKind) -> Option<Fake> {
    Some(match kind {
        PiiKind::Email => Fake::Email,
        PiiKind::Phone => Fake::Phone,
        PiiKind::Ip => Fake::Ip,
        PiiKind::CreditCard => Fake::CardNumber,
        PiiKind::Iban => Fake::Iban,
        PiiKind::Name => Fake::Name,
        PiiKind::Address => Fake::Address,
        // Categories (gender, country), dates and codes (postal code, SSN)
        // are better served by the value-shaped generators below.
        _ => return None,
    })
}

fn is_renamed(values: &[(String, usize)]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(i, (v, _))| *v == format!("value_{}", i + 1))
}

/// The same frequencies with the real values replaced by `value_1`, ...
pub fn renamed_category(values: &[(String, usize)]) -> Generator {
    Generator::Category {
        values: values
            .iter()
            .enumerate()
            .map(|(i, (_, n))| (format!("value_{}", i + 1), *n))
            .collect(),
    }
}

/// Pick a generator for every column of `table`.
pub fn profile_table(table: &DataTable, name: &str) -> TablePlan {
    let pii: HashMap<usize, PiiKind> = scan_pii(table, 500)
        .into_iter()
        .map(|p| (p.column, p.kind))
        .collect();
    let rows = table.row_count();
    let columns = table
        .columns
        .iter()
        .enumerate()
        .map(|(c, info)| profile_column(table, c, info, pii.get(&c).copied(), rows))
        .collect();
    TablePlan {
        name: name.to_string(),
        rows,
        columns,
    }
}

fn profile_column(
    table: &DataTable,
    c: usize,
    info: &ColumnInfo,
    pii: Option<PiiKind>,
    rows: usize,
) -> ColumnPlan {
    let step = (rows / SAMPLE).max(1);
    let cells: Vec<CellValue> = (0..rows)
        .step_by(step)
        .filter_map(|r| table.get(r, c).cloned())
        .collect();
    let seen = cells.len().max(1);
    let values: Vec<&CellValue> = cells.iter().filter(|v| !is_blank(v)).collect();
    let null_share = 1.0 - values.len() as f64 / seen as f64;
    let texts: Vec<String> = values.iter().map(|v| v.to_string()).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for t in &texts {
        *counts.entry(t.as_str()).or_default() += 1;
    }
    let unique = !texts.is_empty() && counts.len() == texts.len();
    let ty = info.data_type.to_ascii_lowercase();

    let mut category: Vec<(String, usize)> =
        counts.iter().map(|(v, n)| (v.to_string(), *n)).collect();
    category.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    // Few distinct values that repeat. A rare one among them is still kept
    // (a status seen once); names and the like are caught by the PII scan
    // before this is consulted.
    let is_category =
        !texts.is_empty() && counts.len() <= CATEGORY_MAX && counts.len() * 2 <= texts.len();

    let mut alts: Vec<Generator> = Vec::new();
    let detected = if values.is_empty() {
        Generator::Empty
    } else if ty.contains("bool") {
        let t = values
            .iter()
            .filter(|v| matches!(v, CellValue::Bool(true)) || v.to_string() == "true")
            .count();
        Generator::Bool {
            true_share: t as f64 / values.len() as f64,
        }
    } else if ty.contains("timestamp") || ty.contains("datetime") {
        let secs: Vec<f64> = texts
            .iter()
            .filter_map(|s| parse_datetime(s))
            .map(|d| d.and_utc().timestamp() as f64)
            .collect();
        if secs.is_empty() {
            text_generator(&texts)
        } else {
            Generator::DateTime {
                quantiles: quantiles(secs),
            }
        }
    } else if ty.contains("date") {
        let days: Vec<f64> = texts
            .iter()
            .filter_map(|s| parse_date(s).or_else(|| parse_datetime(s).map(|d| d.date())))
            .map(|d| d.num_days_from_ce() as f64)
            .collect();
        if days.is_empty() {
            text_generator(&texts)
        } else {
            Generator::Date {
                quantiles: quantiles(days),
            }
        }
    } else if is_numeric_data_type(&info.data_type) {
        let xs: Vec<f64> = values.iter().filter_map(|v| as_f64(v)).collect();
        let whole = xs.iter().all(|x| x.fract() == 0.0);
        if unique && whole {
            alts.push(Generator::Number {
                quantiles: quantiles(xs.clone()),
                decimals: 0,
                whole,
            });
            Generator::RunningNumber
        } else if xs.is_empty() {
            Generator::Empty
        } else {
            Generator::Number {
                quantiles: quantiles(xs),
                decimals: values.iter().map(|v| decimals_of(v)).max().unwrap_or(0),
                whole,
            }
        }
    } else if let Some(g) = dates_in_text(&texts) {
        g
    } else if let Some(f) = pii.and_then(fake_for).or_else(|| by_name(&info.name)) {
        Generator::Fake(f)
    } else if is_category {
        Generator::Category {
            values: category.clone(),
        }
    } else {
        text_generator(&texts)
    };

    if is_category && !matches!(detected, Generator::Category { .. }) {
        alts.push(Generator::Category {
            values: category.clone(),
        });
    }
    if is_category {
        alts.push(renamed_category(&category));
    }
    if !texts.is_empty() && !matches!(detected, Generator::Pattern { .. }) {
        alts.push(pattern_generator(&texts));
    }
    if !matches!(detected, Generator::Text { .. }) {
        alts.push(text_generator_words(&texts));
    }
    alts.extend(Fake::ALL.map(Generator::Fake));
    alts.push(Generator::Empty);
    let mut alternatives = vec![detected.clone()];
    for a in alts {
        if !alternatives.contains(&a) {
            alternatives.push(a);
        }
    }

    ColumnPlan {
        name: info.name.clone(),
        data_type: info.data_type.clone(),
        generator: detected,
        null_share,
        unique,
        alternatives,
    }
}

/// A text column whose values are nearly all dates or datetimes (a CSV
/// column the reader left as text) is generated as one.
fn dates_in_text(texts: &[String]) -> Option<Generator> {
    let enough = |n: usize| n * 10 >= texts.len() * 9;
    let days: Vec<f64> = texts
        .iter()
        .filter_map(|t| parse_date(t))
        .map(|d| d.num_days_from_ce() as f64)
        .collect();
    if enough(days.len()) {
        return Some(Generator::Date {
            quantiles: quantiles(days),
        });
    }
    let secs: Vec<f64> = texts
        .iter()
        .filter_map(|t| parse_datetime(t))
        .map(|d| d.and_utc().timestamp() as f64)
        .collect();
    enough(secs.len()).then(|| Generator::DateTime {
        quantiles: quantiles(secs),
    })
}

/// Short codes without spaces that carry digits keep their shape;
/// everything else becomes neutral words of a similar length.
fn text_generator(texts: &[String]) -> Generator {
    let code_like = texts
        .iter()
        .all(|t| t.chars().count() <= 24 && !t.contains(char::is_whitespace))
        && texts
            .iter()
            .filter(|t| t.contains(|c: char| c.is_ascii_digit()))
            .count()
            * 2
            >= texts.len();
    if code_like {
        pattern_generator(texts)
    } else {
        text_generator_words(texts)
    }
}

fn pattern_generator(texts: &[String]) -> Generator {
    let prefix = common_prefix(texts);
    let mut shapes: HashMap<String, usize> = HashMap::new();
    for t in texts {
        *shapes.entry(shape(&t[prefix.len()..])).or_default() += 1;
    }
    let mut shapes: Vec<(String, usize)> = shapes.into_iter().collect();
    shapes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    shapes.truncate(20);
    Generator::Pattern { prefix, shapes }
}

fn text_generator_words(texts: &[String]) -> Generator {
    let lens = texts.iter().map(|t| t.chars().count());
    Generator::Text {
        min_len: lens.clone().min().unwrap_or(5).max(1),
        max_len: lens.max().unwrap_or(20).max(1),
    }
}

/// Parent/child column pairs across `tables`, from the Join key finder: a
/// near-unique column whose values contain every value of another table's
/// column. Each child column gets at most its best parent.
pub fn suggest_links(tables: &[&DataTable]) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    for k in crate::data::join_keys::suggest_keys(tables, 10_000) {
        if k.left.0 == k.right.0 || k.overlap < 0.8 {
            continue;
        }
        // Two running numbers (`customers.id` 1..200 inside `orders.order_id`
        // 1..500) look like a link and are not one. With both sides unique
        // there is no telling parent from child, so no link is guessed.
        // ponytail: a real 1:1 link is missed too; the user can still pick
        // it in the dialog.
        if k.left_distinct >= 0.99 && k.right_distinct >= 0.99 {
            continue;
        }
        // The parent is the side whose values are all different; the child
        // is the other side, nearly every one of whose values must be found.
        // Real data has the odd orphan (an order for a deleted customer), so
        // one in ten may miss.
        let few = |orphans: usize, values: usize| orphans <= (values / 10).max(1);
        let link = if k.right_distinct >= 0.99 && few(k.left_orphans, k.left_values) {
            Link {
                parent: k.right,
                child: k.left,
            }
        } else if k.left_distinct >= 0.99 && few(k.right_orphans, k.right_values) {
            Link {
                parent: k.left,
                child: k.right,
            }
        } else {
            continue;
        };
        if taken.insert(link.child) {
            out.push(link);
        }
    }
    out
}

/// Point each link's child column at its parent.
pub fn apply_links(plans: &mut [TablePlan], links: &[Link]) {
    for l in links {
        if let Some(col) = plans
            .get_mut(l.child.0)
            .and_then(|p| p.columns.get_mut(l.child.1))
        {
            let g = Generator::Link {
                table: l.parent.0,
                column: l.parent.1,
            };
            if !col.alternatives.contains(&g) {
                col.alternatives.insert(0, g.clone());
            }
            col.generator = g;
        }
    }
}

// ------------------------------------------------------------------ generate

const WORDS: &[&str] = &[
    "alpha", "bravo", "cedar", "delta", "ember", "fable", "grove", "harbor", "iris", "juniper",
    "kelp", "lumen", "maple", "nova", "orbit", "pebble", "quartz", "river", "slate", "timber",
    "umber", "vale", "willow", "yarrow", "zephyr", "amber", "birch", "comet", "dune", "fern",
];

fn pick<'a>(rng: &mut StdRng, pool: &[&'a str]) -> &'a str {
    pool[rng.random_range(0..pool.len())]
}

fn from_quantiles(rng: &mut StdRng, q: &[f64]) -> f64 {
    let pos = rng.random::<f64>() * (q.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(q.len() - 1);
    q[lo] + (q[hi] - q[lo]) * (pos - lo as f64)
}

fn weighted<'a>(rng: &mut StdRng, items: &'a [(String, usize)]) -> &'a str {
    let total: usize = items.iter().map(|(_, n)| n).sum();
    let mut x = rng.random_range(0..total.max(1));
    for (v, n) in items {
        if x < *n {
            return v;
        }
        x -= n;
    }
    &items[0].0
}

fn digits(rng: &mut StdRng, n: usize) -> String {
    (0..n)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect()
}

fn fake_value(rng: &mut StdRng, f: Fake) -> String {
    match f {
        Fake::Name => {
            let first = pick(rng, FAKE_NAMES).split(' ').next().unwrap_or("Alex");
            let last = pick(rng, FAKE_NAMES).split(' ').nth(1).unwrap_or("Lee");
            format!("{first} {last}")
        }
        Fake::Email => format!(
            "{}.{}{}@{}",
            pick(rng, FAKE_LOCALPARTS),
            pick(rng, FAKE_LOCALPARTS),
            rng.random_range(1..1000),
            pick(rng, FAKE_DOMAINS)
        ),
        Fake::City => pick(rng, FAKE_CITIES).to_string(),
        Fake::Company => pick(rng, FAKE_COMPANIES).to_string(),
        Fake::Address => {
            let mut street = pick(rng, WORDS).to_string();
            street[..1].make_ascii_uppercase();
            format!("{} {street} Street", rng.random_range(1..300))
        }
        // UK Ofcom's range reserved for drama: 020 7946 0000 to 0999.
        Fake::Phone => format!("+44 20 7946 0{}", digits(rng, 3)),
        Fake::Iban => {
            let bban = digits(rng, 18);
            let check = 98 - crate::data::id_checks::mod97_of(&format!("{bban}DE00"));
            let iban = format!("DE{check:02}{bban}");
            debug_assert!(IdKind::Iban.check(&iban));
            iban
        }
        Fake::CardNumber => {
            // 4000 00: a test-card prefix, never issued.
            let body = format!("400000{}", digits(rng, 9));
            let check = crate::data::id_checks::luhn_check_digit(&body);
            format!("{body}{check}")
        }
        // RFC 5737 documentation ranges.
        Fake::Ip => format!(
            "{}.{}",
            pick(rng, &["192.0.2", "198.51.100", "203.0.113"]),
            rng.random_range(1..255)
        ),
        Fake::Uuid => {
            let h: String = (0..32)
                .map(|_| char::from_digit(rng.random_range(0..16), 16).unwrap_or('0'))
                .collect();
            format!(
                "{}-{}-4{}-a{}-{}",
                &h[..8],
                &h[8..12],
                &h[13..16],
                &h[17..20],
                &h[20..32]
            )
        }
    }
}

fn pattern_value(rng: &mut StdRng, prefix: &str, shape: &str) -> String {
    let mut s = prefix.to_string();
    for c in shape.chars() {
        s.push(match c {
            '9' => char::from(b'0' + rng.random_range(0..10u8)),
            'A' => char::from(b'A' + rng.random_range(0..26u8)),
            'a' => char::from(b'a' + rng.random_range(0..26u8)),
            other => other,
        });
    }
    s
}

fn text_value(rng: &mut StdRng, min_len: usize, max_len: usize) -> String {
    let target = rng.random_range(min_len..=max_len.max(min_len));
    let mut s = String::new();
    while s.chars().count() < target {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(pick(rng, WORDS));
    }
    s.chars()
        .take(target)
        .collect::<String>()
        .trim()
        .to_string()
}

fn value(rng: &mut StdRng, g: &Generator, row: usize) -> CellValue {
    match g {
        Generator::RunningNumber => CellValue::Int(row as i64 + 1),
        Generator::Number {
            quantiles,
            decimals,
            whole,
        } => {
            let x = from_quantiles(rng, quantiles);
            if *whole {
                CellValue::Int(x.round() as i64)
            } else {
                let p = 10f64.powi(*decimals as i32);
                CellValue::Float((x * p).round() / p)
            }
        }
        Generator::Date { quantiles } => {
            let d = from_quantiles(rng, quantiles).round() as i32;
            NaiveDate::from_num_days_from_ce_opt(d)
                .map(|d| CellValue::Date(d.format("%Y-%m-%d").to_string()))
                .unwrap_or(CellValue::Null)
        }
        Generator::DateTime { quantiles } => {
            let s = from_quantiles(rng, quantiles).round() as i64;
            DateTime::from_timestamp(s, 0)
                .map(|d| CellValue::DateTime(d.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()))
                .unwrap_or(CellValue::Null)
        }
        Generator::Bool { true_share } => CellValue::Bool(rng.random::<f64>() < *true_share),
        Generator::Category { values } => CellValue::String(weighted(rng, values).to_string()),
        Generator::Fake(f) => CellValue::String(fake_value(rng, *f)),
        Generator::Pattern { prefix, shapes } => {
            let shape = weighted(rng, shapes).to_string();
            CellValue::String(pattern_value(rng, prefix, &shape))
        }
        Generator::Text { min_len, max_len } => {
            CellValue::String(text_value(rng, *min_len, *max_len))
        }
        Generator::Link { .. } | Generator::Empty => CellValue::Null,
    }
}

/// `_2`, `_3` ... after repeats, so a unique column stays unique.
fn make_unique(cells: &mut [CellValue]) {
    let mut seen: HashSet<String> = HashSet::new();
    for c in cells.iter_mut() {
        if is_blank(c) {
            continue;
        }
        let base = c.to_string();
        if seen.insert(base.clone()) {
            continue;
        }
        let v = (2..)
            .map(|n| format!("{base}_{n}"))
            .find(|v| !seen.contains(v))
            .expect("an unbounded range always finds a free name");
        seen.insert(v.clone());
        *c = CellValue::String(v);
    }
}

fn output_type(col: &ColumnPlan) -> String {
    match &col.generator {
        Generator::RunningNumber => "Int64".into(),
        Generator::Number { whole: true, .. } => "Int64".into(),
        Generator::Number { .. } => "Float64".into(),
        Generator::Date { .. } => "Date32".into(),
        Generator::DateTime { .. } => "Timestamp(Microsecond, None)".into(),
        Generator::Bool { .. } => "Boolean".into(),
        Generator::Link { .. } => col.data_type.clone(),
        _ => "Utf8".into(),
    }
}

/// Generate every plan, parents before the children that link to them.
/// Deterministic for a given `seed`.
pub fn generate(plans: &[TablePlan], seed: u64) -> anyhow::Result<Vec<DataTable>> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out: Vec<Option<Vec<Vec<CellValue>>>> = vec![None; plans.len()];
    // Columns by table, filled in as they are generated.
    let mut cols: Vec<Vec<Option<Vec<CellValue>>>> =
        plans.iter().map(|p| vec![None; p.columns.len()]).collect();

    // Keep sweeping until every column is done; a sweep that finishes
    // nothing means the links form a loop.
    loop {
        let mut progress = false;
        for (t, plan) in plans.iter().enumerate() {
            for (c, col) in plan.columns.iter().enumerate() {
                if cols[t][c].is_some() {
                    continue;
                }
                let mut cells: Vec<CellValue> = match &col.generator {
                    Generator::Link { table, column } => {
                        let Some(parent) = cols.get(*table).and_then(|tc| tc.get(*column)) else {
                            anyhow::bail!(
                                "column `{}` of `{}` links to a column that is not being generated",
                                col.name,
                                plan.name
                            );
                        };
                        let Some(parent) = parent.as_ref() else {
                            continue;
                        };
                        let pool: Vec<CellValue> =
                            parent.iter().filter(|v| !is_blank(v)).cloned().collect();
                        if pool.is_empty() {
                            vec![CellValue::Null; plan.rows]
                        } else if col.unique {
                            // A unique child (1:1) takes distinct parent
                            // values, in a shuffled order.
                            let mut p = pool;
                            for i in (1..p.len()).rev() {
                                p.swap(i, rng.random_range(0..=i));
                            }
                            (0..plan.rows).map(|r| p[r % p.len()].clone()).collect()
                        } else {
                            (0..plan.rows)
                                .map(|_| pool[rng.random_range(0..pool.len())].clone())
                                .collect()
                        }
                    }
                    g if col.unique => {
                        // Redraw a repeat a few times before falling back to
                        // a `_2` suffix, which would break a code's shape.
                        let mut seen: HashSet<String> = HashSet::new();
                        (0..plan.rows)
                            .map(|r| {
                                let mut v = value(&mut rng, g, r);
                                for _ in 0..20 {
                                    if seen.insert(v.to_string()) {
                                        break;
                                    }
                                    v = value(&mut rng, g, r);
                                }
                                v
                            })
                            .collect()
                    }
                    g => (0..plan.rows).map(|r| value(&mut rng, g, r)).collect(),
                };
                if col.null_share > 0.0 && !matches!(col.generator, Generator::RunningNumber) {
                    for cell in cells.iter_mut() {
                        if rng.random::<f64>() < col.null_share {
                            *cell = CellValue::Null;
                        }
                    }
                }
                if col.unique && !matches!(col.generator, Generator::Link { .. }) {
                    make_unique(&mut cells);
                }
                cols[t][c] = Some(cells);
                progress = true;
            }
        }
        if cols.iter().all(|tc| tc.iter().all(Option::is_some)) {
            break;
        }
        if !progress {
            anyhow::bail!("the links between these tables go round in a circle");
        }
    }

    for (t, plan) in plans.iter().enumerate() {
        let columns: Vec<Vec<CellValue>> = cols[t]
            .iter_mut()
            .map(|c| c.take().unwrap_or_default())
            .collect();
        out[t] = Some(
            (0..plan.rows)
                .map(|r| columns.iter().map(|c| c[r].clone()).collect())
                .collect(),
        );
    }

    Ok(plans
        .iter()
        .zip(out)
        .map(|(plan, rows)| {
            let mut t = DataTable::empty();
            t.columns = plan
                .columns
                .iter()
                .map(|c| ColumnInfo {
                    name: c.name.clone(),
                    data_type: output_type(c),
                })
                .collect();
            t.rows = rows.unwrap_or_default();
            t
        })
        .collect())
}

/// One row per column: what made it, for the CLI / MCP report and the
/// dialog's summary.
pub fn plan_table(plans: &[TablePlan]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [
        "table",
        "column",
        "generator",
        "empty_share",
        "real_values_kept",
    ]
    .iter()
    .map(|n| ColumnInfo {
        name: n.to_string(),
        data_type: "Utf8".into(),
    })
    .collect();
    for p in plans {
        for c in &p.columns {
            let generator = match &c.generator {
                Generator::Link { table, column } => format!(
                    "link to {}.{}",
                    plans.get(*table).map(|p| p.name.as_str()).unwrap_or("?"),
                    plans
                        .get(*table)
                        .and_then(|p| p.columns.get(*column))
                        .map(|c| c.name.as_str())
                        .unwrap_or("?")
                ),
                g => g.id().to_string(),
            };
            t.rows.push(vec![
                CellValue::String(p.name.clone()),
                CellValue::String(c.name.clone()),
                CellValue::String(generator),
                CellValue::String(format!("{:.2}", c.null_share)),
                CellValue::String(
                    if c.generator.keeps_real_values() {
                        "yes"
                    } else {
                        "no"
                    }
                    .into(),
                ),
            ]);
        }
    }
    t
}

/// A seed for "no seed given": the current time, printed so a run can be
/// repeated.
pub fn clock_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "test_data_tests.rs"]
mod tests;
