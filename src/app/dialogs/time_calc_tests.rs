//! The working-days column, through the dialog's own state and apply step.

use super::*;
use octa::data::ColumnInfo;

fn app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let date = |s: &str| CellValue::Date(s.to_string());
    let table = &mut app.tabs[0].table;
    table.columns = ["ordered", "delivered"]
        .map(|name| ColumnInfo {
            name: name.into(),
            data_type: "Date32".into(),
        })
        .to_vec();
    table.rows = vec![
        // Monday to Friday of one week.
        vec![date("2024-03-04"), date("2024-03-08")],
        // Thursday before Easter to the Tuesday after it.
        vec![date("2024-03-28"), date("2024-04-02")],
        vec![date("2024-03-04"), CellValue::Null],
    ];
    app
}

fn column(app: &OctaApp, col: usize) -> Vec<String> {
    let table = &app.tabs[0].table;
    (0..table.row_count())
        .map(|r| table.get(r, col).map(|c| c.to_string()).unwrap_or_default())
        .collect()
}

#[test]
fn working_days_become_a_column_and_the_calendar_is_kept() {
    let mut app = app();
    app.open_time_calc_dialog();
    {
        let state = app.tabs[0].time_calc.as_mut().unwrap();
        // Weekends off and both days counted are how it opens.
        assert_eq!(
            state.days_off,
            [false, false, false, false, false, true, true]
        );
        assert!(state.both_ends);
        state.kind = TimeCalcKind::WorkingDays;
        state.col_a = 0;
        state.col_b = 1;
        state.new_name = "days".into();
        state.holidays_buf = "29.03.2024\n2024-04-01".into();
    }
    apply_time_calc(&mut app);

    let table = &app.tabs[0].table;
    assert_eq!(table.columns[2].name, "days");
    assert_eq!(table.columns[2].data_type, "Int64");
    // Thu, (Good Friday), (weekend), (Easter Monday), Tue = 2.
    assert_eq!(column(&app, 2), ["5", "2", ""]);
    assert!(
        app.tabs[0].parse_error_banner.is_some(),
        "the empty end is counted"
    );

    // One Undo takes the whole column away, not one cell.
    app.tabs[0].table.undo();
    assert_eq!(app.tabs[0].table.col_count(), 2);

    // The next calculation starts from the same holidays.
    app.tabs[0].time_calc = None;
    app.open_time_calc_dialog();
    let state = app.tabs[0].time_calc.as_ref().unwrap();
    assert_eq!(state.holidays_buf, "29.03.2024\n2024-04-01");
}

#[test]
fn a_new_column_left_of_its_sources_still_reads_them() {
    let mut app = app();
    app.open_time_calc_dialog();
    {
        let state = app.tabs[0].time_calc.as_mut().unwrap();
        state.kind = TimeCalcKind::WorkingDays;
        state.col_a = 0;
        state.col_b = 1;
        state.both_ends = false;
        state.insert_at_text = "1".into();
    }
    apply_time_calc(&mut app);
    assert_eq!(column(&app, 0), ["4", "3", ""]);
    assert_eq!(column(&app, 1)[0], "2024-03-04");
}

/// Drawn with working days picked: the weekday boxes are there, a holiday
/// that is no date is named, and Add column stays greyed until it is fixed.
#[test]
fn the_working_days_form_draws_and_names_a_bad_holiday() {
    let mut app = app();
    app.open_time_calc_dialog();
    {
        let state = app.tabs[0].time_calc.as_mut().unwrap();
        state.kind = TimeCalcKind::WorkingDays;
        state.holidays_buf = "2024-12-25\nsoon".into();
    }
    let ctx = egui::Context::default();
    let texts = |app: &mut OctaApp| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 1000.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| render_time_calc_dialog(app, ui.ctx()));
        output.textures_delta.clear();
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        output.shapes.iter().for_each(|s| walk(&s.shape, &mut out));
        out
    };
    texts(&mut app);
    let shown = texts(&mut app);
    let first_day = octa::i18n::t("chart.weekdays");
    let first_day = first_day.split(',').next().unwrap().to_string();
    assert!(shown.contains(&first_day), "{shown:?}");
    assert!(
        shown.iter().any(|s| s.contains("soon")),
        "the bad entry is named: {shown:?}"
    );
    // Applying with a bad holiday changes nothing.
    let before = app.tabs[0].table.col_count();
    apply_time_calc(&mut app);
    assert_eq!(app.tabs[0].table.col_count(), before);
}

/// An age is today minus the date of birth: the second date is **Today**
/// instead of a column. Also with the new column put left of its source,
/// which used to read the shifted column.
#[test]
fn a_difference_against_today_gives_an_age() {
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    let born = |years: i32| CellValue::Date(format!("{}-01-01", today.year() - years));
    let mut app = app();
    app.tabs[0].table.rows = vec![
        vec![born(30), CellValue::Null],
        vec![born(5), CellValue::Null],
        vec![CellValue::Null, CellValue::Null],
    ];
    app.open_time_calc_dialog();
    {
        let state = app.tabs[0].time_calc.as_mut().unwrap();
        state.kind = TimeCalcKind::Difference;
        state.unit = TimeUnit::Years;
        state.col_a = 0;
        state.b_today = true;
        state.new_name = "age".into();
        state.insert_at_text = "1".into();
    }
    apply_time_calc(&mut app);

    let table = &app.tabs[0].table;
    assert_eq!(table.columns[0].name, "age");
    let age = |row: usize| match table.get(row, 0) {
        Some(CellValue::Float(years)) => years.floor() as i32,
        other => panic!("no age in row {row}: {other:?}"),
    };
    assert_eq!((age(0), age(1)), (30, 5));
    assert_eq!(
        table.get(2, 0),
        Some(&CellValue::Null),
        "no birthday, no age"
    );

    // Today only counts for the kinds that read two dates: a flag left set
    // does not replace the column of another calculation.
    let state = app.tabs[0].time_calc.as_mut().unwrap();
    state.kind = TimeCalcKind::Extract;
    state.component = DateComponent::Year;
    state.a_today = true;
    state.col_a = 1;
    state.new_name = "year".into();
    state.insert_at_text = "9".into();
    apply_time_calc(&mut app);
    let table = &app.tabs[0].table;
    assert_eq!(
        table.get(0, 3),
        Some(&CellValue::Int(i64::from(today.year() - 30)))
    );
}

/// Every operation in the list answers a hover, in every language.
#[test]
fn every_operation_has_a_hover_text() {
    for kind in KINDS {
        let key = kind_hint(*kind);
        assert_ne!(octa::i18n::t(key), key, "{key} has no text");
    }
}
