//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

/// The original (pre-freeze) drag-target arithmetic, kept verbatim as the
/// no-regression oracle for `drag_target_at_x` with `frozen_cols == 0`.
fn original_drag_target(rel_x: f32, col_widths: &[f32], scroll_x: f32) -> usize {
    let pointer_x = rel_x + scroll_x;
    let mut acc = 0.0f32;
    let mut target = col_widths.len().saturating_sub(1);
    for (i, &cw) in col_widths.iter().enumerate() {
        if pointer_x < acc + cw / 2.0 {
            target = i;
            break;
        }
        acc += cw;
        target = i;
    }
    target
}

#[test]
fn drag_target_matches_original_when_nothing_is_frozen() {
    let widths = [100.0, 80.0, 120.0, 60.0];
    for scroll_x in [0.0, 50.0, 173.0] {
        for rel_x in [-20.0, 0.0, 10.0, 99.0, 150.0, 250.0, 400.0, 1000.0] {
            assert_eq!(
                drag_target_at_x(rel_x, &widths, 0, 0.0, scroll_x),
                original_drag_target(rel_x, &widths, scroll_x),
                "rel_x={rel_x} scroll_x={scroll_x}"
            );
        }
    }
}

#[test]
fn drag_target_resolves_frozen_band_positions_ignoring_scroll() {
    // Two frozen columns of 100 + 80 px; scrolled hard to the right.
    let widths = [100.0, 80.0, 120.0, 60.0];
    let frozen_width = 180.0;
    let scroll = 500.0;
    // Inside the frozen band the scroll offset is irrelevant.
    assert_eq!(drag_target_at_x(10.0, &widths, 2, frozen_width, scroll), 0);
    assert_eq!(drag_target_at_x(120.0, &widths, 2, frozen_width, scroll), 1);
    // Just right of the band: content coordinate = rel - band + scroll.
    // rel_x 190 -> content 510 -> past both scrolled columns' midpoints.
    assert_eq!(drag_target_at_x(190.0, &widths, 2, frozen_width, scroll), 3);
    // With no scroll, just right of the band is the first scrolled column.
    assert_eq!(drag_target_at_x(190.0, &widths, 2, frozen_width, 0.0), 2);
}

#[test]
fn frozen_band_width_skips_hidden_columns() {
    let widths = vec![100.0, 80.0, 120.0];
    let mut hidden = HashSet::new();
    assert_eq!(frozen_band_width(&widths, &hidden, 0), 0.0);
    assert_eq!(frozen_band_width(&widths, &hidden, 2), 180.0);
    assert_eq!(frozen_band_width(&widths, &hidden, 99), 300.0);
    hidden.insert(1);
    assert_eq!(frozen_band_width(&widths, &hidden, 2), 100.0);
}

#[test]
fn scroll_col_into_view_is_unchanged_with_no_frozen_band() {
    let mut state = TableViewState {
        col_widths: vec![100.0, 100.0, 100.0, 100.0],
        row_number_width: 60.0,
        ..Default::default()
    };
    // Viewport fits two columns beside the gutter; bring column 3 in.
    scroll_col_into_view(&mut state, 3, 260.0, 1000.0, 0, 0.0);
    // col 3 right edge = 400; window = 260 - 60 = 200 -> scroll_x = 200.
    assert_eq!(state.scroll_x, 200.0);
    // Scrolling back to column 0 returns to the origin.
    scroll_col_into_view(&mut state, 0, 260.0, 1000.0, 0, 0.0);
    assert_eq!(state.scroll_x, 0.0);
}

#[test]
fn scroll_col_into_view_accounts_for_the_frozen_band() {
    let mut state = TableViewState {
        col_widths: vec![100.0, 100.0, 100.0, 100.0],
        row_number_width: 60.0,
        ..Default::default()
    };
    // One frozen column: the scrollable window shrinks by its width and
    // offsets are measured from the first scrolled column.
    scroll_col_into_view(&mut state, 3, 360.0, 1000.0, 1, 100.0);
    // cols 1..3 left = 200, right = 300; window = 360 - 60 - 100 = 200
    // -> scroll_x = 300 - 200 = 100.
    assert_eq!(state.scroll_x, 100.0);
    // A frozen column never changes the scroll.
    scroll_col_into_view(&mut state, 0, 360.0, 1000.0, 1, 100.0);
    assert_eq!(state.scroll_x, 100.0);
}

/// The virtual scrollbar has to address the whole file, not the loaded page:
/// the thumb reaches both ends, and a drag across the full travel spans every
/// row. Without that, a 2,000-row window over 100M rows can only be walked.
#[test]
fn virtual_thumb_spans_the_whole_file() {
    let track = 600.0;
    let rows = 100_000_000usize;
    let visible = 40.0;

    let top = virtual_thumb(0.0, rows, visible, track);
    assert_eq!(top.offset, 0.0, "row 0 parks the thumb at the top");
    assert!(top.height >= 24.0, "thumb stays grabbable: {}", top.height);
    assert!(top.travel > 0.0);

    // The drag handler inverts this geometry with `max_row / travel` rows per
    // pixel. Dragging the thumb its full travel must therefore land on the last
    // reachable row - that is what "the bar addresses the file" means.
    let rows_per_pixel = top.max_row / top.travel;
    let landed = (0.0 + top.travel * rows_per_pixel).min(top.max_row);
    assert!(
        landed as usize >= rows - visible as usize - 1,
        "a full drag reached only row {landed} of {rows}"
    );

    // And the inversion round-trips: a thumb painted for row R, read back
    // through the same rows-per-pixel, is row R again.
    for row in [1_000.0f32, 25_000_000.0, 99_000_000.0] {
        let g = virtual_thumb(row, rows, visible, track);
        let back = g.offset * (g.max_row / g.travel);
        assert!(
            (back - row).abs() < row * 0.001 + 1.0,
            "row {row} painted at {} reads back as {back}",
            g.offset
        );
    }

    let bottom = virtual_thumb(top.max_row, rows, visible, track);
    assert!(
        (bottom.offset - bottom.travel).abs() < 0.5,
        "the last row parks the thumb at the bottom: {} vs {}",
        bottom.offset,
        bottom.travel
    );

    // Halfway down the file is halfway down the track.
    let mid = virtual_thumb(rows as f32 / 2.0, rows, visible, track);
    assert!((mid.offset - mid.travel / 2.0).abs() < 1.0);
}

/// A table shorter than the viewport must not produce a thumb taller than the
/// track or a negative travel, which would invert the drag.
#[test]
fn virtual_thumb_survives_a_tiny_file() {
    let t = virtual_thumb(0.0, 3, 40.0, 600.0);
    assert!(t.height <= 600.0);
    assert!(t.travel >= 0.0);
    assert!(t.max_row >= 1.0, "never divide by zero");
}

#[test]
fn split_sizes_keep_every_band_usable() {
    use super::split::split_sizes;

    // Half and half, minus the divider.
    assert_eq!(split_sizes(606.0, 2, &[]), vec![300.0, 300.0]);

    // A divider dragged to the very top still leaves a usable band there.
    assert_eq!(split_sizes(606.0, 2, &[0.0]), vec![80.0, 520.0]);
    // And to the very bottom.
    assert_eq!(split_sizes(606.0, 2, &[1.0]), vec![520.0, 80.0]);

    // A panel too short for two bands splits evenly rather than reporting a
    // negative height (400x300 window, deep zoom, both side panels open).
    assert_eq!(split_sizes(106.0, 2, &[0.9]), vec![50.0, 50.0]);

    // Six even bands, dividers included.
    let six = split_sizes(1000.0, 6, &[]);
    assert_eq!(six.len(), 6);
    assert!(
        six.iter().all(|s| (s - 970.0 / 6.0).abs() < 0.001),
        "{six:?}"
    );
    assert!((six.iter().sum::<f32>() - 970.0).abs() < 0.01, "{six:?}");

    // One divider shoved past its neighbours cannot squeeze the bands behind
    // it, nor the ones still ahead of it.
    let squeezed = split_sizes(1000.0, 4, &[0.0, 0.0, 0.0]);
    assert!(
        squeezed.iter().all(|s| *s >= 80.0),
        "no band collapses: {squeezed:?}"
    );
    assert!(
        (squeezed.iter().sum::<f32>() - 982.0).abs() < 0.01,
        "{squeezed:?}"
    );

    // Six bands in a short panel: even shares, never negative.
    let tight = split_sizes(300.0, 6, &[]);
    assert!(tight.iter().all(|s| *s > 0.0), "{tight:?}");
    assert_eq!(tight.len(), 6);
}

/// The count moves inside 2..=MAX and nowhere else, and turning the split off
/// and on again keeps the count the user chose.
#[test]
fn the_pane_count_stays_inside_its_range() {
    let mut state = TableViewState::default();
    assert!(!state.is_split());
    assert_eq!(state.split_panes(), 1);
    // Nothing to add to while the view is whole.
    assert!(!state.add_split_pane());

    state.set_split(true, false);
    assert!(state.is_split());
    assert_eq!(state.split_panes(), 2);
    // Two is the floor: below it the split is simply off, which is a
    // different action.
    assert!(!state.remove_split_pane());

    while state.add_split_pane() {}
    assert_eq!(state.split_panes(), super::MAX_SPLIT_PANES);
    assert!(!state.add_split_pane(), "the cap holds");

    // Flipping orientation keeps the bands: four stacked becomes four beside.
    state.set_split(true, true);
    assert_eq!(state.split_panes(), super::MAX_SPLIT_PANES);
    assert!(state.split_side_by_side);

    state.set_split(false, true);
    assert!(!state.is_split());
}

/// The regression this replaces: two bands that shared an axis showed the same
/// cells along it, so half the split was wasted. Every pane owns both offsets.
#[test]
fn every_pane_keeps_its_own_two_offsets() {
    let mut state = TableViewState::default();
    state.set_split(true, false);
    state.add_split_pane();
    assert_eq!(
        state.pane_scroll.len(),
        2,
        "one slot per pane after the first"
    );

    // Pane 0 reads the plain fields; the others are swapped in around their
    // own draw call, which is what `split::draw_table_split` does.
    state.scroll_x = 11.0;
    state.scroll_y = 22.0;
    state.pane_scroll[0] = (33.0, 44.0);
    state.pane_scroll[1] = (55.0, 66.0);

    for pane in 1..3 {
        super::split::swap_pane_scroll(&mut state, pane);
        let seen = (state.scroll_x, state.scroll_y);
        super::split::swap_pane_scroll(&mut state, pane);
        assert_eq!(
            seen,
            (11.0 + pane as f32 * 22.0, 22.0 + pane as f32 * 22.0),
            "pane {pane} draws with its own offsets, both axes"
        );
    }
    assert_eq!(
        (state.scroll_x, state.scroll_y),
        (11.0, 22.0),
        "and hands them back"
    );
    assert_eq!(state.pane_scroll, vec![(33.0, 44.0), (55.0, 66.0)]);
}

/// Dropping a pane must not leave a stale offset behind for a band that is no
/// longer drawn, nor a divider position the new count cannot use.
#[test]
fn changing_the_count_resizes_the_per_pane_state() {
    let mut state = TableViewState::default();
    state.set_split(true, false);
    state.add_split_pane();
    state.add_split_pane();
    assert_eq!(state.split_panes(), 4);
    assert_eq!(state.pane_scroll.len(), 3);

    state.remove_split_pane();
    assert_eq!(state.split_panes(), 3);
    assert_eq!(state.pane_scroll.len(), 2);
    assert!(
        state.split_fractions.is_empty(),
        "a changed count re-spaces the dividers evenly"
    );
}

/// A table tall and wide enough that every pane has somewhere to scroll.
fn scrollable_table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = (0..12)
        .map(|c| crate::data::ColumnInfo {
            name: format!("col{c}"),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = (0..400)
        .map(|r| {
            (0..12)
                .map(|c| crate::data::CellValue::String(format!("r{r}c{c}")))
                .collect()
        })
        .collect();
    t
}

/// One headless frame of the split view with the given events. Returns the
/// per-pane vertical offsets afterwards, pane 0 first.
fn split_frame(
    ctx: &egui::Context,
    table: &mut DataTable,
    state: &mut TableViewState,
    events: Vec<egui::Event>,
) -> Vec<f32> {
    run_frame(ctx, table, state, events);
    std::iter::once(state.scroll_y)
        .chain(state.pane_scroll.iter().map(|(_, y)| *y))
        .collect()
}

/// One headless frame of the table with the given events, returning egui's
/// output so a test can read the cursor it asked the platform for.
fn run_frame(
    ctx: &egui::Context,
    table: &mut DataTable,
    state: &mut TableViewState,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let filtered: Vec<usize> = (0..table.rows.len()).collect();
    let shortcuts = crate::ui::shortcuts::Shortcuts::default();
    let empty_cols: HashSet<usize> = HashSet::new();
    let empty_cells: HashSet<(usize, usize)> = HashSet::new();
    let formats = std::collections::HashMap::new();
    let input = egui::RawInput {
        events,
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(900.0, 900.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let cx = TableCtx {
            theme_mode: ThemeMode::Dark,
            filtered_rows: &filtered,
            os_clipboard_has_content: false,
            show_row_numbers: true,
            show_sequential_numbers: false,
            alternating_row_colors: false,
            negative_numbers_red: false,
            highlight_edits: false,
            font_size: 13.0,
            cell_line_breaks: false,
            clickable_links: false,
            binary_display_mode: crate::data::BinaryDisplayMode::default(),
            welcome_logo_texture: None,
            shortcuts: &shortcuts,
            readonly: false,
            filtered_columns: &empty_cols,
            hidden_columns: &empty_cols,
            thousands_separators: false,
            separator_style: crate::data::num_format::SeparatorStyle::default(),
            column_number_formats: &formats,
            search_matches: &empty_cells,
            current_match: None,
            conditional_format_rules: &[],
            validation_violations: &empty_cells,
            outlier_cells: &empty_cells,
            handles_input: true,
            scroll_all: false,
        };
        super::split::draw_table_split(ui, table, state, cx);
    });
    out.textures_delta.clear();
    out
}

/// One wheel notch downwards, optionally with Alt held.
///
/// Two events, because that is what a real backend sends: egui tracks the held
/// modifiers across frames from `ModifiersChanged` (a wheel event's own
/// `modifiers` field does not update them), and the split view reads the held
/// ones so the smoothed tail of a scroll stays in step with its start.
fn wheel(alt: bool) -> Vec<egui::Event> {
    let modifiers = egui::Modifiers {
        alt,
        ..Default::default()
    };
    vec![
        egui::Event::ModifiersChanged(modifiers),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            phase: egui::TouchPhase::Move,
            modifiers,
        },
    ]
}

/// The wheel belongs to the band the pointer is over, and to that band alone.
#[test]
fn the_wheel_scrolls_only_the_band_under_the_pointer() {
    let ctx = egui::Context::default();
    let mut table = scrollable_table();
    let mut state = TableViewState::default();
    state.set_split(true, false);
    state.add_split_pane();

    // Lay the panes out once, then park the pointer in the middle band. With
    // 900 pixels and three bands, band 1 spans roughly y=300..600.
    split_frame(&ctx, &mut table, &mut state, vec![]);
    split_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(egui::pos2(400.0, 450.0))],
    );
    let before = split_frame(&ctx, &mut table, &mut state, vec![]);
    let after = split_frame(&ctx, &mut table, &mut state, wheel(false));

    assert!(
        after[1] > before[1],
        "the band under the pointer scrolls: {before:?} -> {after:?}"
    );
    assert_eq!(after[0], before[0], "the band above stays put: {after:?}");
    assert_eq!(after[2], before[2], "the band below stays put: {after:?}");
}

/// Alt is the "move them together" modifier: every band takes the same notch.
#[test]
fn alt_and_the_wheel_scroll_every_band() {
    let ctx = egui::Context::default();
    let mut table = scrollable_table();
    let mut state = TableViewState::default();
    state.set_split(true, false);
    state.add_split_pane();

    split_frame(&ctx, &mut table, &mut state, vec![]);
    split_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(egui::pos2(400.0, 450.0))],
    );
    let before = split_frame(&ctx, &mut table, &mut state, vec![]);
    let after = split_frame(&ctx, &mut table, &mut state, wheel(true));

    for pane in 0..3 {
        assert!(
            after[pane] > before[pane],
            "band {pane} moves with Alt held: {before:?} -> {after:?}"
        );
    }
}

/// The reported bug: every pane's scrollbar was one widget, because egui gives
/// each `allocate_ui` child the same id. Grabbing any thumb dragged every band
/// at once and only the last one drawn could be aimed. Each pane is now salted
/// by index, so a thumb belongs to its own band.
#[test]
fn dragging_one_bands_scrollbar_leaves_the_others_alone() {
    let ctx = egui::Context::default();
    let mut table = scrollable_table();
    let mut state = TableViewState::default();
    state.set_split(true, false);
    state.add_split_pane();

    // 900 pixels, three bands, two 6-pixel dividers: band 1 starts at 302.
    // Its scrollbar sits at the right edge, thumb at the top while unscrolled.
    let thumb = egui::pos2(894.0, 320.0);
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };

    split_frame(&ctx, &mut table, &mut state, vec![]);
    let before = split_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(thumb), press(thumb, true)],
    );
    let dragged = egui::pos2(894.0, 400.0);
    let after = split_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(dragged)],
    );
    split_frame(&ctx, &mut table, &mut state, vec![press(dragged, false)]);

    assert!(
        after[1] > before[1],
        "the grabbed band scrolls: {before:?} -> {after:?}"
    );
    assert_eq!(after[0], before[0], "the band above stays put: {after:?}");
    assert_eq!(after[2], before[2], "the band below stays put: {after:?}");
}

/// A cell legend explains one value in one column. Both halves of the lookup
/// matter: a column with no legend explains nothing, and a value that happens
/// to match a *different* column's legend must not pick it up. The quality
/// report has a `column_name` column, so a table whose column is literally
/// named `gaps` is not a contrived case.
#[test]
fn a_cell_legend_only_answers_for_its_own_column() {
    let state = TableViewState {
        cell_tooltips: vec![
            std::collections::HashMap::new(),
            [(
                "gaps".to_string(),
                "Time that should have rows.".to_string(),
            )]
            .into_iter()
            .collect(),
        ],
        ..Default::default()
    };

    let mut table = DataTable::empty();
    table.columns = vec![
        crate::data::ColumnInfo {
            name: "column_name".into(),
            data_type: "Utf8".into(),
        },
        crate::data::ColumnInfo {
            name: "calendar_verdict".into(),
            data_type: "Utf8".into(),
        },
    ];
    table.rows = vec![vec![
        crate::data::CellValue::String("gaps".into()),
        crate::data::CellValue::String("gaps".into()),
    ]];

    // Same text, two columns: only the one carrying the legend answers.
    assert_eq!(rows::cell_tooltip(&state, &table, 0, 0), None);
    assert_eq!(
        rows::cell_tooltip(&state, &table, 0, 1).as_deref(),
        Some("Time that should have rows.")
    );
}

/// An ordinary table carries no legend at all, and the lookup has to stay out
/// of the way rather than panic on the missing column entry.
#[test]
fn a_table_without_a_legend_has_no_cell_tooltips() {
    let state = TableViewState::default();
    let mut table = DataTable::empty();
    table.columns = vec![crate::data::ColumnInfo {
        name: "city".into(),
        data_type: "Utf8".into(),
    }];
    table.rows = vec![vec![crate::data::CellValue::String("gaps".into())]];

    assert_eq!(rows::cell_tooltip(&state, &table, 0, 0), None);
    // Out of range on both axes, which is what a stale index looks like.
    assert_eq!(rows::cell_tooltip(&state, &table, 9, 9), None);
}

/// Build the prefix sums headlessly with wrapping off, which is the mode the
/// row-resize feature has to work in: nothing is measured, so every row is
/// exactly the base height unless the user dragged one.
fn offsets_with(overrides: &[(usize, f32)], rows: usize, base: f32) -> Vec<f32> {
    let ctx = egui::Context::default();
    let mut table = DataTable::empty();
    table.columns = vec![crate::data::ColumnInfo {
        name: "c".into(),
        data_type: "Utf8".into(),
    }];
    table.rows = (0..rows)
        .map(|r| vec![crate::data::CellValue::String(format!("r{r}"))])
        .collect();
    let mut state = TableViewState {
        col_widths: vec![100.0],
        ..Default::default()
    };
    for &(row, h) in overrides {
        state.row_heights.insert(row, h);
    }
    state.invalidate_row_heights();
    let filtered: Vec<usize> = (0..rows).collect();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(400.0, 400.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        ensure_row_y_offsets(
            ui,
            &mut state,
            &table,
            &filtered,
            RowHeightOpts {
                font_size: 13.0,
                base_row_height: base,
                binary_display_mode: BinaryDisplayMode::Hex,
                wrap: false,
            },
        );
    });
    out.textures_delta.clear();
    state.row_y_offsets
}

/// A height the user dragged wins; every other row keeps the base height, and
/// the last entry is the height of the whole table.
#[test]
fn dragged_row_heights_feed_the_offsets_table() {
    let offsets = offsets_with(&[(1, 60.0), (3, 10.0)], 5, 20.0);

    assert_eq!(offsets.len(), 6, "one entry per row plus the closing total");
    // 20, 60, 20, 10, 20
    assert_eq!(offsets, vec![0.0, 20.0, 80.0, 100.0, 110.0, 130.0]);
}

/// With no overrides and wrapping off the table is uniform, so the prefix sums
/// have to agree with the plain `rows * height` the fast path uses.
#[test]
fn unresized_rows_all_get_the_base_height() {
    let offsets = offsets_with(&[], 4, 22.0);

    assert_eq!(offsets, vec![0.0, 22.0, 44.0, 66.0, 88.0]);
}

/// The scroll position is turned back into a row by binary search, and it has
/// to land on the row that actually covers the offset even when the rows in
/// front of it are all different heights.
#[test]
fn row_at_offset_handles_mixed_heights() {
    // Rows of 20, 60, 20, 10, 20 -> boundaries at 0, 20, 80, 100, 110, 130.
    let offsets = offsets_with(&[(1, 60.0), (3, 10.0)], 5, 20.0);

    assert_eq!(row_at_offset(&offsets, 0.0), 0);
    assert_eq!(row_at_offset(&offsets, 19.9), 0);
    assert_eq!(
        row_at_offset(&offsets, 20.0),
        1,
        "exactly on a seam is the row below"
    );
    assert_eq!(
        row_at_offset(&offsets, 79.0),
        1,
        "the tall row spans 20..80"
    );
    assert_eq!(row_at_offset(&offsets, 80.0), 2);
    assert_eq!(
        row_at_offset(&offsets, 105.0),
        3,
        "the short row spans 100..110"
    );
    assert_eq!(row_at_offset(&offsets, 129.0), 4);
}

/// Press on a row's bottom seam in the gutter, drag down, release. The seam
/// is a thin strip that the row-number cell (registered after it) overlaps on
/// both halves, so the test drives real pointer events through egui's hit
/// test rather than calling the height code directly.
fn drag_row_seam(
    rows_below_header: usize,
    dy: f32,
    batched: bool,
) -> (TableViewState, egui::CursorIcon) {
    let ctx = egui::Context::default();
    let mut table = scrollable_table();
    let mut state = TableViewState::default();
    let base = base_row_height(13.0);
    let seam_y = HEADER_HEIGHT + 1.0 + base * (rows_below_header as f32 + 1.0);
    let seam = egui::pos2(10.0, seam_y);
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    run_frame(&ctx, &mut table, &mut state, vec![]);
    let hover = run_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(seam)],
    );
    let cursor = hover.platform_output.cursor_icon;
    let target = egui::pos2(seam.x, seam.y + dy);
    // A fast mouse delivers the press and the first move inside one frame,
    // with the pointer already past the seam band when egui looks.
    let mut press_frame = vec![press(seam, true)];
    if batched {
        press_frame.push(egui::Event::PointerMoved(egui::pos2(seam.x, seam.y + 8.0)));
    }
    run_frame(&ctx, &mut table, &mut state, press_frame);
    run_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(target)],
    );
    run_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(target)],
    );
    run_frame(&ctx, &mut table, &mut state, vec![press(target, false)]);
    (state, cursor)
}

#[test]
fn dragging_a_row_seam_resizes_that_row_only() {
    let base = base_row_height(13.0);
    for row in [0usize, 1, 2, 5] {
        let (state, cursor) = drag_row_seam(row, 30.0, false);
        let got = state.row_heights.get(&row).copied();
        assert!(
            got.is_some_and(|h| (h - (base + 30.0)).abs() < 1.0),
            "row {row}: height {got:?}, expected ~{}; all heights {:?}",
            base + 30.0,
            state.row_heights
        );
        assert_eq!(
            state.row_heights.len(),
            1,
            "row {row}: only that row changed"
        );
        assert_eq!(
            cursor,
            egui::CursorIcon::ResizeVertical,
            "row {row}: hovering the seam shows the resize cursor"
        );
    }
}

#[test]
fn a_press_and_move_batched_into_one_frame_still_starts_the_drag() {
    let base = base_row_height(13.0);
    let (state, _) = drag_row_seam(2, 30.0, true);
    let got = state.row_heights.get(&2).copied();
    assert!(
        got.is_some_and(|h| (h - (base + 30.0)).abs() < 1.0),
        "height {got:?}, expected ~{}",
        base + 30.0
    );
}

/// The seam adopts a drag on the press frame, which must not eat the click
/// side of its own double-click, nor the row-number click beside it.
#[test]
fn seam_double_click_and_row_number_click_survive_the_drag_adoption() {
    let ctx = egui::Context::default();
    let mut table = scrollable_table();
    let mut state = TableViewState::default();
    let base = base_row_height(13.0);
    state.row_heights.insert(2, base + 40.0);
    let seam = egui::pos2(10.0, HEADER_HEIGHT + 1.0 + base * 3.0 + 40.0);
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    run_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(seam)],
    );
    for _ in 0..2 {
        run_frame(&ctx, &mut table, &mut state, vec![press(seam, true)]);
        run_frame(&ctx, &mut table, &mut state, vec![press(seam, false)]);
    }
    assert!(
        !state.row_heights.contains_key(&2),
        "double-click drops the hand-set height: {:?}",
        state.row_heights
    );

    let row_number = egui::pos2(10.0, HEADER_HEIGHT + 1.0 + base * 1.5);
    run_frame(
        &ctx,
        &mut table,
        &mut state,
        vec![egui::Event::PointerMoved(row_number)],
    );
    run_frame(&ctx, &mut table, &mut state, vec![press(row_number, true)]);
    run_frame(&ctx, &mut table, &mut state, vec![press(row_number, false)]);
    assert!(
        state.selected_rows.contains(&1),
        "row 1 selected: {:?}",
        state.selected_rows
    );
}
