//! MATLAB files written by independent tools, read back cell by cell.
//!
//! `products_v5.mat` and `products_v5z.mat` (compressed) were written by
//! `scipy.io.savemat`, `products_v73.mat` by `hdf5storage.savemat(...,
//! format="7.3", matlab_compatible=True)`, all from
//! `samples/tables/products.csv`. Not by MATLAB itself.

use std::path::{Path, PathBuf};

use octa::data::{CellValue, DataTable};
use octa::formats::FormatRegistry;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read(file: &str, table: &str) -> anyhow::Result<DataTable> {
    let registry = FormatRegistry::new();
    let path = fixture(file);
    let reader = registry.reader_for_path(&path).expect("a reader for .mat");
    assert_eq!(reader.name(), "MATLAB");
    reader.read_table(&path, table)
}

fn names(t: &DataTable) -> Vec<&str> {
    t.columns.iter().map(|c| c.name.as_str()).collect()
}

fn types(t: &DataTable) -> Vec<&str> {
    t.columns.iter().map(|c| c.data_type.as_str()).collect()
}

fn column(t: &DataTable, name: &str) -> Vec<String> {
    let c = t.column_index(name).expect("column");
    t.rows.iter().map(|r| r[c].to_string()).collect()
}

const PRODUCTS: [&str; 8] = [
    "Café Latte",
    "Blaubeer-Muffin",
    "Espresso",
    "Croissant",
    "Matcha Latte",
    "Bagel",
    "Flat White",
    "Käsekuchen",
];

fn check_products(t: &DataTable) {
    assert_eq!(
        names(t),
        ["id", "product", "category", "price", "in_stock", "rating"]
    );
    assert_eq!(
        types(t),
        ["Int64", "Utf8", "Utf8", "Float64", "Boolean", "Float64"]
    );
    assert_eq!(column(t, "id"), ["1", "2", "3", "4", "5", "6", "7", "8"]);
    assert_eq!(column(t, "product"), PRODUCTS);
    assert_eq!(column(t, "price")[1], "2.75");
    assert_eq!(t.rows[2][4], CellValue::Bool(false));
    assert_eq!(column(t, "rating")[7], "4.7");
}

#[test]
fn version_5_files_read_every_kind_of_variable() {
    for file in ["products_v5.mat", "products_v5z.mat"] {
        let registry = FormatRegistry::new();
        let path = fixture(file);
        let reader = registry.reader_for_path(&path).unwrap();
        let listed: Vec<String> = reader
            .list_tables(&path)
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(
            listed,
            [
                "products", "prices", "names", "flags", "counts", "orders", "z", "sp", "cube"
            ],
            "{file}"
        );

        // A struct of equally long columns, and the first variable on a
        // plain open.
        check_products(&read(file, "products").unwrap());
        check_products(&reader.read_file(&path).unwrap());

        // A matrix: c1..cn.
        let prices = read(file, "prices").unwrap();
        assert_eq!(names(&prices), ["c1", "c2"]);
        assert_eq!(column(&prices, "c2")[0], "4.5");

        // A char matrix: one row per text row, padding dropped.
        assert_eq!(column(&read(file, "names").unwrap(), "names"), PRODUCTS);

        let flags = read(file, "flags").unwrap();
        assert_eq!(types(&flags), ["Boolean"]);
        let counts = read(file, "counts").unwrap();
        assert_eq!(types(&counts), ["Int64"]);
        assert_eq!(column(&counts, "counts")[7], "70");

        // A struct array: one row per element.
        let orders = read(file, "orders").unwrap();
        assert_eq!(names(&orders), ["qty", "item"]);
        assert_eq!(
            column(&orders, "item"),
            ["Espresso", "Croissant", "Café Latte"]
        );
        assert_eq!(types(&orders)[0], "Int64");

        // Listed, but refused with the reason.
        for (var, why) in [("z", "complex"), ("sp", "sparse"), ("cube", "3 dimensions")] {
            let err = format!("{:#}", read(file, var).unwrap_err());
            assert!(err.contains(why), "{file} {var}: {err}");
        }
    }
}

#[test]
fn a_version_7_3_file_reads_its_variables() {
    let registry = FormatRegistry::new();
    let path = fixture("products_v73.mat");
    let reader = registry.reader_for_path(&path).unwrap();
    let mut listed: Vec<String> = reader
        .list_tables(&path)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    listed.sort();
    assert_eq!(listed, ["counts", "flags", "prices", "products", "title"]);

    // Fields in MATLAB's order, text in cells followed through `#refs#`.
    check_products(&read("products_v73.mat", "products").unwrap());

    // Stored transposed: MATLAB's 8 x 2 is HDF5's 2 x 8.
    let prices = read("products_v73.mat", "prices").unwrap();
    assert_eq!(names(&prices), ["c1", "c2"]);
    assert_eq!(prices.row_count(), 8);
    assert_eq!(column(&prices, "c1")[3], "2.4");
    assert_eq!(column(&prices, "c2")[3], "3.9");

    assert_eq!(
        types(&read("products_v73.mat", "flags").unwrap()),
        ["Boolean"]
    );
    assert_eq!(
        column(&read("products_v73.mat", "counts").unwrap(), "counts")[2],
        "20"
    );
    assert_eq!(
        column(&read("products_v73.mat", "title").unwrap(), "title"),
        ["Products"]
    );
}
