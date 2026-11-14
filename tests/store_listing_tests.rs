//! A language in the app must reach the Microsoft Store: the manifest declares it
//! (which is what Partner Center reads) and a listing file says it. Nothing else
//! notices a miss, as a language without a listing just goes out in English.

use std::collections::BTreeSet;
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn manifest_languages() -> Vec<String> {
    let text = std::fs::read_to_string(root().join("windows/AppxManifest.xml")).unwrap();
    text.split("<Resource Language=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap().to_string())
        .collect()
}

fn stems(dir: &str) -> BTreeSet<String> {
    std::fs::read_dir(root().join(dir))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "toml"))
        .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// The Store's tag for an app language, where the two differ.
fn store_tag(locale: &str) -> String {
    match locale {
        "en" => "en-us".to_string(),
        other => other.to_string(),
    }
}

#[test]
fn every_app_language_is_declared_for_the_store_and_nothing_else_is() {
    let declared: BTreeSet<String> = manifest_languages().into_iter().collect();
    let wanted: BTreeSet<String> = stems("locales").iter().map(|l| store_tag(l)).collect();
    assert_eq!(declared, wanted);
}

#[test]
fn every_declared_language_has_its_listing_and_the_reverse() {
    let mut listings = stems("docs/assets/store/listings");
    assert!(listings.remove("_default"));
    let mut declared: BTreeSet<String> = manifest_languages().into_iter().collect();
    // English is `_default.toml`.
    assert!(declared.remove("en-us"));
    assert_eq!(declared, listings);
}

/// English first, as the first `Resource` is the package's default language.
#[test]
fn the_default_language_comes_first_and_none_twice() {
    let declared = manifest_languages();
    assert_eq!(declared[0], "en-us");
    let unique: BTreeSet<&String> = declared.iter().collect();
    assert_eq!(unique.len(), declared.len());
}

/// Both failed us. Windows refused bare `sr` at registration (0x80070057, the
/// 0.16.0.0 rejection). Microsoft lists bare `pt` as Portugal, so beside `pt-pt` it
/// declares Portugal twice and Brazil not at all.
#[test]
fn no_tag_windows_or_the_store_would_misread() {
    let declared = manifest_languages();
    for bare in ["sr", "pt", "zh"] {
        assert!(!declared.iter().any(|tag| tag == bare), "bare {bare}");
    }
    for needed in ["sr-cyrl", "pt-br", "pt-pt", "zh-hans", "zh-hant", "ca"] {
        assert!(declared.iter().any(|tag| tag == needed), "{needed}");
    }
}
