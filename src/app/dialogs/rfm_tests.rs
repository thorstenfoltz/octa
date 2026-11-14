use super::*;
use octa::data::{CellValue, ColumnInfo};

#[test]
fn as_of_is_empty_or_a_date_in_any_layout() {
    assert_eq!(parse_as_of(""), Ok(None));
    let day = chrono::NaiveDate::from_ymd_opt(2024, 3, 31);
    assert_eq!(parse_as_of(" 2024-03-31 "), Ok(day));
    assert_eq!(parse_as_of("31.03.2024"), Ok(day));
    assert_eq!(parse_as_of("soon"), Err(()));
}

/// The dialog's run on the loaded rows opens one row per customer, with the
/// segment names explained on hover.
#[test]
fn a_run_opens_the_segments_with_their_hover_texts() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let table = &mut app.tabs[0].table;
    table.columns = [
        ("customer", "Utf8"),
        ("at", "Date32"),
        ("amount", "Float64"),
    ]
    .map(|(n, d)| ColumnInfo {
        name: n.into(),
        data_type: d.into(),
    })
    .to_vec();
    table.rows = [("a", "2024-01-01", 5.0), ("b", "2024-03-01", 50.0)]
        .map(|(c, d, a)| {
            vec![
                CellValue::String(c.into()),
                CellValue::Date(d.into()),
                CellValue::Float(a),
            ]
        })
        .to_vec();
    app.open_rfm();
    assert_eq!(
        app.rfm_dialog.as_ref().unwrap().date,
        Some(1),
        "the date column"
    );

    let totals = rfm::collect(
        &app.tabs[0].table,
        &RfmSpec {
            customer: 0,
            date: 1,
            amount: 2,
            order: None,
            as_of: None,
        },
    );
    app.open_rfm_result(&totals, None, Names::Eleven, None)
        .unwrap();
    let tab = &app.tabs[app.active_tab];
    assert_eq!(tab.table.row_count(), 2);
    assert_eq!(tab.table_state.header_tooltips.len(), 9);
    let segment = tab.table.rows[0][8].to_string();
    assert!(
        tab.table_state.cell_tooltips[8].contains_key(&segment),
        "{segment} is explained"
    );
}

/// Every text painted in one frame of the dialog, with its rect.
fn frame(
    app: &mut OctaApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
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
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| render_rfm_dialog(app, ui.ctx()));
    output.textures_delta.clear();
    let mut out = Vec::new();
    output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
    out
}

fn click(at: egui::Pos2) -> [Vec<egui::Event>; 2] {
    [true, false].map(|pressed| {
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    })
}

/// Drawn and run the way a user does: the Run button opens the result tab.
#[test]
fn the_dialog_draws_and_its_run_button_opens_the_segments() {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let table = &mut app.tabs[0].table;
    table.columns = [
        ("customer", "Utf8"),
        ("at", "Date32"),
        ("amount", "Float64"),
    ]
    .map(|(n, d)| ColumnInfo {
        name: n.into(),
        data_type: d.into(),
    })
    .to_vec();
    table.rows = vec![vec![
        CellValue::String("a".into()),
        CellValue::Date("2024-01-01".into()),
        CellValue::Float(5.0),
    ]];
    app.open_rfm();
    let st = app.rfm_dialog.as_mut().unwrap();
    st.customer = Some(0);
    st.amount = Some(2);
    let ctx = egui::Context::default();
    frame(&mut app, &ctx, vec![]);
    let texts = frame(&mut app, &ctx, vec![]);
    let run = octa::i18n::t("cohort.run");
    let (_, rect) = texts
        .iter()
        .find(|(t, _)| *t == run)
        .unwrap_or_else(|| panic!("no Run button: {texts:?}"));
    for events in click(rect.center()) {
        frame(&mut app, &ctx, events);
    }
    assert!(app.rfm_dialog.is_none(), "the dialog closed");
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.tabs[1].table.row_count(), 1);
}
