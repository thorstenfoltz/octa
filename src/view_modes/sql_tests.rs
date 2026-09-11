//! Unit tests for [`sql`](sql). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn prefix_picks_up_word_before_cursor() {
    let s = "SELECT na";
    let (start, pfx) = current_prefix_at(s, s.len());
    assert_eq!(pfx, "na");
    assert_eq!(start, 7);
}

#[test]
fn prefix_is_empty_after_whitespace() {
    let s = "SELECT ";
    let (start, pfx) = current_prefix_at(s, s.len());
    assert_eq!(pfx, "");
    assert_eq!(start, s.len());
}

#[test]
fn suggestions_match_columns_and_keywords() {
    let cols = vec!["name".to_string(), "age".to_string()];
    let out = collect_suggestions("n", &cols, 8);
    assert!(out.contains(&"name".to_string()));
    assert!(out.contains(&"NOT".to_string()));
}

#[test]
fn suggestions_respect_limit() {
    let cols: Vec<String> = (0..20).map(|i| format!("col_{i}")).collect();
    let out = collect_suggestions("col", &cols, 5);
    assert_eq!(out.len(), 5);
}

#[test]
fn empty_prefix_yields_no_suggestions() {
    let cols = vec!["name".to_string()];
    let out = collect_suggestions("", &cols, 8);
    assert!(out.is_empty());
}

/// Milliseconds up to a second, then seconds. Both are SI symbols, so the
/// line stays readable in every locale without a key.
#[test]
fn durations_switch_unit_at_one_second() {
    assert_eq!(format_duration(0), "0 ms");
    assert_eq!(format_duration(999), "999 ms");
    assert_eq!(format_duration(1000), "1.0 s");
    assert_eq!(format_duration(90_500), "90.5 s");
}

/// The Ask box takes the width its own row leaves it. A narrow dock leaves
/// less than the controls beside it need, and an unclamped subtraction would
/// hand `TextEdit::desired_width` a negative number.
#[test]
fn the_ask_box_never_asks_for_a_negative_width() {
    assert_eq!(ask_box_width(800.0, true), 570.0);
    assert_eq!(ask_box_width(800.0, false), 640.0, "capped, not endless");
    assert_eq!(
        ask_box_width(180.0, true),
        160.0,
        "narrow dock, still positive"
    );
    assert_eq!(ask_box_width(0.0, false), 160.0);
}

/// The Ask row lands on one centre line: box, Ask button and the model
/// picker. Measured, because every earlier attempt at this row was judged by
/// eye and shipped staggered - egui centres each widget against the row
/// height known when *that* widget is added, and the three had three
/// different heights (box 9.5, button 13.5, combo 18.0 under Octa's
/// `button_padding`, against egui's default of 1px where they happen to
/// agree). The row therefore pins `interact_size.y` to
/// [`ask_row_height`] and gives the box the button's vertical padding as its
/// margin; this test drives the same sequence headlessly and fails on any
/// drift, including an egui upgrade that changes the layout rules.
#[test]
fn the_ask_row_shares_one_centre_line() {
    for rows in [1usize, 3] {
        let ctx = egui::Context::default();
        let mut buf = String::new();
        let mut profile = "local".to_string();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(900.0, 400.0),
            )),
            ..Default::default()
        };
        let mut centres: Vec<(&str, f32)> = Vec::new();
        // The font atlas built by the first pass has to be taken, or dropping
        // the output panics ("Dropped TexturesDelta with 1 unapplied deltas").
        let mut out = ctx.run_ui(input, |ui| {
            // A theme's padding, not egui's default: the defect only shows
            // with a button taller than a one-line text box.
            ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().interact_size.y = ask_row_height(ui);
                let pad = ui.spacing().button_padding.y.round() as i8;
                let box_resp = ui.add(
                    egui::TextEdit::multiline(&mut buf)
                        .desired_rows(rows)
                        .margin(egui::Margin::symmetric(4, pad))
                        .desired_width(ask_box_width(ui.available_width(), true)),
                );
                let button = ui.button("Ask");
                let combo = egui::ComboBox::from_id_salt("sql_ask_profile")
                    .width(130.0)
                    .selected_text(profile.clone())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut profile, "remote".to_string(), "remote");
                    })
                    .response;
                centres.push(("box", box_resp.rect.center().y));
                centres.push(("button", button.rect.center().y));
                centres.push(("combo", combo.rect.center().y));
            });
        });
        out.textures_delta.clear();
        let base = centres[0].1;
        for (what, y) in &centres {
            assert!(
                (y - base).abs() < 0.51,
                "{what} sits at {y}, not on the row's centre line {base} ({rows} rows): {centres:?}"
            );
        }
    }
}
