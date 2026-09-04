//! Painting the map: nodes, edges, hover tooltips and drag handling.
//!
//! Split out of `app/dialogs/rel_map.rs` (1,434 lines). Code moved unchanged.

use super::layout::{edge_geometry, listed_cols, node_height};
use super::*;

/// Draw the diagram. Returns the edge index the user clicked, if any.
pub(super) fn draw_map(ui: &mut egui::Ui, st: &mut RelMapState, map: &RelMap) -> Option<usize> {
    let mut clicked_edge = None;

    let extent = st
        .positions
        .iter()
        .enumerate()
        .fold(Vec2::new(600.0, 400.0), |acc, (i, p)| {
            let cols = map.nodes[i].columns.len();
            let h = node_height(cols, listed_cols(cols, st.expanded.contains(&i)));
            Vec2::new(acc.x.max(p.x + NODE_W + 40.0), acc.y.max(p.y + h + 40.0))
        });

    let (resp, painter) = ui.allocate_painter(extent, egui::Sense::hover());
    let origin = resp.rect.min.to_vec2();
    let visuals = ui.visuals().clone();
    let text_color = visuals.text_color();
    let weak = visuals.weak_text_color();
    let line_color = visuals.hyperlink_color;

    // Edges first, so the boxes sit on top of the lines rather than under.
    for (ei, e) in map.edges.iter().enumerate() {
        let Some((a, b, mid, bent)) = edge_geometry(st, map, ei) else {
            continue;
        };
        // Map space to screen space; the export skips exactly this step.
        let (a, b, mid) = (a + origin, b + origin, mid + origin);

        // The chip is the curve's handle: it sits wherever it was dragged and
        // the connection bends to pass through it. A quadratic Bezier's
        // midpoint is (a + 2c + b) / 4, so to make the curve pass through the
        // chip at t=0.5 the control point is 2*chip - (a + b) / 2. With no
        // offset that lands back on the midpoint and the curve is a straight
        // line, so an untouched map looks exactly as it did.
        let straight_mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        if !bent {
            painter.line_segment([a, b], Stroke::new(1.5, line_color));
        } else {
            let control = Pos2::new(2.0 * mid.x - straight_mid.x, 2.0 * mid.y - straight_mid.y);
            painter.add(egui::epaint::QuadraticBezierShape::from_points_stroke(
                [a, control, b],
                false,
                egui::Color32::TRANSPARENT,
                Stroke::new(1.5, line_color),
            ));
        }

        // The chip is the edge's hit target too: a click on a hairline is not
        // something anyone should have to aim at.
        // A declared foreign key has no score to show until it is measured,
        // and a chip reading 0.00 would say the opposite of what is true.
        let label = if e.scored {
            format!("{:.2}", e.score)
        } else {
            t("relmap.chip_declared")
        };
        let chip = Rect::from_center_size(mid, Vec2::new(46.0, 18.0));
        painter.rect_filled(chip, 4.0, visuals.extreme_bg_color);
        painter.rect_stroke(
            chip,
            4.0,
            Stroke::new(1.0, line_color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            mid,
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(11.0),
            text_color,
        );

        // Click to prefill the Join dialog, drag to move the chip: egui's
        // click/drag split keeps both on one response.
        let hit = ui.interact(
            chip,
            ui.id().with(("relmap_edge", ei)),
            egui::Sense::click_and_drag(),
        );
        if hit.dragged()
            && let Some(o) = st.chip_offsets.get_mut(ei)
        {
            *o += hit.drag_delta();
        }
        let hit = hit.on_hover_text(edge_tooltip(map, e));
        if hit.clicked() {
            clicked_edge = Some(ei);
        }
    }

    // Boxes, and the dragging that moves them.
    for (i, node) in map.nodes.iter().enumerate() {
        let Some(pos) = st.positions.get(i).copied() else {
            continue;
        };
        let top_left = pos + origin;
        let expanded = st.expanded.contains(&i);
        let listed = listed_cols(node.columns.len(), expanded);
        let rect = Rect::from_min_size(
            top_left,
            Vec2::new(NODE_W, node_height(node.columns.len(), listed)),
        );
        painter.rect_filled(rect, 6.0, visuals.faint_bg_color);
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, visuals.widgets.noninteractive.fg_stroke.color),
            egui::StrokeKind::Inside,
        );
        painter.text(
            top_left + Vec2::new(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            &node.name,
            FontId::proportional(13.0),
            text_color,
        );
        for (ci, col) in node.columns.iter().take(listed).enumerate() {
            painter.text(
                top_left + Vec2::new(10.0, HEADER_H + ci as f32 * ROW_H),
                egui::Align2::LEFT_TOP,
                col,
                FontId::proportional(11.0),
                weak,
            );
        }
        // `+N` folded, `-N` expanded: a signed count needs no words, so it
        // reads the same in every language. The tooltip says it is clickable.
        let hidden = node.columns.len() - listed;
        let toggle_row = if expanded {
            (node.columns.len() > MAX_LISTED_COLS).then(|| {
                (
                    format!("-{}", node.columns.len() - MAX_LISTED_COLS),
                    node.columns.len(),
                )
            })
        } else {
            (hidden > 0).then(|| (format!("+{hidden}"), listed))
        };
        if let Some((label, row)) = &toggle_row {
            painter.text(
                top_left + Vec2::new(10.0, HEADER_H + *row as f32 * ROW_H),
                egui::Align2::LEFT_TOP,
                label,
                FontId::proportional(11.0),
                line_color,
            );
        }

        let drag = ui.interact(rect, ui.id().with(("relmap_node", i)), egui::Sense::drag());
        debug_assert_eq!(drag.interact_rect, rect);
        if drag.dragged()
            && let Some(p) = st.positions.get_mut(i)
        {
            *p += drag.drag_delta();
        }

        // Registered AFTER the box, so it wins the click; it senses clicks
        // only and the box senses drags only, so dragging the box by this
        // row still works. Same split the custom title bar relies on.
        if let Some((_, row)) = &toggle_row {
            let strip = Rect::from_min_size(
                top_left + Vec2::new(0.0, HEADER_H + *row as f32 * ROW_H),
                Vec2::new(NODE_W, ROW_H),
            );
            let hit = ui.interact(
                strip,
                ui.id().with(("relmap_cols", i)),
                egui::Sense::click(),
            );
            if hit.clicked() {
                if expanded {
                    st.expanded.remove(&i);
                } else {
                    st.expanded.insert(i);
                }
            }
            hit.on_hover_text(t("relmap.cols_toggle_hint"));
        }
        drag.on_hover_text(format!(
            "{}\n{}",
            t("relmap.node_tooltip")
                .replace("{name}", &node.name)
                .replace("{cols}", &node.columns.len().to_string())
                .replace("{rows}", &node.rows.to_string()),
            t("relmap.drag_hint")
        ));
    }

    clicked_edge
}

/// The sentence a line explains itself with, on hover and in the export's
/// SVG `<title>`. One wording, so a map read in a browser says what the map
/// in Octa said.
pub(super) fn edge_tooltip(map: &RelMap, e: &octa::data::rel_map::Relationship) -> String {
    let left = format!(
        "{}.{}",
        map.nodes[e.left_table].name, map.nodes[e.left_table].columns[e.left_col]
    );
    let right = format!(
        "{}.{}",
        map.nodes[e.right_table].name, map.nodes[e.right_table].columns[e.right_col]
    );
    if e.scored {
        // Both directions, one line each. Only one of them can break a tie
        // between two candidates that score the same, and which one depends on
        // which side is the child - something the map cannot know for a scan of
        // tabs or files. Showing one direction alone left `orders.id` and
        // `orders.customer_id` looking identical.
        let direction = |matched: usize, total: usize, orphans: usize, from: &str, to: &str| {
            t("relmap.edge_direction")
                .replace("{matched}", &matched.to_string())
                .replace("{total}", &total.to_string())
                .replace("{from}", from)
                .replace("{to}", to)
                .replace("{orphans}", &orphans.to_string())
        };
        format!(
            "{}\n{}\n\n{}",
            direction(
                e.matched(),
                e.left_distinct_values,
                e.left_orphans,
                &left,
                &right
            ),
            direction(
                e.right_matched(),
                e.right_distinct_values,
                e.right_orphans,
                &right,
                &left
            ),
            t("relmap.score_tooltip")
        )
    } else {
        t("relmap.edge_declared_tooltip")
            .replace("{name}", e.constraint.as_deref().unwrap_or("-"))
            .replace("{left}", &left)
            .replace("{right}", &right)
    }
}
