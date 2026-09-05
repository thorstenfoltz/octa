//! Geometry: node sizes, edge anchors, seed positions, the laid-out map
//! and its PDF / PNG / SVG / HTML export.
//!
//! Split out of `app/dialogs/rel_map.rs` (1,434 lines). Code moved unchanged.

use super::draw::edge_tooltip;
use super::*;

/// How many columns a box lists: all of them once the user has clicked its
/// `+N` row open, otherwise the first `MAX_LISTED_COLS`.
pub(super) fn listed_cols(cols: usize, expanded: bool) -> usize {
    if expanded {
        cols
    } else {
        cols.min(MAX_LISTED_COLS)
    }
}

/// Height of a node box, given how many columns it lists.
pub(super) fn node_height(cols: usize, listed: usize) -> f32 {
    // One extra row for the "+N more" line when the list was cut short.
    let extra = if listed < cols { 1.0 } else { 0.0 };
    HEADER_H + (listed as f32 + extra) * ROW_H + 6.0
}

/// Where a column's line should attach, on the given side of a node.
///
/// A column past the end of the list attaches to the last visible row, which
/// is why `listed` has to be passed rather than assumed: expanding a box
/// moves the line to the column it actually names.
pub(super) fn anchor(top_left: Pos2, col: usize, right_side: bool, listed: usize) -> Pos2 {
    let row = col.min(listed.saturating_sub(1));
    let y = top_left.y + HEADER_H + row as f32 * ROW_H + ROW_H / 2.0;
    let x = if right_side {
        top_left.x + NODE_W
    } else {
        top_left.x
    };
    Pos2::new(x, y)
}

/// Both ends of one edge and its chip, in map space (no scroll offset).
///
/// The drawing adds the painter's origin to these and the export does not,
/// which is the only difference between what is on screen and what is
/// written to a file. Sharing the arithmetic is what stops the two drifting.
pub(super) fn edge_geometry(
    st: &RelMapState,
    map: &RelMap,
    ei: usize,
) -> Option<(Pos2, Pos2, Pos2, bool)> {
    let e = map.edges.get(ei)?;
    let lp = st.positions.get(e.left_table).copied()?;
    let rp = st.positions.get(e.right_table).copied()?;
    let ln = map.nodes.get(e.left_table)?;
    let rn = map.nodes.get(e.right_table)?;
    // Attach to whichever sides face each other, so the line does not cross
    // back over its own box.
    let left_first = lp.x <= rp.x;
    let a = anchor(
        lp,
        e.left_col,
        left_first,
        listed_cols(ln.columns.len(), st.expanded.contains(&e.left_table)),
    );
    let b = anchor(
        rp,
        e.right_col,
        !left_first,
        listed_cols(rn.columns.len(), st.expanded.contains(&e.right_table)),
    );
    let straight_mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let bend = st.chip_offsets.get(ei).copied().unwrap_or_default();
    Some((a, b, straight_mid + bend, bend != Vec2::ZERO))
}

/// Parse a typed threshold. Accepts a comma decimal mark, because half of
/// the locales Octa ships write `0,35`, and clamps rather than rejecting a
/// value outside 0..1 - a threshold above 1 is reachable by holding a key
/// down and means "show nothing", which is not worth an error message.
pub(super) fn parse_score(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().map(|v| v.clamp(0.0, 1.0))
}

/// Seed a grid so the first render is readable before anyone drags anything.
pub(super) fn seed_positions(count: usize) -> Vec<Pos2> {
    let per_row = (count as f32).sqrt().ceil().max(1.0) as usize;
    (0..count)
        .map(|i| {
            Pos2::new(
                20.0 + (i % per_row) as f32 * GRID_X,
                20.0 + (i / per_row) as f32 * GRID_Y,
            )
        })
        .collect()
}

/// Snapshot the map exactly as it stands, ready to be written to a file.
///
/// Uses the same `edge_geometry`, `node_height` and `listed_cols` the drawing
/// does, minus the painter's origin, so what lands in the PDF is what was on
/// screen: the boxes where they were dragged, the lines where they were bent,
/// the column lists as far as they were opened.
pub(super) fn build_layout(st: &RelMapState, map: &RelMap, visuals: &egui::Visuals) -> MapLayout {
    let nodes = map
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let pos = st.positions.get(i).copied().unwrap_or_default();
            NodeBox {
                name: n.name.clone(),
                columns: n.columns.clone(),
                listed: listed_cols(n.columns.len(), st.expanded.contains(&i)),
                x: pos.x,
                y: pos.y,
                rows: n.rows,
            }
        })
        .collect();

    let edges = map
        .edges
        .iter()
        .enumerate()
        .filter_map(|(ei, e)| {
            let (a, b, mid, bent) = edge_geometry(st, map, ei)?;
            Some(EdgeLine {
                left_table: e.left_table,
                left_col: e.left_col,
                right_table: e.right_table,
                right_col: e.right_col,
                ax: a.x,
                ay: a.y,
                bx: b.x,
                by: b.y,
                mx: mid.x,
                my: mid.y,
                bent,
                label: if e.scored {
                    format!("{:.2}", e.score)
                } else {
                    t("relmap.chip_declared")
                },
                tooltip: edge_tooltip(map, e),
            })
        })
        .collect();

    MapLayout {
        title: t("relmap.title"),
        nodes,
        edges,
        colors: MapColors {
            background: visuals.panel_fill.to_array(),
            node_fill: visuals.faint_bg_color.to_array(),
            node_stroke: visuals.widgets.noninteractive.fg_stroke.color.to_array(),
            text: visuals.text_color().to_array(),
            weak_text: visuals.weak_text_color().to_array(),
            line: visuals.hyperlink_color.to_array(),
            chip_fill: visuals.extreme_bg_color.to_array(),
        },
        node_w: NODE_W,
        header_h: HEADER_H,
        row_h: ROW_H,
    }
}

/// Ask for a path and write the map to it. Returns what to tell the user.
pub(super) fn export_map(layout: &MapLayout, format: RelMapExportFormat) -> Option<(bool, String)> {
    let ext = format.extension();
    let path = rfd::FileDialog::new()
        .set_title(t("relmap.export"))
        .add_filter(format.label(), &[ext])
        .set_file_name(format!("relationship-map.{ext}"))
        .save_file()?;
    match octa::data::rel_map_export::render(layout, format)
        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
    {
        Ok(()) => Some((
            true,
            t("relmap.export_done").replace("{path}", &path.display().to_string()),
        )),
        Err(e) => Some((false, e)),
    }
}

/// Put a freshly built map on screen: reset the layout, straighten every line.
///
/// Shared by the scan and by the redraw that follows a table being ticked,
/// because both change how many boxes and lines there are, and both index
/// vectors have to stay parallel to them.
pub(super) fn apply_map(st: &mut RelMapState, map: RelMap) {
    st.positions = seed_positions(map.nodes.len());
    // Node indices belong to the map that produced them, so a new map cannot
    // inherit which boxes were open.
    st.expanded.clear();
    st.export_result = None;
    // Index-parallel to the edges, so `get(ei)` is always in range. ZERO
    // means every edge starts out straight.
    st.chip_offsets = vec![Vec2::ZERO; map.edges.len()];
    st.map = Some(map);
}
