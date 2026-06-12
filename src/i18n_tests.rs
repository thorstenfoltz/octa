//! Unit tests for [`i18n`](i18n). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use std::sync::Mutex;

// The active language is global process state; serialise the tests that
// mutate it so the parallel test harness can't interleave them.
static LANG_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn english_keys_resolve() {
    let _g = LANG_LOCK.lock().unwrap();
    set_language("en");
    assert_eq!(t("menu.file"), "File");
    assert_eq!(t("menu.help"), "Help");
}

#[test]
fn missing_key_returns_itself() {
    let _g = LANG_LOCK.lock().unwrap();
    set_language("en");
    assert_eq!(t("does.not.exist"), "does.not.exist");
}

#[test]
fn switching_language_changes_output() {
    let _g = LANG_LOCK.lock().unwrap();
    set_language("de");
    assert_eq!(t("menu.file"), "Datei");
    set_language("fr");
    assert_eq!(t("menu.file"), "Fichier");
    set_language("en");
    assert_eq!(t("menu.file"), "File");
}

#[test]
fn unknown_language_falls_back_to_english() {
    let _g = LANG_LOCK.lock().unwrap();
    set_language("xx");
    assert_eq!(current_language(), "en");
    assert_eq!(t("menu.file"), "File");
}

#[test]
fn every_language_covers_every_english_key() {
    // Guards against a translation file drifting out of sync with the
    // English master: every en key must exist in every other language.
    let cat = catalog();
    let en = cat.get("en").expect("en locale");
    for (code, _) in LANGUAGES {
        if *code == "en" {
            continue;
        }
        let map = cat
            .get(*code)
            .unwrap_or_else(|| panic!("missing locale {code}"));
        let missing: Vec<&String> = en.keys().filter(|k| !map.contains_key(*k)).collect();
        assert!(
            missing.is_empty(),
            "locale {code} is missing keys: {missing:?}"
        );
    }
}
