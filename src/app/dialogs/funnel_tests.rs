use super::*;

#[test]
fn within_days_is_empty_or_a_whole_number_from_one() {
    assert_eq!(parse_within(""), Ok(None));
    assert_eq!(parse_within(" 7 "), Ok(Some(7)));
    assert_eq!(parse_within("0"), Err(()));
    assert_eq!(parse_within("1.5"), Err(()));
    assert_eq!(parse_within("soon"), Err(()));
}
