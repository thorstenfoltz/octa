use super::*;
use crate::app::file_io::passes::Promotion;
use octa::data::date_infer::DateLayout;
use octa::data::num_parse::NumberStyle;

fn text_column() -> ColumnInfo {
    ColumnInfo {
        name: "c".to_string(),
        data_type: "Utf8".to_string(),
    }
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.to_string())
}

#[test]
fn a_date_answer_converts_the_copy_as_the_picker_did() {
    let (values, ty) = convert(
        text_column(),
        vec![s("02/03/2024"), CellValue::Null, s("not a date")],
        ChunkCast::Date(Promotion::Date(DateLayout::DmySlash)),
    );
    assert_eq!(ty, octa::data::date_infer::DATE_TYPE);
    assert_eq!(values[0], CellValue::Date("2024-03-02".to_string()));
    assert_eq!(values[1], CellValue::Null);
    // The picker's own rule: a value its layout cannot read becomes empty.
    assert_eq!(values[2], CellValue::Null);
}

#[test]
fn a_number_answer_converts_the_copy_as_the_picker_did() {
    let (values, ty) = convert(
        text_column(),
        vec![s("1.234,5"), s("2,0"), CellValue::Null],
        ChunkCast::Number(NumberStyle::European),
    );
    assert_eq!(ty, "Float64");
    assert_eq!(
        values,
        vec![
            CellValue::Float(1234.5),
            CellValue::Float(2.0),
            CellValue::Null
        ]
    );
}
