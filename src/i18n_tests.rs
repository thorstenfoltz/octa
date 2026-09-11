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

/// Keys whose text carries a `{...}` placeholder the code substitutes. A
/// translation that drops one produces a message with a hole in it - a prompt
/// that never names the file, a status line that never names the path - and
/// nothing else would notice.
const PLACEHOLDERS: &[(&str, &str)] = &[
    ("chat.explain_prompt", "{file}"),
    ("chat.usage", "{in}"),
    ("pdf.pages", "{n}"),
    ("pdf.written", "{path}"),
    ("pdf.failed", "{error}"),
    ("transpose.too_many_rows", "{n}"),
];

#[test]
fn every_translation_keeps_its_placeholders() {
    let _g = LANG_LOCK.lock().unwrap();
    for (lang, _) in LANGUAGES {
        set_language(lang);
        for (key, placeholder) in PLACEHOLDERS {
            let text = t(key);
            assert!(
                text.contains(placeholder),
                "[{lang}] {key} lost its {placeholder} placeholder: {text:?}"
            );
        }
    }
    set_language("en");
}

/// Locales written in a non-Latin script, with a character range that any
/// real translation in that language must contain.
const NON_LATIN_SCRIPTS: &[(&str, [char; 2])] = &[
    ("ru", ['\u{0400}', '\u{04FF}']), // Cyrillic
    ("uk", ['\u{0400}', '\u{04FF}']),
    ("bg", ['\u{0400}', '\u{04FF}']),
    ("sr", ['\u{0400}', '\u{04FF}']), // Serbian is Cyrillic, not Latin
    ("el", ['\u{0370}', '\u{03FF}']), // Greek
    ("ja", ['\u{3040}', '\u{9FFF}']), // kana + kanji
    ("ko", ['\u{AC00}', '\u{D7AF}']), // hangul syllables
    ("zh", ['\u{4E00}', '\u{9FFF}']), // han
];

/// A locale written in a non-Latin script must not carry transliterated
/// values such as `"Otkryt papku tablicy..."` for Открыть папку таблицы.
///
/// The rule: a value that contains a Latin *word* (three or more letters in a
/// row) and shares no character with its own script is only acceptable when it
/// is identical to the English string, which is how product and format names
/// (`JSON`, `Parquet`, `SHA-256`, `Azure AD`) legitimately stay Latin
/// everywhere.
///
/// This exists because the failure it catches is invisible to
/// `every_language_covers_every_english_key`, which only checks that a key is
/// present. `sr.toml` once held 168 transliterated strings, and seven other
/// locales held 426 more, all of them shipped green.
#[test]
fn non_latin_locales_are_written_in_their_own_script() {
    let cat = catalog();
    let en = cat.get("en").expect("en locale");

    // Three or more consecutive ASCII letters: a word, not an initial such as
    // the `X` of an axis label.
    fn has_latin_word(s: &str) -> bool {
        let mut run = 0;
        for ch in s.chars() {
            if ch.is_ascii_alphabetic() {
                run += 1;
                if run >= 3 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    }

    let mut offenders: Vec<String> = Vec::new();
    for (code, [lo, hi]) in NON_LATIN_SCRIPTS {
        let map = cat
            .get(*code)
            .unwrap_or_else(|| panic!("missing locale {code}"));
        for (key, value) in map {
            if !has_latin_word(value) {
                continue;
            }
            if value.chars().any(|c| c >= *lo && c <= *hi) {
                continue;
            }
            // Product and format names legitimately stay Latin in every
            // locale (`JSON`, `Parquet`, `SHA-256`, `AWS IAM (RDS)`). Compare
            // letters only, so a locale writing `AWS IAM（RDS）` with fullwidth
            // brackets is still recognised as the same untranslated name.
            let letters = |s: &str| -> String {
                s.chars()
                    .filter(|c| c.is_ascii_alphabetic())
                    .map(|c| c.to_ascii_lowercase())
                    .collect()
            };
            if en.get(key).is_some_and(|e| letters(e) == letters(value)) {
                continue;
            }
            offenders.push(format!("  {code}: {key} = {value:?}"));
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "{} value(s) are transliterated instead of written in their own script:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// Multi-word English phrases that are the same in every language because
/// they name a product or a vendor's mechanism, not because nobody translated
/// them.
const UNTRANSLATABLE_PHRASES: &[&str] = &[
    "db.auth_aws_iam",           // AWS IAM (RDS)
    "db.auth_gcp_iam",           // GCP IAM (Cloud SQL)
    "db.auth_gcp_adc",           // Application Default Credentials
    "dialog.swb_target_is_file", // File (DuckDB / SQLite)
];

/// A locale must not carry whole English sentences.
///
/// The sibling of `non_latin_locales_are_written_in_their_own_script`, and the
/// other half of the same blind spot: `every_language_covers_every_english_key`
/// checks that a key exists, never that it was translated, so an entire
/// section can be fanned out in English and ship green. The `[union_tree]`
/// section did exactly that, in all 31 locales.
///
/// Single words are ignored, because format and engine names (`Parquet`,
/// `Snowflake`, `JSON Lines`) are identical everywhere by design. Three or
/// more words repeated verbatim is a sentence nobody translated.
#[test]
fn locales_do_not_carry_untranslated_english_sentences() {
    let cat = catalog();
    let en = cat.get("en").expect("en locale");

    fn word_count(s: &str) -> usize {
        s.split(|c: char| !c.is_ascii_alphabetic() && c != '\'')
            .filter(|w| w.len() > 1)
            .count()
    }

    let mut offenders: Vec<String> = Vec::new();
    for (code, _) in LANGUAGES {
        if *code == "en" {
            continue;
        }
        let map = cat
            .get(*code)
            .unwrap_or_else(|| panic!("missing locale {code}"));
        for (key, value) in map {
            if UNTRANSLATABLE_PHRASES.contains(&key.as_str()) {
                continue;
            }
            if en.get(key).is_some_and(|e| e == value) && word_count(value) >= 3 {
                offenders.push(format!("  {code}: {key} = {value:?}"));
            }
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "{} value(s) are still the English text:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// Menu entries whose click opens a new tab or window (a dialog, a file picker,
/// a result tab). These must end in an ellipsis, in every language: the "..."
/// is the promise that something is about to open.
const OPENS_SOMETHING: &[&str] = &[
    "file_menu.new_file",          // new tab
    "file_menu.new_table",         // dialog
    "file_menu.open_as",           // file picker
    "file_menu.open_table_folder", // folder picker
    "file_menu.batch_convert",     // dialog
    "file_menu.schema_drift",
    "file_menu.harmonise",          // dialog
    "file_menu.report",             // dialog
    "file_menu.open_directory",     // folder picker
    "file_menu.export_schema",      // dialog
    "file_menu.save_to_db",         // dialog
    "file_menu.save_sql",           // file picker
    "file_menu.export_workbook",    // dialog
    "file_menu.open_url",           // dialog
    "common.open",                  // file picker
    "common.save_as",               // file picker
    "edit_menu.rename_columns",     // dialog
    "edit_menu.fix_duplicate_cols", // dialog
    "edit_menu.conditional_format", // dialog
    "edit_menu.validation",         // dialog
    "edit_menu.scope_cell",         // parse dialog
    "edit_menu.scope_row",
    "edit_menu.scope_column",
    "edit_menu.scope_table",
    "view_menu.compare_with",      // file picker
    "view_menu.compare_git",       // dialog
    "analyse_menu.chart",          // new tab
    "analyse_menu.transpose",      // new tab
    "analyse_menu.describe",       // new tab
    "analyse_menu.quality",        // new tab
    "analyse_menu.file_internals", // new tab
    "analyse_menu.db_compare",     // dialog
    "analyse_menu.join_keys",
    "analyse_menu.join_diag", // dialog
    "drift.scan_schemas",     // dialog, from the folder context menu
    "fuzzy_join.menu",        // dialog
    "datadrift.menu",         // dialog
    "relmap.menu",            // dialog
    "analyse_menu.pivot",
    "analyse_menu.timeseries",      // dialog
    "analyse_menu.correlation",     // dialog
    "distcmp.menu",                 // dialog
    "refint.menu",                  // dialog
    "analyse_menu.multi_sort",      // dialog
    "analyse_menu.random_sample",   // dialog
    "analyse_menu.value_frequency", // dialog
    "help_menu.documentation",      // window
    "help_menu.settings",           // window
    "help_menu.about",              // window
    "help_menu.check_updates",      // window
    "ai_report.menu",               // window
    "analyse_menu.row_compare",
    "file_menu.export_pdf",
    "context_menu.export_pdf",
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
    "view_menu.record",    // switches view mode in place
    "view_menu.reopen_as", // re-reads the file in place
    "view_menu.readonly",
    "view_menu.split",      // toggles the second row band in place
    "view_menu.split_side", // same, side by side
    "view_menu.add_pane",   // one more band, in place
    "view_menu.remove_pane",
    "view_menu.zoom_reset",
    "search_menu.find",
    "search_menu.find_replace",
    "search_menu.multi_search", // toggles a docked panel, like SQL / Assistant
    "analyse_menu.cleanup",     // toggles a docked panel
    "analyse_menu.sql",         // toggles a docked panel
    "analyse_menu.assistant",   // toggles a docked panel
    "chat.explain",             // opens the docked chat panel and sends
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

/// egui's bundled font has no glyph for typographic punctuation, so an em
/// dash, en dash, arrow or ellipsis character in a UI string paints as a
/// tofu box on the one screen the reader is looking at. Octa's prose rule
/// bans em dashes outright and asks for `...` rather than a single-character
/// ellipsis, so this holds every catalogue to what the English one already
/// does. 174 of these had crept into nine locales before the test existed.
///
/// Script punctuation is deliberately NOT on this list: the Korean
/// interpunct in `가운뎃점` lists and the Greek ano teleia are letters'
/// company, not decoration, and replacing them would damage the sentence.
#[test]
fn locales_carry_no_typographic_punctuation() {
    const TOFU: [(char, &str); 4] = [
        ('\u{2014}', "em dash, write '-'"),
        ('\u{2013}', "en dash, write '-'"),
        ('\u{2192}', "arrow, write '->'"),
        ('\u{2026}', "ellipsis, write '...'"),
    ];
    let cat = catalog();
    let mut offenders: Vec<String> = Vec::new();
    for (lang, _) in LANGUAGES {
        let map = cat
            .get(*lang)
            .unwrap_or_else(|| panic!("missing locale {lang}"));
        for (key, value) in map {
            for (ch, advice) in TOFU {
                if value.contains(ch) {
                    offenders.push(format!("[{lang}] {key}: {advice} - {value:?}"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} UI string(s) carry punctuation egui renders as tofu:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}
