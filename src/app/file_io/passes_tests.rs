use super::format_is_text_document;

#[test]
fn text_document_formats_are_excluded_from_trim() {
    for name in ["Text", "Markdown", "Jupyter Notebook", "EPUB"] {
        assert!(
            format_is_text_document(Some(name)),
            "{name} should be treated as a non-tabular text document"
        );
    }
}

#[test]
fn tabular_and_unknown_formats_still_trim() {
    for name in ["CSV", "Parquet", "JSON", "YAML"] {
        assert!(!format_is_text_document(Some(name)));
    }
    assert!(!format_is_text_document(None));
}

fn dated(cells: &[&str]) -> octa::data::DataTable {
    let mut t = octa::data::DataTable::empty();
    t.format_name = Some("CSV".into());
    t.columns = vec![octa::data::ColumnInfo {
        name: "d".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = cells
        .iter()
        .map(|s| vec![octa::data::CellValue::String((*s).into())])
        .collect();
    t
}

/// What the reader thread runs: dates only when switched on, and after the
/// trim, so a padded value is still a date.
#[test]
fn dates_are_promoted_only_when_switched_on_and_after_the_trim() {
    use super::{PassSwitches, run_load_passes};
    let off = PassSwitches {
        trim: true,
        clean_headers: false,
        dates: false,
    };
    let mut t = dated(&[" 15.04.2024 ", "01.12.2025"]);
    let passes = run_load_passes(&mut t, off);
    assert_eq!(t.columns[0].data_type, "Utf8");
    assert!(passes.dates.promoted.is_empty());

    let mut t = dated(&[" 15.04.2024 ", "01.12.2025"]);
    let passes = run_load_passes(&mut t, PassSwitches { dates: true, ..off });
    assert_eq!(t.columns[0].data_type, "Date32");
    assert_eq!(passes.dates.promoted, [0]);
    // Not ISO on disk, so the banner gets the text to put back.
    assert_eq!(passes.dates.reformatted.len(), 1);
}
