use super::*;

#[test]
fn cloud_and_connections_need_their_own_switch() {
    let tabs = vec![
        SavedTab::File {
            path: "a.csv".into(),
        },
        SavedTab::Cloud {
            conn_id: "c".into(),
            key: "k".into(),
        },
        SavedTab::Db {
            conn_id: "d".into(),
            catalog: None,
            schema: "s".into(),
            table: "t".into(),
        },
        SavedTab::Api {
            conn_id: "api".into(),
            path: None,
        },
    ];
    let mut settings = AppSettings {
        restore_session: true,
        ..Default::default()
    };
    assert_eq!(wanted(tabs.clone(), &settings).len(), 1);
    settings.restore_session_cloud = true;
    assert_eq!(wanted(tabs.clone(), &settings).len(), 2);
    settings.restore_session_connections = true;
    assert_eq!(wanted(tabs, &settings).len(), 4);
}

#[test]
fn session_file_round_trips() {
    let file = SessionFile {
        tabs: vec![
            SavedTab::File {
                path: "/x/a.csv".into(),
            },
            SavedTab::Db {
                conn_id: "d".into(),
                catalog: Some("cat".into()),
                schema: "s".into(),
                table: "t".into(),
            },
        ],
    };
    let text = toml::to_string_pretty(&file).unwrap();
    let back: SessionFile = toml::from_str(&text).unwrap();
    assert_eq!(back.tabs, file.tabs);
}

/// A tab opened from an encrypted zip is restored from the zip (which asks
/// for the passphrase again), never from its plain temporary copy.
#[test]
fn a_protected_tab_is_saved_as_its_zip() {
    let temp = std::sync::Arc::new(octa::formats::encrypt::ProtectedTempDir::new().unwrap());
    let temp_file = temp.path().join("report.csv");
    let mut tab = TabState::new(Default::default());
    tab.table.source_path = Some(temp_file.to_string_lossy().into_owned());
    tab.protected_origin = Some(crate::app::state::ProtectedOrigin {
        zip: std::path::PathBuf::from("/data/report.csv.zip"),
        entry: "report.csv".into(),
        passphrase: "pw".into(),
        temp_file,
        _temp: temp,
    });
    match saved_tab(&tab) {
        Some(SavedTab::File { path }) => assert_eq!(path, "/data/report.csv.zip"),
        other => panic!("expected the zip, got {other:?}"),
    }
}
