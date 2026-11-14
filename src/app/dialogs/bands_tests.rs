use super::*;

#[test]
fn the_form_says_what_is_missing() {
    let mut st = BandsState::default();
    assert!(matches!(st.spec(), Ok(BandSpec::ToBands(BandBy::Width(5)))));
    st.count_buf = "0".into();
    assert_eq!(st.spec().unwrap_err(), t("bands.bad_count"));
    st.by = 2;
    assert_eq!(st.spec().unwrap_err(), t("bands.no_edges"));
    st.edges_buf = "18, x".into();
    assert!(st.spec().unwrap_err().ends_with(" x"));
    st.edges_buf = "18, 65".into();
    assert_eq!(
        st.spec(),
        Ok(BandSpec::ToBands(BandBy::Edges(vec![18.0, 65.0])))
    );
    st.back = true;
    assert_eq!(st.spec(), Ok(BandSpec::Back(Back::Middle)));
}

/// Opening starts on the selected column; a read-only tab opens nothing.
#[test]
fn it_opens_on_the_selected_column_and_not_when_read_only() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let table = &mut app.tabs[0].table;
    table.columns = ["name", "age"]
        .map(|n| octa::data::ColumnInfo {
            name: n.into(),
            data_type: "Utf8".into(),
        })
        .to_vec();
    app.tabs[0].table_state.selected_cell = Some((0, 1));
    app.open_bands();
    assert_eq!(app.bands_dialog.as_ref().unwrap().col, Some(1));

    app.bands_dialog = None;
    app.readonly_mode = true;
    app.open_bands();
    assert!(app.bands_dialog.is_none());
}

/// Drawn and applied the way a user does: Add column puts the bands right
/// after the source, as one undo step.
#[test]
fn the_dialog_draws_and_adds_the_column_in_one_undo_step() {
    use octa::data::{CellValue, ColumnInfo};
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let table = &mut app.tabs[0].table;
    table.columns = vec![
        ColumnInfo {
            name: "age".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "name".into(),
            data_type: "Utf8".into(),
        },
    ];
    table.rows = [12, 40, 70]
        .map(|a| vec![CellValue::Int(a), CellValue::String("x".into())])
        .to_vec();
    app.open_bands();
    let st = app.bands_dialog.as_mut().unwrap();
    st.col = Some(0);
    st.by = 2;
    st.edges_buf = "18, 65".into();
    let ctx = egui::Context::default();
    let frame = |app: &mut OctaApp, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| render_bands_dialog(app, ui.ctx()));
        output.textures_delta.clear();
        let mut out = Vec::new();
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Text(t) => out.push((
                    t.galley.text().to_string(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                )),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    };
    frame(&mut app, vec![]);
    let texts = frame(&mut app, vec![]);
    let add = t("bands.apply");
    let (_, rect) = texts
        .iter()
        .find(|(s, _)| *s == add)
        .unwrap_or_else(|| panic!("no {add} button: {texts:?}"));
    let at = rect.center();
    for pressed in [true, false] {
        let events = vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        frame(&mut app, events);
    }
    let table = &app.tabs[0].table;
    assert_eq!(table.columns[1].name, "age_band");
    assert_eq!(table.get(0, 1).unwrap().to_string(), "<18");
    assert_eq!(table.get(2, 1).unwrap().to_string(), "65+");
    app.tabs[0].table.undo();
    assert_eq!(
        app.tabs[0].table.col_count(),
        2,
        "one undo takes it all back"
    );
}
