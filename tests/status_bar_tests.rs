use octa::ui::status_bar::format_number;

#[test]
fn test_format_number_zero() {
    assert_eq!(format_number(0), "0");
}

#[test]
fn test_format_number_small() {
    assert_eq!(format_number(1), "1");
    assert_eq!(format_number(12), "12");
    assert_eq!(format_number(999), "999");
}

#[test]
fn test_format_number_thousands() {
    assert_eq!(format_number(1_000), "1,000");
    assert_eq!(format_number(1_234), "1,234");
    assert_eq!(format_number(12_345), "12,345");
    assert_eq!(format_number(999_999), "999,999");
}

#[test]
fn test_format_number_millions() {
    assert_eq!(format_number(1_000_000), "1,000,000");
    assert_eq!(format_number(1_234_567), "1,234,567");
    assert_eq!(format_number(123_456_789), "123,456,789");
}
