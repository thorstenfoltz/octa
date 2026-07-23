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

/// Menu entries whose click opens a new tab or window (a dialog, a file picker,
/// a result tab). These must end in an ellipsis, in every language: the "..."
/// is the promise that something is about to open.
const OPENS_SOMETHING: &[&str] = &[
    "file_menu.new_file",           // new tab
    "file_menu.open_as",            // file picker
    "file_menu.open_table_folder",  // folder picker
    "file_menu.open_directory",     // folder picker
    "file_menu.export_schema",      // dialog
    "common.open",                  // file picker
    "common.save_as",               // file picker
    "edit_menu.rename_columns",     // dialog
    "edit_menu.conditional_format", // dialog
    "edit_menu.validation",         // dialog
    "edit_menu.scope_cell",         // parse dialog
    "edit_menu.scope_row",
    "edit_menu.scope_column",
    "edit_menu.scope_table",
    "view_menu.compare_with",       // file picker
    "view_menu.compare_git",        // dialog
    "analyse_menu.chart",           // new tab
    "analyse_menu.transpose",       // new tab
    "analyse_menu.describe",        // new tab
    "analyse_menu.quality",         // new tab
    "analyse_menu.pivot",           // dialog
    "analyse_menu.correlation",     // dialog
    "analyse_menu.multi_sort",      // dialog
    "analyse_menu.random_sample",   // dialog
    "analyse_menu.value_frequency", // dialog
    "help_menu.documentation",      // window
    "help_menu.settings",           // window
    "help_menu.about",              // window
    "help_menu.check_updates",      // window
];

/// Menu entries that just do the thing, in place: no tab, no window, nothing to
/// fill in. An ellipsis on these is a broken promise.
const JUST_EXECUTES: &[&str] = &[
    "file_menu.close_directory",
    "file_menu.cloud_connections", // toggles the sidebar
    "file_menu.databases",         // toggles the sidebar
    "file_menu.exit",
    "common.save",
    "edit_menu.fit_all_columns",
    "edit_menu.copy_markdown",
    "edit_menu.insert_row",
    "edit_menu.clear_all_marks",
    "edit_menu.discard_all_edits",
    "view_menu.reopen_as", // re-reads the file in place
    "view_menu.readonly",
    "view_menu.zoom_reset",
    "search_menu.find",
    "search_menu.find_replace",
    "search_menu.multi_search", // toggles a docked panel, like SQL / Assistant
    "analyse_menu.sql",         // toggles a docked panel
    "analyse_menu.assistant",   // toggles a docked panel
    "diagnostics.menu_export",  // writes the report and reveals it
];

/// Whether a label carries an ellipsis at all. Deliberately not "ends with":
/// languages whose word order differs put it mid-string (Chinese renders
/// "Compare with..." as "与...比较"), and the ellipsis still means the same thing
/// there.
fn has_ellipsis(s: &str) -> bool {
    s.contains("...") || s.contains('\u{2026}')
}

#[test]
fn menu_ellipsis_means_something_opens() {
    let _g = LANG_LOCK.lock().unwrap();
    for (lang, _) in LANGUAGES {
        set_language(lang);
        for key in OPENS_SOMETHING {
            assert!(
                has_ellipsis(&t(key)),
                "[{lang}] {key} opens a tab or window, so it must carry '...': {:?}",
                t(key)
            );
        }
        for key in JUST_EXECUTES {
            assert!(
                !has_ellipsis(&t(key)),
                "[{lang}] {key} just executes, so it must NOT carry '...': {:?}",
                t(key)
            );
        }
    }
    set_language("en");
}
