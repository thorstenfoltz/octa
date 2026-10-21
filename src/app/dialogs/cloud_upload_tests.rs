use super::*;
use octa::cloud::{CloudConnection, CloudKind, CloudSecret};
use octa::data::{CellValue, ColumnInfo, DataTable};
use octa::ui::settings::AppSettings;

fn two_rows() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "id".into(),
        data_type: "Int64".into(),
    }];
    t.rows = vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]];
    t
}

fn sftp(host: &str, port: u16, user: &str, start: Option<&str>) -> CloudConnection {
    CloudConnection {
        id: format!("upload-test-{}", std::process::id()),
        name: "files".into(),
        kind: CloudKind::Sftp,
        bucket: octa::cloud::sftp_authority(host, port),
        prefix: start.map(str::to_string),
        username: Some(user.into()),
        ssh_auth: Some(octa::ssh::SshAuth::Password),
        accept_new_host_key: true,
        allow_writes: true,
        ..Default::default()
    }
}

/// An app holding `two_rows` and the one connection `conn`, its password in
/// the settings rather than the keyring.
fn app_with(conn: CloudConnection, password: &str) -> OctaApp {
    let mut settings = AppSettings::default();
    settings.cloud_secrets.insert(
        conn.id.clone(),
        serde_json::to_string(&CloudSecret::Password(password.into())).unwrap(),
    );
    settings.cloud_connections.push(conn);
    let mut app = OctaApp::new(vec![], settings, octa::ui::settings::IconVariant::Rose);
    app.tabs[0].table = two_rows();
    // What the app's filter pass fills in: every row shown.
    app.tabs[0].filtered_rows = vec![0, 1];
    app
}

/// Run the save jobs, as the frame loop does, until every pending upload
/// has been handed to the worker.
fn land_saves(app: &mut OctaApp) {
    let ctx = egui::Context::default();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !app.pending_uploads.is_empty() {
        assert!(std::time::Instant::now() < until, "the save never landed");
        app.drive_save_jobs(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// What the upload worker reported, waiting up to `secs`.
fn upload_status(app: &OctaApp, secs: u64) -> String {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < until {
        if let Some(msg) = app.cloud_browser.status.lock().unwrap().take() {
            return msg;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("no upload status after {secs} s");
}

#[test]
fn the_path_starts_in_the_connections_start_folder() {
    let c = sftp("h", 22, "u", Some("home/anna/"));
    assert_eq!(default_key(&c, None, "t.csv"), "home/anna/t.csv");
    // A folder picked in the tree wins.
    assert_eq!(default_key(&c, Some("srv/in/"), "t.csv"), "srv/in/t.csv");
    let top = sftp("h", 22, "u", None);
    assert_eq!(default_key(&top, None, "t.csv"), "t.csv");
    assert_eq!(default_key(&top, Some(""), "t.csv"), "t.csv");
}

#[test]
fn an_upload_says_why_it_cannot_start() {
    let mut app = app_with(sftp("127.0.0.1", 1, "u", None), "pw");
    app.open_cloud_upload(None);
    let mut st = app.cloud_upload_dialog.take().unwrap();
    assert_eq!(st.key, "untitled.csv", "a tab without a file");

    st.key = "out/".into();
    assert_eq!(app.upload_problem(&st), Some(t("upload.need_name")));
    // A format Octa only reads. An unknown ending is written as text, as
    // Save As does.
    st.key = "out/t.pdf".into();
    assert_eq!(
        app.upload_problem(&st),
        Some(t("upload.no_writer").replace("{name}", "t.pdf"))
    );
    st.key = "out/t.parquet".into();
    assert_eq!(app.upload_problem(&st), None);

    app.settings.cloud_connections[0].allow_writes = false;
    assert_eq!(
        app.upload_problem(&st),
        Some(t("cloud.conn_writes_off").replace("{name}", "files"))
    );
    st.conn_id = "gone".into();
    assert_eq!(app.upload_problem(&st), Some(t("upload.pick_conn")));
}

#[test]
fn an_upload_writes_the_table_and_hands_it_to_the_worker() {
    // Port 1 refuses at once: the whole chain runs, offline, up to the
    // worker's report. A small Parquet is written at once, an .xlsx by a
    // background save job: both land in the upload.
    for name in ["t.parquet", "t.xlsx"] {
        let mut app = app_with(sftp("127.0.0.1", 1, "u", None), "pw");
        app.open_cloud_upload(None);
        let mut st = app.cloud_upload_dialog.take().unwrap();
        st.key = format!("out/{name}");
        assert!(app.start_upload(&mut st), "{:?}", st.error);
        land_saves(&mut app);
        let msg = upload_status(&app, 20);
        assert!(msg.starts_with(&t("cloud.upload_failed")), "{msg}");
        assert!(
            msg.contains(&format!("sftp://127.0.0.1:1/out/{name}")),
            "{msg}"
        );
        // The tab is untouched: still no file of its own, nothing to save.
        assert!(app.tabs[0].table.source_path.is_none());
    }
}

#[test]
fn a_csv_upload_asks_the_csv_questions_under_its_own_name() {
    let mut app = app_with(sftp("127.0.0.1", 1, "u", None), "pw");
    app.open_cloud_upload(None);
    let mut st = app.cloud_upload_dialog.take().unwrap();
    st.key = "out/t.csv".into();
    assert!(app.start_upload(&mut st));
    let prompt = app.pending_save_as.as_ref().expect("the CSV questions");
    assert_eq!(
        prompt.upload.as_deref(),
        Some("sftp://127.0.0.1:1/out/t.csv")
    );
    assert!(
        !prompt.ask_open && !prompt.open,
        "nothing to open afterwards"
    );
    assert!(prompt.csv.is_some());
    assert_eq!(app.pending_uploads.len(), 1, "waits for the answers");
}

/// Against a real server, like `tests/storage_live_tests.rs`:
/// `OCTA_TEST_SFTP_URL='host=127.0.0.1;port=2222;user=octa;pass=octa_test_pw;dir=upload'`.
#[test]
fn an_upload_reaches_a_real_sftp_server() {
    let Ok(raw) = std::env::var("OCTA_TEST_SFTP_URL") else {
        println!("skipped: OCTA_TEST_SFTP_URL not set");
        return;
    };
    let s: std::collections::HashMap<&str, &str> =
        raw.split(';').filter_map(|p| p.split_once('=')).collect();
    let port = s.get("port").and_then(|p| p.parse().ok()).unwrap_or(22);
    let dir = s.get("dir").map(|d| format!("{}/", d.trim_matches('/')));
    let conn = sftp(s["host"], port, s["user"], dir.as_deref());
    let mut app = app_with(conn.clone(), s["pass"]);
    app.tabs[0].table.source_path = Some("/somewhere/sales.parquet".into());
    app.open_cloud_upload(None);
    let mut st = app.cloud_upload_dialog.take().unwrap();
    let key = format!(
        "{}octa-upload-{}/sales.parquet",
        dir.clone().unwrap_or_default(),
        std::process::id()
    );
    assert_eq!(
        st.key,
        format!("{}sales.parquet", dir.unwrap_or_default()),
        "start folder plus the tab's file name"
    );
    st.key = key.clone();
    assert!(app.start_upload(&mut st), "{:?}", st.error);
    land_saves(&mut app);
    let msg = upload_status(&app, 60);
    assert!(msg.starts_with(&t("cloud.uploaded")), "{msg}");

    let provider = octa::cloud::build_provider(
        &conn,
        &octa::cloud::ProviderCreds::Password(s["pass"].into()),
    )
    .unwrap();
    let bytes = provider.get(&key).unwrap();
    assert!(bytes.starts_with(b"PAR1"), "a Parquet file");
    provider.delete(&key).unwrap();
}

/// One frame of the dialog; returns what was painted.
fn frame(
    ctx: &egui::Context,
    app: &mut OctaApp,
    events: Vec<egui::Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let input = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1200.0, 900.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| render_cloud_upload_dialog(app, ui.ctx()));
    out.textures_delta.clear();
    out.shapes
}

/// Where the text `want` was painted.
fn painted_at(shapes: &[egui::epaint::ClippedShape], want: &str) -> Option<egui::Pos2> {
    fn walk(s: &egui::Shape, want: &str) -> Option<egui::Pos2> {
        match s {
            egui::Shape::Text(t) if t.galley.text() == want => {
                Some(t.visual_bounding_rect().center())
            }
            egui::Shape::Vec(v) => v.iter().find_map(|s| walk(s, want)),
            _ => None,
        }
    }
    shapes.iter().find_map(|c| walk(&c.shape, want))
}

#[test]
fn clicking_upload_in_the_dialog_starts_it() {
    let mut app = app_with(sftp("127.0.0.1", 1, "u", Some("home/u/")), "pw");
    app.open_cloud_upload(None);
    app.cloud_upload_dialog.as_mut().unwrap().key = "home/u/t.parquet".into();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![]);
    let shapes = frame(&ctx, &mut app, vec![]);
    // The address it resolves to is shown under the path.
    assert!(painted_at(&shapes, "sftp://127.0.0.1:1/home/u/t.parquet").is_some());
    let pos = painted_at(&shapes, &t("common.save")).expect("the Upload button");
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    frame(&ctx, &mut app, vec![egui::Event::PointerMoved(pos)]);
    frame(&ctx, &mut app, vec![button(true)]);
    frame(&ctx, &mut app, vec![button(false)]);
    assert!(app.cloud_upload_dialog.is_none(), "the dialog closes");
    land_saves(&mut app);
    let msg = upload_status(&app, 20);
    assert!(msg.contains("sftp://127.0.0.1:1/home/u/t.parquet"), "{msg}");
}

#[test]
fn without_a_connection_the_dialog_points_at_settings() {
    let mut app = OctaApp::new(
        vec![],
        AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs[0].table = two_rows();
    app.open_cloud_upload(None);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![]);
    let shapes = frame(&ctx, &mut app, vec![]);
    assert!(painted_at(&shapes, &t("upload.no_conns")).is_some());
    assert!(painted_at(&shapes, &t("dialog.versions_open_settings")).is_some());
}
