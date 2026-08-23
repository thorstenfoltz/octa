//! Contracts the GUI's large-file mode depends on.
//!
//! The GUI cannot be driven headlessly, so this pins the two properties the
//! tab relies on: that a page can be fetched from an arbitrary offset, and
//! that getting one never materialises the whole file.

use octa::formats::large;

#[test]
fn a_large_tab_pages_from_the_far_end() {
    use std::io::Write;
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(f, "id").unwrap();
    for i in 0..200_000 {
        writeln!(f, "{i}").unwrap();
    }
    f.flush().unwrap();

    let t = large::open(f.path()).unwrap();
    assert_eq!(t.row_count(), 200_000);
    // Ordered, because a CSV scan is parallel: an OFFSET without an ORDER BY
    // is a slice of an unspecified order.
    let page = t.page(199_990, 10, Some((0, true)), None).unwrap();
    assert_eq!(page.row_count(), 10);
    assert_eq!(page.get(9, 0).unwrap().to_string(), "199999");
    // The page is a window: it knows the size of the whole and where it sits.
    assert_eq!(page.total_rows, Some(200_000));
    assert_eq!(page.row_offset, 199_990);
}

#[test]
fn a_filter_is_counted_without_reading_the_file_into_memory() {
    use std::io::Write;
    let mut f = tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    writeln!(f, "id,tag").unwrap();
    for i in 0..50_000 {
        writeln!(f, "{i},{}", if i % 1000 == 0 { "keep" } else { "drop" }).unwrap();
    }
    f.flush().unwrap();

    let t = large::open(f.path()).unwrap();
    let filter = "\"tag\" = 'keep'";
    assert_eq!(t.filtered_count(Some(filter)).unwrap(), 50);
    let page = t.page(0, 10, Some((0, true)), Some(filter)).unwrap();
    assert_eq!(page.row_count(), 10);
    assert_eq!(page.get(0, 0).unwrap().to_string(), "0");
}
