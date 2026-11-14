use std::time::{Duration, Instant};

use super::*;
use crate::app::cloud_browser::CloudOpenResult;

fn app() -> OctaApp {
    OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    )
}

#[test]
fn tab_mode_calls_a_cloud_tab_what_the_tab_bar_calls_it() {
    let mut app = app();
    // Opened from a bucket: the path is the temp download, the label the object.
    let mut tab = TabState::new(app.settings.default_search_mode);
    tab.table.source_path = Some("/tmp/octa-cloud-a8F3k2.csv".into());
    tab.custom_tab_label = Some("sales_2024.csv".into());
    app.tabs = vec![tab];
    let st = UnionState {
        selected_tabs: vec![true],
        plan: UnionPlan {
            columns: vec![],
            ignore_case: false,
        },
        error: None,
        size: DialogSize::default(),
        file_sources: vec![],
        file_names: vec![],
        file_tables: vec![],
        file_selected: vec![],
        ignore_case: false,
    };
    assert_eq!(source_labels(&app, &st), [app.tabs[0].title_display()]);
    assert_eq!(source_labels(&app, &st), ["sales_2024.csv"]);
}

#[test]
fn the_sidebar_union_keeps_each_objects_name_beside_its_download() {
    let dir = tempfile::tempdir().unwrap();
    let jan = dir.path().join("octa-cloud-a1B2.csv");
    let feb = dir.path().join("octa-cloud-c3D4.csv");
    std::fs::write(&jan, "x,y\n1,2\n").unwrap();
    std::fs::write(&feb, "x,y\n3,4\n").unwrap();
    let mut app = app();
    // What the cloud sidebar's download worker hands over.
    app.cloud_browser
        .pending_open
        .lock()
        .unwrap()
        .push(CloudOpenResult::UnionReady {
            files: vec![
                (jan.clone(), "jan.csv".into()),
                (feb.clone(), "feb.csv".into()),
            ],
            skipped: 0,
        });
    let ctx = egui::Context::default();
    app.drain_cloud_pending_open(&ctx);
    // The files are read on a worker; poll as the update loop does.
    let started = Instant::now();
    while app.union_dialog.is_none() && started.elapsed() < Duration::from_secs(20) {
        app.drive_union_prep(&ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    let st = app.union_dialog.as_ref().expect("the Union dialog opened");
    assert_eq!(st.file_sources, [jan, feb]);
    assert_eq!(st.file_names, ["jan.csv", "feb.csv"]);
    assert_eq!(source_labels(&app, st), ["jan.csv", "feb.csv"]);
}
