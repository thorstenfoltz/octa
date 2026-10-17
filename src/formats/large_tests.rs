use super::*;
use std::io::Write;

fn big_csv(rows: usize) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(f, "id,name").unwrap();
    for i in 0..rows {
        writeln!(f, "{i},row{i}").unwrap();
    }
    f.flush().unwrap();
    f
}

#[test]
fn reports_columns_and_row_count_without_loading() {
    let f = big_csv(1000);
    let t = open(f.path()).unwrap();
    assert_eq!(t.row_count(), 1000);
    assert_eq!(t.columns().len(), 2);
    assert_eq!(t.columns()[0].name, "id");
}

#[test]
fn pages_from_the_middle() {
    let f = big_csv(1000);
    let t = open(f.path()).unwrap();
    // Ordered, because DuckDB parallelises a CSV scan: OFFSET without an
    // ORDER BY is a slice of an unspecified order, so asserting a value
    // there would be asserting an accident.
    let page = t.page(500, 3, Some((0, true)), None).unwrap();
    assert_eq!(page.row_count(), 3);
    assert_eq!(page.get(0, 0).unwrap().to_string(), "500");
    assert_eq!(page.total_rows, Some(1000), "the page knows its whole");
    assert_eq!(page.row_offset, 500);
    // Unordered still pages, it just does not promise which rows.
    assert_eq!(t.page(500, 3, None, None).unwrap().row_count(), 3);
}

#[test]
fn sorts_descending_on_a_column() {
    let f = big_csv(100);
    let t = open(f.path()).unwrap();
    let page = t.page(0, 1, Some((0, false)), None).unwrap();
    assert_eq!(page.get(0, 0).unwrap().to_string(), "99");
}

#[test]
fn filters_and_counts() {
    let f = big_csv(100);
    let t = open(f.path()).unwrap();
    let filter = "\"name\" = 'row7'";
    assert_eq!(t.filtered_count(Some(filter)).unwrap(), 1);
    let page = t.page(0, 10, None, Some(filter)).unwrap();
    assert_eq!(page.row_count(), 1);
}

#[test]
fn refuses_a_format_duckdb_cannot_scan() {
    let f = tempfile::Builder::new().suffix(".sav").tempfile().unwrap();
    assert!(ScanKind::for_path(f.path()).is_none());
}

#[test]
fn exports_the_whole_file_or_what_the_filter_keeps_in_order() {
    let f = big_csv(1000);
    let t = open(f.path()).unwrap();
    let dir = tempfile::tempdir().unwrap();

    // Whole file, sorted descending, as a semicolon CSV with a BOM.
    let out = dir.path().join("all.csv");
    let fmt = ExportFormat::Delimited {
        delimiter: b';',
        decimal_comma: false,
        bom: true,
    };
    let (job, _) = t
        .export_job(&out, fmt, Some((0, false)), String::new())
        .unwrap();
    assert_eq!(job().unwrap(), 1000);
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"\xEF\xBB\xBF"), "the BOM goes first");
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "id;name");
    assert_eq!(lines[1], "999;row999");
    assert_eq!(lines.len(), 1001);
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no temp file stays behind");

    // What a filter keeps, as Parquet, read back.
    let out = dir.path().join("some.parquet");
    let fmt = ExportFormat::for_path(&out, None, &Default::default()).unwrap();
    let (job, _) = t
        .export_job(&out, fmt, Some((0, true)), "id < 10".to_string())
        .unwrap();
    assert_eq!(job().unwrap(), 10);
    let back = open(&out).unwrap();
    assert_eq!(back.row_count(), 10);
    assert_eq!(
        back.page(0, 1, None, None)
            .unwrap()
            .get(0, 1)
            .unwrap()
            .to_string(),
        "row0"
    );
}

#[test]
fn exports_decimal_commas_and_refuses_formats_copy_cannot_write() {
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(f, "x").unwrap();
    writeln!(f, "1.5").unwrap();
    f.flush().unwrap();
    let t = open(f.path()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("eu.csv");
    let fmt = ExportFormat::for_path(&out, Some((b';', true, false)), &Default::default()).unwrap();
    let (job, _) = t.export_job(&out, fmt, None, String::new()).unwrap();
    if let Err(e) = job() {
        panic!("{e:#}");
    }
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "x\n1,5\n");
    assert!(
        ExportFormat::for_path(&dir.path().join("a.xlsx"), None, &Default::default()).is_none()
    );
}
