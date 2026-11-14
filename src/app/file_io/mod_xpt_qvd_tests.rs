use super::*;

/// Opened the way a user opens them, not only through the reader: the mail
/// formats read fine by reader while the GUI refused them.
#[test]
fn a_transport_file_and_a_qvd_open_as_tables() {
    for (file, format) in [("products.xpt", "SAS transport"), ("products.qvd", "QVD")] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(file);
        let mut app = OctaApp::new(
            vec![],
            ui::settings::AppSettings::default(),
            ui::settings::IconVariant::Rose,
        );

        app.load_file(path);

        assert_eq!(app.tabs.len(), 1, "{file}");
        let table = &app.tabs[0].table;
        assert_eq!(table.format_name.as_deref(), Some(format));
        assert_eq!((table.row_count(), table.col_count()), (8, 8), "{file}");
    }
}

/// An Access database holds several tables, so opening it asks which one.
#[test]
fn an_access_database_offers_its_tables() {
    for file in ["products.mdb", "products.accdb"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(file);
        let mut app = OctaApp::new(
            vec![],
            ui::settings::AppSettings::default(),
            ui::settings::IconVariant::Rose,
        );

        app.load_file(path);

        let picker = app.pending_table_picker.as_ref().expect(file);
        let names: Vec<&str> = picker.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["categories", "products"], "{file}");
    }
}

/// A protected Access database asks for its password, says so when it is
/// wrong and opens with the right one.
#[test]
fn a_protected_access_database_asks_for_its_password() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join("protected.accdb");
    let mut app = OctaApp::new(
        vec![],
        ui::settings::AppSettings::default(),
        ui::settings::IconVariant::Rose,
    );

    app.load_file(path.clone());
    assert!(app.passphrase_prompt.is_some());
    assert!(app.tabs.iter().all(|t| t.table.col_count() == 0));

    let secret = |p: &str| octa::formats::decrypt::OpenSecret::Passphrase(p.to_string());
    let wrong = app.open_protected(&path, &secret("wrong")).unwrap_err();
    assert!(
        wrong.contains("rejected") && !wrong.contains("wrong"),
        "{wrong}"
    );

    app.open_protected(&path, &secret("1234567890")).unwrap();
    assert!(app.passphrase_prompt.is_none());
    let opened = app.pending_table_picker.is_some()
        || app
            .tabs
            .iter()
            .any(|t| t.table.format_name.as_deref() == Some("Access"));
    assert!(opened);
}

/// A MAT-file holds variables, so opening it asks which one, in both layouts.
#[test]
fn a_mat_file_offers_its_variables() {
    for (file, count) in [("products_v5z.mat", 9), ("products_v73.mat", 5)] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(file);
        let mut app = OctaApp::new(
            vec![],
            ui::settings::AppSettings::default(),
            ui::settings::IconVariant::Rose,
        );

        app.load_file(path);

        let picker = app.pending_table_picker.as_ref().expect(file);
        assert_eq!(picker.tables.len(), count, "{file}");
        assert!(picker.tables.iter().any(|t| t.name == "products"), "{file}");
    }
}
