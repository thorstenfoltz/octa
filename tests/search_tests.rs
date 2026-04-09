use octa::data::*;

// --- wildcard_to_regex ---

#[test]
fn wildcard_star_matches_any() {
    let re = regex::Regex::new(&wildcard_to_regex("foo*bar")).unwrap();
    assert!(re.is_match("fooXYZbar"));
    assert!(re.is_match("foobar"));
    assert!(!re.is_match("foXbar"));
}

#[test]
fn wildcard_question_mark_matches_single_char() {
    let re = regex::Regex::new(&wildcard_to_regex("item?")).unwrap();
    assert!(re.is_match("itemA"));
    assert!(re.is_match("item1"));
    // ? matches exactly one char inside the string
    assert!(!re.is_match("item"));
}

#[test]
fn wildcard_escaped_star_is_literal() {
    let re = regex::Regex::new(&wildcard_to_regex("2\\*3")).unwrap();
    assert!(re.is_match("2*3"));
    assert!(!re.is_match("2X3"));
    assert!(!re.is_match("23"));
}

#[test]
fn wildcard_escaped_question_is_literal() {
    let re = regex::Regex::new(&wildcard_to_regex("what\\?")).unwrap();
    assert!(re.is_match("what?"));
    assert!(!re.is_match("whatX"));
}

#[test]
fn wildcard_case_insensitive() {
    let re = regex::Regex::new(&wildcard_to_regex("hello*")).unwrap();
    assert!(re.is_match("HELLO world"));
    assert!(re.is_match("Hello"));
}

#[test]
fn wildcard_special_regex_chars_escaped() {
    let re = regex::Regex::new(&wildcard_to_regex("price ($10.00)")).unwrap();
    assert!(re.is_match("price ($10.00)"));
    assert!(!re.is_match("price X$10Y00Z"));
}

#[test]
fn wildcard_combined_star_and_question() {
    let re = regex::Regex::new(&wildcard_to_regex("a?c*z")).unwrap();
    assert!(re.is_match("abcXYZz"));
    assert!(re.is_match("axcz"));
    assert!(!re.is_match("az"));
}

// --- SearchMode ---

#[test]
fn search_mode_labels() {
    assert_eq!(SearchMode::Plain.label(), "Plain");
    assert_eq!(SearchMode::Wildcard.label(), "Wildcard");
    assert_eq!(SearchMode::Regex.label(), "Regex");
}

#[test]
fn search_mode_default_is_plain() {
    assert_eq!(SearchMode::default(), SearchMode::Plain);
}

// --- Regex replace via wildcard ---

#[test]
fn wildcard_regex_replace() {
    let re = regex::Regex::new(&wildcard_to_regex("foo*bar")).unwrap();
    let result = re.replace("fooXYZbar", "replaced");
    assert_eq!(result, "replaced");
}

#[test]
fn wildcard_regex_replace_preserves_surrounding() {
    let re = regex::Regex::new(&wildcard_to_regex("world")).unwrap();
    let result = re.replace("hello world!", "earth");
    assert_eq!(result, "hello earth!");
}

#[test]
fn regex_replace_with_capture_groups() {
    let re = regex::Regex::new(r"(\d+)\.(\d+)").unwrap();
    let result = re.replace("price 12.50", "$1,$2");
    assert_eq!(result, "price 12,50");
}
