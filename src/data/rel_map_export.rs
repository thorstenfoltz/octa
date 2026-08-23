//! Export a relationship map exactly as it stands on screen.
//!
//! Like [`super::chart_export`], every format goes through one hand-emitted
//! **SVG**: PNG and PDF are that SVG converted by the writers already in
//! `chart_export`, and HTML is that SVG plus a little JavaScript. We never
//! screenshot the egui painter, so the export is resolution-independent and
//! does not vary with the window size or the display's DPI.
//!
//! **This module computes no geometry.** The dialog draws the map from
//! `anchor` / `node_height` / the chip offsets and hands the finished
//! [`MapLayout`] here, so an exported line cannot land somewhere the drawn
//! line did not. Everything that decides where something sits, including
//! which boxes the user expanded and which lines they bent, is already
//! resolved by the time this module sees it.

use std::fmt::Write;

use super::chart_export::escape_xml;

/// What an export is written as. PDF by default: the map is a diagram to put
/// in a document or print, and the vector formats keep it readable at any
/// size. The choice is remembered in `AppSettings`, so "always SVG" is a
/// setting rather than a click every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum RelMapExportFormat {
    #[default]
    Pdf,
    Png,
    Svg,
    /// The SVG in a page that still pans, zooms and lets boxes be dragged.
    Html,
}

impl RelMapExportFormat {
    pub const ALL: [RelMapExportFormat; 4] = [
        RelMapExportFormat::Pdf,
        RelMapExportFormat::Png,
        RelMapExportFormat::Svg,
        RelMapExportFormat::Html,
    ];

    /// Also the label: these are file formats, the same word everywhere, so
    /// they are deliberately not translated.
    pub fn extension(self) -> &'static str {
        match self {
            RelMapExportFormat::Pdf => "pdf",
            RelMapExportFormat::Png => "png",
            RelMapExportFormat::Svg => "svg",
            RelMapExportFormat::Html => "html",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RelMapExportFormat::Pdf => "PDF",
            RelMapExportFormat::Png => "PNG",
            RelMapExportFormat::Svg => "SVG",
            RelMapExportFormat::Html => "HTML",
        }
    }
}

/// Render a layout to the bytes of one file.
///
/// PNG and PDF go through the writers `chart_export` already owns, so the
/// two exporters cannot disagree about how an SVG becomes a raster or a page.
pub fn render(layout: &MapLayout, format: RelMapExportFormat) -> Result<Vec<u8>, String> {
    let svg = to_svg(layout);
    match format {
        // 2x, matching the chart exporter, so a PNG dropped into a document
        // is not soft on a high-DPI screen.
        RelMapExportFormat::Png => super::chart_export::to_png(&svg, 2.0),
        RelMapExportFormat::Pdf => super::chart_export::to_pdf(&svg),
        RelMapExportFormat::Svg => Ok(svg.into_bytes()),
        RelMapExportFormat::Html => Ok(to_html(layout).into_bytes()),
    }
}

/// Every colour the map draws with, RGBA, taken from the live theme so an
/// export matches the screen it was taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapColors {
    pub background: [u8; 4],
    pub node_fill: [u8; 4],
    pub node_stroke: [u8; 4],
    pub text: [u8; 4],
    pub weak_text: [u8; 4],
    pub line: [u8; 4],
    pub chip_fill: [u8; 4],
}

impl Default for MapColors {
    /// The light theme, so a test or a headless caller gets a readable
    /// picture without building an egui context.
    fn default() -> Self {
        Self {
            background: [255, 255, 255, 255],
            node_fill: [246, 246, 248, 255],
            node_stroke: [150, 150, 155, 255],
            text: [30, 30, 34, 255],
            weak_text: [110, 110, 118, 255],
            line: [40, 110, 200, 255],
            chip_fill: [255, 255, 255, 255],
        }
    }
}

/// One table, at the position it was dragged to.
#[derive(Debug, Clone)]
pub struct NodeBox {
    pub name: String,
    /// Every column of the table, in catalog order.
    pub columns: Vec<String>,
    /// How many of them the box lists. Less than `columns.len()` when the
    /// user has not expanded it, which the `+N` row states.
    pub listed: usize,
    pub x: f32,
    pub y: f32,
    /// Rows loaded, for the tooltip. 0 when nothing was read.
    pub rows: usize,
}

/// One line, with both ends already resolved to a point.
#[derive(Debug, Clone)]
pub struct EdgeLine {
    /// Node indices, so the HTML export can re-route the line when a box is
    /// dragged in the browser.
    pub left_table: usize,
    pub left_col: usize,
    pub right_table: usize,
    pub right_col: usize,
    pub ax: f32,
    pub ay: f32,
    pub bx: f32,
    pub by: f32,
    /// Centre of the score chip, which is also the curve's control handle.
    pub mx: f32,
    pub my: f32,
    /// The chip was dragged off the straight midpoint, so the line curves.
    pub bent: bool,
    /// What the chip reads: a score, or `FK` for an unmeasured declared key.
    pub label: String,
    /// The sentence the on-screen tooltip shows.
    pub tooltip: String,
}

/// A whole map, ready to render.
#[derive(Debug, Clone)]
pub struct MapLayout {
    pub title: String,
    pub nodes: Vec<NodeBox>,
    pub edges: Vec<EdgeLine>,
    pub colors: MapColors,
    /// Box geometry, passed in rather than duplicated, so the SVG and the
    /// JavaScript place a row exactly where the dialog placed it.
    pub node_w: f32,
    pub header_h: f32,
    pub row_h: f32,
}

impl MapLayout {
    /// Height of one box, the same arithmetic the dialog's `node_height` does.
    fn node_height(&self, n: &NodeBox) -> f32 {
        let extra = if n.listed < n.columns.len() { 1.0 } else { 0.0 };
        self.header_h + (n.listed as f32 + extra) * self.row_h + 6.0
    }

    /// The drawing's own size, with a margin, so nothing sits on the edge.
    /// Chips can be dragged outside the boxes' bounding box, so they count.
    fn extent(&self) -> (f32, f32) {
        let mut w: f32 = 320.0;
        let mut h: f32 = 200.0;
        for n in &self.nodes {
            w = w.max(n.x + self.node_w);
            h = h.max(n.y + self.node_height(n));
        }
        for e in &self.edges {
            w = w.max(e.mx + CHIP_W / 2.0);
            h = h.max(e.my + CHIP_H / 2.0);
        }
        (w + MARGIN, h + MARGIN)
    }
}

const CHIP_W: f32 = 46.0;
const CHIP_H: f32 = 18.0;
const MARGIN: f32 = 24.0;

fn rgba(c: [u8; 4]) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        c[0],
        c[1],
        c[2],
        c[3] as f32 / 255.0
    )
}

/// The `d` of one edge. A straight line when the chip was never dragged, so
/// an untouched map exports as the straight lines it showed.
fn path_d(e: &EdgeLine) -> String {
    if e.bent {
        // Same control point as the dialog: a quadratic's t=0.5 point is
        // (a + 2c + b) / 4, so c = 2*mid - (a + b) / 2 puts the curve through
        // the chip.
        let cx = 2.0 * e.mx - (e.ax + e.bx) / 2.0;
        let cy = 2.0 * e.my - (e.ay + e.by) / 2.0;
        format!(
            "M {:.1} {:.1} Q {:.1} {:.1} {:.1} {:.1}",
            e.ax, e.ay, cx, cy, e.bx, e.by
        )
    } else {
        format!("M {:.1} {:.1} L {:.1} {:.1}", e.ax, e.ay, e.bx, e.by)
    }
}

/// How the SVG is framed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SvgMode {
    /// A document of its own: intrinsic size and a `viewBox`, which is what
    /// a PDF page, a raster or another program embedding the file needs.
    Standalone,
    /// Inside the HTML page: fills the window, **no `viewBox`**, and the
    /// drawing lives in a group that pans and zooms.
    ///
    /// An `<svg>` clips to its own viewport, so at its intrinsic size a box
    /// dragged past the drawing's bounds disappears and stops being
    /// clickable, and the next grab at that spot pans the whole picture
    /// instead. Filling the window is what stops both. No `viewBox` keeps one
    /// user unit equal to one CSS pixel, so a dragged box tracks the pointer
    /// exactly.
    Embedded,
}

/// The map as a standalone SVG document.
pub fn to_svg(layout: &MapLayout) -> String {
    svg_document(layout, SvgMode::Standalone)
}

fn svg_document(layout: &MapLayout, mode: SvgMode) -> String {
    let (w, h) = layout.extent();
    let mut out = String::new();
    let font = "DejaVu Sans, Helvetica, Arial, sans-serif";
    match mode {
        SvgMode::Standalone => {
            let _ = write!(
                out,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" font-family="{font}">"#
            );
        }
        SvgMode::Embedded => {
            let _ = write!(
                out,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%" font-family="{font}">"#
            );
        }
    }
    let _ = write!(out, "<title>{}</title>", escape_xml(&layout.title));
    // The background sits outside the panning group, or panning would drag
    // it away and expose the page behind the drawing.
    let size = match mode {
        SvgMode::Standalone => format!(r#"width="{w:.0}" height="{h:.0}""#),
        SvgMode::Embedded => r#"width="100%" height="100%""#.to_string(),
    };
    let _ = write!(
        out,
        r#"<rect {size} fill="{}"/>"#,
        rgba(layout.colors.background)
    );
    if mode == SvgMode::Embedded {
        out.push_str(r#"<g id="octa-view">"#);
    }
    emit_edges(&mut out, layout);
    emit_nodes(&mut out, layout);
    if mode == SvgMode::Embedded {
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    out
}

/// Lines and chips first, so the boxes sit on top of them exactly as they do
/// on screen.
fn emit_edges(out: &mut String, layout: &MapLayout) {
    let line = rgba(layout.colors.line);
    let _ = write!(out, r#"<g id="octa-edges">"#);
    for (i, e) in layout.edges.iter().enumerate() {
        let _ = write!(
            out,
            r#"<path id="octa-edge-{i}" d="{}" fill="none" stroke="{line}" stroke-width="1.5"/>"#,
            path_d(e)
        );
        let _ = write!(
            out,
            r#"<g id="octa-chip-{i}"><title>{}</title><rect x="{:.1}" y="{:.1}" width="{CHIP_W}" height="{CHIP_H}" rx="4" fill="{}" stroke="{line}" stroke-width="1"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="{}">{}</text></g>"#,
            escape_xml(&e.tooltip),
            e.mx - CHIP_W / 2.0,
            e.my - CHIP_H / 2.0,
            rgba(layout.colors.chip_fill),
            e.mx,
            // +4 puts an 11px glyph's baseline on the chip's centre line.
            e.my + 4.0,
            rgba(layout.colors.text),
            escape_xml(&e.label)
        );
    }
    out.push_str("</g>");
}

fn emit_nodes(out: &mut String, layout: &MapLayout) {
    let _ = write!(out, r#"<g id="octa-nodes">"#);
    for (i, n) in layout.nodes.iter().enumerate() {
        let h = layout.node_height(n);
        let _ = write!(
            out,
            r#"<g id="octa-node-{i}" transform="translate({:.1},{:.1})"><title>{}: {} columns</title>"#,
            n.x,
            n.y,
            escape_xml(&n.name),
            n.columns.len()
        );
        let _ = write!(
            out,
            r#"<rect width="{:.1}" height="{h:.1}" rx="6" fill="{}" stroke="{}" stroke-width="1"/>"#,
            layout.node_w,
            rgba(layout.colors.node_fill),
            rgba(layout.colors.node_stroke)
        );
        let _ = write!(
            out,
            r#"<text x="8" y="17" font-size="13" fill="{}">{}</text>"#,
            rgba(layout.colors.text),
            escape_xml(&n.name)
        );
        let weak = rgba(layout.colors.weak_text);
        for (ci, col) in n.columns.iter().take(n.listed).enumerate() {
            let _ = write!(
                out,
                r#"<text x="10" y="{:.1}" font-size="11" fill="{weak}">{}</text>"#,
                layout.header_h + ci as f32 * layout.row_h + 9.0,
                escape_xml(col)
            );
        }
        if n.listed < n.columns.len() {
            let _ = write!(
                out,
                r#"<text x="10" y="{:.1}" font-size="11" fill="{weak}">+{}</text>"#,
                layout.header_h + n.listed as f32 * layout.row_h + 9.0,
                n.columns.len() - n.listed
            );
        }
        out.push_str("</g>");
    }
    out.push_str("</g>");
}

/// The map as a self-contained HTML page: the same SVG, plus panning,
/// zooming and draggable boxes.
///
/// Nothing is fetched and no library is loaded, so the file opens from disk
/// or from an email attachment. Dragging re-runs the dialog's own anchor
/// arithmetic in JavaScript, which is why [`EdgeLine`] carries the table and
/// column indices rather than points alone.
pub fn to_html(layout: &MapLayout) -> String {
    let svg = svg_document(layout, SvgMode::Embedded);
    let geom = geometry_json(layout);
    let title = escape_xml(&layout.title);
    let (chip_w, chip_h) = (CHIP_W, CHIP_H);
    let bg = rgba(layout.colors.background);
    let fg = rgba(layout.colors.text);
    format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  html,body {{ margin:0; height:100%; background:{bg}; color:{fg};
    font-family: system-ui, sans-serif; }}
  /* touch-action:none so a drag on a touchpad or a tablet moves the map
     instead of scrolling the page out from under it. */
  #wrap {{ position:fixed; inset:0; overflow:hidden; cursor:grab;
    touch-action:none; }}
  #wrap.panning {{ cursor:grabbing; }}
  svg {{ position:absolute; inset:0; width:100%; height:100%;
    display:block; }}
  /* pointer-events:none so the bar cannot steal a drag from a box that
     happens to sit under it; the button opts back in. */
  #bar {{ position:fixed; top:8px; left:8px; z-index:2; font-size:13px;
    background:{bg}; border:1px solid {fg}; border-radius:6px;
    padding:4px 8px; opacity:.9; pointer-events:none; }}
  #bar button {{ font:inherit; margin-left:6px; pointer-events:auto; }}
  [id^="octa-node-"] {{ cursor:move; }}
</style></head><body>
<div id="bar">{title} <button id="reset">Reset view</button></div>
<div id="wrap">{svg}</div>
<script>
const G = {geom};
const wrap = document.getElementById('wrap');
// The drawing lives in this group. Panning and zooming move the group, never
// the <svg>, which stays the size of the window so nothing is ever clipped
// out of reach.
const stage = document.getElementById('octa-view');
let cam = {{ x: 0, y: 0, k: 1 }};
function apply() {{
  stage.setAttribute('transform',
    `translate(${{cam.x}},${{cam.y}}) scale(${{cam.k}})`);
}}
document.getElementById('reset').onclick = () => {{
  cam = {{ x: 0, y: 0, k: 1 }}; apply();
}};
// Wheel zooms about the pointer, so the thing under the cursor stays put.
wrap.addEventListener('wheel', ev => {{
  ev.preventDefault();
  const k = cam.k * (ev.deltaY < 0 ? 1.1 : 1 / 1.1);
  const next = Math.min(8, Math.max(0.1, k));
  const r = wrap.getBoundingClientRect();
  const px = ev.clientX - r.left, py = ev.clientY - r.top;
  cam.x = px - (px - cam.x) * (next / cam.k);
  cam.y = py - (py - cam.y) * (next / cam.k);
  cam.k = next; apply();
}}, {{ passive: false }});

// One box's height, the same arithmetic the exporter used.
function nodeHeight(n) {{
  const extra = n.listed < n.columns ? 1 : 0;
  return G.header_h + (n.listed + extra) * G.row_h + 6;
}}
// Where a column's line attaches, mirroring the dialog's `anchor`.
function anchor(n, col, rightSide) {{
  const row = Math.min(col, Math.max(n.listed - 1, 0));
  return {{
    x: n.x + (rightSide ? G.node_w : 0),
    y: n.y + G.header_h + row * G.row_h + G.row_h / 2
  }};
}}
function reroute(ei) {{
  const e = G.edges[ei];
  const l = G.nodes[e.lt], r = G.nodes[e.rt];
  const leftFirst = l.x <= r.x;
  const a = anchor(l, e.lc, leftFirst);
  const b = anchor(r, e.rc, !leftFirst);
  // The chip keeps the offset it was exported with, so a bent line stays bent.
  const mx = (a.x + b.x) / 2 + e.dx, my = (a.y + b.y) / 2 + e.dy;
  const path = document.getElementById('octa-edge-' + ei);
  if (e.bent) {{
    const cx = 2 * mx - (a.x + b.x) / 2, cy = 2 * my - (a.y + b.y) / 2;
    path.setAttribute('d', `M ${{a.x}} ${{a.y}} Q ${{cx}} ${{cy}} ${{b.x}} ${{b.y}}`);
  }} else {{
    path.setAttribute('d', `M ${{a.x}} ${{a.y}} L ${{b.x}} ${{b.y}}`);
  }}
  const chip = document.getElementById('octa-chip-' + ei);
  const rect = chip.querySelector('rect'), text = chip.querySelector('text');
  rect.setAttribute('x', mx - {chip_w} / 2);
  rect.setAttribute('y', my - {chip_h} / 2);
  text.setAttribute('x', mx);
  text.setAttribute('y', my + 4);
}}

let drag = null;
wrap.addEventListener('pointerdown', ev => {{
  const g = ev.target.closest('[id^="octa-node-"]');
  if (g) {{
    drag = {{ node: +g.id.split('-').pop(), x: ev.clientX, y: ev.clientY }};
  }} else {{
    drag = {{ node: -1, x: ev.clientX, y: ev.clientY }};
    wrap.classList.add('panning');
  }}
  wrap.setPointerCapture(ev.pointerId);
}});
wrap.addEventListener('pointermove', ev => {{
  if (!drag) return;
  const dx = ev.clientX - drag.x, dy = ev.clientY - drag.y;
  drag.x = ev.clientX; drag.y = ev.clientY;
  if (drag.node < 0) {{ cam.x += dx; cam.y += dy; apply(); return; }}
  const n = G.nodes[drag.node];
  // Divide by the zoom, or a dragged box outruns the pointer when zoomed in.
  n.x += dx / cam.k; n.y += dy / cam.k;
  document.getElementById('octa-node-' + drag.node)
    .setAttribute('transform', `translate(${{n.x}},${{n.y}})`);
  G.edges.forEach((e, i) => {{
    if (e.lt === drag.node || e.rt === drag.node) reroute(i);
  }});
}});
wrap.addEventListener('pointerup', () => {{ drag = null; wrap.classList.remove('panning'); }});
apply();
</script></body></html>
"##
    )
}

/// The layout as JSON for the HTML export's JavaScript. Column *names* are
/// left out: the SVG already carries them, and the script only needs where
/// things are.
fn geometry_json(layout: &MapLayout) -> String {
    let mut nodes = String::new();
    for (i, n) in layout.nodes.iter().enumerate() {
        if i > 0 {
            nodes.push(',');
        }
        let _ = write!(
            nodes,
            r#"{{"x":{:.1},"y":{:.1},"listed":{},"columns":{}}}"#,
            n.x,
            n.y,
            n.listed,
            n.columns.len()
        );
    }
    let mut edges = String::new();
    for (i, e) in layout.edges.iter().enumerate() {
        if i > 0 {
            edges.push(',');
        }
        // The chip's offset from the straight midpoint, not its position:
        // once a box moves, the midpoint moves and the bend must follow it.
        let dx = e.mx - (e.ax + e.bx) / 2.0;
        let dy = e.my - (e.ay + e.by) / 2.0;
        let _ = write!(
            edges,
            r#"{{"lt":{},"lc":{},"rt":{},"rc":{},"dx":{:.1},"dy":{:.1},"bent":{}}}"#,
            e.left_table, e.left_col, e.right_table, e.right_col, dx, dy, e.bent
        );
    }
    format!(
        r#"{{"node_w":{},"header_h":{},"row_h":{},"nodes":[{nodes}],"edges":[{edges}]}}"#,
        layout.node_w, layout.header_h, layout.row_h
    )
}

#[cfg(test)]
#[path = "rel_map_export_tests.rs"]
mod tests;
