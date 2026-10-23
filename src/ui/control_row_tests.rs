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

#[test]
fn a_control_line_shares_one_centre_line() {
    assert_level(&centres(|ui, out| {
        control_line(ui, |ui| widgets(ui, out));
    }));
}

/// A dialog's title row: the title on the left, minimise / maximise / close
/// pushed to the right. The title used to sit 5.5px above the buttons, and
/// the buttons (text glyphs at their own font sizes) were not one height.
#[test]
fn a_dialog_title_shares_the_window_buttons_centre_line() {
    let c = centres(|ui, out| {
        control_line(ui, |ui| {
            let title = ui.label(
                egui::RichText::new("Conditional formatting")
                    .strong()
                    .size(16.0),
            );
            out.push(("title", title.rect.center().y));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut size = crate::ui::dialog_chrome::DialogSize::Normal;
                crate::ui::dialog_chrome::draw_window_controls(ui, &mut size);
                // All three buttons are one height, so the row of them is
                // centred exactly where each of them is.
                let buttons = ui.min_rect();
                assert!(
                    (buttons.height() - control_height(ui)).abs() < 0.51,
                    "window buttons are {} tall, not one control height",
                    buttons.height()
                );
                out.push(("buttons", buttons.center().y));
            });
        });
    });
    assert_level(&c);
}

/// A title with a long file name stays inside the dialog: the buttons keep
/// their place and the title is cut short beside them. The old "label, then
/// the buttons in what is left" row overflows, which is what laid the
/// Earlier versions title under its own buttons.
#[test]
fn a_long_dialog_title_leaves_room_for_the_window_buttons() {
    let long = "Earlier versions of a_rather_long_export_file_name_2026-10-08_final_v2.parquet";
    let width = |old: bool| {
        let ctx = egui::Context::default();
        let mut row = egui::Rect::NOTHING;
        let mut avail = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(340.0, 200.0),
            )),
            ..Default::default()
        };
        let mut full = ctx.run_ui(input, |ui| {
            let mut size = crate::ui::dialog_chrome::DialogSize::Normal;
            avail = ui.available_width();
            row = ui
                .scope(|ui| {
                    if old {
                        control_line(ui, |ui| {
                            ui.label(egui::RichText::new(long).strong().size(16.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| crate::ui::dialog_chrome::draw_window_controls(ui, &mut size),
                            );
                        });
                    } else {
                        crate::ui::dialog_chrome::draw_title_bar(
                            ui,
                            egui::RichText::new(long),
                            &mut size,
                        );
                    }
                })
                .response
                .rect;
        });
        full.textures_delta.clear();
        (row.width(), avail)
    };
    let (new, avail) = width(false);
    assert!(
        new <= avail,
        "title row is {new} wide in a {avail} wide dialog"
    );
    let (old, avail) = width(true);
    assert!(
        old > avail,
        "the old row should overflow, the test no longer bites"
    );
}

/// A wide grid of controls, the shape of conditional formatting's rule rows:
/// pickers, a value field, a checkbox and the move / remove buttons.
fn rule_grid(pin: bool) -> Vec<(&'static str, f32)> {
    centres(|ui, out| {
        egui::Grid::new("rules").num_columns(5).show(ui, |ui| {
            if pin {
                pin_control_height(ui);
            }
            let mut pick = 0;
            let mut text = String::new();
            let mut on = false;
            let combo = egui::ComboBox::from_id_salt("op")
                .selected_text("equals")
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, 1, "b");
                })
                .response;
            out.push(("combo", combo.rect.center().y));
            let field = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0));
            out.push(("text", field.rect.center().y));
            out.push(("checkbox", ui.checkbox(&mut on, "c").rect.center().y));
            out.push(("button", ui.button("X").rect.center().y));
            ui.end_row();
        });
    })
}

#[test]
fn a_pinned_grid_row_shares_one_centre_line() {
    assert_level(&rule_grid(true));
}

#[test]
fn an_unpinned_grid_row_is_ragged() {
    let c = rule_grid(false);
    let combo = c[0].1;
    let button = c[3].1;
    assert!(
        (combo - button).abs() > 1.0,
        "expected the combo to sink: {c:?}"
    );
}

/// Without the pinned height the same rows are ragged, so the tests above
/// measure something real.
#[test]
fn an_unpinned_row_is_ragged() {
    let spread = |c: Vec<(&str, f32)>| {
        c.iter().map(|x| x.1).fold(f32::MIN, f32::max)
            - c.iter().map(|x| x.1).fold(f32::MAX, f32::min)
    };
    let wrapped = centres(|ui, out| {
        ui.horizontal_wrapped(|ui| widgets(ui, out));
    });
    assert!(
        spread(wrapped.clone()) > 1.0,
        "expected a ragged row: {wrapped:?}"
    );
    let line = centres(|ui, out| {
        ui.horizontal(|ui| widgets(ui, out));
    });
    assert!(
        spread(line.clone()) > 1.0,
        "expected a ragged row: {line:?}"
    );
}

/// Where a list row put its text and buttons, in a `width` wide panel.
fn list_row(width: f32, text: &str) -> (egui::Rect, egui::Rect, egui::Rect) {
    let ctx = egui::Context::default();
    let mut seen = (
        egui::Rect::NOTHING,
        egui::Rect::NOTHING,
        egui::Rect::NOTHING,
    );
    // A grid lays itself out invisibly on its first pass.
    for _ in 0..3 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 400.0),
            )),
            ..Default::default()
        };
        let mut full = ctx.run_ui(input, |ui| {
            button_list_grid(ui, "list", &["Edit", "Remove"]).show(ui, |ui| {
                let label = ui.label(text).rect;
                let edit = ui.small_button("Edit").rect;
                let remove = ui.small_button("Remove").rect;
                ui.end_row();
                seen = (label, edit, remove);
            });
        });
        full.textures_delta.clear();
    }
    seen
}

#[test]
fn a_long_entry_wraps_and_keeps_its_buttons_in_view() {
    // The Settings connection lists: a long name ran on in one line and
    // pushed Edit and Remove past the window's edge.
    let long = "warehouse_reporting_replica_eu_central_1 (PostgreSQL: \
                reporting-replica.eu-central-1.example.com:5432/warehouse)";
    let (label, edit, remove) = list_row(320.0, long);
    assert!(
        remove.right() <= 320.0,
        "Remove ends at {} in a 320px panel",
        remove.right()
    );
    assert!(edit.right() < remove.left() && label.right() < edit.left());
    let (short, _, _) = list_row(320.0, "x");
    assert!(
        label.height() > 2.0 * short.height(),
        "the long name did not wrap: {label:?}"
    );
}

#[test]
fn a_short_entry_keeps_its_buttons_beside_it() {
    // Room to spare: the buttons follow the text, not the far edge.
    let (label, edit, _) = list_row(900.0, "prod (PostgreSQL: db/main)");
    assert!(edit.left() - label.right() < 40.0, "{label:?} {edit:?}");
}
