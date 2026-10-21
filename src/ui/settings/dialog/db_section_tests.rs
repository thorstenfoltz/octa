use super::*;

#[test]
fn auth_options_follow_engine() {
    for &e in DbEngine::ALL {
        assert_eq!(auth_options(e), e.supported_auth(), "{e:?}");
    }
}

#[test]
fn secret_bearing_kinds() {
    assert!(auth_uses_secret(DbAuthKind::Token));
    assert!(auth_uses_secret(DbAuthKind::OAuthClientCredentials));
    assert!(!auth_uses_secret(DbAuthKind::GcpAdc));
    assert!(!auth_uses_secret(DbAuthKind::OAuthBrowser));
}

/// Every text one frame of the SSL block paints, for `engine` and `mode`,
/// with its colour.
fn ssl_block_text(engine: DbEngine, mode: TlsMode) -> Vec<(String, egui::Color32)> {
    let mut dialog = SettingsDialog {
        db_form_engine: engine,
        db_form_ssl_mode: mode,
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    // The first frame only measures; the second paints settled text.
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| dialog.draw_db_tls_fields(ui));
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Text(t) => Some((
                t.galley.text().to_string(),
                t.galley.job.sections[0].format.color,
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn every_ssl_mode_says_what_it_does() {
    // The closed combo shows only the mode's word (`require`), so the line
    // under it has to explain every mode, not only the weak ones.
    for engine in [DbEngine::Postgres, DbEngine::Redshift, DbEngine::MySql] {
        for &mode in TlsMode::offered(engine) {
            let texts = ssl_block_text(engine, mode);
            let desc = t(&format!("db.{}_desc", mode.i18n_key()));
            assert!(
                !desc.starts_with("db."),
                "{mode:?} has no description string"
            );
            // Every mode's line in the one warning colour, never the plain one.
            let warn = egui::Context::default()
                .global_style()
                .visuals
                .warn_fg_color;
            assert!(
                texts.iter().any(|(s, c)| s == &desc && *c == warn),
                "{engine:?} {mode:?}: description not shown in the warning colour in {texts:?}"
            );
            assert!(
                texts.iter().any(|(s, _)| s == mode.label(engine)),
                "{engine:?} {mode:?}: the combo does not show the mode's word"
            );
            assert!(
                texts.iter().any(|(s, _)| s == &t("dialog.swb_browse")),
                "no Browse button next to the CA file"
            );
        }
    }
}

/// Every text one frame of the add/edit form paints for `engine`.
fn form_text(engine: DbEngine) -> Vec<String> {
    let mut dialog = SettingsDialog {
        db_form_engine: engine,
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 900.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| dialog.db_connection_form(ui));
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_file_engine_asks_for_a_file_and_nothing_a_server_needs() {
    for engine in [DbEngine::DuckDb, DbEngine::Sqlite] {
        let texts = form_text(engine);
        for wanted in [t("db.file"), t("dialog.swb_browse"), t("db.allow_writes")] {
            assert!(
                texts.contains(&wanted),
                "{engine:?}: no {wanted:?} in {texts:?}"
            );
        }
        for gone in [
            t("db.host"),
            t("db.port"),
            t("db.username"),
            t("db.auth"),
            t("db.password"),
            t("db.ssh_section"),
            t("db.ssl_mode"),
        ] {
            assert!(!texts.contains(&gone), "{engine:?}: {gone:?} is shown");
        }
    }
    // A server engine still shows its fields and no File row.
    let pg = form_text(DbEngine::Postgres);
    assert!(pg.contains(&t("db.host")));
    assert!(!pg.contains(&t("db.file")));
}

#[test]
fn a_file_connection_is_saved_with_its_path_and_no_server_fields() {
    let mut dialog = SettingsDialog {
        db_form_engine: DbEngine::Sqlite,
        db_form_name: "shop".into(),
        db_form_database: "~/data/shop.sqlite".into(),
        db_form_host: "left.over".into(),
        db_form_username: "anna".into(),
        ..Default::default()
    };
    dialog.save_db_form();
    let c = dialog.draft.db_connections.last().expect("saved");
    assert_eq!(c.engine, DbEngine::Sqlite);
    assert_eq!(c.database, "~/data/shop.sqlite");
    assert!(
        c.host.is_empty() && c.username.is_empty() && c.port == 0,
        "{c:?}"
    );
    assert!(c.ssh.is_none());

    let mut nameless = SettingsDialog {
        db_form_engine: DbEngine::DuckDb,
        db_form_name: "x".into(),
        ..Default::default()
    };
    nameless.save_db_form();
    assert!(nameless.draft.db_connections.is_empty());
    assert_eq!(nameless.db_secret_status_msg, Some(t("db.need_name_file")));
}
