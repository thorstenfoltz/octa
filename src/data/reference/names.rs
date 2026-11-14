//! What a currency or country code is called, in the language Octa speaks:
//! nobody knows `XOF` or `HR` by heart (the user, 2026-11-14). Names come from
//! Unicode CLDR, bundled as `assets/cldr-names.tsv.gz` and rebuilt by
//! `scripts/build-names.py`.

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

const BUNDLED: &[u8] = include_bytes!("../../../assets/cldr-names.tsv.gz");

#[derive(Default)]
struct Names {
    /// (language, C or T, code) -> name.
    by_code: HashMap<(String, char, String), String>,
    /// Normalised currency name in any language -> code.
    currency_by_name: HashMap<String, String>,
}

fn names() -> &'static Names {
    static NAMES: OnceLock<Names> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut raw = String::new();
        // The asset is ours; a broken one just leaves the codes bare.
        let _ = flate2::read::GzDecoder::new(BUNDLED).read_to_string(&mut raw);
        let mut n = Names::default();
        for line in raw.lines() {
            let mut p = line.splitn(4, '\t');
            let (Some(lang), Some(kind), Some(code), Some(name)) =
                (p.next(), p.next(), p.next(), p.next())
            else {
                continue;
            };
            let kind = kind.chars().next().unwrap_or('T');
            if kind == 'C' {
                n.currency_by_name
                    .entry(crate::data::country_match::normalise(name))
                    .or_insert_with(|| code.to_string());
            }
            n.by_code
                .insert((lang.to_string(), kind, code.to_string()), name.to_string());
        }
        n
    })
}

fn name(kind: char, code: &str) -> Option<&'static str> {
    let n = names();
    let get = |lang: String| n.by_code.get(&(lang, kind, code.to_string()));
    get(crate::i18n::current_language())
        .or_else(|| get("en".into()))
        .map(String::as_str)
}

pub fn currency_name(code: &str) -> Option<&'static str> {
    name('C', code)
}

/// BIS's `XM` is the euro area, CLDR's `EZ`.
pub fn country_name(code: &str) -> Option<&'static str> {
    name('T', if code == "XM" { "EZ" } else { code })
}

/// The ISO code of a currency written out, in any language Octa speaks.
pub fn currency_code(text: &str) -> Option<String> {
    names()
        .currency_by_name
        .get(&crate::data::country_match::normalise(text))
        .cloned()
}

/// `Euro (EUR)`; the bare code when it has no name.
fn label(name: Option<&str>, code: &str) -> String {
    name.map_or_else(|| code.to_string(), |n| format!("{n} ({code})"))
}

pub fn currency_label(code: &str) -> String {
    label(currency_name(code), code)
}

pub fn country_label(code: &str) -> String {
    label(country_name(code), code)
}

/// `(code, label)` sorted by label, for a picker.
pub fn labelled(codes: &[String], label: fn(&str) -> String) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = codes.iter().map(|c| (c.clone(), label(c))).collect();
    v.sort_by_cached_key(|(_, l)| crate::data::country_match::normalise(l));
    v
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
