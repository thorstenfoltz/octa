//! `.xlsx` styling on save. calamine (our reader) does not expose cell styles,
//! so the assertions are: the data is untouched, the workbook gained style
//! records, and a plain save is unchanged.

use octa::data::conditional_format::{CondOp, CondRule};
use octa::data::num_format::{NumberFormat, RoundingMode};
use octa::data::{CellValue, ColumnInfo, DataTable, MarkColor, MarkKey};
use octa::formats::FormatRegistry;
use octa::formats::write_options::{TableStyle, WriteOptions, XlsxOptions};
use octa::formats::xlsx_style::mark_rgb;

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "amount".into(),
            data_type: "Float64".into(),
        },
    ];
    t.rows = vec![
        vec![CellValue::String("alpha".into()), CellValue::Float(1500.5)],
        vec![CellValue::String("beta".into()), CellValue::Float(12.25)],
    ];
    t
}

fn styled() -> TableStyle {
    let mut number_formats = std::collections::HashMap::new();
    number_formats.insert(
        1,
        NumberFormat {
            decimals: Some(2),
            rounding: RoundingMode::Normal,
        },
    );
    TableStyle {
        conditional: vec![CondRule {
            column: Some(1),
            op: CondOp::Gt,
            value: "1000".into(),
            color: MarkColor::Red,
            case_sensitive: false,
        }],
        number_formats,
        frozen_cols: 1,
    }
}

fn write(table: &DataTable, opts: &WriteOptions) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("out.xlsx");
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&path).expect("xlsx reader");
    reader
        .write_file_with_options(&path, table, opts)
        .expect("write xlsx");
    std::fs::read(&path).expect("read back")
}

/// The point of the feature: styling must not disturb the values.
#[test]
fn styling_does_not_change_the_data() {
    let t = table();
    // Struct-update syntax, not field reassignment after `default()`:
    // clippy's `field_reassign_with_default` is an error under `-D warnings`.
    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(styled()),
        ..WriteOptions::default()
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("styled.xlsx");
    let reg = FormatRegistry::new();
    let reader = reg.reader_for_path(&path).expect("xlsx reader");
    reader
        .write_file_with_options(&path, &t, &opts)
        .expect("write");

    let back = reader.read_file(&path).expect("read back");
    assert_eq!(back.row_count(), 2);
    assert_eq!(back.columns.len(), 2);
    assert_eq!(back.columns[0].name, "name");
    assert_eq!(
        back.get(0, 0).map(|c| c.to_string()),
        Some("alpha".to_string())
    );
    // The number keeps full precision; only its *display* is two decimals.
    match back.get(0, 1) {
        Some(CellValue::Float(f)) => assert!((f - 1500.5).abs() < 1e-9, "got {f}"),
        other => panic!("expected a float, got {other:?}"),
    }
}

/// Off by default means the written sheet is unchanged by the feature.
///
/// The comparison is per zip entry, not whole-file: `DocProperties::new()`
/// stamps `dcterms:created` from `utc_now()`, so two workbooks written a
/// second apart differ in `docProps/core.xml` for reasons that have nothing
/// to do with this feature. Comparing the sheet and the style table is what
/// the claim actually means.
#[test]
fn plain_save_is_unchanged_by_the_feature() {
    let t = table();
    let a = write(&t, &WriteOptions::default());

    // Style present but the switch off: the writer must ignore it.
    let with_style_but_disabled = WriteOptions {
        style: Some(styled()),
        ..WriteOptions::default()
    };
    let b = write(&t, &with_style_but_disabled);

    for entry in ["xl/worksheets/sheet1.xml", "xl/styles.xml"] {
        assert_eq!(
            zip_entry(&a, entry),
            zip_entry(&b, entry),
            "an unenabled style changed {entry}"
        );
    }
}

/// A styled workbook must actually carry more style records than a plain one.
#[test]
fn styled_workbook_gains_style_records() {
    let t = table();
    let plain = write(&t, &WriteOptions::default());

    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(styled()),
        ..WriteOptions::default()
    };
    let fancy = write(&t, &opts);

    let plain_styles = zip_entry(&plain, "xl/styles.xml");
    let fancy_styles = zip_entry(&fancy, "xl/styles.xml");
    assert!(
        fancy_styles.len() > plain_styles.len(),
        "styled styles.xml ({} bytes) should exceed plain ({} bytes)",
        fancy_styles.len(),
        plain_styles.len()
    );

    let sheet = String::from_utf8_lossy(&zip_entry(&fancy, "xl/worksheets/sheet1.xml")).to_string();
    assert!(
        sheet.contains("<pane"),
        "frozen columns should emit a pane element"
    );
    assert!(
        sheet.contains("conditionalFormatting"),
        "a numeric rule should emit a live conditional format"
    );
}

/// Manual marks paint cells even where no rule applies.
#[test]
fn manual_marks_are_written() {
    let mut t = table();
    t.marks.insert(MarkKey::Cell(0, 0), MarkColor::Green);

    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(TableStyle::default()),
        ..WriteOptions::default()
    };

    let bytes = write(&t, &opts);
    let styles = String::from_utf8_lossy(&zip_entry(&bytes, "xl/styles.xml")).to_string();
    assert!(
        styles.to_lowercase().contains("22c55e"),
        "the green mark colour should appear in styles.xml"
    );
}

/// Off means off for manual marks too. `table.marks` lives on `DataTable`,
/// not `TableStyle`, so it is tempting for the writer to paint it regardless
/// of the switch; that would silently ignore the "Plain data" choice in the
/// save-time prompt for any tab carrying a mark.
#[test]
fn marks_are_not_painted_when_formatting_is_off() {
    let mut t = table();
    t.marks.insert(MarkKey::Cell(0, 0), MarkColor::Green);

    let bytes = write(&t, &WriteOptions::default());
    let styles = String::from_utf8_lossy(&zip_entry(&bytes, "xl/styles.xml")).to_string();
    assert!(
        !styles.to_lowercase().contains("22c55e"),
        "a manual mark leaked into styles.xml with formatting switched off"
    );
}

/// A case-sensitive `Eq` rule cannot be a live Excel rule (`map_rule` bakes
/// it, see `xlsx_style.rs`): it must be painted directly onto the matching
/// cell and never appear as a `conditionalFormatting` record.
#[test]
fn baked_rule_paints_cells_and_stays_out_of_live_conditional_formatting() {
    let t = table();
    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(TableStyle {
            conditional: vec![CondRule {
                column: Some(0),
                op: CondOp::Eq,
                value: "alpha".into(),
                color: MarkColor::Purple,
                case_sensitive: true,
            }],
            ..TableStyle::default()
        }),
        ..WriteOptions::default()
    };

    let bytes = write(&t, &opts);
    let sheet = String::from_utf8_lossy(&zip_entry(&bytes, "xl/worksheets/sheet1.xml")).to_string();
    assert!(
        !sheet.contains("conditionalFormatting"),
        "a case-sensitive rule cannot be a live Excel rule and must not appear as one"
    );

    let styles = String::from_utf8_lossy(&zip_entry(&bytes, "xl/styles.xml")).to_string();
    assert!(
        hex_present(&styles, MarkColor::Purple),
        "the baked colour should be painted directly onto the matching cell"
    );
}

/// Finding 1's ordering case: a baked rule first, a live rule second, both
/// matching the same cells. Once a rule bakes, every rule after it must bake
/// too (see the partition comment on `write_excel_styled`), so the second
/// rule must not be emitted as a live Excel rule even though `map_rule` would
/// otherwise export it live on its own.
#[test]
fn a_rule_after_the_first_baked_rule_is_baked_too() {
    let t = table();
    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(TableStyle {
            conditional: vec![
                CondRule {
                    column: Some(0),
                    op: CondOp::Eq,
                    value: "alpha".into(),
                    color: MarkColor::Purple,
                    case_sensitive: true, // bakes; matches row 0 only
                },
                CondRule {
                    column: Some(0),
                    op: CondOp::Contains,
                    value: "a".into(),
                    color: MarkColor::Blue,
                    case_sensitive: false, // would export live on its own; matches both rows
                },
            ],
            ..TableStyle::default()
        }),
        ..WriteOptions::default()
    };

    let bytes = write(&t, &opts);
    let sheet = String::from_utf8_lossy(&zip_entry(&bytes, "xl/worksheets/sheet1.xml")).to_string();
    assert!(
        !sheet.contains("conditionalFormatting"),
        "a rule after the first bake must itself be baked, not exported live"
    );

    let styles = String::from_utf8_lossy(&zip_entry(&bytes, "xl/styles.xml")).to_string();
    assert!(
        hex_present(&styles, MarkColor::Purple),
        "row 0 matches both rules; the earlier one should still win, same as on screen"
    );
    assert!(
        hex_present(&styles, MarkColor::Blue),
        "row 1 only matches the second rule, which should still be painted once baked"
    );
}

/// Guard against over-baking: a rule list with no baked rule at all must
/// still export every rule as a live Excel rule, which is the common case.
#[test]
fn no_baked_rules_still_export_live_conditional_formatting() {
    let t = table();
    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(styled()), // a plain Gt rule, not case sensitive: stays live
        ..WriteOptions::default()
    };

    let bytes = write(&t, &opts);
    let sheet = String::from_utf8_lossy(&zip_entry(&bytes, "xl/worksheets/sheet1.xml")).to_string();
    assert!(
        sheet.contains("conditionalFormatting"),
        "a rule list with no baked rule must still export every rule live"
    );
}

/// `cell_fill` claims cell beats row beats column, the same precedence the
/// grid uses. Each mark below uses a colour no other mark uses, so with a
/// single data cell only the precedence winner's colour can ever appear in
/// the workbook.
#[test]
fn cell_mark_beats_row_mark_beats_column_mark() {
    let one_cell = || {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "v".into(),
            data_type: "Utf8".into(),
        }];
        t.rows = vec![vec![CellValue::String("x".into())]];
        t
    };

    // A single row and single column: the row mark and column mark both
    // target the only cell that exists, with no cell mark to short-circuit.
    let mut row_vs_col = one_cell();
    row_vs_col.marks.insert(MarkKey::Row(0), MarkColor::Blue);
    row_vs_col
        .marks
        .insert(MarkKey::Column(0), MarkColor::Yellow);
    let styles = styles_xml(&row_vs_col);
    assert!(
        hex_present(&styles, MarkColor::Blue),
        "row mark should win over column mark"
    );
    assert!(
        !hex_present(&styles, MarkColor::Yellow),
        "column mark should not win over row mark"
    );

    // All three marks on the same one-cell table: the cell mark should short
    // circuit before row or column are ever consulted.
    let mut cell_vs_all = one_cell();
    cell_vs_all
        .marks
        .insert(MarkKey::Cell(0, 0), MarkColor::Green);
    cell_vs_all.marks.insert(MarkKey::Row(0), MarkColor::Blue);
    cell_vs_all
        .marks
        .insert(MarkKey::Column(0), MarkColor::Yellow);
    let styles = styles_xml(&cell_vs_all);
    assert!(
        hex_present(&styles, MarkColor::Green),
        "cell mark should win over row and column marks"
    );
    assert!(
        !hex_present(&styles, MarkColor::Blue),
        "row mark should not win when a cell mark exists"
    );
    assert!(
        !hex_present(&styles, MarkColor::Yellow),
        "column mark should not win when a cell mark exists"
    );
}

/// An explicit mark beats a matching conditional rule, even a baked one.
/// The rule below only matches the marked cell, so if the mark did not win,
/// the rule's colour would be the only one painted.
#[test]
fn explicit_mark_beats_a_matching_conditional_rule() {
    let mut t = table();
    t.marks.insert(MarkKey::Cell(0, 0), MarkColor::Green);

    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(TableStyle {
            conditional: vec![CondRule {
                column: Some(0),
                op: CondOp::Eq,
                value: "alpha".into(),
                color: MarkColor::Purple,
                case_sensitive: true, // bakes; matches only the marked cell
            }],
            ..TableStyle::default()
        }),
        ..WriteOptions::default()
    };

    let bytes = write(&t, &opts);
    let styles = String::from_utf8_lossy(&zip_entry(&bytes, "xl/styles.xml")).to_string();
    assert!(
        hex_present(&styles, MarkColor::Green),
        "the explicit mark should win over the matching conditional rule"
    );
    assert!(
        !hex_present(&styles, MarkColor::Purple),
        "the rule's colour should not be painted where a mark already wins"
    );
}

/// Write `t` with formatting on but no conditional/number-format style, and
/// return the workbook's `xl/styles.xml` as text.
fn styles_xml(t: &DataTable) -> String {
    let opts = WriteOptions {
        xlsx: XlsxOptions {
            include_formatting: true,
        },
        style: Some(TableStyle::default()),
        ..WriteOptions::default()
    };
    let bytes = write(t, &opts);
    String::from_utf8_lossy(&zip_entry(&bytes, "xl/styles.xml")).to_string()
}

/// Whether a mark colour's RGB hex appears anywhere in `styles`.
fn hex_present(styles: &str, color: MarkColor) -> bool {
    styles
        .to_lowercase()
        .contains(&format!("{:06x}", mark_rgb(color)))
}

fn zip_entry(bytes: &[u8], name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("open xlsx zip");
    let mut file = zip
        .by_name(name)
        .unwrap_or_else(|_| panic!("no {name} in workbook"));
    let mut out = Vec::new();
    file.read_to_end(&mut out).expect("read zip entry");
    out
}
