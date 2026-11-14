//! Lightweight internationalisation for the GUI.
//!
//! Octa is a single package that is both a library (`octa::ui`,
//! `octa::view_modes` live here) and a binary (`src/app`, `src/main`). UI
//! strings live on *both* sides of that split, so the translation lookup lives
//! in the library and the binary calls it through `octa::i18n::t`. That avoids
//! the macro/crate-split complications a derive-based i18n crate would hit
//! here.
//!
//! Translation files are TOML, one per language under `locales/`, embedded at
//! compile time, compressed (`build.rs`). A language is unpacked and parsed the
//! first time it is asked for, so a run pays for the one or two it uses, not for
//! all of them. Keys are dotted paths into the nested tables (e.g.
//! `[menu] file = "File"` -> key `menu.file`). [`t`] looks the key up in the
//! current language and falls back to English, then to the key itself, so a
//! missing translation degrades gracefully instead of panicking.
//!
//! Offered are the languages whose script the bundled fonts cover: Latin,
//! Greek and Cyrillic (Roboto) and CJK (the Noto Sans CJK subset, which holds
//! the Traditional characters too). Right-to-left scripts are out of scope:
//! egui does not lay out mixed-direction text.

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock, RwLock};

/// One language as it sits in the binary: (code, compressed TOML).
macro_rules! locale {
    ($code:literal) => {
        (
            $code,
            include_bytes!(concat!(env!("OUT_DIR"), "/embedded/", $code, ".toml.zst")) as &[u8],
        )
    };
}

/// Embedded locale sources.
const LOCALE_SOURCES: &[(&str, &[u8])] = &[
    locale!("en"),
    locale!("de"),
    locale!("es"),
    locale!("ca"),
    locale!("fr"),
    locale!("it"),
    locale!("nl"),
    locale!("pt-pt"),
    locale!("pt-br"),
    locale!("pl"),
    locale!("sv"),
    locale!("da"),
    locale!("nb"),
    locale!("fi"),
    locale!("tr"),
    locale!("id"),
    locale!("vi"),
    locale!("ro"),
    locale!("hu"),
    locale!("cs"),
    locale!("el"),
    locale!("ru"),
    locale!("ja"),
    locale!("ko"),
    locale!("zh-hans"),
    locale!("zh-hant"),
    locale!("uk"),
    locale!("bg"),
    locale!("sr-cyrl"),
    locale!("hr"),
    locale!("sl"),
    locale!("sk"),
    locale!("lt"),
    locale!("lv"),
    locale!("et"),
];

/// User-facing list of supported UI languages: (code, native name). Drives the
/// Settings language dropdown. English is first (the fallback).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("ca", "Català"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("pt-pt", "Português (Portugal)"),
    ("pt-br", "Português (Brasil)"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("da", "Dansk"),
    ("nb", "Norsk"),
    ("fi", "Suomi"),
    ("tr", "Türkçe"),
    ("id", "Bahasa Indonesia"),
    ("vi", "Tiếng Việt"),
    ("ro", "Română"),
    ("hu", "Magyar"),
    ("cs", "Čeština"),
    ("el", "Ελληνικά"),
    ("ru", "Русский"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh-hans", "简体中文"),
    ("zh-hant", "繁體中文"),
    ("uk", "Українська"),
    ("bg", "Български"),
    ("sr-cyrl", "Српски"),
    ("hr", "Hrvatski"),
    ("sl", "Slovenščina"),
    ("sk", "Slovenčina"),
    ("lt", "Lietuvių"),
    ("lv", "Latviešu"),
    ("et", "Eesti"),
];

/// One language: dotted key -> text.
type Catalog = HashMap<String, String>;

/// A language's compressed source, and its catalogue once read.
type Locale = (&'static [u8], OnceLock<Catalog>);

/// Every language by code.
static CATALOGS: LazyLock<HashMap<&'static str, Locale>> = LazyLock::new(|| {
    LOCALE_SOURCES
        .iter()
        .map(|(code, packed)| (*code, (*packed, OnceLock::new())))
        .collect()
});
static CURRENT: RwLock<String> = RwLock::new(String::new());

/// Language `code`, read on first use. Unreadable means empty, so `t` falls back
/// to English rather than failing.
fn catalog_of(code: &str) -> Option<&'static Catalog> {
    let (packed, catalog) = CATALOGS.get(code)?;
    Some(catalog.get_or_init(|| {
        let mut flat = HashMap::new();
        let value = zstd::decode_all(*packed)
            .ok()
            .and_then(|raw| String::from_utf8(raw).ok())
            .and_then(|text| toml::from_str::<toml::Value>(&text).ok());
        if let Some(value) = value {
            flatten(None, &value, &mut flat);
        }
        flat
    }))
}

/// Flatten nested TOML tables into dotted keys; only string leaves are kept.
fn flatten(prefix: Option<&str>, value: &toml::Value, out: &mut HashMap<String, String>) {
    match value {
        toml::Value::Table(table) => {
            for (k, v) in table {
                let key = match prefix {
                    Some(p) => format!("{p}.{k}"),
                    None => k.clone(),
                };
                flatten(Some(&key), v, out);
            }
        }
        toml::Value::String(s) => {
            if let Some(p) = prefix {
                out.insert(p.to_string(), s.clone());
            }
        }
        _ => {}
    }
}

/// Whether `code` is one of the supported languages.
pub fn is_supported(code: &str) -> bool {
    LANGUAGES.iter().any(|(c, _)| *c == code)
}

/// Today's code for a language an earlier release saved in `settings.toml`. These
/// took the Store's tags in 0.22.0; without this they would read as unknown and
/// drop the user to English.
pub fn current_code(saved: &str) -> &str {
    match saved {
        "no" => "nb",
        "pt" => "pt-pt",
        "sr" => "sr-cyrl",
        "zh" => "zh-hans",
        other => other,
    }
}

/// Set the active UI language. Unknown codes fall back to English. Cheap to
/// call (used at startup and whenever the user applies a new language); takes
/// effect on the next frame's `t` calls without a restart.
pub fn set_language(code: &str) {
    let code = if is_supported(code) { code } else { "en" };
    if let Ok(mut cur) = CURRENT.write() {
        *cur = code.to_string();
    }
}

/// The active language code (defaults to English).
pub fn current_language() -> String {
    match CURRENT.read() {
        Ok(c) if !c.is_empty() => c.clone(),
        _ => "en".to_string(),
    }
}

/// Whether people writing in language `code` usually write decimal numbers
/// with a comma (`1234,56`). Used to pre-select how a new CSV is written; the
/// user can always pick otherwise. An unknown code says no, the English way.
pub fn uses_decimal_comma(code: &str) -> bool {
    matches!(
        code,
        "bg" | "ca"
            | "cs"
            | "da"
            | "de"
            | "el"
            | "es"
            | "et"
            | "fi"
            | "fr"
            | "hr"
            | "hu"
            | "id"
            | "it"
            | "lt"
            | "lv"
            | "nb"
            | "nl"
            | "pl"
            | "pt-pt"
            | "pt-br"
            | "ro"
            | "ru"
            | "sk"
            | "sl"
            | "sr-cyrl"
            | "sv"
            | "tr"
            | "uk"
            | "vi"
    )
}

/// Translate a key into the active language. Falls back to English, then to
/// the key itself, so an unmigrated/missing key shows the developer string
/// rather than crashing.
pub fn t(key: &str) -> String {
    t_in(&current_language(), key)
}

/// [`t`] in English whatever the active language, for matching by the name
/// the documentation uses (Find a command).
pub fn t_english(key: &str) -> String {
    t_in("en", key)
}

/// [`t`] for language `lang`.
fn t_in(lang: &str, key: &str) -> String {
    if let Some(v) = catalog_of(lang).and_then(|m| m.get(key)) {
        return v.clone();
    }
    if let Some(v) = catalog_of("en").and_then(|m| m.get(key)) {
        return v.clone();
    }
    key.to_string()
}

#[cfg(test)]
#[path = "i18n_tests.rs"]
mod tests;
