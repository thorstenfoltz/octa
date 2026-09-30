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
fn prefix_takes_non_ascii_letters() {
    // `Gr\u{f6}` used to stop at the umlaut and offer nothing for `Gr\u{f6}\u{df}e`.
    let s = "SELECT Gr\u{f6}";
    let (start, pfx) = current_prefix_at(s, s.len());
    assert_eq!(pfx, "Gr\u{f6}");
    assert_eq!(start, 7);
    // A cursor inside a multi-byte char must not panic.
    let (_, pfx) = current_prefix_at(s, s.len() - 1);
    assert_eq!(pfx, "Gr");
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
    tab.sql.query = (0..editor_lines)
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
                        history: &[],
                        server_running: false,
                        db_connections: Vec::new(),
                        cloud_connections: Vec::new(),
                        auto_registered: &[],
                        show_auto_register_notice: false,
                        chat_profile_available: false,
                        ask_profiles: Vec::new(),
                        extra_identifiers: &[],
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

#[test]
fn line_comments_run_to_end_of_line_and_skip_strings() {
    let s = "SELECT 1 -- one\nSELECT '--x' -- two";
    let got: Vec<&str> = line_comment_ranges(s).into_iter().map(|r| &s[r]).collect();
    assert_eq!(got, vec!["-- one", "-- two"]);
    assert!(line_comment_ranges("SELECT 'it''s -- no'").is_empty());
}

/// Paste and copy reach the SQL editor through the real view, on a tab that
/// holds a table and has a selected result cell (the Ctrl+C hijack beside it).
#[test]
fn editor_takes_paste_and_copy_with_a_table_open() {
    use crate::app::state::TabState;

    let ctx = egui::Context::default();
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    tab.table = octa::data::DataTable::empty();
    tab.table.columns.push(octa::data::ColumnInfo {
        name: "a".into(),
        data_type: "Utf8".into(),
    });
    tab.sql.query = "SELECT 1".into();
    tab.sql.focus_pending = true;
    let frame = |events: Vec<egui::Event>, tab: &mut TabState| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::Panel::bottom("sql_panel_bottom")
                .default_size(500.0)
                .show(ui, |ui| {
                    render_sql_view(
                        ui,
                        tab,
                        SqlViewContext {
                            autocomplete_enabled: true,
                            default_row_limit: 1000,
                            partial_rows: None,
                            editor_font: octa::ui::settings::SqlEditorFont::SystemMonospace,
                            workspace_tables: &[],
                            workspace_attachments: &[],
                            inspector_selection: None,
                            inspector_entry: None,
                            history: &[],
                            server_running: false,
                            db_connections: Vec::new(),
                            cloud_connections: Vec::new(),
                            auto_registered: &[],
                            show_auto_register_notice: false,
                            chat_profile_available: false,
                            ask_profiles: Vec::new(),
                            extra_identifiers: &[],
                        },
                    );
                });
        });
        out.textures_delta.clear();
        out
    };
    for _ in 0..3 {
        frame(Vec::new(), &mut tab);
    }
    frame(vec![egui::Event::Paste(" -- x".into())], &mut tab);
    assert_eq!(tab.sql.query, "SELECT 1 -- x");

    // Mark "SELECT" and copy it.
    let id = editor_id(tab.sql.id);
    let mut state = egui::TextEdit::load_state(&ctx, id).expect("editor state");
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(6),
        )));
    state.store(&ctx, id);
    assert_eq!(query_to_run(&ctx, &tab.sql), "SELECT");
    let out = frame(vec![egui::Event::Copy], &mut tab);
    let copied = out
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "SELECT"));
    assert!(copied, "{:?}", out.platform_output.commands);
}

fn grid() -> octa::data::DataTable {
    let mut t = octa::data::DataTable::empty();
    t.columns = ["a", "b", "c"]
        .iter()
        .map(|n| octa::data::ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = (0..3)
        .map(|r| {
            (0..3)
                .map(|c| CellValue::String(format!("{r}{c}")))
                .collect()
        })
        .collect();
    t
}

#[test]
fn result_selection_copies_cells_rows_and_columns() {
    let t = grid();
    let none = egui::Modifiers::NONE;
    let ctrl = egui::Modifiers::COMMAND;
    let mut sel = SqlResultSelection::default();
    // Ctrl+click scattered cells: one line per row, left to right.
    sel.click_cell(2, 2, none);
    sel.click_cell(0, 1, ctrl);
    sel.click_cell(2, 0, ctrl);
    assert_eq!(selection_to_tsv(&t, &sel), "01\n20\t22\n");
    // Ctrl+click again takes a cell back out.
    sel.click_cell(2, 0, ctrl);
    assert_eq!(selection_to_tsv(&t, &sel), "01\n22\n");
    // A row header replaces the selection with the whole row.
    sel.click_line(1, true, none);
    assert_eq!(selection_to_tsv(&t, &sel), "10\t11\t12\n");
    // A column added with Ctrl: every row, only the selected columns.
    sel.click_line(0, false, ctrl);
    assert_eq!(selection_to_tsv(&t, &sel), "00\n10\t11\t12\n20\n");
}

#[test]
fn result_selection_shift_click_takes_the_rectangle() {
    let t = grid();
    let mut sel = SqlResultSelection::default();
    sel.click_cell(0, 1, egui::Modifiers::NONE);
    sel.click_cell(1, 2, egui::Modifiers::SHIFT);
    assert_eq!(selection_to_tsv(&t, &sel), "01\t02\n11\t12\n");
}

#[test]
fn history_sources_resolve_to_names() {
    let dbs = vec![DbAttachEntry {
        id: "c1".into(),
        name: "prod".into(),
        drill: None,
    }];
    let clouds = vec![("s3".to_string(), "bucket".to_string())];
    assert_eq!(history_source_label("db:c1", &dbs, &clouds).0, "prod");
    assert_eq!(
        history_source_label("cloud:s3:dir/x.csv", &dbs, &clouds),
        ("bucket: x.csv".to_string(), "bucket: dir/x.csv".to_string())
    );
    assert_eq!(
        history_source_label("file:/tmp/sales.csv", &dbs, &clouds).0,
        "sales.csv"
    );
}

/// `[` and `]` mark the selection in these cases; `toggle` returns the text
/// after one press with the new selection marked the same way.
fn toggle(marked: &str) -> String {
    let start = marked.find('[').unwrap();
    let end = marked.find(']').unwrap() - 1;
    let text = marked.replace(['[', ']'], "");
    let (out, sel) = toggle_line_comments(&text, start..end);
    let mut shown = out.clone();
    shown.insert(sel.end, ']');
    shown.insert(sel.start, '[');
    shown
}

#[test]
fn comment_toggle_adds_after_leading_blanks_and_skips_blank_lines() {
    assert_eq!(
        toggle("[SELECT a\n\n  FROM t]"),
        "[--SELECT a\n\n  --FROM t]"
    );
    // The caret alone counts as its line.
    assert_eq!(toggle("SELECT a\nFR[]OM t"), "SELECT a\n--FR[]OM t");
}

#[test]
fn comment_toggle_removes_only_the_first_marker() {
    assert_eq!(toggle("[  --SELECT a\n-- -- b]"), "[  SELECT a\n -- b]");
}

#[test]
fn comment_toggle_comments_all_when_lines_are_mixed() {
    // Pressing twice gets back where you started.
    let once = toggle("[--a\nb]");
    assert_eq!(once, "[----a\n--b]");
    assert_eq!(toggle(&once), "[--a\nb]");
}

#[test]
fn comment_toggle_treats_a_trailing_comment_as_code_unless_it_is_marked() {
    // Selection over the code: the line gets commented at its start.
    assert_eq!(toggle("[SELECT] a -- note"), "[--SELECT] a -- note");
    // Selection only inside the trailing comment: that `--` goes.
    assert_eq!(toggle("SELECT a -- [note]"), "SELECT a  [note]");
    // A `--` inside a string is text, not a comment.
    assert_eq!(toggle("[SELECT '--x']"), "[--SELECT '--x']");
}

#[test]
fn comment_toggle_leaves_the_line_after_a_full_line_selection_alone() {
    assert_eq!(toggle("[a\n]b"), "[--a\n]b");
}

/// Several editors through the real view: each draws its own editor, side
/// by side left to right without overlap, and typing into one lands in that
/// one and makes it the active pane (the one Run and Format act on).
#[test]
fn side_by_side_editors_each_take_their_own_input() {
    use crate::app::state::TabState;

    let ctx = egui::Context::default();
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    tab.set_sql_queries(vec!["a".into(), "b".into(), "c".into()]);
    let frame = |events: Vec<egui::Event>, tab: &mut TabState| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1200.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                render_sql_view(
                    ui,
                    tab,
                    SqlViewContext {
                        autocomplete_enabled: false,
                        default_row_limit: 1000,
                        partial_rows: None,
                        editor_font: octa::ui::settings::SqlEditorFont::SystemMonospace,
                        workspace_tables: &[],
                        workspace_attachments: &[],
                        inspector_selection: None,
                        inspector_entry: None,
                        history: &[],
                        server_running: false,
                        db_connections: Vec::new(),
                        cloud_connections: Vec::new(),
                        auto_registered: &[],
                        show_auto_register_notice: false,
                        chat_profile_available: false,
                        ask_profiles: Vec::new(),
                        extra_identifiers: &[],
                    },
                );
            });
        });
        out.textures_delta.clear();
    };
    for _ in 0..3 {
        frame(Vec::new(), &mut tab);
    }
    let ids: Vec<u64> = (0..3).map(|i| tab.sql_pane_id(i)).collect();
    let rects: Vec<egui::Rect> = ids
        .iter()
        .map(|&id| ctx.read_response(editor_id(id)).expect("editor drawn").rect)
        .collect();
    for w in rects.windows(2) {
        assert!(w[0].right() <= w[1].left(), "editors overlap: {rects:?}");
    }

    ctx.memory_mut(|m| m.request_focus(editor_id(ids[1])));
    frame(Vec::new(), &mut tab);
    frame(vec![egui::Event::Text("X".into())], &mut tab);
    frame(Vec::new(), &mut tab);
    let queries = tab.sql_queries();
    assert_eq!(queries[0], "a");
    assert!(queries[1].contains('X'), "typed into pane 2: {queries:?}");
    assert_eq!(queries[2], "c");
    assert_eq!(
        tab.sql_active_pane, 1,
        "the focused editor is the active one"
    );
}
