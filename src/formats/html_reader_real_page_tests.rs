/// Point `OCTA_TEST_HTML` at a real page and this reports what the reader
/// makes of it: table count, names, and how long the parse took. Skipped
/// when unset, so it costs nothing in CI.
#[test]
fn report_on_a_real_page() {
    let Ok(path) = std::env::var("OCTA_TEST_HTML") else {
        return;
    };
    let html = super::read_lossy(std::path::Path::new(&path)).unwrap();
    let started = std::time::Instant::now();
    let tables = super::parse_tables(&html);
    let elapsed = started.elapsed();
    let names = super::table_names(&tables);
    eprintln!(
        "{} bytes, {} tables, parsed in {elapsed:?}",
        html.len(),
        tables.len()
    );
    let mut widest = 0;
    for (t, name) in tables.iter().zip(&names) {
        let dt = super::to_data_table(t);
        widest = widest.max(dt.columns.len());
        eprintln!(
            "  {name}: {} cols x {} rows",
            dt.columns.len(),
            dt.row_count()
        );
    }
    assert!(!tables.is_empty(), "{path} has no table Octa can see");
    assert!(widest > 1, "every table came out one column wide");
    // A page is hundreds of kilobytes at most; anything near a second
    // means the walk has gone quadratic.
    assert!(elapsed.as_secs() < 5, "parsing took {elapsed:?}");
}
