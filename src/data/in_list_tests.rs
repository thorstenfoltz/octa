use super::*;

fn s(v: &str) -> CellValue {
    CellValue::String(v.to_string())
}

#[test]
fn quotes_text_and_doubles_inner_quotes() {
    let v = [s("A-17"), s("O'Brien")];
    assert_eq!(sql_in_list(&v).unwrap(), "('A-17', 'O''Brien')");
}

#[test]
fn numbers_stay_bare_unless_mixed() {
    let nums = [CellValue::Int(3), CellValue::Float(2.5)];
    assert_eq!(sql_in_list(&nums).unwrap(), "(3, 2.5)");
    let mixed = [CellValue::Int(3), s("007")];
    assert_eq!(sql_in_list(&mixed).unwrap(), "('3', '007')");
}

#[test]
fn skips_empty_and_repeats() {
    let v = [s("a"), CellValue::Null, s("b"), s("a")];
    assert_eq!(sql_in_list(&v).unwrap(), "('a', 'b')");
    assert_eq!(sql_in_list(&[CellValue::Null]), None);
}
