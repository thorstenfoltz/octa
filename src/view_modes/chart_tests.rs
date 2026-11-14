use super::*;

/// Every length of number: the first group may be one, two or three digits.
#[test]
fn counts_are_grouped_in_threes_from_the_right() {
    for (n, shown) in [
        (0, "0"),
        (7, "7"),
        (24, "24"),
        (123, "123"),
        (1_234, "1,234"),
        (12_345, "12,345"),
        (123_456, "123,456"),
        (1_234_567, "1,234,567"),
    ] {
        assert_eq!(fmt_count(n), shown);
    }
}
