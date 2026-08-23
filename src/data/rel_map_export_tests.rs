//! The exporter draws what the dialog drew. These pin the parts that would
//! silently diverge: the curve's control point, the size of the picture, and
//! the fact that the HTML carries the offsets its script needs.

use super::*;

fn node(name: &str, cols: usize, listed: usize, x: f32, y: f32) -> NodeBox {
    NodeBox {
        name: name.to_string(),
        columns: (0..cols).map(|i| format!("col_{i}")).collect(),
        listed,
        x,
        y,
        rows: 0,
    }
}

fn layout(edges: Vec<EdgeLine>, nodes: Vec<NodeBox>) -> MapLayout {
    MapLayout {
        title: "Relationship map".to_string(),
        nodes,
        edges,
        colors: MapColors::default(),
        node_w: 190.0,
        header_h: 24.0,
        row_h: 16.0,
    }
}

fn edge(bent: bool, mx: f32, my: f32) -> EdgeLine {
    EdgeLine {
        left_table: 0,
        left_col: 1,
        right_table: 1,
        right_col: 0,
        ax: 100.0,
        ay: 100.0,
        bx: 300.0,
        by: 200.0,
        mx,
        my,
        bent,
        label: "FK".to_string(),
        tooltip: "orders.customer_id points at customers.id".to_string(),
    }
}

/// A map nobody bent must export as straight lines, not as curves that
/// happen to look straight.
#[test]
fn an_untouched_edge_is_a_line_segment() {
    let e = edge(false, 200.0, 150.0);
    assert_eq!(path_d(&e), "M 100.0 100.0 L 300.0 200.0");
}

/// The whole point of the control point: a quadratic's t=0.5 point is
/// (a + 2c + b) / 4, so the curve has to pass through the chip the user
/// dragged. If this drifts, an exported line stops matching the drawn one.
#[test]
fn a_bent_edge_passes_through_its_chip() {
    let e = edge(true, 250.0, 90.0);
    let d = path_d(&e);
    let nums: Vec<f32> = d
        .split_whitespace()
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    // M ax ay Q cx cy bx by
    let (cx, cy) = (nums[2], nums[3]);
    let midpoint_x = (e.ax + 2.0 * cx + e.bx) / 4.0;
    let midpoint_y = (e.ay + 2.0 * cy + e.by) / 4.0;
    assert!((midpoint_x - e.mx).abs() < 0.05, "x was {midpoint_x}");
    assert!((midpoint_y - e.my).abs() < 0.05, "y was {midpoint_y}");
}

/// An expanded box is taller, and the picture has to grow with it or the
/// bottom rows fall outside the viewBox and vanish from the PDF.
#[test]
fn expanding_a_box_grows_the_document() {
    let collapsed = layout(vec![], vec![node("t", 40, 12, 20.0, 20.0)]);
    let expanded = layout(vec![], vec![node("t", 40, 40, 20.0, 20.0)]);
    assert!(
        expanded.extent().1 > collapsed.extent().1 + 400.0,
        "28 more rows should add real height: {:?} vs {:?}",
        collapsed.extent(),
        expanded.extent()
    );
}

/// A chip dragged far past every box still has to be inside the picture.
#[test]
fn a_chip_dragged_outside_the_boxes_still_fits() {
    let l = layout(
        vec![edge(true, 1400.0, 900.0)],
        vec![node("a", 3, 3, 20.0, 20.0)],
    );
    let (w, h) = l.extent();
    assert!(w > 1400.0, "width {w} cuts the chip off");
    assert!(h > 900.0, "height {h} cuts the chip off");
}

/// The SVG has to be a complete document, carry the tooltip text, and give
/// every node and edge the ids the HTML script addresses them by.
#[test]
fn the_svg_is_self_contained_and_addressable() {
    let l = layout(
        vec![edge(false, 200.0, 150.0)],
        vec![
            node("sales.orders", 3, 3, 20.0, 20.0),
            node("x", 2, 2, 400.0, 20.0),
        ],
    );
    let svg = to_svg(&l);
    assert!(svg.starts_with("<svg "), "{}", &svg[..40.min(svg.len())]);
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("octa-node-0") && svg.contains("octa-node-1"));
    assert!(svg.contains("octa-edge-0") && svg.contains("octa-chip-0"));
    assert!(svg.contains("points at customers.id"), "tooltip is missing");
    assert!(svg.contains("sales.orders"));
    // Nothing may be fetched: the file has to render from disk.
    assert!(!svg.contains("http://") || !svg.contains("<image"));
}

/// `+N` is the map's own statement that a box is folded, so the export has
/// to carry it rather than silently showing a short list as if it were whole.
#[test]
fn a_folded_box_exports_its_plus_n_row() {
    let l = layout(vec![], vec![node("wide", 20, 12, 20.0, 20.0)]);
    assert!(to_svg(&l).contains(">+8<"), "the +8 row is missing");
}

/// Text that is XML in its own right must not break the document. Column
/// names come from a database, so `<` and `&` are entirely possible.
#[test]
fn names_are_escaped() {
    let mut n = node("a&b", 1, 1, 0.0, 0.0);
    n.columns[0] = "x<y".to_string();
    let svg = to_svg(&layout(vec![], vec![n]));
    assert!(svg.contains("a&amp;b"));
    assert!(svg.contains("x&lt;y"));
    assert!(!svg.contains("x<y"));
}

/// The script re-routes lines from the chip's *offset*, not its position:
/// once a box is dragged in the browser the midpoint moves and a bent line
/// has to follow it. Exporting the absolute point would straighten the
/// curve on the first drag.
#[test]
fn the_html_carries_chip_offsets_not_positions() {
    let l = layout(
        vec![edge(true, 250.0, 90.0)],
        vec![node("a", 3, 3, 20.0, 20.0), node("b", 3, 3, 400.0, 20.0)],
    );
    let json = geometry_json(&l);
    // straight midpoint is (200, 150), chip at (250, 90) -> offset (50, -60)
    assert!(json.contains(r#""dx":50.0"#), "{json}");
    assert!(json.contains(r#""dy":-60.0"#), "{json}");
    assert!(json.contains(r#""lt":0"#) && json.contains(r#""rt":1"#));
}

/// An `<svg>` clips to its own viewport. At its intrinsic size that meant a
/// box dragged past the drawing's bounds vanished, stopped being clickable,
/// and the next grab at that spot panned the whole picture instead: the two
/// symptoms were one bug. The embedded SVG must fill the window and carry no
/// `viewBox`, so one user unit stays one CSS pixel and a dragged box tracks
/// the pointer exactly.
#[test]
fn the_html_svg_fills_the_window_and_never_clips() {
    let l = layout(
        vec![edge(false, 200.0, 150.0)],
        vec![node("a", 3, 3, 20.0, 20.0), node("b", 3, 3, 400.0, 20.0)],
    );
    let html = to_html(&l);
    assert!(
        html.contains(r#"width="100%" height="100%""#),
        "svg does not fill the window"
    );
    assert!(
        !html.contains("viewBox"),
        "a viewBox would rescale and break the drag math"
    );
    assert!(
        html.contains(r#"<g id="octa-view">"#),
        "the pan/zoom stage is missing"
    );
    // Panning must move the drawing, not the <svg>, or the window stops
    // being the interactive surface.
    assert!(html.contains("stage.setAttribute('transform'"));
    // The toolbar must not steal a drag from a box parked underneath it.
    assert!(
        html.contains("pointer-events:none"),
        "the bar can still swallow drags"
    );
}

/// The standalone file is a document in its own right: a PDF page, a raster
/// and any program embedding it all need an intrinsic size and a viewBox.
/// Giving it the HTML treatment would produce a blank PDF.
#[test]
fn the_standalone_svg_keeps_its_intrinsic_size() {
    let l = layout(vec![], vec![node("a", 3, 3, 20.0, 20.0)]);
    let svg = to_svg(&l);
    assert!(
        svg.contains("viewBox=\"0 0 "),
        "no viewBox: {}",
        &svg[..90.min(svg.len())]
    );
    assert!(
        !svg.contains("100%"),
        "the standalone file must not size itself to a window"
    );
    assert!(
        !svg.contains("octa-view"),
        "the pan stage belongs to the HTML export only"
    );
}

/// The background sits outside the group that pans, or dragging the map
/// drags the backdrop away with it and exposes the page behind.
#[test]
fn the_background_does_not_pan_with_the_drawing() {
    let l = layout(vec![], vec![node("a", 3, 3, 20.0, 20.0)]);
    let html = to_html(&l);
    let bg = html.find("<rect width=\"100%\"").expect("background rect");
    let stage = html.find("<g id=\"octa-view\">").expect("stage");
    assert!(bg < stage, "the background is inside the panning group");
}

/// The HTML export must open from a file with no network and no library.
#[test]
fn the_html_is_one_self_contained_file() {
    let l = layout(
        vec![edge(false, 200.0, 150.0)],
        vec![node("a", 3, 3, 20.0, 20.0), node("b", 3, 3, 400.0, 20.0)],
    );
    let html = to_html(&l);
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<svg "), "the drawing itself is missing");
    assert!(
        !html.contains("<script src"),
        "no external script may be pulled"
    );
    assert!(!html.contains("cdn."), "nothing may be fetched");
    // The interactive part the user asked for.
    assert!(html.contains("wheel") && html.contains("pointermove"));
}

/// The SVG has to be something `usvg` will actually parse. A malformed
/// attribute would still pass every string assertion above and only fail
/// when a user picks PDF, which is the default format.
#[test]
fn the_default_format_really_renders() {
    let l = layout(
        vec![edge(true, 250.0, 90.0)],
        vec![
            node("sales.orders", 15, 12, 20.0, 20.0),
            node("sales.customers", 4, 4, 400.0, 20.0),
        ],
    );
    let pdf = render(&l, RelMapExportFormat::Pdf).expect("pdf");
    assert!(
        pdf.starts_with(b"%PDF"),
        "not a PDF: {:?}",
        &pdf[..8.min(pdf.len())]
    );

    let png = render(&l, RelMapExportFormat::Png).expect("png");
    assert!(png.starts_with(b"\x89PNG"), "not a PNG");

    let svg = render(&l, RelMapExportFormat::Svg).expect("svg");
    assert!(svg.starts_with(b"<svg "));

    let html = render(&l, RelMapExportFormat::Html).expect("html");
    assert!(html.starts_with(b"<!doctype html>"));
}

/// Every format has to write a different file, and the picker offers all of
/// them, so a missing arm in `render` would be a silent wrong export.
#[test]
fn every_offered_format_writes_something() {
    let l = layout(vec![], vec![node("t", 3, 3, 10.0, 10.0)]);
    for f in RelMapExportFormat::ALL {
        let bytes = render(&l, f).unwrap_or_else(|e| panic!("{} failed: {e}", f.label()));
        assert!(!bytes.is_empty(), "{} wrote nothing", f.label());
        assert!(!f.extension().is_empty());
    }
}
