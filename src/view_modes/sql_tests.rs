//! Unit tests for [`sql`](sql). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::ui::settings::SqlPanelPosition;

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
/// [`control_height`] and gives the box the button's vertical padding as its
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
                ui.spacing_mut().interact_size.y = control_height(ui);
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

/// Drive a real press-drag-release on a nested bottom pane's resize handle and
/// report the height it settles at.
///
/// `fill` picks whether the body claims the whole pane, which is the single
/// line the SQL result area gained: `egui::Panel` persists the rect its
/// *content* produced, not the size the drag asked for, so a body shorter than
/// the pane rewrites the pane back down to itself on the very next frame. That
/// is the "I drag the results bigger and it jumps back" report.
#[cfg(test)]
fn settled_pane_height(fill: bool) -> f32 {
    use std::cell::Cell;

    const SCREEN: f32 = 900.0;
    const START_H: f32 = 120.0;
    const DRAG_TO_Y: f32 = 600.0;

    let ctx = egui::Context::default();
    let height = Cell::new(0.0_f32);
    let handle_y = SCREEN - START_H;

    let modifiers = egui::Modifiers::default();
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    let at_handle = egui::pos2(400.0, handle_y);
    let at_target = egui::pos2(400.0, DRAG_TO_Y);

    // One frame per step: the resize widget is registered at the end of a
    // frame and read back on the next, so press and move cannot share one.
    let frames = [
        vec![egui::Event::PointerMoved(at_handle)],
        vec![egui::Event::PointerMoved(at_handle), press(at_handle, true)],
        vec![egui::Event::PointerMoved(at_target)],
        vec![press(at_target, false)],
        vec![],
        vec![],
    ];

    for events in frames {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(900.0, SCREEN),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let resp = egui::Panel::bottom("pane_under_test")
                .resizable(true)
                .default_size(START_H)
                .min_size(80.0)
                .show(ui, |ui| {
                    if fill {
                        ui.set_min_height(ui.available_height());
                    }
                    ui.label("a short body");
                });
            height.set(resp.response.rect.height());
        });
        out.textures_delta.clear();
    }
    height.get()
}

/// The fix: a pane whose body fills it keeps the height it was dragged to.
#[test]
fn a_filled_pane_keeps_the_height_it_was_dragged_to() {
    let h = settled_pane_height(true);
    assert!(
        (h - 300.0).abs() < 8.0,
        "dragged to ~300px, settled at {h}px"
    );
}

/// The bug, pinned so the reason for that one line cannot be forgotten: the
/// same drag on a pane whose body does not fill it collapses back to the body.
#[test]
fn an_unfilled_pane_snaps_back_to_its_content() {
    let h = settled_pane_height(false);
    assert!(h < 200.0, "expected the snap-back, settled at {h}px");
}

/// What one headless run of the real panel tells us: the heights the splitter
/// settled on, and whether any two panes ended up partially covering each
/// other on screen.
#[cfg(test)]
struct PanelRun {
    pane_heights: Vec<f32>,
    overlaps: Vec<(egui::Rect, egui::Rect)>,
}

/// Drive the real [`render_sql_view`] headlessly in one dock position, with
/// an optional drag on the handle between pane `handle` and the one below it.
///
/// This goes through the actual view, not a stand-in for it: three earlier
/// attempts at this layout were judged against a simplified copy of the chain
/// and passed while the panel itself was still drawing panes over each other.
#[cfg(test)]
fn run_sql_panel(
    panel_h: f32,
    workspace_open: bool,
    editor_lines: usize,
    position: SqlPanelPosition,
    drag: Option<(usize, f32)>,
) -> PanelRun {
    use crate::app::state::TabState;

    let ctx = egui::Context::default();
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    tab.sql_query = (0..editor_lines)
        .map(|i| format!("SELECT {i} FROM data"))
        .collect::<Vec<_>>()
        .join("\n");
    tab.sql_workspace_open = workspace_open;
    let splitter_id = std::cell::Cell::new(egui::Id::NULL);
    let mut painted: Vec<egui::epaint::ClippedShape> = Vec::new();

    let mut frame = |events: Vec<egui::Event>, painted: &mut Vec<_>| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let body = |ui: &mut egui::Ui| {
                splitter_id.set(ui.id().with("sql_panes"));
                render_sql_view(
                    ui,
                    &mut tab,
                    SqlViewContext {
                        autocomplete_enabled: false,
                        default_row_limit: 1000,
                        partial_rows: None,
                        editor_font: octa::ui::settings::SqlEditorFont::SystemMonospace,
                        workspace_tables: &[],
                        workspace_attachments: &[],
                        inspector_selection: None,
                        inspector_entry: None,
                        server_conn_name: None,
                        server_running: false,
                        db_connections: Vec::new(),
                        cloud_connections: Vec::new(),
                        auto_registered: &[],
                        show_auto_register_notice: false,
                        chat_profile_available: false,
                        ask_profiles: Vec::new(),
                    },
                );
            };
            match position {
                SqlPanelPosition::Bottom => egui::Panel::bottom("sql_panel_bottom"),
                SqlPanelPosition::Top => egui::Panel::top("sql_panel_top"),
                SqlPanelPosition::Left => egui::Panel::left("sql_panel_left"),
                SqlPanelPosition::Right => egui::Panel::right("sql_panel_right"),
            }
            .resizable(true)
            .default_size(panel_h)
            .min_size(140.0)
            .show(ui, body);
        });
        out.textures_delta.clear();
        *painted = out.shapes;
    };

    for _ in 0..3 {
        frame(Vec::new(), &mut painted);
    }
    if let Some((handle, dy)) = drag {
        // The handle registers itself, so its rect comes back off the context.
        let rect = ctx
            .read_response(splitter_id.get().with(handle))
            .expect("the splitter registers a handle between every two panes")
            .rect;
        let from = rect.center();
        let to = egui::pos2(from.x, from.y + dy);
        let modifiers = egui::Modifiers::default();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        frame(vec![egui::Event::PointerMoved(from)], &mut painted);
        frame(
            vec![egui::Event::PointerMoved(from), press(from, true)],
            &mut painted,
        );
        frame(vec![egui::Event::PointerMoved(to)], &mut painted);
        frame(vec![press(to, false)], &mut painted);
        frame(Vec::new(), &mut painted);
    }

    // Two panes overlap on screen when two clip rects partially cover each
    // other. One nested inside another is an ordinary child `Ui`, not an
    // overlap; partial cover is exactly the defect.
    let mut clips: Vec<egui::Rect> = Vec::new();
    for cs in &painted {
        let r = cs.shape.visual_bounding_rect().intersect(cs.clip_rect);
        if r.is_negative() || r.height() <= 0.5 || r.width() <= 0.5 {
            continue;
        }
        if !clips.contains(&cs.clip_rect) {
            clips.push(cs.clip_rect);
        }
    }
    let mut overlaps = Vec::new();
    for (i, a) in clips.iter().enumerate() {
        for b in clips.iter().skip(i + 1) {
            if a.contains_rect(*b) || b.contains_rect(*a) {
                continue;
            }
            let over = a.intersect(*b);
            if !over.is_negative() && over.height() > 1.0 && over.width() > 1.0 {
                overlaps.push((*a, *b));
            }
        }
    }
    PanelRun {
        pane_heights: ctx
            .data(|d| d.get_temp::<Vec<f32>>(splitter_id.get()))
            .unwrap_or_default(),
        overlaps,
    }
}

/// The requirement, in every dock, at every height, with the workspace open
/// and closed, and after dragging each handle both ways: nothing is ever
/// drawn over anything else.
#[test]
fn no_two_sql_panes_ever_cover_each_other() {
    for position in [
        SqlPanelPosition::Bottom,
        SqlPanelPosition::Top,
        SqlPanelPosition::Left,
        SqlPanelPosition::Right,
    ] {
        for (h, workspace, lines) in [
            (600.0_f32, false, 3usize),
            (600.0, true, 3),
            (280.0, true, 20),
            (160.0, true, 40),
        ] {
            for drag in [None, Some((0, -400.0)), Some((0, 400.0))] {
                let run = run_sql_panel(h, workspace, lines, position, drag);
                assert!(
                    run.overlaps.is_empty(),
                    "{position:?} {h}px workspace={workspace} drag={drag:?}: \
                     {} panes cover each other, first {:?}",
                    run.overlaps.len(),
                    run.overlaps.first(),
                );
            }
        }
    }
}

/// Dragging a handle has to actually move the boundary. The layout this
/// replaced capped each split against what the one before it left, and with
/// the workspace section open that cap pinned the result pane: its handle
/// looked draggable and did nothing at all.
#[test]
fn every_sql_handle_moves_its_boundary() {
    for workspace in [false, true] {
        let panes = if workspace { 4 } else { 2 };
        for handle in 0..panes - 1 {
            let before = run_sql_panel(600.0, workspace, 3, SqlPanelPosition::Bottom, None);
            let after = run_sql_panel(
                600.0,
                workspace,
                3,
                SqlPanelPosition::Bottom,
                Some((handle, 70.0)),
            );
            assert_eq!(before.pane_heights.len(), panes);
            let grew = after.pane_heights[handle] - before.pane_heights[handle];
            let shrank = before.pane_heights[handle + 1] - after.pane_heights[handle + 1];
            assert!(
                grew > 50.0 && shrank > 50.0,
                "workspace={workspace} handle {handle}: {:?} -> {:?}",
                before.pane_heights,
                after.pane_heights
            );
            // Space moved between the two neighbours and nowhere else.
            assert!(
                (grew - shrank).abs() < 1.0,
                "workspace={workspace} handle {handle} leaked space: {:?} -> {:?}",
                before.pane_heights,
                after.pane_heights
            );
        }
    }
}
