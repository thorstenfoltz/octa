use super::*;

#[test]
fn codes_get_their_names_and_names_find_their_codes() {
    // English is the language of a test run: no test sets another.
    assert_eq!(currency_label("EUR"), "Euro (EUR)");
    assert_eq!(country_label("DE"), "Germany (DE)");
    assert_eq!(country_name("XM"), Some("Eurozone"));
    assert_eq!(currency_label("QQQ"), "QQQ");
    assert_eq!(currency_code("Japanese Yen").as_deref(), Some("JPY"));
    assert_eq!(currency_code("US-Dollar").as_deref(), Some("USD"));
    assert_eq!(currency_code("Schweizer Franken").as_deref(), Some("CHF"));
    assert_eq!(currency_code("twelve"), None);
}

#[test]
fn a_picker_is_sorted_by_name_not_by_code() {
    let codes = ["USD".to_string(), "CHF".to_string(), "EUR".to_string()];
    let labels: Vec<String> = labelled(&codes, currency_label)
        .into_iter()
        .map(|(_, l)| l)
        .collect();
    assert_eq!(
        labels,
        ["Euro (EUR)", "Swiss Franc (CHF)", "US Dollar (USD)"]
    );
}

#[test]
fn every_language_has_the_names() {
    for (lang, _) in crate::i18n::LANGUAGES {
        for (kind, code) in [('C', "EUR"), ('C', "USD"), ('T', "DE"), ('T', "EZ")] {
            assert!(
                names()
                    .by_code
                    .contains_key(&(lang.to_string(), kind, code.to_string())),
                "{lang} lacks {code}"
            );
        }
    }
}
