//! Excel formulas: read them alongside the values, and put them back only when
//! asked and only where they still mean something.

use octa::data::CellValue;
use octa::formats::write_options::WriteOptions;
use octa::formats::{FormatReader, excel_reader::ExcelReader};

/// A sheet with a header, two literal columns and a third that is computed.
fn workbook_with_formulas(path: &std::path::Path) {
    use rust_xlsxwriter::{Formula, Workbook};
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (c, name) in ["qty", "price", "total"].iter().enumerate() {
        ws.write_string(0, c as u16, *name).unwrap();
    }
    for (i, (qty, price)) in [(2.0, 1.5), (3.0, 2.0), (4.0, 0.25)].iter().enumerate() {
        let row = i as u32 + 1;
        ws.write_number(row, 0, *qty).unwrap();
        ws.write_number(row, 1, *price).unwrap();
        // `set_result` is what Excel itself stores: the formula *and* the value
        // it last produced. Without it the fixture would claim every total is
        // zero, which is not what a real workbook looks like.
        ws.write_formula(
            row,
            2,
            Formula::new(format!("=A{r}*B{r}", r = row + 1)).set_result((qty * price).to_string()),
        )
        .unwrap();
    }
    wb.save(path).unwrap();
}

/// The `<f>` elements of the first worksheet, in document order.
fn sheet_formulas(path: &std::path::Path) -> Vec<String> {
    let file = std::fs::File::open(path).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut xml = String::new();
    {
        use std::io::Read;
        let mut sheet = zip.by_name("xl/worksheets/sheet1.xml").unwrap();
        sheet.read_to_string(&mut xml).unwrap();
    }
    let mut out = Vec::new();
    let mut rest = xml.as_str();
    while let Some(start) = rest.find("<f>") {
        rest = &rest[start + 3..];
        let Some(end) = rest.find("</f>") else { break };
        out.push(rest[..end].to_string());
        rest = &rest[end + 4..];
    }
    out
}

#[test]
fn formulas_are_read_alongside_the_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("calc.xlsx");
    workbook_with_formulas(&path);

    let table = ExcelReader.read_file(&path).unwrap();
    assert_eq!(table.row_count(), 3);
    // The value is what Excel computed and stored; the formula rides beside it.
    assert_eq!(table.get(0, 2), Some(&CellValue::Float(3.0)));
    assert_eq!(table.formula(0, 2), Some("=A2*B2"));
    assert_eq!(table.formula(2, 2), Some("=A4*B4"));
    // Literal cells have none, and the map is sparse rather than a second grid.
    assert_eq!(table.formula(0, 0), None);
    assert_eq!(table.formulas.len(), 3);
}

/// The default has to be exactly what Octa wrote before this existed.
#[test]
fn a_plain_save_writes_values_not_formulas() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("calc.xlsx");
    workbook_with_formulas(&src);
    let table = ExcelReader.read_file(&src).unwrap();

    let out = dir.path().join("plain.xlsx");
    ExcelReader
        .write_file_with_options(&out, &table, &WriteOptions::default())
        .unwrap();
    assert!(
        sheet_formulas(&out).is_empty(),
        "formulas must not appear unless asked for"
    );
    let back = ExcelReader.read_file(&out).unwrap();
    assert_eq!(back.get(0, 2), Some(&CellValue::Float(3.0)));
}

#[test]
fn preserving_formulas_writes_them_back() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("calc.xlsx");
    workbook_with_formulas(&src);
    let table = ExcelReader.read_file(&src).unwrap();

    let mut opts = WriteOptions::default();
    opts.xlsx.preserve_formulas = true;
    let out = dir.path().join("kept.xlsx");
    ExcelReader
        .write_file_with_options(&out, &table, &opts)
        .unwrap();
    assert_eq!(sheet_formulas(&out), vec!["A2*B2", "A3*B3", "A4*B4"]);
    // And the value Octa showed rides along as the cached result, so the file
    // does not read as zeros until something recalculates it.
    let back = ExcelReader.read_file(&out).unwrap();
    assert_eq!(back.get(0, 2), Some(&CellValue::Float(3.0)));
    assert_eq!(back.formula(0, 2), Some("=A2*B2"));
}

/// The two retractions, which are the whole reason `DataTable::formula` is an
/// accessor rather than a plain map read.
#[test]
fn an_edited_cell_and_a_restructured_table_drop_their_formulas() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("calc.xlsx");
    workbook_with_formulas(&src);

    // An edited cell: the typed value wins, so its formula is withdrawn.
    let mut edited = ExcelReader.read_file(&src).unwrap();
    edited.edits.insert((1, 2), CellValue::Float(99.0));
    assert_eq!(edited.formula(1, 2), None);
    assert_eq!(
        edited.formula(0, 2),
        Some("=A2*B2"),
        "other cells keep theirs"
    );

    let mut opts = WriteOptions::default();
    opts.xlsx.preserve_formulas = true;
    let out = dir.path().join("edited.xlsx");
    // The real save path merges the edit overlay into the rows *before* it
    // calls the writer, so the retraction has to survive that or it protects
    // nothing. `apply_edits` drops the formula of every cell it merges.
    edited.apply_edits();
    assert!(edited.edits.is_empty());
    assert_eq!(
        edited.formula(1, 2),
        None,
        "the merge must not resurrect it"
    );
    ExcelReader
        .write_file_with_options(&out, &edited, &opts)
        .unwrap();
    assert_eq!(
        sheet_formulas(&out),
        vec!["A2*B2", "A4*B4"],
        "the edited cell must be written as its value"
    );

    // A restructured table: every reference may now point elsewhere, and Octa
    // cannot rewrite them, so none are kept.
    let mut moved = ExcelReader.read_file(&src).unwrap();
    moved.structural_changes = true;
    assert_eq!(moved.formula(0, 2), None);
    let out = dir.path().join("moved.xlsx");
    ExcelReader
        .write_file_with_options(&out, &moved, &opts)
        .unwrap();
    assert!(sheet_formulas(&out).is_empty());
}
