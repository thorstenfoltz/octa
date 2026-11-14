//! Access databases written by another program's library (Jackcess, Java),
//! read back and compared with the CSV they were made from. `products.mdb` is
//! the Access 2003 layout, `products.accdb` the Access 2010 one.
//!
//! Both hold `products` (`samples/tables/products.csv`, with `price` as
//! Currency, `in_stock` as Yes/No, `released` as Date/Time and `note` as a
//! Memo) and `categories` (each category with a `checked_at` time of day).

use octa::data::DataTable;
use octa::formats::FormatRegistry;
use std::path::{Path, PathBuf};

fn root(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn same_as_the_csv(rel: &str) {
    let registry = FormatRegistry::new();
    let csv_path = root("samples/tables/products.csv");
    let csv = registry
        .reader_for_path(&csv_path)
        .unwrap()
        .read_file(&csv_path)
        .unwrap();
    let path = root(rel);
    let reader = registry.reader_for_path(&path).unwrap();
    assert_eq!(reader.name(), "Access");
    assert!(!reader.supports_write());

    let tables = reader.list_tables(&path).unwrap().unwrap();
    let listed: Vec<(&str, Option<usize>, usize)> = tables
        .iter()
        .map(|t| (t.name.as_str(), t.row_count, t.columns.len()))
        .collect();
    assert_eq!(
        listed,
        [("categories", Some(2), 2), ("products", Some(8), 8)],
        "{rel}"
    );

    let got = reader.read_table(&path, "products").unwrap();
    let names =
        |t: &DataTable| -> Vec<String> { t.columns.iter().map(|c| c.name.clone()).collect() };
    assert_eq!(names(&got), names(&csv), "{rel}");
    let types: Vec<&str> = got.columns.iter().map(|c| c.data_type.as_str()).collect();
    assert_eq!(
        types,
        [
            "Int64",
            "Utf8",
            "Utf8",
            "Decimal128(19, 4)",
            "Boolean",
            "Date",
            "Float64",
            "Utf8"
        ],
        "{rel}"
    );
    assert_eq!(got.rows.len(), csv.rows.len(), "{rel}");
    for (r, (a, b)) in got.rows.iter().zip(&csv.rows).enumerate() {
        for col in 0..8 {
            assert_eq!(
                a[col].to_string(),
                b[col].to_string(),
                "{rel} row {r} col {col}"
            );
        }
    }

    // A column with a time of day stays a date and time.
    let categories = reader.read_table(&path, "categories").unwrap();
    assert_eq!(categories.columns[1].data_type, "DateTime", "{rel}");
    let cells: Vec<String> = categories
        .rows
        .iter()
        .flatten()
        .map(|c| c.to_string())
        .collect();
    assert_eq!(
        cells,
        [
            "drinks",
            "2024-03-01 08:30:15",
            "food",
            "2024-03-01 09:30:15"
        ],
        "{rel}"
    );
    // Without a table name the first one opens.
    assert_eq!(reader.read_file(&path).unwrap().rows.len(), 2, "{rel}");
    assert!(reader.read_table(&path, "nope").is_err(), "{rel}");
}

#[test]
fn an_access_2010_database_reads_like_its_source() {
    same_as_the_csv("tests/fixtures/products.accdb");
}

#[test]
fn an_access_2003_database_reads_like_its_source() {
    same_as_the_csv("tests/fixtures/products.mdb");
}

/// `protected.accdb` is `enc_vbaV2007.accdb` from the jetdb and Jackcess test
/// data (Apache-2.0), encrypted with the password `1234567890`. One test, so
/// nothing else reads the file between the steps.
#[test]
fn a_protected_database_opens_with_its_password_only() {
    use octa::formats::access_reader::{needs_password, unlock};
    let path = root("tests/fixtures/protected.accdb");
    let registry = FormatRegistry::new();
    let reader = registry.reader_for_path(&path).unwrap();

    assert!(needs_password(&path));
    assert!(!needs_password(&root("tests/fixtures/products.accdb")));
    assert!(!needs_password(&root("samples/tables/products.csv")));
    let refused = format!("{:#}", reader.list_tables(&path).unwrap_err());
    assert!(refused.contains("protected with a password"), "{refused}");

    let wrong = format!("{:#}", unlock(&path, "not-the-password").unwrap_err());
    assert!(wrong.contains("rejected"), "{wrong}");
    assert!(!wrong.contains("not-the-password"), "{wrong}");
    assert!(needs_password(&path));

    unlock(&path, "1234567890").unwrap();
    assert!(!needs_password(&path));
    let tables = reader.list_tables(&path).unwrap().unwrap();
    assert!(!tables.is_empty());
    let table = reader.read_table(&path, &tables[0].name).unwrap();
    assert_eq!(Some(table.row_count()), tables[0].row_count);
}
