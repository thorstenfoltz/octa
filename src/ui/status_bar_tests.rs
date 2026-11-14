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
    let pill = pill(LiveState::Unavailable);
    let (action, texts, _) = frame_with_pill(ctx, table, zoom_percent, zoom_step, pill, events);
    (action, texts)
}

fn pill(state: LiveState) -> LivePill<'static> {
    LivePill {
        text: "Auto-refresh",
        hint: "hint",
        state,
    }
}

/// [`frame`] with a chosen live pill, also returning the fill of the dot.
fn frame_with_pill(
    ctx: &egui::Context,
    table: &DataTable,
    zoom_percent: u32,
    zoom_step: u32,
    live_pill: LivePill<'_>,
    events: Vec<egui::Event>,
) -> (
    StatusBarAction,
    Vec<(String, egui::Rect)>,
    Option<egui::Color32>,
) {
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
            live_pill,
            busy: false,
            busy_hint: None,
            busy_cancellable: false,
            column_filter_count: 0,
            first_filtered_col: None,
            hidden_row_count: 0,
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
    let dot = full.shapes.iter().find_map(|c| match &c.shape {
        egui::Shape::Circle(dot) => Some(dot.fill),
        _ => None,
    });
    (action, texts, dot)
}

/// The user's request: the bottom line says whether a tab refreshes by itself
/// in every state, not only while it does. Each state looks different, and a
/// click does what that state offers.
#[test]
fn the_live_pill_is_always_shown_and_a_click_does_what_its_state_offers() {
    use egui::Color32;
    let table = DataTable::empty();
    let colors = ThemeColors::for_mode(ThemeMode::Dark);
    for (state, stops, opens, fill) in [
        (LiveState::On, true, false, colors.success),
        (LiveState::Paused, true, false, colors.warning),
        (LiveState::Off, false, true, Color32::TRANSPARENT),
        (LiveState::Unavailable, false, false, Color32::TRANSPARENT),
    ] {
        let ctx = egui::Context::default();
        let (_, texts, dot) = frame_with_pill(&ctx, &table, 100, 5, pill(state), Vec::new());
        assert_eq!(dot, Some(fill), "{state:?}");
        let at = texts
            .iter()
            .find(|(t, _)| t == "Auto-refresh")
            .map(|(_, r)| r.center())
            .unwrap_or_else(|| panic!("{state:?} not drawn in {texts:?}"));
        let (action, ..) = frame_with_pill(&ctx, &table, 100, 5, pill(state), click(at));
        assert_eq!(action.stop_live, stops, "{state:?}");
        assert_eq!(action.open_auto_refresh, opens, "{state:?}");
    }
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
