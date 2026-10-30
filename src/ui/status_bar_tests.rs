use super::*;
use std::collections::HashSet;

/// One frame of the bar, 900 px wide, with `events`. Returns its action and
/// where each text was painted.
fn frame(
    ctx: &egui::Context,
    table: &DataTable,
    zoom_percent: u32,
    zoom_step: u32,
    events: Vec<egui::Event>,
) -> (StatusBarAction, Vec<(String, egui::Rect)>) {
    let state = TableViewState::default();
    let (rows, cells) = (HashSet::new(), HashSet::new());
    let raw = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 28.0),
        )),
        ..Default::default()
    };
    let mut action = StatusBarAction::default();
    let mut full = ctx.run_ui(raw, |ui| {
        let cx = StatusBarCtx {
            table,
            state: &state,
            theme_mode: ThemeMode::Dark,
            filtered_count: 0,
            search_active: false,
            nav_focus_requested: false,
            zoom_percent,
            zoom_step,
            readonly: false,
            busy: false,
            busy_hint: None,
            busy_cancellable: false,
            column_filter_count: 0,
            first_filtered_col: None,
            selected_rows: &rows,
            selected_cells: &cells,
        };
        action = draw_status_bar(ui, cx, &mut String::new());
    });
    full.textures_delta.clear();
    let texts = full
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_string(), t.visual_bounding_rect())),
            _ => None,
        })
        .collect();
    (action, texts)
}

/// Press and release at `pos`.
fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(pos), button(true), button(false)]
}

/// The zoom used to appear only away from 100%, and only with a table open.
/// It is always there now, and its -, percentage and + each ask for a zoom.
#[test]
fn the_zoom_is_always_shown_and_its_buttons_zoom() {
    let ctx = egui::Context::default();
    let table = DataTable::empty();
    let (_, texts) = frame(&ctx, &table, 100, 5, Vec::new());
    let at = |text: &str| {
        texts
            .iter()
            .find(|(t, _)| t == text)
            .map(|(_, r)| r.center())
            .unwrap_or_else(|| panic!("{text:?} not drawn in {texts:?}"))
    };
    let (minus, percent, plus) = (at("-"), at("100%"), at("+"));
    assert!(
        minus.x < percent.x && percent.x < plus.x,
        "not - 100% +: {minus:?} {percent:?} {plus:?}"
    );

    // The step is the one from Settings > Appearance, 5 unless changed.
    for (pos, zoom, step, wanted) in [
        (plus, 100, 5, 105),
        (minus, 100, 5, 95),
        (plus, 100, 12, 112),
        (minus, 100, 25, 75),
        (percent, 140, 5, 100),
    ] {
        frame(&ctx, &table, zoom, step, Vec::new());
        let (action, _) = frame(&ctx, &table, zoom, step, click(pos));
        assert_eq!(
            action.zoom_to,
            Some(wanted),
            "click at {pos:?} from {zoom}% by {step}"
        );
    }
}
