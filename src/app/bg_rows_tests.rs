use super::*;

#[test]
fn eviction_follows_the_settings_row_limit() {
    // The 2M default keeps the old 3M -> 2M behaviour.
    assert_eq!(rows_to_evict(3_000_000, 2_000_000), 0);
    assert_eq!(rows_to_evict(4_000_000, 2_000_000), 2_000_000);
    // A raised limit is honoured: 5M + one 5M chunk is not cut to 2M.
    assert_eq!(rows_to_evict(7_000_000, 5_000_000), 0);
    assert_eq!(rows_to_evict(10_000_000, 5_000_000), 5_000_000);
    // Unlimited never evicts.
    assert_eq!(rows_to_evict(usize::MAX - 1, usize::MAX), 0);
}

#[test]
fn streamed_rows_get_the_conversions_the_first_rows_got() {
    use octa::data::date_infer::DateLayout;

    let col = |t: &str| ColumnInfo {
        name: "c".into(),
        data_type: t.into(),
    };
    let s = |v: &str| CellValue::String(v.into());
    // Promoted at load: a DD.MM.YYYY date, a European integer column, and a
    // date column the user turned back into text (Dismiss).
    let mut columns = vec![col("Date32"), col("Int64"), col("Utf8")];
    let casts = [
        (0, ChunkCast::Date(Promotion::Date(DateLayout::DmyDot))),
        (1, ChunkCast::Number(NumberStyle::European)),
        (2, ChunkCast::Date(Promotion::Date(DateLayout::DmyDot))),
    ];
    let mut rows = vec![
        vec![s(" 31.12.2024 "), s("1.234"), s("01.02.2024")],
        vec![s("n/a"), s("2,5"), s("")],
        vec![s(""), s(" "), CellValue::Null],
    ];
    cast_chunk(&mut rows, &mut columns, &casts, true);

    assert_eq!(
        rows[0][0],
        CellValue::Date("2024-12-31".into()),
        "trimmed, then read"
    );
    assert_eq!(rows[0][1], CellValue::Int(1234));
    assert_eq!(rows[0][2], s("01.02.2024"), "the column is text again");
    assert_eq!(rows[1][0], s("n/a"), "unreadable stays text, never empty");
    assert_eq!(rows[1][1], CellValue::Float(2.5));
    assert_eq!(
        columns[1].data_type, "Float64",
        "a fraction widens the column"
    );
    assert_eq!(rows[2][0], CellValue::Null);
    assert_eq!(rows[2][1], CellValue::Null);
}
