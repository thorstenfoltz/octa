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
//! compile time. Keys are dotted paths into the nested tables (e.g.
//! `[menu] file = "File"` -> key `menu.file`). [`t`] looks the key up in the
//! current language and falls back to English, then to the key itself, so a
//! missing translation degrades gracefully instead of panicking.
//!
//! Only the Latin-script languages whose glyphs the bundled Roboto font
//! already covers are offered. Cyrillic / CJK / right-to-left scripts are out
//! of scope until the font and layout work lands.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Embedded locale sources: (language code, TOML text).
const LOCALE_SOURCES: &[(&str, &str)] = &[
    ("en", include_str!("../locales/en.toml")),
    ("de", include_str!("../locales/de.toml")),
    ("es", include_str!("../locales/es.toml")),
    ("fr", include_str!("../locales/fr.toml")),
    ("it", include_str!("../locales/it.toml")),
    ("nl", include_str!("../locales/nl.toml")),
    ("pt", include_str!("../locales/pt.toml")),
    ("pl", include_str!("../locales/pl.toml")),
    ("sv", include_str!("../locales/sv.toml")),
    ("da", include_str!("../locales/da.toml")),
    ("no", include_str!("../locales/no.toml")),
    ("fi", include_str!("../locales/fi.toml")),
    ("tr", include_str!("../locales/tr.toml")),
    ("id", include_str!("../locales/id.toml")),
    ("vi", include_str!("../locales/vi.toml")),
    ("ro", include_str!("../locales/ro.toml")),
    ("hu", include_str!("../locales/hu.toml")),
    ("cs", include_str!("../locales/cs.toml")),
    ("el", include_str!("../locales/el.toml")),
    ("ru", include_str!("../locales/ru.toml")),
    ("ja", include_str!("../locales/ja.toml")),
    ("ko", include_str!("../locales/ko.toml")),
    ("zh", include_str!("../locales/zh.toml")),
    ("uk", include_str!("../locales/uk.toml")),
    ("bg", include_str!("../locales/bg.toml")),
    ("sr", include_str!("../locales/sr.toml")),
    ("hr", include_str!("../locales/hr.toml")),
    ("sl", include_str!("../locales/sl.toml")),
    ("sk", include_str!("../locales/sk.toml")),
    ("lt", include_str!("../locales/lt.toml")),
    ("lv", include_str!("../locales/lv.toml")),
    ("et", include_str!("../locales/et.toml")),
];

/// User-facing list of supported UI languages: (code, native name). Drives the
/// Settings language dropdown. English is first (the fallback).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("pt", "Português"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("da", "Dansk"),
    ("no", "Norsk"),
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
    ("zh", "中文"),
    ("uk", "Українська"),
    ("bg", "Български"),
    ("sr", "Српски"),
    ("hr", "Hrvatski"),
    ("sl", "Slovenščina"),
    ("sk", "Slovenčina"),
    ("lt", "Lietuvių"),
    ("lv", "Latviešu"),
    ("et", "Eesti"),
];

type LangMap = HashMap<String, HashMap<String, String>>;

static CATALOG: OnceLock<LangMap> = OnceLock::new();
static CURRENT: RwLock<String> = RwLock::new(String::new());

/// Parse every embedded locale into a `lang -> (key -> value)` map.
fn catalog() -> &'static LangMap {
    CATALOG.get_or_init(|| {
        let mut langs: LangMap = HashMap::new();
        for (code, src) in LOCALE_SOURCES {
            let mut flat = HashMap::new();
            if let Ok(value) = toml::from_str::<toml::Value>(src) {
                flatten(None, &value, &mut flat);
            }
            langs.insert((*code).to_string(), flat);
        }
        langs
    })
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

/// Translate a key into the active language. Falls back to English, then to
/// the key itself, so an unmigrated/missing key shows the developer string
/// rather than crashing.
pub fn t(key: &str) -> String {
    let cat = catalog();
    let lang = current_language();
    if let Some(v) = cat.get(&lang).and_then(|m| m.get(key)) {
        return v.clone();
    }
    if let Some(v) = cat.get("en").and_then(|m| m.get(key)) {
        return v.clone();
    }
    key.to_string()
}

#[cfg(test)]
#[path = "i18n_tests.rs"]
mod tests;
