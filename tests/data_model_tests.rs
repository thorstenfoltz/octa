use octa::data::*;
use std::collections::HashMap;

fn sample_table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "score".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: vec![
            vec![
                CellValue::Int(1),
                CellValue::String("Alice".into()),
                CellValue::Float(9.5),
            ],
            vec![
                CellValue::Int(2),
                CellValue::String("Bob".into()),
                CellValue::Float(7.0),
            ],
            vec![
                CellValue::Int(3),
                CellValue::String("Charlie".into()),
                CellValue::Float(8.2),
            ],
        ],
        edits: HashMap::new(),
        source_path: None,
        format_name: None,
        structural_changes: false,
        total_rows: None,
        row_offset: 0,
        marks: HashMap::new(),
        undo_stack: Vec::new(),
        redo_stack: Vec::new(),
    }
}

// --- CellValue Display ---

#[test]
fn test_cell_value_display_null() {
    assert_eq!(CellValue::Null.to_string(), "");
}

#[test]
fn test_cell_value_display_bool() {
    assert_eq!(CellValue::Bool(true).to_string(), "true");
    assert_eq!(CellValue::Bool(false).to_string(), "false");
}

#[test]
fn test_cell_value_display_int() {
    assert_eq!(CellValue::Int(42).to_string(), "42");
    assert_eq!(CellValue::Int(-1).to_string(), "-1");
}

#[test]
fn test_cell_value_display_float() {
    assert_eq!(CellValue::Float(3.0).to_string(), "3.0");
    assert_eq!(CellValue::Float(3.14).to_string(), "3.14");
}

#[test]
fn test_cell_value_display_string() {
    assert_eq!(CellValue::String("hello".into()).to_string(), "hello");
}

#[test]
fn test_cell_value_display_binary() {
    assert_eq!(CellValue::Binary(vec![1, 2, 3]).to_string(), "<3 bytes>");
}

// --- CellValue::parse_like ---

#[test]
fn test_parse_like_empty_is_null() {
    assert_eq!(
        CellValue::parse_like(&CellValue::Int(0), ""),
        CellValue::Null
    );
}

#[test]
fn test_parse_like_bool() {
    assert_eq!(
        CellValue::parse_like(&CellValue::Bool(false), "true"),
        CellValue::Bool(true)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Bool(true), "yes"),
        CellValue::Bool(true)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Bool(true), "0"),
        CellValue::Bool(false)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Bool(true), "no"),
        CellValue::Bool(false)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Bool(true), "maybe"),
        CellValue::String("maybe".into())
    );
}

#[test]
fn test_parse_like_int() {
    assert_eq!(
        CellValue::parse_like(&CellValue::Int(0), "42"),
        CellValue::Int(42)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Int(0), "abc"),
        CellValue::String("abc".into())
    );
}

#[test]
fn test_parse_like_float() {
    assert_eq!(
        CellValue::parse_like(&CellValue::Float(0.0), "3.14"),
        CellValue::Float(3.14)
    );
    assert_eq!(
        CellValue::parse_like(&CellValue::Float(0.0), "xyz"),
        CellValue::String("xyz".into())
    );
}

#[test]
fn test_parse_like_string_hint() {
    assert_eq!(
        CellValue::parse_like(&CellValue::String("".into()), "42"),
        CellValue::String("42".into())
    );
}

// --- CellValue::type_name ---

#[test]
fn test_type_name() {
    assert_eq!(CellValue::Null.type_name(), "null");
    assert_eq!(CellValue::Bool(true).type_name(), "bool");
    assert_eq!(CellValue::Int(1).type_name(), "int");
    assert_eq!(CellValue::Float(1.0).type_name(), "float");
    assert_eq!(CellValue::String("x".into()).type_name(), "string");
    assert_eq!(CellValue::Date("2024-01-01".into()).type_name(), "date");
    assert_eq!(
        CellValue::DateTime("2024-01-01T00:00:00".into()).type_name(),
        "datetime"
    );
    assert_eq!(CellValue::Binary(vec![]).type_name(), "binary");
    assert_eq!(CellValue::Nested("{}".into()).type_name(), "nested");
}

// --- DataTable basics ---

#[test]
fn test_empty_table() {
    let t = DataTable::empty();
    assert_eq!(t.row_count(), 0);
    assert_eq!(t.col_count(), 0);
    assert!(!t.is_modified());
}

#[test]
fn test_row_col_count() {
    let t = sample_table();
    assert_eq!(t.row_count(), 3);
    assert_eq!(t.col_count(), 3);
}

#[test]
fn test_get_returns_original_value() {
    let t = sample_table();
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(1)));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("Bob".into())));
}

#[test]
fn test_get_out_of_bounds() {
    let t = sample_table();
    assert_eq!(t.get(99, 0), None);
    assert_eq!(t.get(0, 99), None);
}

// --- Edit overlay ---

#[test]
fn test_set_creates_edit() {
    let mut t = sample_table();
    t.set(0, 1, CellValue::String("Alicia".into()));
    assert!(t.is_edited(0, 1));
    assert_eq!(t.get(0, 1), Some(&CellValue::String("Alicia".into())));
    assert_eq!(t.rows[0][1], CellValue::String("Alice".into()));
}

#[test]
fn test_set_out_of_bounds_ignored() {
    let mut t = sample_table();
    t.set(99, 0, CellValue::Int(999));
    assert!(!t.is_edited(99, 0));
}

#[test]
fn test_discard_edits() {
    let mut t = sample_table();
    t.set(0, 0, CellValue::Int(100));
    assert!(t.is_modified());
    t.discard_edits();
    assert!(!t.is_modified());
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(1)));
}

#[test]
fn test_apply_edits() {
    let mut t = sample_table();
    t.set(1, 1, CellValue::String("Bobby".into()));
    t.apply_edits();
    assert!(t.edits.is_empty());
    assert_eq!(t.rows[1][1], CellValue::String("Bobby".into()));
}

// --- Row operations ---

#[test]
fn test_insert_row() {
    let mut t = sample_table();
    t.insert_row(1);
    assert_eq!(t.row_count(), 4);
    assert_eq!(t.get(1, 0), Some(&CellValue::Null));
    assert_eq!(t.get(2, 1), Some(&CellValue::String("Bob".into())));
    assert!(t.structural_changes);
}

#[test]
fn test_insert_row_shifts_edits() {
    let mut t = sample_table();
    t.set(1, 0, CellValue::Int(20));
    t.set(2, 0, CellValue::Int(30));
    t.insert_row(1);
    assert_eq!(t.get(2, 0), Some(&CellValue::Int(20)));
    assert_eq!(t.get(3, 0), Some(&CellValue::Int(30)));
    assert!(!t.is_edited(0, 0));
}

#[test]
fn test_insert_row_at_end() {
    let mut t = sample_table();
    t.insert_row(100);
    assert_eq!(t.row_count(), 4);
    assert_eq!(t.get(3, 0), Some(&CellValue::Null));
}

#[test]
fn test_delete_row() {
    let mut t = sample_table();
    t.delete_row(1);
    assert_eq!(t.row_count(), 2);
    assert_eq!(t.get(0, 1), Some(&CellValue::String("Alice".into())));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("Charlie".into())));
}

#[test]
fn test_delete_row_removes_edits() {
    let mut t = sample_table();
    t.set(1, 0, CellValue::Int(99));
    t.delete_row(1);
    assert!(!t.is_edited(1, 0));
}

#[test]
fn test_delete_row_shifts_edits_down() {
    let mut t = sample_table();
    t.set(2, 0, CellValue::Int(99));
    t.delete_row(0);
    assert_eq!(t.get(1, 0), Some(&CellValue::Int(99)));
}

// --- Column operations ---

#[test]
fn test_insert_column() {
    let mut t = sample_table();
    t.insert_column(1, "middle".into(), "Utf8".into());
    assert_eq!(t.col_count(), 4);
    assert_eq!(t.columns[1].name, "middle");
    assert_eq!(t.get(0, 1), Some(&CellValue::Null));
    assert_eq!(t.get(0, 2), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_insert_column_shifts_edits() {
    let mut t = sample_table();
    t.set(0, 1, CellValue::String("edited".into()));
    t.insert_column(1, "new".into(), "Utf8".into());
    assert_eq!(t.get(0, 2), Some(&CellValue::String("edited".into())));
    assert!(!t.is_edited(0, 1));
}

#[test]
fn test_delete_column() {
    let mut t = sample_table();
    t.delete_column(1);
    assert_eq!(t.col_count(), 2);
    assert_eq!(t.columns[0].name, "id");
    assert_eq!(t.columns[1].name, "score");
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(9.5)));
}

#[test]
fn test_delete_column_shifts_edits() {
    let mut t = sample_table();
    t.set(0, 2, CellValue::Float(10.0));
    t.delete_column(1);
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(10.0)));
}

#[test]
fn test_delete_column_removes_edits() {
    let mut t = sample_table();
    t.set(0, 1, CellValue::String("edited".into()));
    t.delete_column(1);
    assert!(!t.is_edited(0, 1));
}

// --- Move operations ---

#[test]
fn test_move_row_down() {
    let mut t = sample_table();
    t.move_row(0, 2);
    assert_eq!(t.get(0, 1), Some(&CellValue::String("Bob".into())));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("Charlie".into())));
    assert_eq!(t.get(2, 1), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_move_row_up() {
    let mut t = sample_table();
    t.move_row(2, 0);
    assert_eq!(t.get(0, 1), Some(&CellValue::String("Charlie".into())));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("Alice".into())));
    assert_eq!(t.get(2, 1), Some(&CellValue::String("Bob".into())));
}

#[test]
fn test_move_row_noop() {
    let mut t = sample_table();
    t.move_row(1, 1);
    assert!(!t.structural_changes);
}

#[test]
fn test_move_row_remaps_edits() {
    let mut t = sample_table();
    t.set(0, 0, CellValue::Int(100));
    t.move_row(0, 2);
    assert_eq!(t.get(2, 0), Some(&CellValue::Int(100)));
}

#[test]
fn test_move_column() {
    let mut t = sample_table();
    t.move_column(0, 2);
    assert_eq!(t.columns[0].name, "name");
    assert_eq!(t.columns[1].name, "score");
    assert_eq!(t.columns[2].name, "id");
    assert_eq!(t.get(0, 0), Some(&CellValue::String("Alice".into())));
    assert_eq!(t.get(0, 2), Some(&CellValue::Int(1)));
}

#[test]
fn test_move_column_remaps_edits() {
    let mut t = sample_table();
    t.set(0, 0, CellValue::Int(100));
    t.move_column(0, 2);
    assert_eq!(t.get(0, 2), Some(&CellValue::Int(100)));
}

// --- Reorder columns ---

#[test]
fn test_reorder_columns() {
    let mut t = sample_table();
    t.reorder_columns(&[2, 1, 0]);
    assert_eq!(t.columns[0].name, "score");
    assert_eq!(t.columns[1].name, "name");
    assert_eq!(t.columns[2].name, "id");
    assert_eq!(t.get(0, 0), Some(&CellValue::Float(9.5)));
    assert_eq!(t.get(0, 2), Some(&CellValue::Int(1)));
}

#[test]
fn test_reorder_columns_remaps_edits() {
    let mut t = sample_table();
    t.set(0, 0, CellValue::Int(100));
    t.reorder_columns(&[2, 1, 0]);
    assert_eq!(t.get(0, 2), Some(&CellValue::Int(100)));
}

#[test]
fn test_reorder_columns_wrong_length_noop() {
    let mut t = sample_table();
    t.reorder_columns(&[0, 1]);
    assert!(!t.structural_changes);
    assert_eq!(t.columns[0].name, "id");
}

// --- Sorting ---

#[test]
fn test_sort_ascending() {
    let mut t = sample_table();
    t.sort_rows_by_column(1, true);
    assert_eq!(t.get(0, 1), Some(&CellValue::String("Alice".into())));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("Bob".into())));
    assert_eq!(t.get(2, 1), Some(&CellValue::String("Charlie".into())));
}

#[test]
fn test_sort_descending() {
    let mut t = sample_table();
    t.sort_rows_by_column(1, false);
    assert_eq!(t.get(0, 1), Some(&CellValue::String("Charlie".into())));
    assert_eq!(t.get(1, 1), Some(&CellValue::String("Bob".into())));
    assert_eq!(t.get(2, 1), Some(&CellValue::String("Alice".into())));
}

#[test]
fn test_sort_applies_edits_first() {
    let mut t = sample_table();
    t.set(0, 1, CellValue::String("Zara".into()));
    t.sort_rows_by_column(1, true);
    assert!(t.edits.is_empty());
    assert_eq!(t.get(2, 1), Some(&CellValue::String("Zara".into())));
}

#[test]
fn test_sort_with_nulls() {
    let mut t = sample_table();
    t.rows[1][1] = CellValue::Null;
    t.sort_rows_by_column(1, true);
    assert_eq!(t.get(0, 1), Some(&CellValue::Null));
}

#[test]
fn test_sort_numeric() {
    let mut t = sample_table();
    t.sort_rows_by_column(2, true);
    assert_eq!(t.get(0, 2), Some(&CellValue::Float(7.0)));
    assert_eq!(t.get(1, 2), Some(&CellValue::Float(8.2)));
    assert_eq!(t.get(2, 2), Some(&CellValue::Float(9.5)));
}

#[test]
fn test_sort_invalid_column_noop() {
    let mut t = sample_table();
    t.sort_rows_by_column(99, true);
    assert!(!t.structural_changes);
}

// --- is_modified ---

#[test]
fn test_is_modified_with_edits() {
    let mut t = sample_table();
    assert!(!t.is_modified());
    t.set(0, 0, CellValue::Int(100));
    assert!(t.is_modified());
}

#[test]
fn test_is_modified_with_structural_changes() {
    let mut t = sample_table();
    t.insert_row(0);
    assert!(t.is_modified());
}

#[test]
fn test_clear_modified() {
    let mut t = sample_table();
    t.insert_row(0);
    t.apply_edits();
    t.clear_modified();
    assert!(!t.is_modified());
}

// --- cmp_cell_values ---

#[test]
fn test_cmp_null_ordering() {
    assert_eq!(
        cmp_cell_values(&CellValue::Null, &CellValue::Null),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        cmp_cell_values(&CellValue::Null, &CellValue::Int(1)),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        cmp_cell_values(&CellValue::Int(1), &CellValue::Null),
        std::cmp::Ordering::Greater
    );
}

#[test]
fn test_cmp_int_float_cross() {
    assert_eq!(
        cmp_cell_values(&CellValue::Int(3), &CellValue::Float(3.5)),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        cmp_cell_values(&CellValue::Float(2.5), &CellValue::Int(3)),
        std::cmp::Ordering::Less
    );
}

#[test]
fn test_cmp_strings_case_insensitive() {
    assert_eq!(
        cmp_cell_values(
            &CellValue::String("apple".into()),
            &CellValue::String("Banana".into())
        ),
        std::cmp::Ordering::Less
    );
}

// --- can_convert_value / can_convert_column ---

#[test]
fn test_null_converts_to_anything() {
    for t in &[
        "String",
        "Int64",
        "Float64",
        "Boolean",
        "Date32",
        "Timestamp(Microsecond, None)",
    ] {
        assert!(can_convert_value(&CellValue::Null, t));
    }
}

#[test]
fn test_int_converts_to_string_float_bool() {
    assert!(can_convert_value(&CellValue::Int(42), "String"));
    assert!(can_convert_value(&CellValue::Int(42), "Float64"));
    assert!(can_convert_value(&CellValue::Int(1), "Boolean"));
}

#[test]
fn test_int_does_not_convert_to_date() {
    assert!(!can_convert_value(&CellValue::Int(42), "Date32"));
}

#[test]
fn test_string_to_int_valid() {
    assert!(can_convert_value(&CellValue::String("42".into()), "Int64"));
}

#[test]
fn test_string_to_int_invalid() {
    assert!(!can_convert_value(
        &CellValue::String("hello".into()),
        "Int64"
    ));
}

#[test]
fn test_string_to_float_valid() {
    assert!(can_convert_value(
        &CellValue::String("3.14".into()),
        "Float64"
    ));
}

#[test]
fn test_string_to_float_invalid() {
    assert!(!can_convert_value(
        &CellValue::String("abc".into()),
        "Float64"
    ));
}

#[test]
fn test_string_to_bool_valid() {
    assert!(can_convert_value(
        &CellValue::String("true".into()),
        "Boolean"
    ));
    assert!(can_convert_value(
        &CellValue::String("false".into()),
        "Boolean"
    ));
    assert!(can_convert_value(
        &CellValue::String("yes".into()),
        "Boolean"
    ));
    assert!(can_convert_value(&CellValue::String("0".into()), "Boolean"));
}

#[test]
fn test_string_to_bool_invalid() {
    assert!(!can_convert_value(
        &CellValue::String("maybe".into()),
        "Boolean"
    ));
}

#[test]
fn test_string_to_date_valid() {
    assert!(can_convert_value(
        &CellValue::String("2024-01-15".into()),
        "Date32"
    ));
}

#[test]
fn test_string_to_date_invalid() {
    assert!(!can_convert_value(
        &CellValue::String("not-a-date".into()),
        "Date32"
    ));
}

#[test]
fn test_float_to_int_whole() {
    assert!(can_convert_value(&CellValue::Float(3.0), "Int64"));
}

#[test]
fn test_float_to_int_fractional() {
    assert!(!can_convert_value(&CellValue::Float(3.14), "Int64"));
}

#[test]
fn test_can_convert_column_mixed() {
    let t = sample_table();
    assert!(!t.can_convert_column(1, "Int64"));
    assert!(t.can_convert_column(0, "String"));
    assert!(t.can_convert_column(0, "Float64"));
    assert!(!t.can_convert_column(2, "Int64"));
}

#[test]
fn test_empty_string_converts_to_anything() {
    assert!(can_convert_value(&CellValue::String("".into()), "Int64"));
    assert!(can_convert_value(&CellValue::String("".into()), "Boolean"));
    assert!(can_convert_value(&CellValue::String("".into()), "Date32"));
}

// --- Cell edit no-op detection ---

#[test]
fn test_parse_like_roundtrip_preserves_string() {
    let original = CellValue::String("hello".into());
    let text = original.to_string();
    let parsed = CellValue::parse_like(&original, &text);
    assert_eq!(parsed, original);
}

#[test]
fn test_parse_like_roundtrip_preserves_int() {
    let original = CellValue::Int(42);
    let text = original.to_string();
    let parsed = CellValue::parse_like(&original, &text);
    assert_eq!(parsed, original);
}

#[test]
fn test_parse_like_roundtrip_preserves_float() {
    let original = CellValue::Float(3.14);
    let text = original.to_string();
    let parsed = CellValue::parse_like(&original, &text);
    assert_eq!(parsed, original);
}

#[test]
fn test_parse_like_roundtrip_preserves_bool() {
    let original = CellValue::Bool(true);
    let text = original.to_string();
    let parsed = CellValue::parse_like(&original, &text);
    assert_eq!(parsed, original);
}

#[test]
fn test_noop_edit_does_not_mark_as_edited() {
    let mut table = sample_table();
    let original = table.get(0, 0).cloned().unwrap();
    let text = original.to_string();
    let new_val = CellValue::parse_like(&original, &text);
    // Simulate the fixed logic: only set if different
    if new_val != original {
        table.set(0, 0, new_val);
    }
    assert!(!table.is_edited(0, 0));
}

#[test]
fn test_actual_edit_marks_as_edited() {
    let mut table = sample_table();
    let original = table.get(0, 1).cloned().unwrap(); // "Alice"
    let new_val = CellValue::parse_like(&original, "Alicia");
    if new_val != original {
        table.set(0, 1, new_val);
    }
    assert!(table.is_edited(0, 1));
}

// --- format_number (thousand separator) ---

#[test]
fn test_format_number_small() {
    assert_eq!(octa::ui::status_bar::format_number(0), "0");
    assert_eq!(octa::ui::status_bar::format_number(1), "1");
    assert_eq!(octa::ui::status_bar::format_number(999), "999");
}

#[test]
fn test_format_number_thousands() {
    assert_eq!(octa::ui::status_bar::format_number(1_000), "1,000");
    assert_eq!(octa::ui::status_bar::format_number(12_345), "12,345");
    assert_eq!(octa::ui::status_bar::format_number(999_999), "999,999");
}

#[test]
fn test_format_number_millions() {
    assert_eq!(octa::ui::status_bar::format_number(1_000_000), "1,000,000");
    assert_eq!(
        octa::ui::status_bar::format_number(123_456_789),
        "123,456,789"
    );
}

// --- Replace logic (data-level) ---

#[test]
fn test_replace_in_string_cell() {
    let mut table = sample_table();
    // Replace "Alice" with "Alicia"
    let val = table.get(0, 1).cloned().unwrap();
    let new_val = CellValue::parse_like(&val, "Alicia");
    assert_ne!(new_val, val);
    table.set(0, 1, new_val);
    assert_eq!(
        table.get(0, 1).unwrap(),
        &CellValue::String("Alicia".into())
    );
}

#[test]
fn test_replace_preserves_type_for_int() {
    let mut table = sample_table();
    let val = table.get(0, 0).cloned().unwrap(); // Int(1)
    let new_val = CellValue::parse_like(&val, "99");
    assert_eq!(new_val, CellValue::Int(99));
    table.set(0, 0, new_val);
    assert_eq!(table.get(0, 0).unwrap(), &CellValue::Int(99));
}

#[test]
fn test_replace_all_matching_cells() {
    let mut table = sample_table();
    // Replace all scores > 0 (all of them) with new value
    let count = (0..table.row_count())
        .filter(|&row| {
            let val = table.get(row, 2).cloned().unwrap();
            let text = val.to_string();
            if text.contains('.') {
                let new_val = CellValue::parse_like(&val, "0.0");
                if new_val != val {
                    table.set(row, 2, new_val);
                    return true;
                }
            }
            false
        })
        .count();
    assert_eq!(count, 3); // all 3 rows had floats
    for row in 0..3 {
        assert_eq!(table.get(row, 2).unwrap(), &CellValue::Float(0.0));
    }
}

// --- Settings serialization ---

#[test]
fn test_settings_defaults() {
    let settings = octa::ui::settings::AppSettings::default();
    assert_eq!(settings.font_size, 13.0);
    assert!(settings.show_row_numbers);
    assert!(settings.alternating_row_colors);
    assert!(!settings.negative_numbers_red);
    assert_eq!(settings.default_search_mode, SearchMode::Plain);
    assert_eq!(settings.tab_size, 4);
    assert_eq!(settings.max_recent_files, 5);
}

#[test]
fn test_settings_tab_size_default() {
    let settings = octa::ui::settings::AppSettings::default();
    assert_eq!(settings.tab_size, 4);
}

#[test]
fn test_settings_tab_size_deserialization_missing() {
    // When tab_size is missing from TOML, it should default to 4
    let toml_str = r#"
font_size = 14.0
default_theme = "Dark"
icon_variant = "Rose"
"#;
    let settings: octa::ui::settings::AppSettings = toml::from_str(toml_str).unwrap();
    assert_eq!(settings.tab_size, 4);
}

#[test]
fn test_settings_tab_size_deserialization_present() {
    let toml_str = r#"
font_size = 14.0
default_theme = "Dark"
icon_variant = "Rose"
tab_size = 2
"#;
    let settings: octa::ui::settings::AppSettings = toml::from_str(toml_str).unwrap();
    assert_eq!(settings.tab_size, 2);
}

#[test]
fn test_settings_serialization_roundtrip() {
    let settings = octa::ui::settings::AppSettings::default();
    let toml_str = toml::to_string_pretty(&settings).unwrap();
    let deserialized: octa::ui::settings::AppSettings = toml::from_str(&toml_str).unwrap();
    assert_eq!(deserialized.tab_size, settings.tab_size);
    assert_eq!(deserialized.max_recent_files, settings.max_recent_files);
    assert_eq!(deserialized.font_size, settings.font_size);
}

// --- Number alignment tests ---

#[test]
fn test_is_right_aligned() {
    use octa::ui::table_view::is_right_aligned;

    assert!(is_right_aligned(&CellValue::Int(42)));
    assert!(is_right_aligned(&CellValue::Int(-1)));
    assert!(is_right_aligned(&CellValue::Float(3.14)));
    assert!(is_right_aligned(&CellValue::Float(0.0)));

    assert!(!is_right_aligned(&CellValue::String("123".into())));
    assert!(!is_right_aligned(&CellValue::String("hello".into())));
    assert!(!is_right_aligned(&CellValue::Null));
    assert!(!is_right_aligned(&CellValue::Bool(true)));
    assert!(!is_right_aligned(&CellValue::Date("2024-01-01".into())));
    assert!(!is_right_aligned(&CellValue::DateTime(
        "2024-01-01 00:00:00".into()
    )));
    assert!(!is_right_aligned(&CellValue::Nested("{}".into())));
    assert!(!is_right_aligned(&CellValue::Binary(vec![1, 2, 3])));
}

// --- Column type conversion tests ---

#[test]
fn test_convert_value_int_to_float() {
    assert_eq!(
        convert_value(&CellValue::Int(42), "Float64"),
        CellValue::Float(42.0)
    );
    assert_eq!(
        convert_value(&CellValue::Int(-5), "Float64"),
        CellValue::Float(-5.0)
    );
    assert_eq!(
        convert_value(&CellValue::Int(0), "Float64"),
        CellValue::Float(0.0)
    );
}

#[test]
fn test_convert_value_float_to_int() {
    assert_eq!(
        convert_value(&CellValue::Float(3.0), "Int64"),
        CellValue::Int(3)
    );
    assert_eq!(
        convert_value(&CellValue::Float(-7.0), "Int64"),
        CellValue::Int(-7)
    );
    assert_eq!(
        convert_value(&CellValue::Float(0.0), "Int64"),
        CellValue::Int(0)
    );
}

#[test]
fn test_convert_value_int_to_string() {
    assert_eq!(
        convert_value(&CellValue::Int(42), "Utf8"),
        CellValue::String("42".into())
    );
    assert_eq!(
        convert_value(&CellValue::Int(42), "String"),
        CellValue::String("42".into())
    );
}

#[test]
fn test_convert_value_float_to_string() {
    assert_eq!(
        convert_value(&CellValue::Float(3.14), "String"),
        CellValue::String("3.14".into())
    );
}

#[test]
fn test_convert_value_string_to_int() {
    assert_eq!(
        convert_value(&CellValue::String("42".into()), "Int64"),
        CellValue::Int(42)
    );
}

#[test]
fn test_convert_value_string_to_float() {
    assert_eq!(
        convert_value(&CellValue::String("3.14".into()), "Float64"),
        CellValue::Float(3.14)
    );
}

#[test]
fn test_convert_value_string_to_bool() {
    assert_eq!(
        convert_value(&CellValue::String("true".into()), "Boolean"),
        CellValue::Bool(true)
    );
    assert_eq!(
        convert_value(&CellValue::String("false".into()), "Boolean"),
        CellValue::Bool(false)
    );
    assert_eq!(
        convert_value(&CellValue::String("1".into()), "Boolean"),
        CellValue::Bool(true)
    );
    assert_eq!(
        convert_value(&CellValue::String("no".into()), "Boolean"),
        CellValue::Bool(false)
    );
}

#[test]
fn test_convert_value_bool_to_int() {
    assert_eq!(
        convert_value(&CellValue::Bool(true), "Int64"),
        CellValue::Int(1)
    );
    assert_eq!(
        convert_value(&CellValue::Bool(false), "Int64"),
        CellValue::Int(0)
    );
}

#[test]
fn test_convert_value_bool_to_float() {
    assert_eq!(
        convert_value(&CellValue::Bool(true), "Float64"),
        CellValue::Float(1.0)
    );
    assert_eq!(
        convert_value(&CellValue::Bool(false), "Float64"),
        CellValue::Float(0.0)
    );
}

#[test]
fn test_convert_value_null_stays_null() {
    assert_eq!(convert_value(&CellValue::Null, "Int64"), CellValue::Null);
    assert_eq!(convert_value(&CellValue::Null, "Float64"), CellValue::Null);
    assert_eq!(convert_value(&CellValue::Null, "String"), CellValue::Null);
    assert_eq!(convert_value(&CellValue::Null, "Boolean"), CellValue::Null);
}

#[test]
fn test_convert_value_empty_string_to_null() {
    assert_eq!(
        convert_value(&CellValue::String("".into()), "Int64"),
        CellValue::Null
    );
    assert_eq!(
        convert_value(&CellValue::String("".into()), "Float64"),
        CellValue::Null
    );
}

#[test]
fn test_convert_value_date_to_string() {
    assert_eq!(
        convert_value(&CellValue::Date("2024-01-15".into()), "String"),
        CellValue::String("2024-01-15".into())
    );
}

#[test]
fn test_convert_value_datetime_to_date() {
    assert_eq!(
        convert_value(&CellValue::DateTime("2024-01-15 10:30:00".into()), "Date32"),
        CellValue::Date("2024-01-15".into())
    );
}

#[test]
fn test_convert_value_date_to_datetime() {
    assert_eq!(
        convert_value(
            &CellValue::Date("2024-01-15".into()),
            "Timestamp(Microsecond, None)"
        ),
        CellValue::DateTime("2024-01-15 00:00:00".into())
    );
}

#[test]
fn test_can_convert_value_rejects_fractional_float_to_int() {
    assert!(!can_convert_value(&CellValue::Float(3.5), "Int64"));
    assert!(can_convert_value(&CellValue::Float(3.0), "Int64"));
}

#[test]
fn test_can_convert_value_rejects_non_numeric_string_to_int() {
    assert!(!can_convert_value(
        &CellValue::String("abc".into()),
        "Int64"
    ));
    assert!(can_convert_value(&CellValue::String("42".into()), "Int64"));
}

#[test]
fn test_convert_column_int_to_float() {
    let mut table = sample_table();
    // Column 0 is "id" (Int64)
    assert!(table.convert_column(0, "Float64"));
    assert_eq!(table.columns[0].data_type, "Float64");
    assert_eq!(table.rows[0][0], CellValue::Float(1.0));
    assert_eq!(table.rows[1][0], CellValue::Float(2.0));
    assert_eq!(table.rows[2][0], CellValue::Float(3.0));
}

#[test]
fn test_convert_column_float_to_int() {
    let mut table = sample_table();
    // Column 2 is "score" (Float64): 9.5, 7.0, 8.2
    // 9.5 and 8.2 have fractional parts, so this should fail
    assert!(!table.convert_column(2, "Int64"));
    // Column data should be unchanged
    assert_eq!(table.columns[2].data_type, "Float64");
    assert_eq!(table.rows[0][2], CellValue::Float(9.5));
}

#[test]
fn test_convert_column_with_edits() {
    let mut table = sample_table();
    // Edit row 0, col 0 from Int(1) to Int(10)
    table.set(0, 0, CellValue::Int(10));
    assert!(table.convert_column(0, "Float64"));
    // The edit should also be converted
    let val = table.get(0, 0).unwrap();
    assert_eq!(*val, CellValue::Float(10.0));
}

#[test]
fn test_convert_column_undo() {
    let mut table = sample_table();
    assert!(table.convert_column(0, "Float64"));
    assert_eq!(table.rows[0][0], CellValue::Float(1.0));
    // Undo
    assert!(table.undo());
    assert_eq!(table.columns[0].data_type, "Int64");
    assert_eq!(table.rows[0][0], CellValue::Int(1));
}

#[test]
fn test_convert_column_undo_redo() {
    let mut table = sample_table();
    assert!(table.convert_column(0, "Float64"));
    assert!(table.undo());
    assert_eq!(table.rows[0][0], CellValue::Int(1));
    // Redo
    assert!(table.redo());
    assert_eq!(table.columns[0].data_type, "Float64");
    assert_eq!(table.rows[0][0], CellValue::Float(1.0));
}

#[test]
fn test_convert_column_same_type_noop() {
    let mut table = sample_table();
    assert!(table.convert_column(0, "Int64"));
    // Should succeed but not push undo action (it's a no-op)
    assert!(
        table.undo_stack.is_empty()
            || table
                .undo_stack
                .last()
                .map(|a| matches!(a, UndoAction::ConvertColumn { .. }))
                .unwrap_or(false)
                == false
    );
}

#[test]
fn test_convert_column_invalid_index() {
    let mut table = sample_table();
    assert!(!table.convert_column(99, "Float64"));
}
