//! European-formatted numbers must arrive as numbers on every surface, not
//! just in the GUI. These go through the shared headless entry point.

use std::io::Write;

use octa::data::CellValue;
use octa::formats::{compression, read_table_auto};

fn write_temp(name: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("octa_euro_number_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

#[test]
fn german_semicolon_csv_reads_amounts_as_numbers() {
    let path = write_temp(
        "german.csv",
        "Artikel;Betrag;Menge\nSchraube;1.234,56;10\nMutter;99,90;3\n",
    );
    let t = read_table_auto(&path, None, compression::DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();

    assert_eq!(t.columns[1].data_type, "Float64");
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(1234.56)));
    assert_eq!(t.get(1, 1), Some(&CellValue::Float(99.90)));
    // The plain integer column is untouched by the number pass.
    assert_eq!(t.columns[2].data_type, "Int64");
}

#[test]
fn ambiguous_column_stays_text_when_nobody_can_be_asked() {
    let path = write_temp("ambiguous.csv", "id,amount\n1,\"1,234\"\n2,\"2,345\"\n");
    let t = read_table_auto(&path, None, compression::DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    assert_eq!(t.columns[1].data_type, "Utf8");
}

#[test]
fn german_dates_are_not_eaten_by_the_number_pass() {
    let path = write_temp("dates.csv", "tag;wert\n31.12.2024;1,5\n01.01.2025;2,5\n");
    let t = read_table_auto(&path, None, compression::DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    // Dates stay text at the library level (the GUI date pass promotes them);
    // what matters is that they were not turned into 31122024.
    assert_eq!(
        t.get(0, 0),
        Some(&CellValue::String("31.12.2024".to_string()))
    );
    assert_eq!(t.get(0, 1), Some(&CellValue::Float(1.5)));
}
