use crate::app::state::{OctaApp, SaveMode};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::formats::FormatReader;
use octa::formats::lance_reader::LanceReader;
use octa::ui;

fn app() -> OctaApp {
    OctaApp::new(
        vec![],
        ui::settings::AppSettings::default(),
        ui::settings::IconVariant::Rose,
    )
}

fn wait_until(mut done: impl FnMut() -> bool) {
    for _ in 0..30_000 {
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("never finished");
}

/// Through the app, as a user reaches it: `load_file` on the folder, one edit,
/// Save. Both ends run on a worker, because the first use is a 100 MB download.
/// Needs the real extension, so it runs only with `OCTA_TEST_LAKEHOUSE=1`.
#[test]
fn a_dataset_opens_and_saves_on_workers_through_the_app() {
    if !std::env::var("OCTA_TEST_LAKEHOUSE").is_ok_and(|v| v == "1") {
        // The word the `lance-live` CI job fails on.
        eprintln!("skipped: OCTA_TEST_LAKEHOUSE is not 1");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.lance");
    let table = DataTable {
        columns: vec![
            ColumnInfo {
                name: "id".to_string(),
                data_type: "Int32".to_string(),
            },
            ColumnInfo {
                name: "note".to_string(),
                data_type: "Utf8".to_string(),
            },
        ],
        rows: vec![vec![CellValue::Int(1), CellValue::String("a".to_string())]],
        ..DataTable::empty()
    };
    LanceReader.write_file(&path, &table).unwrap();

    let mut app = app();
    let ctx = eframe::egui::Context::default();
    app.load_file(path.clone());
    assert!(app.pending_load.is_some(), "the read is on a worker");
    wait_until(|| {
        app.drive_pending_load(&ctx);
        app.pending_load.is_none()
    });

    let i = app.active_tab;
    assert_eq!(app.tabs[i].table.format_name.as_deref(), Some("Lance"));
    assert_eq!(app.tabs[i].table.get(0, 1).unwrap().to_string(), "a");

    app.tabs[i]
        .table
        .set(0, 1, CellValue::String("b".to_string()));
    app.do_save_tab_inner(i, path.clone(), SaveMode::Rebase, None, None, None);
    assert!(app.tabs[i].saving.is_some(), "the save is on a worker");
    wait_until(|| {
        app.drive_save_jobs(&ctx);
        app.tabs[i].saving.is_none()
    });

    assert!(!app.tabs[i].is_modified(), "{:?}", app.status_message);
    let back = LanceReader.read_file(&path).unwrap();
    assert_eq!(back.get(0, 1).unwrap().to_string(), "b");
    assert_eq!(back.columns[0].data_type, "Int32");
}

/// Each save stores the whole table again, so a timer must never do it.
#[test]
fn auto_save_leaves_a_dataset_alone() {
    let mut app = app();
    let i = app.active_tab;
    let table = &mut app.tabs[i].table;
    table.columns = vec![ColumnInfo {
        name: "id".to_string(),
        data_type: "Int64".to_string(),
    }];
    table.rows = vec![vec![CellValue::Int(1)]];
    table.set(0, 0, CellValue::Int(2));

    app.tabs[i].table.source_path = Some("/tmp/octa-no-such/d.csv".to_string());
    assert!(app.tab_is_auto_saveable(i), "a file is auto-saved");

    app.tabs[i].table.source_path = Some("/tmp/octa-no-such/d.lance".to_string());
    assert!(!app.tab_is_auto_saveable(i));
}
