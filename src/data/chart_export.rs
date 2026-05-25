//! Chart export (PNG / SVG / PDF).
//!
//! All three formats go through one hand-emitted **SVG** representation —
//! we never screenshot the egui_plot widget. That keeps the export
//! resolution-independent and reproducible (no DPI / window-size
//! variance) and lets the same source feed all three writers:
//!
//! - **SVG**: the emitted string is the artefact.
//! - **PNG**: SVG → `resvg::usvg::Tree` → `resvg::render` → `tiny_skia::Pixmap`
//!   → PNG bytes via the `png` encoder shipped with `resvg::tiny_skia`.
//! - **PDF**: SVG → `svg2pdf::usvg::Tree` → `svg2pdf::to_pdf` → PDF bytes.
//!
//! The chart itself is laid out in a fixed 800×500 SVG viewport; PNG
//! callers can up-scale by passing a higher `scale` factor.

use std::fmt::Write;

use super::chart::{
    ChartData, ChartPrep, LegendPosition, SeriesStyle, XAxisKind, format_days_as_date,
    format_seconds_as_datetime,
};

/// One series-display tweak the renderer made: legend name + RGBA color.
/// Kept parallel to the chart data so the exporter doesn't have to re-derive
/// names / colors from the live config.
#[derive(Debug, Clone)]
pub struct ResolvedSeries {
    pub display_name: String,
    /// `None` → fall back to the palette color at this slot.
    pub color: Option<[u8; 4]>,
}

/// Inputs to the exporter beyond the prepped chart data. Title + axis
/// overrides are the same the on-screen renderer uses, threaded through
/// so the export visually matches what the user sees.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub legend: LegendPosition,
    pub series: Vec<ResolvedSeries>,
}

impl ExportOptions {
    /// Helper for the binary side: build options from a `ChartPrep` + the
    /// live `ChartConfig.series_styles` map. The renderer call site has
    /// both, so this keeps the export call short.
    pub fn from_prep(
        prep: &ChartPrep,
        title: impl Into<String>,
        x_label_override: &str,
        y_label_override: &str,
        legend: LegendPosition,
        styles: impl Fn(usize) -> SeriesStyle,
    ) -> Self {
        let resolve_axis = |over: &str, fallback: &str| {
            if over.is_empty() {
                fallback.to_string()
            } else {
                over.to_string()
            }
        };
        let series = series_names(&prep.data)
            .into_iter()
            .enumerate()
            .map(|(idx, default_name)| {
                let style = styles(idx);
                let display_name = if style.display_name.is_empty() {
                    default_name
                } else {
                    style.display_name
                };
                ResolvedSeries {
                    display_name,
                    color: style.color,
                }
            })
            .collect();
        Self {
            title: title.into(),
            x_label: resolve_axis(x_label_override, &prep.x_label),
            y_label: resolve_axis(y_label_override, &prep.y_label),
            legend,
            series,
        }
    }
}

/// Pull a default name per series from the prepped data — used as the
/// fallback when no per-series rename was set.
pub fn series_names(data: &ChartData) -> Vec<String> {
    match data {
        ChartData::Histogram { .. } => vec!["Count".to_string()],
        ChartData::Bars { series, .. } => series.iter().map(|s| s.name.clone()).collect(),
        ChartData::Lines { series, .. } | ChartData::Scatter { series, .. } => {
            series.iter().map(|s| s.name.clone()).collect()
        }
        ChartData::Boxes(b) => b.iter().map(|b| b.name.clone()).collect(),
    }
}

const SVG_W: f64 = 800.0;
const SVG_H: f64 = 500.0;
const PAD_TOP: f64 = 56.0;
const PAD_BOTTOM: f64 = 70.0;
const PAD_LEFT: f64 = 80.0;
const PAD_RIGHT: f64 = 40.0;

/// Default color palette — one per series. Mirrors egui_plot's
/// default auto-color cycle closely enough that exports look the
/// same as the on-screen plot.
const PALETTE: &[[u8; 4]] = &[
    [0x4c, 0x72, 0xb0, 0xff],
    [0xdd, 0x85, 0x52, 0xff],
    [0x55, 0xa8, 0x68, 0xff],
    [0xc4, 0x4e, 0x52, 0xff],
    [0x81, 0x72, 0xb2, 0xff],
    [0x93, 0x70, 0x47, 0xff],
    [0xda, 0x8b, 0xc3, 0xff],
    [0x8c, 0x8c, 0x8c, 0xff],
];

fn series_color(idx: usize, override_color: Option<[u8; 4]>) -> [u8; 4] {
    override_color.unwrap_or(PALETTE[idx % PALETTE.len()])
}

fn rgba_css(c: [u8; 4]) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        c[0],
        c[1],
        c[2],
        c[3] as f64 / 255.0
    )
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Emit the chart as an SVG document.
pub fn to_svg(prep: &ChartPrep, opts: &ExportOptions) -> String {
    let mut out = String::with_capacity(8 * 1024);
    let _ = writeln!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"##,
        w = SVG_W,
        h = SVG_H,
    );
    let _ = writeln!(
        out,
        r##"<rect width="{}" height="{}" fill="#ffffff" />"##,
        SVG_W, SVG_H
    );

    // Title.
    if !opts.title.is_empty() {
        let _ = writeln!(
            out,
            r##"<text x="{x:.1}" y="28" font-family="sans-serif" font-size="18" font-weight="600" text-anchor="middle" fill="#222">{title}</text>"##,
            x = SVG_W / 2.0,
            title = escape_xml(&opts.title),
        );
    }

    // Axis labels.
    let _ = writeln!(
        out,
        r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="12" text-anchor="middle" fill="#555">{label}</text>"##,
        x = (PAD_LEFT + SVG_W - PAD_RIGHT) / 2.0,
        y = SVG_H - 20.0,
        label = escape_xml(&opts.x_label),
    );
    let _ = writeln!(
        out,
        r##"<text transform="translate(22 {y:.1}) rotate(-90)" font-family="sans-serif" font-size="12" text-anchor="middle" fill="#555">{label}</text>"##,
        y = (PAD_TOP + SVG_H - PAD_BOTTOM) / 2.0,
        label = escape_xml(&opts.y_label),
    );

    // Plot box.
    let _ = writeln!(
        out,
        r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="#fafafa" stroke="#888" stroke-width="1" />"##,
        x = PAD_LEFT,
        y = PAD_TOP,
        w = SVG_W - PAD_LEFT - PAD_RIGHT,
        h = SVG_H - PAD_TOP - PAD_BOTTOM,
    );

    let x_kind = prep.x_axis_kind;
    match &prep.data {
        ChartData::Histogram { bins, bin_width } => {
            emit_histogram(&mut out, bins, *bin_width, opts, x_kind)
        }
        ChartData::Bars { categories, series } => emit_bars(&mut out, categories, series, opts),
        ChartData::Lines { series, categories } => {
            emit_lines(&mut out, series, categories.as_deref(), opts, false, x_kind);
        }
        ChartData::Scatter { series, categories } => {
            emit_lines(&mut out, series, categories.as_deref(), opts, true, x_kind);
        }
        ChartData::Boxes(b) => emit_boxes(&mut out, b, opts),
    }

    emit_legend(&mut out, opts);

    out.push_str("</svg>\n");
    out
}

/// Returns `(min, max)` of an iterable of f64, treating an empty iterator
/// as (0.0, 1.0) so plots still draw their axes.
fn extent(iter: impl IntoIterator<Item = f64>) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for v in iter {
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if (max - min).abs() < f64::EPSILON {
        return (min - 0.5, max + 0.5);
    }
    (min, max)
}

fn plot_rect() -> (f64, f64, f64, f64) {
    (
        PAD_LEFT,
        PAD_TOP,
        SVG_W - PAD_LEFT - PAD_RIGHT,
        SVG_H - PAD_TOP - PAD_BOTTOM,
    )
}

fn project(x: f64, y: f64, xr: (f64, f64), yr: (f64, f64)) -> (f64, f64) {
    let (px, py, pw, ph) = plot_rect();
    let nx = (x - xr.0) / (xr.1 - xr.0);
    let ny = (y - yr.0) / (yr.1 - yr.0);
    (px + nx * pw, py + ph - ny * ph)
}

fn emit_histogram(
    out: &mut String,
    bins: &[(f64, f64)],
    bin_width: f64,
    opts: &ExportOptions,
    x_axis_kind: XAxisKind,
) {
    let xr = if let (Some(first), Some(last)) = (bins.first(), bins.last()) {
        (first.0, last.0 + bin_width)
    } else {
        (0.0, 1.0)
    };
    let yr = (0.0, extent(bins.iter().map(|b| b.1)).1.max(1.0));
    let color = rgba_css(series_color(0, opts.series.first().and_then(|s| s.color)));
    for (left, count) in bins {
        let (x0, y0) = project(*left, *count, xr, yr);
        let (x1, y1) = project(left + bin_width, 0.0, xr, yr);
        let _ = writeln!(
            out,
            r##"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{color}" stroke="#fff" stroke-width="1" />"##,
            x = x0,
            y = y0,
            w = (x1 - x0).abs(),
            h = (y1 - y0).abs(),
            color = color,
        );
    }
    emit_x_axis_ticks(out, xr, x_axis_kind);
    emit_y_axis_ticks(out, yr);
}

fn emit_bars(
    out: &mut String,
    categories: &[String],
    series: &[super::chart::ChartSeries],
    opts: &ExportOptions,
) {
    if categories.is_empty() {
        return;
    }
    let xr = (-0.5, categories.len() as f64 - 0.5);
    let max_y = series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p[1]))
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let yr = (0.0_f64.min(0.0), max_y);
    let s_count = series.len().max(1) as f64;
    let group_w = 0.8;
    let bar_w = group_w / s_count;
    for (si, ser) in series.iter().enumerate() {
        let color = rgba_css(series_color(si, opts.series.get(si).and_then(|s| s.color)));
        for p in &ser.points {
            let centre = p[0];
            let offset = if s_count <= 1.0 {
                0.0
            } else {
                -group_w / 2.0 + bar_w / 2.0 + si as f64 * bar_w
            };
            let (x0, y0) = project(centre + offset - bar_w / 2.0, p[1], xr, yr);
            let (x1, y1) = project(centre + offset + bar_w / 2.0, 0.0, xr, yr);
            let _ = writeln!(
                out,
                r##"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{color}" stroke="#fff" stroke-width="0.5" />"##,
                x = x0.min(x1),
                y = y0.min(y1),
                w = (x1 - x0).abs(),
                h = (y1 - y0).abs(),
                color = color,
            );
        }
    }
    // Category labels along X.
    let (px, _py, pw, ph) = plot_rect();
    for (i, name) in categories.iter().enumerate() {
        let x = px + (i as f64 + 0.5) * pw / categories.len() as f64;
        let _ = writeln!(
            out,
            r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="10" text-anchor="middle" fill="#444">{label}</text>"##,
            x = x,
            y = PAD_TOP + ph + 16.0,
            label = escape_xml(name),
        );
    }
    emit_y_axis_ticks(out, yr);
}

fn emit_lines(
    out: &mut String,
    series: &[super::chart::ChartSeries],
    categories: Option<&[String]>,
    opts: &ExportOptions,
    with_points: bool,
    x_axis_kind: XAxisKind,
) {
    let xs = series.iter().flat_map(|s| s.points.iter().map(|p| p[0]));
    let ys = series.iter().flat_map(|s| s.points.iter().map(|p| p[1]));
    let xr = extent(xs);
    let yr = extent(ys);
    for (si, ser) in series.iter().enumerate() {
        let color = rgba_css(series_color(si, opts.series.get(si).and_then(|s| s.color)));
        if ser.points.is_empty() {
            continue;
        }
        if !with_points {
            let mut d = String::new();
            for (i, p) in ser.points.iter().enumerate() {
                let (x, y) = project(p[0], p[1], xr, yr);
                let _ = write!(
                    &mut d,
                    "{}{:.2},{:.2} ",
                    if i == 0 { "M" } else { "L" },
                    x,
                    y
                );
            }
            let _ = writeln!(
                out,
                r##"<path d="{d}" fill="none" stroke="{color}" stroke-width="1.6" />"##,
            );
        }
        for p in &ser.points {
            let (x, y) = project(p[0], p[1], xr, yr);
            let _ = writeln!(
                out,
                r##"<circle cx="{x:.2}" cy="{y:.2}" r="2.4" fill="{color}" />"##,
            );
        }
    }
    if let Some(cats) = categories {
        emit_x_axis_category_ticks(out, cats);
        emit_y_axis_ticks(out, yr);
    } else {
        emit_x_axis_ticks(out, xr, x_axis_kind);
        emit_y_axis_ticks(out, yr);
    }
}

/// Tick row for a categorical X axis (Line / Scatter / Bar). The renderer
/// already emits its own bar / category labels for Bar; this one is reused
/// by Line / Scatter when X is categorical and the bar emitter isn't in
/// play.
fn emit_x_axis_category_ticks(out: &mut String, categories: &[String]) {
    if categories.is_empty() {
        return;
    }
    let (_px, _py, pw, ph) = plot_rect();
    for (i, name) in categories.iter().enumerate() {
        let x = PAD_LEFT + (i as f64 + 0.5) * pw / categories.len() as f64;
        let _ = writeln!(
            out,
            r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="10" text-anchor="middle" fill="#444">{label}</text>"##,
            x = x,
            y = PAD_TOP + ph + 16.0,
            label = escape_xml(name),
        );
    }
}

fn emit_boxes(out: &mut String, boxes: &[super::chart::BoxSummary], opts: &ExportOptions) {
    if boxes.is_empty() {
        return;
    }
    let xr = (-0.5, boxes.len() as f64 - 0.5);
    let yr = extent(
        boxes
            .iter()
            .flat_map(|b| [b.lower_whisker, b.q1, b.median, b.q3, b.upper_whisker]),
    );
    let box_w = 0.4;
    for (i, b) in boxes.iter().enumerate() {
        let color = rgba_css(series_color(i, opts.series.get(i).and_then(|s| s.color)));
        let centre = i as f64;
        let (lx, _) = project(centre - box_w / 2.0, 0.0, xr, yr);
        let (rx, _) = project(centre + box_w / 2.0, 0.0, xr, yr);
        let (_, q1y) = project(0.0, b.q1, xr, yr);
        let (_, q3y) = project(0.0, b.q3, xr, yr);
        let (_, medy) = project(0.0, b.median, xr, yr);
        let (_, lwy) = project(0.0, b.lower_whisker, xr, yr);
        let (_, uwy) = project(0.0, b.upper_whisker, xr, yr);
        let (cx, _) = project(centre, 0.0, xr, yr);
        // Box.
        let _ = writeln!(
            out,
            r##"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="{color}" fill-opacity="0.4" stroke="{color}" stroke-width="1.4" />"##,
            x = lx,
            y = q3y.min(q1y),
            w = (rx - lx).abs(),
            h = (q3y - q1y).abs(),
            color = color,
        );
        // Median.
        let _ = writeln!(
            out,
            r##"<line x1="{x1:.2}" y1="{y:.2}" x2="{x2:.2}" y2="{y:.2}" stroke="{color}" stroke-width="2" />"##,
            x1 = lx,
            x2 = rx,
            y = medy,
            color = color,
        );
        // Whiskers.
        let _ = writeln!(
            out,
            r##"<line x1="{cx:.2}" y1="{y1:.2}" x2="{cx:.2}" y2="{y2:.2}" stroke="{color}" stroke-width="1.2" />"##,
            cx = cx,
            y1 = q1y,
            y2 = lwy,
            color = color,
        );
        let _ = writeln!(
            out,
            r##"<line x1="{cx:.2}" y1="{y1:.2}" x2="{cx:.2}" y2="{y2:.2}" stroke="{color}" stroke-width="1.2" />"##,
            cx = cx,
            y1 = q3y,
            y2 = uwy,
            color = color,
        );
    }
    let (_px, _py, pw, ph) = plot_rect();
    for (i, b) in boxes.iter().enumerate() {
        let x = PAD_LEFT + (i as f64 + 0.5) * pw / boxes.len() as f64;
        let _ = writeln!(
            out,
            r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="10" text-anchor="middle" fill="#444">{label}</text>"##,
            x = x,
            y = PAD_TOP + ph + 16.0,
            label = escape_xml(&b.name),
        );
    }
    emit_y_axis_ticks(out, yr);
}

/// Six evenly-spaced X-axis ticks. Labels depend on the axis kind: plain
/// `{v:.2}` for numeric, `YYYY-MM-DD` for date columns,
/// `YYYY-MM-DD HH:MM:SS` for datetime columns.
fn emit_x_axis_ticks(out: &mut String, xr: (f64, f64), kind: XAxisKind) {
    let (px, _py, pw, ph) = plot_rect();
    for i in 0..=5 {
        let t = i as f64 / 5.0;
        let v = xr.0 + (xr.1 - xr.0) * t;
        let x = px + pw * t;
        let label = match kind {
            XAxisKind::Numeric => format!("{v:.2}"),
            XAxisKind::Date => format_days_as_date(v),
            XAxisKind::DateTime => format_seconds_as_datetime(v),
        };
        let _ = writeln!(
            out,
            r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="10" text-anchor="middle" fill="#666">{label}</text>"##,
            x = x,
            y = PAD_TOP + ph + 16.0,
            label = escape_xml(&label),
        );
    }
}

fn emit_y_axis_ticks(out: &mut String, yr: (f64, f64)) {
    let (px, py, _pw, ph) = plot_rect();
    for i in 0..=5 {
        let t = i as f64 / 5.0;
        let v = yr.0 + (yr.1 - yr.0) * t;
        let y = py + ph - ph * t;
        let _ = writeln!(
            out,
            r##"<text x="{x:.1}" y="{y:.1}" font-family="sans-serif" font-size="10" text-anchor="end" fill="#666">{v:.2}</text>"##,
            x = px - 6.0,
            y = y + 3.0,
            v = v,
        );
    }
}

fn emit_legend(out: &mut String, opts: &ExportOptions) {
    if opts.legend == LegendPosition::Off || opts.series.is_empty() {
        return;
    }
    let entry_h = 18.0;
    let pad = 8.0;
    let width = opts
        .series
        .iter()
        .map(|s| s.display_name.chars().count())
        .max()
        .unwrap_or(0) as f64
        * 7.0
        + 28.0;
    let height = opts.series.len() as f64 * entry_h + pad * 2.0;
    let (x, y) = match opts.legend {
        LegendPosition::TopLeft => (PAD_LEFT + 12.0, PAD_TOP + 12.0),
        LegendPosition::TopRight => (SVG_W - PAD_RIGHT - width - 12.0, PAD_TOP + 12.0),
        LegendPosition::BottomLeft => (PAD_LEFT + 12.0, SVG_H - PAD_BOTTOM - height - 12.0),
        LegendPosition::BottomRight => (
            SVG_W - PAD_RIGHT - width - 12.0,
            SVG_H - PAD_BOTTOM - height - 12.0,
        ),
        LegendPosition::Off => return,
    };
    let _ = writeln!(
        out,
        r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="#ffffff" fill-opacity="0.9" stroke="#aaa" stroke-width="1" />"##,
        x = x,
        y = y,
        w = width,
        h = height,
    );
    for (i, s) in opts.series.iter().enumerate() {
        let row_y = y + pad + i as f64 * entry_h + entry_h / 2.0;
        let color = rgba_css(series_color(i, s.color));
        let _ = writeln!(
            out,
            r##"<rect x="{cx:.1}" y="{cy:.1}" width="14" height="10" fill="{color}" />"##,
            cx = x + 6.0,
            cy = row_y - 5.0,
        );
        let _ = writeln!(
            out,
            r##"<text x="{tx:.1}" y="{ty:.1}" font-family="sans-serif" font-size="11" fill="#222">{name}</text>"##,
            tx = x + 26.0,
            ty = row_y + 3.5,
            name = escape_xml(&s.display_name),
        );
    }
}

/// Render the SVG to a PNG byte buffer at `scale * (800, 500)` pixels.
/// 1.0 = native size, 2.0 = retina-quality. `resvg::tiny_skia` ships a PNG
/// encoder so we don't need a separate `png` crate dep.
///
/// Loads system fonts into the usvg fontdb before parsing — without that,
/// every `<text>` element drops out and the rendered PNG is mostly blank
/// (only the rects / lines survive). Slow on first call while fontdb scans
/// the system font directories, near-instant after.
pub fn to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let w = (size.width() * scale).ceil() as u32;
    let h = (size.height() * scale).ceil() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| "pixmap allocation failed".to_string())?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| e.to_string())
}

/// Convert the SVG to a one-page PDF. Same fontdb caveat as [`to_png`].
pub fn to_pdf(svg: &str) -> Result<Vec<u8>, String> {
    let mut opt = svg2pdf::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::chart::{BoxSummary, ChartPrep, ChartSeries, XAxisKind};

    fn prep_lines() -> ChartPrep {
        ChartPrep {
            data: ChartData::Lines {
                categories: None,
                series: vec![ChartSeries {
                    name: "y".into(),
                    points: vec![[0.0, 1.0], [1.0, 2.0], [2.0, 1.5]],
                }],
            },
            total_rows: 3,
            used_rows: 3,
            x_label: "x".into(),
            y_label: "y".into(),
            x_axis_kind: XAxisKind::Numeric,
        }
    }

    #[test]
    fn svg_emits_well_formed_root() {
        let svg = to_svg(
            &prep_lines(),
            &ExportOptions {
                title: "T".into(),
                x_label: "X".into(),
                y_label: "Y".into(),
                legend: LegendPosition::Off,
                series: vec![ResolvedSeries {
                    display_name: "y".into(),
                    color: None,
                }],
            },
        );
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains(">T<"));
    }

    #[test]
    fn svg_includes_legend_when_position_set() {
        let svg = to_svg(
            &prep_lines(),
            &ExportOptions {
                title: String::new(),
                x_label: "X".into(),
                y_label: "Y".into(),
                legend: LegendPosition::TopRight,
                series: vec![ResolvedSeries {
                    display_name: "my-legend".into(),
                    color: None,
                }],
            },
        );
        assert!(svg.contains(">my-legend<"), "{svg}");
    }

    #[test]
    fn svg_escapes_special_chars() {
        let svg = to_svg(
            &prep_lines(),
            &ExportOptions {
                title: "a & b < c".into(),
                x_label: "X".into(),
                y_label: "Y".into(),
                legend: LegendPosition::Off,
                series: Vec::new(),
            },
        );
        assert!(svg.contains("a &amp; b &lt; c"));
    }

    #[test]
    fn png_decodes_to_non_empty_buffer() {
        let svg = to_svg(
            &prep_lines(),
            &ExportOptions {
                title: "T".into(),
                x_label: "X".into(),
                y_label: "Y".into(),
                legend: LegendPosition::Off,
                series: Vec::new(),
            },
        );
        let png = to_png(&svg, 1.0).unwrap();
        // PNG signature: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&png[..4], b"\x89PNG");
    }

    #[test]
    fn pdf_starts_with_magic() {
        let svg = to_svg(
            &prep_lines(),
            &ExportOptions {
                title: "T".into(),
                x_label: "X".into(),
                y_label: "Y".into(),
                legend: LegendPosition::Off,
                series: Vec::new(),
            },
        );
        let pdf = to_pdf(&svg).unwrap();
        assert_eq!(&pdf[..4], b"%PDF");
    }

    #[test]
    fn box_summary_serialises() {
        let prep = ChartPrep {
            data: ChartData::Boxes(vec![BoxSummary {
                name: "v".into(),
                lower_whisker: 1.0,
                q1: 2.0,
                median: 3.0,
                q3: 4.0,
                upper_whisker: 5.0,
            }]),
            total_rows: 5,
            used_rows: 5,
            x_label: "Series".into(),
            y_label: "Value".into(),
            x_axis_kind: XAxisKind::Numeric,
        };
        let svg = to_svg(
            &prep,
            &ExportOptions {
                title: String::new(),
                x_label: "Series".into(),
                y_label: "Value".into(),
                legend: LegendPosition::Off,
                series: vec![ResolvedSeries {
                    display_name: "v".into(),
                    color: None,
                }],
            },
        );
        assert!(svg.contains("<rect"));
        assert!(svg.contains(">v<"));
    }
}
