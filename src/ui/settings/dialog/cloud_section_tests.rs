use super::*;

#[test]
fn a_folder_is_stored_without_slashes_at_either_end() {
    assert_eq!(form_prefix("/home/anna/"), Some("home/anna/".to_string()));
    assert_eq!(form_prefix("team-a"), Some("team-a/".to_string()));
    assert_eq!(form_prefix("  /  "), None);
}

#[test]
fn an_sftp_form_saves_host_and_port_as_the_bucket() {
    let mut d = SettingsDialog {
        cloud_form_kind: CloudKind::Sftp,
        cloud_form_bucket: "files.example.com".into(),
        cloud_form_port: "2222".into(),
        cloud_form_prefix: "/upload".into(),
        ..Default::default()
    };
    assert_eq!(
        d.sftp_form_location(),
        Ok((
            "files.example.com:2222".to_string(),
            Some("upload/".to_string())
        ))
    );
    d.cloud_form_port = String::new();
    assert_eq!(d.sftp_form_location().unwrap().0, "files.example.com");
    d.cloud_form_port = "0".into();
    assert!(d.sftp_form_location().is_err());
    d.cloud_form_port = "22".into();
    d.cloud_form_bucket = " ".into();
    assert!(d.sftp_form_location().is_err());
}

/// Every text one frame of the section paints with the form open, with its
/// top edge.
fn section_texts(dialog: &mut SettingsDialog) -> Vec<(String, f32)> {
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    for _ in 0..2 {
        // The first frame takes this and opens the form for good.
        dialog.focus_cloud_form = true;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 2000.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| dialog.cloud_section_body(ui));
        out.textures_delta.clear();
        shapes = out.shapes;
    }
    shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Text(t) => {
                Some((t.galley.text().to_string(), t.visual_bounding_rect().top()))
            }
            _ => None,
        })
        .collect()
}

fn top_of(texts: &[(String, f32)], key: &str) -> f32 {
    texts
        .iter()
        .find(|(s, _)| *s == t(key))
        .unwrap_or_else(|| panic!("{key} is not painted: {texts:?}"))
        .1
}

#[test]
fn the_secret_comes_before_save_connection() {
    // Save connection stores the secret typed into the form, so the secret
    // has to be filled in first, not found under the button.
    for (kind, label) in [
        (CloudKind::S3, "cloud.secret"),
        (CloudKind::Sftp, "db.ssh_password"),
        (CloudKind::WebDav, "cloud.webdav_password"),
    ] {
        let mut dialog = SettingsDialog {
            cloud_form_kind: kind,
            // SFTP's default, the SSH agent, needs no secret at all.
            cloud_form_ssh_auth: SshAuth::Password,
            ..Default::default()
        };
        let texts = section_texts(&mut dialog);
        assert!(
            top_of(&texts, label) < top_of(&texts, "cloud.save_connection"),
            "{kind:?}: the secret comes after Save connection"
        );
    }
}

#[test]
fn the_secret_field_sits_beside_its_label() {
    // It sat two rows under a "Secret" heading (S3 put Access key ID in
    // between), named only by the placeholder inside it.
    for (kind, label) in [
        (CloudKind::S3, "cloud.secret"),
        (CloudKind::AzureBlob, "cloud.secret"),
        (CloudKind::Sftp, "db.ssh_password"),
        (CloudKind::WebDav, "cloud.webdav_password"),
    ] {
        let mut dialog = SettingsDialog {
            cloud_form_kind: kind,
            cloud_form_ssh_auth: SshAuth::Password,
            // A typed secret paints as a row of mask characters.
            cloud_form_secret: "typed".into(),
            ..Default::default()
        };
        let texts = section_texts(&mut dialog);
        let mask = egui::epaint::text::PASSWORD_REPLACEMENT_CHAR
            .to_string()
            .repeat(5);
        let field = texts
            .iter()
            .find(|(s, _)| *s == mask)
            .unwrap_or_else(|| panic!("{kind:?}: no secret field painted: {texts:?}"))
            .1;
        let label = top_of(&texts, label);
        // Same row: rows are ~30 px apart, and the mask's dots sit a few
        // pixels lower in their line than letters do.
        assert!(
            (field - label).abs() < 8.0,
            "{kind:?}: label at {label}, its field at {field}"
        );
    }
}

#[test]
fn the_form_heading_says_add_or_edit() {
    let count =
        |texts: &[(String, f32)], key: &str| texts.iter().filter(|(s, _)| *s == t(key)).count();
    let texts = section_texts(&mut SettingsDialog::default());
    assert_eq!(count(&texts, "cloud.add_connection"), 1, "{texts:?}");
    // The old heading promised editing in a place that only adds.
    assert!(!texts.iter().any(|(s, _)| s == "Add or edit a connection"));
    assert_eq!(count(&texts, "cloud.edit_connection"), 0);

    let texts = section_texts(&mut SettingsDialog {
        cloud_form_id: "probe".into(),
        ..Default::default()
    });
    assert_eq!(count(&texts, "cloud.edit_connection"), 1, "{texts:?}");
    assert_eq!(count(&texts, "cloud.add_connection"), 0);
}
