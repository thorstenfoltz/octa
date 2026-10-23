//! Long texts in Settings wrap at the window's edge instead of running past
//! it, and can be marked and copied.

use super::*;
use crate::cloud::CloudKind;
use crate::ssh::SshAuth;
use crate::ui::settings::AppSettings;

/// Narrower than the dialog's own minimum, so anything that only fits by
/// luck at the default width shows up.
const WIDTH: f32 = 560.0;

/// Draws one part of the dialog.
type Draw = fn(&mut SettingsDialog, &mut egui::Ui);

/// Texts cut off by the right edge of a `WIDTH` wide panel holding `draw`.
/// Text a widget clips to itself (a hint inside a text field) is fine: that
/// is how a field shows a long value, and its clip ends before the edge.
fn cut_off(dialog: &mut SettingsDialog, draw: Draw) -> Vec<String> {
    texts(dialog, draw)
        .into_iter()
        .filter(|(_, right, clip)| *clip >= WIDTH - 1.0 && *right > clip + 1.0)
        .map(|(text, _, _)| text)
        .collect()
}

/// Every text `draw` paints in a `WIDTH` wide panel, with where it ends and
/// where its clip ends. Text wholly past the edge is not painted at all.
fn texts(dialog: &mut SettingsDialog, draw: Draw) -> Vec<(String, f32, f32)> {
    fn walk(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<(String, f32, f32)>) {
        match shape {
            egui::Shape::Text(t) => out.push((
                t.galley.text().to_owned(),
                t.visual_bounding_rect().right(),
                clip.right(),
            )),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    let mut out = Vec::new();
    // A grid lays itself out invisibly on its first pass.
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(WIDTH, 6000.0),
            )),
            ..Default::default()
        };
        let mut full = ctx.run_ui(input, |ui| draw(dialog, ui));
        full.textures_delta.clear();
        out.clear();
        for c in &full.shapes {
            walk(&c.shape, c.clip_rect, &mut out);
        }
    }
    out
}

fn opened() -> SettingsDialog {
    let mut dialog = SettingsDialog::default();
    dialog.open(&AppSettings::default());
    dialog
}

#[test]
fn every_section_fits_the_window() {
    let sections: [(&str, Draw); 18] = [
        ("appearance", |d, ui| d.appearance_section_body(ui)),
        ("files", |d, ui| d.files_section_body(ui)),
        ("format", |d, ui| d.format_section_body(ui)),
        ("table", |d, ui| d.table_section_body(ui)),
        ("panels", |d, ui| d.panels_section_body(ui)),
        ("summary", |d, ui| d.summary_section_body(ui)),
        ("search", |d, ui| d.search_editor_section_body(ui)),
        ("sql", |d, ui| d.sql_section_body(ui)),
        ("mcp", |d, ui| d.mcp_section_body(ui)),
        ("chat options", |d, ui| d.chat_global_options_grid(ui)),
        ("cloud", |d, ui| d.cloud_section_body(ui)),
        ("db", |d, ui| d.db_section_body(ui)),
        ("api", |d, ui| d.api_section_body(ui)),
        ("map", |d, ui| d.map_section_body(ui)),
        ("tree", |d, ui| d.directory_tree_section_body(ui)),
        ("performance", |d, ui| d.performance_section_body(ui)),
        ("window", |d, ui| d.window_section_body(ui)),
        ("updates", |d, ui| d.updates_section_body(ui)),
    ];
    let mut failures = Vec::new();
    for (name, draw) in sections {
        let cut = cut_off(&mut opened(), draw);
        if !cut.is_empty() {
            failures.push(format!("{name}: {cut:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "text runs past the edge:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_sftp_agent_note_wraps() {
    // The reported case: SFTP over the SSH agent shows a long note in the
    // form's second column, which ran off the window and could not be read.
    let mut dialog = opened();
    dialog.focus_cloud_form = true;
    dialog.cloud_form_kind = CloudKind::Sftp;
    dialog.cloud_form_ssh_auth = SshAuth::Agent;
    let cut = cut_off(&mut dialog, |d, ui| d.cloud_section_body(ui));
    assert!(cut.is_empty(), "cut off: {cut:?}");

    // The same note under a database connection's jump host.
    let mut dialog = opened();
    dialog.db_form_id = "probe".into();
    dialog.db_form_ssh_on = true;
    dialog.db_form_ssh_auth = SshAuth::Agent;
    let cut = cut_off(&mut dialog, |d, ui| d.db_section_body(ui));
    assert!(cut.is_empty(), "cut off: {cut:?}");
}

#[test]
fn settings_text_can_be_marked() {
    // Drag across the first label of an open section: the text is marked,
    // which is what Ctrl+C then copies.
    let ctx = egui::Context::default();
    let mut dialog = opened();
    dialog.focus_cloud_section = true;
    let frame = |dialog: &mut SettingsDialog, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let mut full = ctx.run_ui(input, |ui| {
            dialog.show(ui.ctx(), None);
        });
        full.textures_delta.clear();
        full
    };
    let mut target = None;
    for _ in 0..3 {
        let full = frame(&mut dialog, Vec::new());
        target = full.shapes.iter().find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == crate::i18n::t("cloud.connections") => {
                Some(t.visual_bounding_rect())
            }
            _ => None,
        });
    }
    let rect = target.expect("the cloud section's heading is drawn");
    let (from, to) = (
        rect.left_center() + egui::vec2(1.0, 0.0),
        rect.right_center(),
    );
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        &mut dialog,
        vec![egui::Event::PointerMoved(from), press(from, true)],
    );
    frame(&mut dialog, vec![egui::Event::PointerMoved(to)]);
    frame(&mut dialog, vec![press(to, false)]);
    assert!(
        crate::ui::text_selection::has_active_selection(&ctx),
        "dragging over a Settings label marked nothing"
    );
}

#[test]
fn long_names_in_the_connection_lists_wrap() {
    // The reported case: a long database connection name ran on in one
    // line and pushed its Edit and Remove buttons out of the window.
    // One unbroken word wider than the panel: wherever a space falls, the
    // name alone fills the line.
    let name = "warehouse_reporting_replica_eu_central_1_read_only_analytics_nightly_snapshot";
    let host = "reporting-replica.eu-central-1.example.com";
    let mut dialog = opened();
    dialog.draft.db_connections.push(crate::db::DbConnection {
        id: "db-long".into(),
        name: name.into(),
        engine: crate::db::DbEngine::Postgres,
        host: host.into(),
        port: 5432,
        database: "warehouse".into(),
        username: "u".into(),
        auth: crate::db::DbAuth::Password,
        allow_writes: true,
        oauth_client_id: None,
        oauth_tenant: None,
        athena_workgroup: None,
        athena_output_location: None,
        datetimeoffset_as_timestamp: false,
        ssh: None,
        query_timeout_secs: crate::db::DEFAULT_QUERY_TIMEOUT_SECS,
        tls: Default::default(),
        tunnel_port: None,
    });
    dialog
        .draft
        .cloud_connections
        .push(crate::cloud::CloudConnection {
            id: "cloud-long".into(),
            name: name.into(),
            kind: CloudKind::S3,
            bucket: host.into(),
            ..Default::default()
        });
    let mut profile =
        crate::ui::settings::chat_profiles::seed_profile_from_legacy(&AppSettings::default());
    profile.id = "chat-long".into();
    profile.name = name.into();
    profile.model = host.into();
    dialog.draft.chat_profiles.push(profile);
    dialog
        .draft
        .api_connections
        .push(crate::api::ApiConnection {
            id: "api-long".into(),
            name: name.into(),
            base_url: format!("https://{host}/api/v2/reporting/warehouse"),
            ..Default::default()
        });

    let lists: [(&str, Draw, [&str; 2]); 4] = [
        (
            "db",
            |d, ui| d.db_section_body(ui),
            ["cloud.edit", "cloud.remove"],
        ),
        (
            "cloud",
            |d, ui| d.cloud_section_body(ui),
            ["cloud.edit", "cloud.remove"],
        ),
        (
            "chat",
            |d, ui| d.chat_profiles_section(ui),
            ["cloud.edit", "cloud.remove"],
        ),
        (
            "api",
            |d, ui| d.api_section_body(ui),
            ["api.edit", "api.remove"],
        ),
    ];
    for (list, draw, buttons) in lists {
        let cut = cut_off(&mut dialog, draw);
        assert!(cut.is_empty(), "{list}: cut off {cut:?}");
        // Pushed wholly past the edge, a button is not painted at all.
        let painted = texts(&mut dialog, draw);
        for key in buttons {
            assert!(
                painted
                    .iter()
                    .any(|(text, _, _)| *text == crate::i18n::t(key)),
                "{list}: no {key} button in view"
            );
        }
    }
}
