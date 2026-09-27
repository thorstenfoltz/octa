//! Is this ID correctly built? IBANs, card numbers, barcodes (EAN / UPC /
//! ISBN), EU VAT numbers and email addresses.
//!
//! Every check here is arithmetic on the value itself: nothing is looked up
//! and nothing leaves the machine. So "valid" means **correctly built**, not
//! "exists": a well-formed IBAN can still belong to a closed account, and a
//! VAT number can pass its check digit without being registered (only the
//! EU's online VIES service knows that, and asking it would send the data
//! out).
//!
//! A failed check is information, never a gate. The GUI paints the cell red
//! through Data validation and the user decides what happens next.
//!
//! Phone numbers are deliberately absent: telling a real number from a
//! plausible one needs every country's numbering plan (a large dataset that
//! changes), and a half-right check would paint good numbers red.

use regex::Regex;
use std::sync::OnceLock;

/// The kinds of ID Octa can check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdKind {
    Iban,
    CardNumber,
    /// EAN-8, EAN-13, UPC-A, GTIN-14, ISBN-13 and ISBN-10.
    Gtin,
    /// EU VAT number with its country prefix (`DE`, `FR`, `EL` for Greece).
    VatId,
    Email,
}

impl IdKind {
    pub const ALL: [IdKind; 5] = [
        IdKind::Iban,
        IdKind::CardNumber,
        IdKind::Gtin,
        IdKind::VatId,
        IdKind::Email,
    ];

    /// Stable snake_case name (rules files, recipes, CLI, MCP).
    pub fn id(self) -> &'static str {
        match self {
            IdKind::Iban => "iban",
            IdKind::CardNumber => "card_number",
            IdKind::Gtin => "gtin",
            IdKind::VatId => "vat_id",
            IdKind::Email => "email",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == s)
    }

    /// Whether `value` is a correctly built ID of this kind. Surrounding
    /// blanks and the usual grouping (spaces, dashes, dots in VAT numbers)
    /// are allowed: they are a matter of format, which [`IdKind::tidy`]
    /// fixes, not of correctness.
    pub fn check(self, value: &str) -> bool {
        match self {
            IdKind::Iban => iban_ok(value),
            IdKind::CardNumber => card_ok(value),
            IdKind::Gtin => gtin_ok(value),
            IdKind::VatId => vat_ok(value),
            IdKind::Email => email_ok(value),
        }
    }

    /// The one standard way to write a valid value, or `None` when the value
    /// is not valid (an invalid value is never rewritten, so nothing that
    /// needs a human look is disguised as tidy).
    pub fn tidy(self, value: &str) -> Option<String> {
        if !self.check(value) {
            return None;
        }
        Some(match self {
            IdKind::Iban => group(&compact(value, " -").to_ascii_uppercase(), &[4; 9]),
            IdKind::CardNumber => {
                let d = compact(value, " -");
                // Amex (15 digits, 34 / 37) is printed 4-6-5, the rest in fours.
                if d.len() == 15 && (d.starts_with("34") || d.starts_with("37")) {
                    group(&d, &[4, 6, 5])
                } else {
                    group(&d, &[4; 5])
                }
            }
            IdKind::Gtin => compact(value, " -").to_ascii_uppercase(),
            IdKind::VatId => compact(value, " -.").to_ascii_uppercase(),
            IdKind::Email => {
                let v = value.trim();
                let (local, domain) = v.rsplit_once('@').expect("checked above");
                format!("{local}@{}", domain.to_lowercase())
            }
        })
    }
}

/// `value` without surrounding blanks and without any char in `drop`.
fn compact(value: &str, drop: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| !drop.contains(*c))
        .collect()
}

/// `s` split into runs of the given sizes, joined by spaces. The last size
/// repeats if `s` is longer.
fn group(s: &str, sizes: &[usize]) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    let mut i = 0;
    while at < chars.len() {
        let n = sizes[i.min(sizes.len() - 1)];
        let end = (at + n).min(chars.len());
        out.push(chars[at..end].iter().collect::<String>());
        at = end;
        i += 1;
    }
    out.join(" ")
}

fn digits(s: &str) -> Option<Vec<u32>> {
    s.chars().map(|c| c.to_digit(10)).collect()
}

// ---------------------------------------------------------------- IBAN

/// Official IBAN lengths per country (SWIFT IBAN registry).
const IBAN_LENGTHS: &[(&str, usize)] = &[
    ("AD", 24),
    ("AE", 23),
    ("AL", 28),
    ("AT", 20),
    ("AZ", 28),
    ("BA", 20),
    ("BE", 16),
    ("BG", 22),
    ("BH", 22),
    ("BI", 27),
    ("BR", 29),
    ("BY", 28),
    ("CH", 21),
    ("CR", 22),
    ("CY", 28),
    ("CZ", 24),
    ("DE", 22),
    ("DJ", 27),
    ("DK", 18),
    ("DO", 28),
    ("EE", 20),
    ("EG", 29),
    ("ES", 24),
    ("FI", 18),
    ("FK", 18),
    ("FO", 18),
    ("FR", 27),
    ("GB", 22),
    ("GE", 22),
    ("GI", 23),
    ("GL", 18),
    ("GR", 27),
    ("GT", 28),
    ("HR", 21),
    ("HU", 28),
    ("IE", 22),
    ("IL", 23),
    ("IQ", 23),
    ("IS", 26),
    ("IT", 27),
    ("JO", 30),
    ("KW", 30),
    ("KZ", 20),
    ("LB", 28),
    ("LC", 32),
    ("LI", 21),
    ("LT", 20),
    ("LU", 20),
    ("LV", 21),
    ("LY", 25),
    ("MC", 27),
    ("MD", 24),
    ("ME", 22),
    ("MK", 19),
    ("MN", 20),
    ("MR", 27),
    ("MT", 31),
    ("MU", 30),
    ("NI", 28),
    ("NL", 18),
    ("NO", 15),
    ("OM", 23),
    ("PK", 24),
    ("PL", 28),
    ("PS", 29),
    ("PT", 25),
    ("QA", 29),
    ("RO", 24),
    ("RS", 22),
    ("RU", 33),
    ("SA", 24),
    ("SC", 31),
    ("SD", 18),
    ("SE", 24),
    ("SI", 19),
    ("SK", 24),
    ("SM", 27),
    ("SO", 23),
    ("ST", 25),
    ("SV", 28),
    ("TL", 23),
    ("TN", 24),
    ("TR", 26),
    ("UA", 29),
    ("VA", 22),
    ("VG", 24),
    ("XK", 20),
    ("YE", 30),
];

/// `s` read as a number (letters A=10 .. Z=35, as ISO 7064 does) mod 97,
/// one char at a time so no big integer is needed.
fn mod97(s: &str) -> Option<u32> {
    let mut r = 0u32;
    for c in s.chars() {
        let v = c.to_digit(36)?;
        r = if v >= 10 {
            (r * 100 + v) % 97
        } else {
            (r * 10 + v) % 97
        };
    }
    Some(r)
}

/// `s` mod 97 as ISO 7064 reads it; for building a check digit.
pub(crate) fn mod97_of(s: &str) -> u32 {
    mod97(s).unwrap_or(0)
}

pub fn iban_ok(value: &str) -> bool {
    let s = compact(value, " -").to_ascii_uppercase();
    let b = s.as_bytes();
    if b.len() < 5 || !b[..2].iter().all(u8::is_ascii_uppercase) {
        return false;
    }
    if !b[2..4].iter().all(u8::is_ascii_digit) || !b.iter().all(u8::is_ascii_alphanumeric) {
        return false;
    }
    // A country Octa does not know yet (the registry grows) still gets the
    // checksum and the general length bounds, rather than being painted red.
    let len_ok = match IBAN_LENGTHS.iter().find(|(c, _)| *c == &s[..2]) {
        Some(&(_, n)) => b.len() == n,
        None => (15..=34).contains(&b.len()),
    };
    len_ok && mod97(&format!("{}{}", &s[4..], &s[..4])) == Some(1)
}

// ---------------------------------------------------------------- cards

/// Luhn over `d`, the digit order as written (check digit last).
fn luhn(d: &[u32]) -> bool {
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &n)| {
            if i % 2 == 1 {
                let x = n * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                n
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// The digit that makes `body` pass Luhn once appended.
pub(crate) fn luhn_check_digit(body: &str) -> u32 {
    (0..10)
        .find(|d| digits(&format!("{body}{d}")).is_some_and(|v| luhn(&v)))
        .unwrap_or(0)
}

pub fn card_ok(value: &str) -> bool {
    let s = compact(value, " -");
    match digits(&s) {
        Some(d) if (12..=19).contains(&d.len()) => luhn(&d),
        _ => false,
    }
}

// ---------------------------------------------------------------- barcodes

pub fn gtin_ok(value: &str) -> bool {
    let s = compact(value, " -").to_ascii_uppercase();
    if s.len() == 10 {
        return isbn10_ok(&s);
    }
    let Some(d) = digits(&s) else { return false };
    if ![8, 12, 13, 14].contains(&d.len()) {
        return false;
    }
    // Weights 3,1,3,1,... counted from the digit next to the check digit.
    let (body, check) = d.split_at(d.len() - 1);
    let sum: u32 = body
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &n)| if i % 2 == 0 { n * 3 } else { n })
        .sum();
    (10 - sum % 10) % 10 == check[0]
}

fn isbn10_ok(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() != 10 {
        return false;
    }
    let mut sum = 0;
    for (i, c) in chars.iter().enumerate() {
        let v = match c {
            'X' if i == 9 => 10,
            _ => match c.to_digit(10) {
                Some(v) => v,
                None => return false,
            },
        };
        sum += v * (10 - i as u32);
    }
    sum.is_multiple_of(11)
}

// ---------------------------------------------------------------- VAT

/// ISO 7064 MOD 11,10 (Germany, Croatia): `d` includes the check digit.
fn mod11_10(d: &[u32]) -> bool {
    let (body, check) = d.split_at(d.len() - 1);
    let mut p = 10;
    for &n in body {
        let s = (n + p) % 10;
        p = (if s == 0 { 10 } else { s } * 2) % 11;
    }
    (11 - p) % 10 == check[0]
}

fn weighted(d: &[u32], w: &[u32]) -> u32 {
    d.iter().zip(w).map(|(a, b)| a * b).sum()
}

/// Per-country body check. `None` = the country has no check digit Octa
/// knows (format only), `Some(ok)` otherwise.
fn vat_check(country: &str, body: &str) -> Option<bool> {
    let d = digits(body);
    Some(match country {
        "AT" => {
            let d = digits(body.strip_prefix('U')?)?;
            let mut s = 0;
            for (i, &n) in d[..7].iter().enumerate() {
                s += if i % 2 == 1 {
                    (n * 2) / 10 + (n * 2) % 10
                } else {
                    n
                };
            }
            (10 - (s + 4) % 10) % 10 == d[7]
        }
        "BE" => {
            let n: u64 = body.parse().ok()?;
            97 - (n / 100) % 97 == n % 100
        }
        "DE" | "HR" => mod11_10(&d?),
        "DK" => weighted(&d?, &[2, 7, 6, 5, 4, 3, 2, 1]).is_multiple_of(11),
        "FI" => {
            let d = d?;
            let r = weighted(&d, &[7, 9, 10, 5, 8, 4, 2]) % 11;
            r != 1 && (if r == 0 { 0 } else { 11 - r }) == d[7]
        }
        "FR" => {
            // The two-char key is numeric for most companies; letters are a
            // newer scheme with no published check.
            let key: u64 = body[..2].parse().ok()?;
            let siren: u64 = body[2..].parse().ok()?;
            (12 + 3 * (siren % 97)) % 97 == key
        }
        "IT" => luhn(&d?),
        "LU" => {
            let n: u64 = body.parse().ok()?;
            (n / 100) % 89 == n % 100
        }
        "NL" => {
            let (num, _) = body.split_once('B')?;
            let d = digits(num)?;
            // Two schemes are in use: the classic mod 11 on the digits, and
            // (since 2020, for sole traders) ISO 7064 mod 97 over "NL" + body.
            let classic = weighted(&d, &[9, 8, 7, 6, 5, 4, 3, 2]) % 11 == d[8];
            classic || mod97(&format!("NL{body}")) == Some(1)
        }
        "PL" => {
            let d = d?;
            let r = weighted(&d, &[6, 5, 7, 2, 3, 4, 5, 6, 7]) % 11;
            r != 10 && r == d[9]
        }
        "PT" => {
            let d = d?;
            let r = 11 - weighted(&d, &[9, 8, 7, 6, 5, 4, 3, 2]) % 11;
            (if r >= 10 { 0 } else { r }) == d[8]
        }
        "SE" => luhn(&d?[..10]),
        "SK" => body.parse::<u64>().ok()?.is_multiple_of(11),
        _ => return None,
    })
}

/// Shape of the part after the country prefix, per EU member state (plus
/// `XI`, Northern Ireland).
fn vat_pattern(country: &str) -> Option<&'static str> {
    Some(match country {
        "AT" => r"U\d{8}",
        "BE" => r"[01]\d{9}",
        "BG" => r"\d{9,10}",
        "CY" => r"\d{8}[A-Z]",
        "CZ" => r"\d{8,10}",
        "DE" => r"\d{9}",
        "DK" => r"\d{8}",
        "EE" => r"\d{9}",
        "EL" => r"\d{9}",
        "ES" => r"[A-Z0-9]\d{7}[A-Z0-9]",
        "FI" => r"\d{8}",
        "FR" => r"[A-HJ-NP-Z0-9]{2}\d{9}",
        "HR" => r"\d{11}",
        "HU" => r"\d{8}",
        "IE" => r"\d[A-Z0-9+*]\d{5}[A-Z]{1,2}",
        "IT" => r"\d{11}",
        "LT" => r"\d{9}|\d{12}",
        "LU" => r"\d{8}",
        "LV" => r"\d{11}",
        "MT" => r"\d{8}",
        "NL" => r"\d{9}B\d{2}",
        "PL" => r"\d{10}",
        "PT" => r"\d{9}",
        "RO" => r"\d{2,10}",
        "SE" => r"\d{10}01",
        "SI" => r"\d{8}",
        "SK" => r"\d{10}",
        "XI" => r"\d{9}|\d{12}|GD\d{3}|HA\d{3}",
        _ => return None,
    })
}

pub fn vat_ok(value: &str) -> bool {
    let s = compact(value, " -.").to_ascii_uppercase();
    if s.len() < 4 || !s.is_ascii() {
        return false;
    }
    let (country, body) = s.split_at(2);
    let Some(pat) = vat_pattern(country) else {
        return false;
    };
    let shape = Regex::new(&format!("^(?:{pat})$")).expect("static VAT pattern");
    shape.is_match(body) && vat_check(country, body).unwrap_or(true)
}

// ---------------------------------------------------------------- email

pub fn email_ok(value: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Local part: the characters RFC 5322 allows unquoted, no leading,
        // trailing or doubled dot. Domain: labels of letters (any script),
        // digits and inner hyphens, ending in a top-level domain of 2+ letters.
        Regex::new(
            r"^[A-Za-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[A-Za-z0-9!#$%&'*+/=?^_`{|}~-]+)*@(?:[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?\.)+\p{L}{2,}$",
        )
        .expect("static email pattern")
    });
    let v = value.trim();
    v.len() <= 254 && v.split('@').next().is_some_and(|l| l.len() <= 64) && re.is_match(v)
}

#[cfg(test)]
#[path = "id_checks_tests.rs"]
mod tests;
