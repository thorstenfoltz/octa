//! vCard `.vcf` (3.0 and 4.0, and 2.1 as phones and Outlook export it): one
//! row per card in the Contacts table.

use std::path::Path;

use anyhow::Result;

use super::content_line::{parse, unfold};
use super::{Contact, MailBook, join, read_text, with_format};
use crate::data::DataTable;
use crate::formats::FormatReader;

pub struct VcfReader;

/// `YYYY-MM-DD` from `1980-04-02`, `19800402` or a timestamp.
fn birthday(v: &str) -> Option<String> {
    let v = v.trim();
    let digits: String = v
        .chars()
        .take_while(|c| *c != 'T')
        .filter(char::is_ascii_digit)
        .collect();
    let d = chrono::NaiveDate::parse_from_str(digits.get(..8)?, "%Y%m%d").ok()?;
    Some(d.format("%Y-%m-%d").to_string())
}

/// The components of a structured value (`N`, `ADR`, `ORG`), split at
/// unescaped `;` and each unescaped.
fn components(raw: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.last_mut()
                        .unwrap()
                        .push(if matches!(n, 'n' | 'N') { '\n' } else { n });
                }
            }
            ';' => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out
}

pub fn parse_book(text: &str) -> MailBook {
    let mut book = MailBook::default();
    let mut card: Option<Contact> = None;
    let (mut emails, mut phones): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    for line in unfold(text) {
        let Some(p) = parse(&line) else {
            continue;
        };
        let is_card = p.value.trim().eq_ignore_ascii_case("VCARD");
        match p.name.as_str() {
            "BEGIN" if is_card => {
                card = Some(Contact::default());
                emails.clear();
                phones.clear();
                continue;
            }
            "END" if is_card => {
                if let Some(mut c) = card.take() {
                    c.emails = join(emails.drain(..));
                    c.phones = join(phones.drain(..));
                    book.contacts.push(c);
                }
                continue;
            }
            _ => {}
        }
        let Some(c) = card.as_mut() else {
            continue;
        };
        // Quoted-printable and charset are undone first; the structured
        // fields are then split on their own escapes.
        let decoded = || components(&p.raw());
        match p.name.as_str() {
            "FN" => c.name = p.text(),
            "N" => {
                let n = decoded();
                c.family_name = n.first().cloned().unwrap_or_default();
                c.given_name = n.get(1).cloned().unwrap_or_default();
            }
            "ORG" => c.company = decoded().into_iter().next().unwrap_or_default(),
            "TITLE" => c.job_title = p.text(),
            "EMAIL" => emails.push(p.text()),
            "TEL" => {
                let number = p.text();
                phones.push(match p.param("TYPE").filter(|t| !t.is_empty()) {
                    Some(t) => format!("{number} ({})", t.to_ascii_lowercase().replace(',', ", ")),
                    None => number,
                });
            }
            "ADR" => {
                let parts: Vec<String> = decoded()
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                c.address = parts.join(", ");
            }
            "BDAY" => c.birthday = birthday(&p.value),
            "NOTE" => c.note = p.text(),
            _ => {}
        }
    }
    book
}

impl FormatReader for VcfReader {
    fn name(&self) -> &str {
        "vCard"
    }

    fn extensions(&self) -> &[&str] {
        &["vcf"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        Ok(with_format(
            parse_book(&read_text(path)?).single(),
            self.name(),
        ))
    }
}

#[cfg(test)]
#[path = "vcf_reader_tests.rs"]
mod tests;
