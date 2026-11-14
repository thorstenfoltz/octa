use super::*;

fn app() -> OctaApp {
    OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    )
}

fn painted(ctx: &egui::Context, app: &mut OctaApp) -> Vec<String> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(700.0, 500.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| app.render_chat_trace(ui));
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_box_is_off_until_asked_for() {
    assert!(!app().chat.show_trace);
}

#[test]
fn each_step_shows_its_own_tokens_time_and_tools() {
    let mut app = app();
    {
        let mut state = app.chat.session.lock().unwrap();
        for (input, output, millis, tool) in
            [(6973, 126, 70_400, "schema"), (7200, 40, 9_000, "run_sql")]
        {
            state.push_trace(TraceStep {
                input_tokens: input,
                output_tokens: output,
                millis: Some(millis),
                tool_runs: vec![TraceToolRun {
                    name: tool.into(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
    }
    let ctx = egui::Context::default();
    painted(&ctx, &mut app);
    let text = painted(&ctx, &mut app);
    let first = step_title(1, &app.chat.session.lock().unwrap().trace[0]);
    assert!(
        first.contains("6,973") && first.contains("126") && first.contains("70.4"),
        "{first}"
    );
    assert!(first.ends_with("schema"), "{first}");
    assert!(text.iter().any(|t| t == &first), "{text:?}");
    let second = step_title(2, &app.chat.session.lock().unwrap().trace[1]);
    assert!(text.iter().any(|t| t == &second), "{text:?}");
}

#[test]
fn a_step_still_running_says_so_and_an_empty_chat_explains_itself() {
    let mut app = app();
    let ctx = egui::Context::default();
    painted(&ctx, &mut app);
    let text = painted(&ctx, &mut app);
    assert!(
        text.iter().any(|t| t == &octa::i18n::t("chat.trace_empty")),
        "{text:?}"
    );

    let running = TraceStep::default();
    assert!(step_title(1, &running).contains("..."));
}
