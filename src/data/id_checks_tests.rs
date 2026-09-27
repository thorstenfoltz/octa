//! Unit tests for [`id_checks`](super). Included via `#[path]`.
//!
//! The "good" values are published example / test numbers (ISO IBAN
//! examples, card-network test cards, VAT numbers from public registers),
//! cross-checked against an independent implementation when they were added.

use super::*;

#[test]
fn ibans_pass_their_checksum_and_country_length() {
    for good in [
        "DE89370400440532013000",
        "de89 3704 0044 0532 0130 00",
        "GB82WEST12345698765432",
        "NL91ABNA0417164300",
        "FR1420041010050500013M02606",
        "NO9386011117947",
    ] {
        assert!(iban_ok(good), "{good}");
    }
    for bad in [
        "DE89370400440532013001", // last digit changed
        "GB82WEST1234569876543",  // one short for GB
        "DE8937040044053201300A", // letter in a German BBAN breaks the sum
        "XX0000000000",
        "",
    ] {
        assert!(!iban_ok(bad), "{bad}");
    }
}

#[test]
fn an_iban_from_a_country_not_in_the_table_still_gets_the_checksum() {
    // "ZZ" is not a country, but the checksum is right for it: the general
    // length bounds and mod 97 decide, so a newly added country is not red.
    let body = "123456789012";
    let r = mod97(&format!("{body}ZZ00")).unwrap();
    let check = 98 - r;
    assert!(iban_ok(&format!("ZZ{check:02}{body}")));
}

#[test]
fn card_numbers_use_luhn() {
    for good in [
        "4111111111111111",
        "4111 1111 1111 1111",
        "5500-0055-5555-5559",
        "378282246310005",
    ] {
        assert!(card_ok(good), "{good}");
    }
    for bad in ["4111111111111112", "411111111111", "not a number", "4111"] {
        assert!(!card_ok(bad), "{bad}");
    }
}

#[test]
fn barcodes_and_isbns() {
    for good in [
        "4006381333931",
        "978-0-306-40615-7",
        "036000291452",
        "96385074",
        "0-306-40615-2",
        "080442957X",
    ] {
        assert!(gtin_ok(good), "{good}");
    }
    for bad in ["4006381333932", "400638133393", "abc", "0306406153"] {
        assert!(!gtin_ok(bad), "{bad}");
    }
}

#[test]
fn vat_numbers_with_a_check_digit() {
    for good in [
        "DE136695976",
        "ATU13585627",
        "BE0411905847",
        "DK13585628",
        "FI20774740",
        "FR40303265045",
        "IT00743110157",
        "LU15027442",
        "NL004495445B01",
        "NL 004.495.445.B01",
        "PL5260250274",
        "PT501964843",
        "SE556188840401",
        "SK2022749619",
        "HR33392005961",
    ] {
        assert!(vat_ok(good), "{good}");
    }
    for bad in [
        "DE136695977",
        "DE13669597",
        "ATU13585628",
        "PL5260250275",
        "NL004495445B",
        "US123456789", // not an EU prefix
        "DE12345",
    ] {
        assert!(!vat_ok(bad), "{bad}");
    }
}

#[test]
fn vat_numbers_without_a_known_check_digit_are_format_only() {
    assert!(vat_ok("ESA12345678"));
    assert!(vat_ok("EL123456789"));
    assert!(!vat_ok("ES12345")); // wrong shape still fails
}

#[test]
fn email_is_syntax_only() {
    for good in [
        "anna@example.com",
        "  Ben@EXAMPLE.COM ",
        "carla.m+news@mail.example.org",
        "jose@bücher.de",
    ] {
        assert!(email_ok(good), "{good}");
    }
    for bad in [
        "dora@example",
        "eve@@example.com",
        "no at sign",
        ".dot@example.com",
        "a..b@example.com",
        "x@-example.com",
    ] {
        assert!(!email_ok(bad), "{bad}");
    }
}

#[test]
fn tidy_writes_valid_values_one_way_and_leaves_invalid_ones_alone() {
    assert_eq!(
        IdKind::Iban.tidy("de89370400440532013000").as_deref(),
        Some("DE89 3704 0044 0532 0130 00")
    );
    assert_eq!(
        IdKind::CardNumber.tidy("4111-1111-1111-1111").as_deref(),
        Some("4111 1111 1111 1111")
    );
    assert_eq!(
        IdKind::CardNumber.tidy("378282246310005").as_deref(),
        Some("3782 822463 10005")
    );
    assert_eq!(
        IdKind::Gtin.tidy("978-0-306-40615-7").as_deref(),
        Some("9780306406157")
    );
    assert_eq!(
        IdKind::VatId.tidy("nl 004.495.445.b01").as_deref(),
        Some("NL004495445B01")
    );
    assert_eq!(
        IdKind::Email.tidy("  Ben@EXAMPLE.COM ").as_deref(),
        Some("Ben@example.com")
    );
    assert_eq!(IdKind::Iban.tidy("DE89370400440532013001"), None);
}

#[test]
fn every_kind_round_trips_its_id() {
    for k in IdKind::ALL {
        assert_eq!(IdKind::from_id(k.id()), Some(k));
    }
}
