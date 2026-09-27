use super::*;

/// Lay out `body` twice (a grid settles its row heights on the second pass)
/// under a theme's button padding and return the centres it recorded.
fn centres(
    body: impl Fn(&mut egui::Ui, &mut Vec<(&'static str, f32)>),
) -> Vec<(&'static str, f32)> {
    let ctx = egui::Context::default();
    let mut out = Vec::new();
    for _ in 0..2 {
        out.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(900.0, 400.0),
            )),
            ..Default::default()
        };
        // The font atlas has to be taken, or dropping the output panics.
        let mut full = ctx.run_ui(input, |ui| {
            // A theme's padding, not egui's default: the defect only shows
            // with a button taller than egui's 18px `interact_size`.
            ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
            body(ui, &mut out);
        });
        full.textures_delta.clear();
    }
    out
}

fn widgets(ui: &mut egui::Ui, out: &mut Vec<(&'static str, f32)>) {
    let mut pick = 0;
    let mut on = false;
    let mut text = String::new();
    out.push(("label", ui.label("Start:").rect.center().y));
    let combo = egui::ComboBox::from_id_salt(("combo", out.len()))
        .selected_text("a")
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut pick, 1, "b");
        })
        .response;
    out.push(("combo", combo.rect.center().y));
    out.push(("button", ui.button("X").rect.center().y));
    out.push(("checkbox", ui.checkbox(&mut on, "c").rect.center().y));
    out.push((
        "text",
        control_text_edit(ui, 80.0, egui::TextEdit::singleline(&mut text))
            .rect
            .center()
            .y,
    ));
}

fn assert_level(c: &[(&str, f32)]) {
    let base = c[0].1;
    for (what, y) in c {
        assert!(
            (y - base).abs() < 0.51,
            "{what} sits at {y}, not on the row's centre line {base}: {c:?}"
        );
    }
}

#[test]
fn a_control_row_shares_one_centre_line() {
    assert_level(&centres(|ui, out| control_row(ui, |ui| widgets(ui, out))));
}

#[test]
fn a_control_grid_row_shares_one_centre_line() {
    let c = centres(|ui, out| {
        control_grid(ui, "g", |ui| {
            let mut pick = 0;
            out.push(("label", ui.label("Left table:").rect.center().y));
            let combo = egui::ComboBox::from_id_salt("gc")
                .selected_text("a")
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, 1, "b");
                })
                .response;
            out.push(("combo", combo.rect.center().y));
            ui.end_row();
        });
    });
    assert_level(&c);
}

/// Without the pinned height the same row is ragged, so the tests above
/// measure something real.
#[test]
fn an_unpinned_row_is_ragged() {
    let c = centres(|ui, out| {
        ui.horizontal_wrapped(|ui| widgets(ui, out));
    });
    let spread = c.iter().map(|x| x.1).fold(f32::MIN, f32::max)
        - c.iter().map(|x| x.1).fold(f32::MAX, f32::min);
    assert!(spread > 1.0, "expected a ragged row: {c:?}");
}
