use super::*;

#[test]
fn european_strings_convert_to_numbers() {
    let v = CellValue::String("1.234,56".to_string());
    assert!(can_convert_value(&v, "Float64"));
    assert_eq!(convert_value(&v, "Float64"), CellValue::Float(1234.56));

    let v = CellValue::String("3,25".to_string());
    assert!(can_convert_value(&v, "Float64"));
    assert_eq!(convert_value(&v, "Float64"), CellValue::Float(3.25));

    // A European thousands group with no decimal part is a whole number.
    let v = CellValue::String("1.234.567".to_string());
    assert!(can_convert_value(&v, "Int64"));
    assert_eq!(convert_value(&v, "Int64"), CellValue::Int(1_234_567));
}

#[test]
fn plain_english_numbers_are_unchanged() {
    // The relaxed parser tries the bare English form first, so nothing that
    // worked before this feature reads differently now.
    let v = CellValue::String("1.234".to_string());
    assert!(can_convert_value(&v, "Float64"));
    assert_eq!(convert_value(&v, "Float64"), CellValue::Float(1.234));

    let v = CellValue::String("-42".to_string());
    assert!(can_convert_value(&v, "Int64"));
    assert_eq!(convert_value(&v, "Int64"), CellValue::Int(-42));
}

#[test]
fn non_numbers_are_still_refused() {
    let v = CellValue::String("abc".to_string());
    assert!(!can_convert_value(&v, "Float64"));
    assert!(!can_convert_value(&v, "Int64"));

    // A decimal is not an integer under either convention.
    let v = CellValue::String("1.234,56".to_string());
    assert!(!can_convert_value(&v, "Int64"));
}
