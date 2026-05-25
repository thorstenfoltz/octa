//! GeoJSON Map view. Renders feature geometries on top of a slippy-map
//! tile layer (or a blank canvas, in geometry-only mode) via the
//! `walkers` crate.
//!
//! ## Tile fetching
//!
//! `walkers::HttpTiles` spawns a dedicated thread with a private tokio
//! runtime for tile downloads, so the eframe app thread stays sync.
//! `HttpTiles` and `MapMemory` need the egui `Context`, so we instantiate
//! them lazily on the first `render_map_view` call rather than at file
//! load time.
//!
//! ## Geometry overlay
//!
//! Implemented as a `walkers::Plugin` so the painter has access to the
//! `Projector` and the map's clip rect. Each geometry type maps to a
//! straightforward paint primitive:
//!   - Point / MultiPoint → small filled circle.
//!   - LineString / MultiLineString → connected line segments.
//!   - Polygon / MultiPolygon → outer ring filled, outline stroked. Holes
//!     are deliberately *not* cut out — egui's painter doesn't expose
//!     even-odd / winding fills natively, and the v1 visual is "good
//!     enough" for the common case.
//!   - GeometryCollection → recurse.

use eframe::egui;
use geo_types::Geometry as GeoGeometry;
use walkers::sources::{Attribution, TileSource};
use walkers::{HttpTiles, Map, MapMemory, Plugin, Position, Projector, TileId, lon_lat};

use crate::app::state::TabState;
use crate::ui::settings::AppSettings;
use octa::data::MapMode;
use octa::formats::geojson_reader::MapFeature;

/// Public entry point. Driven by `central_panel::render_central_panel`
/// when the active tab's `view_mode == ViewMode::Map`.
pub fn render_map_view(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tab: &mut TabState,
    settings: &AppSettings,
) {
    if tab.geojson_features.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new("GeoJSON file has no map-displayable features.").weak());
        });
        return;
    }

    draw_toolbar(ui, tab);
    ui.separator();

    // Lazy-init the map widget state. Both `HttpTiles` and `MapMemory`
    // need an `&egui::Context`, which we only have during a frame.
    ensure_map_state(ctx, tab, settings);

    // Centre on the bounding box of all features the first time the
    // memory is created (when the user has not yet dragged). After that,
    // user pans/zooms take over.
    let initial_center =
        compute_centroid(&tab.geojson_features).unwrap_or_else(|| lon_lat(0.0, 0.0));

    let features = tab.geojson_features.clone();
    let mode = tab.map_mode;
    let memory = tab.map_memory.as_mut().expect("ensured above");
    let tiles_handle: Option<&mut HttpTiles> = if mode == MapMode::Tiles {
        tab.map_tiles.as_deref_mut()
    } else {
        None
    };

    let plugin = GeoJsonPlugin { features };
    let map = Map::new(
        tiles_handle.map(|t| t as &mut dyn walkers::Tiles),
        memory,
        initial_center,
    )
    .with_plugin(plugin)
    // Plain mouse-wheel zoom (the default needs Ctrl which most users
    // don't expect from a slippy map) and double-click to zoom in.
    .zoom_with_ctrl(false)
    .double_click_to_zoom(true);
    ui.add(map);

    if mode == MapMode::Tiles {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::BOTTOM), |ui| {
            ui.add_space(4.0);
            ui.hyperlink_to(
                egui::RichText::new("© OpenStreetMap contributors")
                    .size(10.0)
                    .weak(),
                "https://www.openstreetmap.org/copyright",
            );
        });
    }
}

fn draw_toolbar(ui: &mut egui::Ui, tab: &mut TabState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{} features", tab.geojson_features.len())).strong());
        ui.separator();
        let mut mode = tab.map_mode;
        for &m in MapMode::ALL {
            if ui.radio(mode == m, m.label()).clicked() {
                mode = m;
            }
        }
        if mode != tab.map_mode {
            tab.map_mode = mode;
            // Toggling to GeometryOnly doesn't tear down the tile cache;
            // the user can flip back and the cached tiles are still
            // there. Toggling on creates the tile cache lazily next
            // frame via `ensure_map_state`.
        }
        ui.separator();
        if ui.button("Reset view").clicked() {
            // Drop the map memory so the next frame re-creates it with
            // the default zoom, then re-centres on the feature centroid.
            tab.map_memory = None;
        }
    });
}

/// Initialise (or rebuild) the per-tab tile cache + map memory if needed.
/// Called every frame; cheap when both are already populated.
fn ensure_map_state(ctx: &egui::Context, tab: &mut TabState, settings: &AppSettings) {
    if tab.map_memory.is_none() {
        tab.map_memory = Some(Box::new(MapMemory::default()));
    }
    if tab.map_mode == MapMode::Tiles && tab.map_tiles.is_none() {
        let source = TemplatedTileSource::new(settings.map_tile_url_template.clone());
        tab.map_tiles = Some(Box::new(HttpTiles::new(source, ctx.clone())));
    }
}

/// Compute the centroid of every coordinate referenced by every feature.
/// Used to centre the map the first time it renders for a file. Returns
/// `None` when the feature collection has no coordinates.
fn compute_centroid(features: &[MapFeature]) -> Option<Position> {
    let mut sum_lon = 0.0_f64;
    let mut sum_lat = 0.0_f64;
    let mut n = 0_usize;
    for f in features {
        if let Some(ref g) = f.geometry {
            walk_coords(g, &mut |c: geo_types::Coord<f64>| {
                sum_lon += c.x;
                sum_lat += c.y;
                n += 1;
            });
        }
    }
    if n == 0 {
        return None;
    }
    Some(lon_lat(sum_lon / n as f64, sum_lat / n as f64))
}

fn walk_coords<F: FnMut(geo_types::Coord<f64>)>(geom: &GeoGeometry<f64>, f: &mut F) {
    match geom {
        GeoGeometry::Point(p) => f(p.0),
        GeoGeometry::Line(l) => {
            f(l.start);
            f(l.end);
        }
        GeoGeometry::LineString(ls) => {
            for c in &ls.0 {
                f(*c);
            }
        }
        GeoGeometry::Polygon(p) => {
            for c in &p.exterior().0 {
                f(*c);
            }
            for r in p.interiors() {
                for c in &r.0 {
                    f(*c);
                }
            }
        }
        GeoGeometry::MultiPoint(mp) => {
            for p in &mp.0 {
                f(p.0);
            }
        }
        GeoGeometry::MultiLineString(mls) => {
            for ls in &mls.0 {
                for c in &ls.0 {
                    f(*c);
                }
            }
        }
        GeoGeometry::MultiPolygon(mp) => {
            for poly in &mp.0 {
                for c in &poly.exterior().0 {
                    f(*c);
                }
                for r in poly.interiors() {
                    for c in &r.0 {
                        f(*c);
                    }
                }
            }
        }
        GeoGeometry::GeometryCollection(gc) => {
            for g in &gc.0 {
                walk_coords(g, f);
            }
        }
        GeoGeometry::Rect(r) => {
            f(r.min());
            f(r.max());
        }
        GeoGeometry::Triangle(t) => {
            f(t.v1());
            f(t.v2());
            f(t.v3());
        }
    }
}

/// `walkers::TileSource` backed by a user-configurable `{z}/{x}/{y}`
/// URL template. The default points at OSM but Octa's
/// `AppSettings.map_tile_url_template` lets the user repoint at any
/// XYZ-compatible server.
struct TemplatedTileSource {
    template: String,
}

impl TemplatedTileSource {
    fn new(template: String) -> Self {
        Self { template }
    }
}

impl TileSource for TemplatedTileSource {
    fn tile_url(&self, tile_id: TileId) -> String {
        self.template
            .replace("{z}", &tile_id.zoom.to_string())
            .replace("{x}", &tile_id.x.to_string())
            .replace("{y}", &tile_id.y.to_string())
    }

    fn attribution(&self) -> Attribution {
        // We don't know which provider the user repointed at, so the
        // attribution is generic. The Map view also paints a separate
        // OSM-specific notice when the URL still points at OSM.
        Attribution {
            text: "Map data",
            url: "https://www.openstreetmap.org/copyright",
            logo_light: None,
            logo_dark: None,
        }
    }
}

/// Plugin that paints every feature geometry on top of the map. The
/// `Projector` argument converts lon/lat into screen pixels at the
/// current pan + zoom.
struct GeoJsonPlugin {
    features: Vec<MapFeature>,
}

impl Plugin for GeoJsonPlugin {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        _response: &egui::Response,
        projector: &Projector,
        _memory: &MapMemory,
    ) {
        // Steel-blue palette. Sits well against OSM's pale tiles without
        // dominating the map; the low fill alpha keeps polygon interiors
        // legible (street labels still readable through them).
        let fill = egui::Color32::from_rgba_unmultiplied(0x46, 0x82, 0xb4, 70);
        let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(0x1f, 0x3d, 0x5c));
        let point_color = egui::Color32::from_rgb(0x1f, 0x3d, 0x5c);

        let painter = ui.painter().clone();
        for f in &self.features {
            let Some(g) = f.geometry.as_ref() else {
                continue;
            };
            paint_geometry(&painter, projector, g, fill, stroke, point_color);
        }
    }
}

fn paint_geometry(
    painter: &egui::Painter,
    projector: &Projector,
    geom: &GeoGeometry<f64>,
    fill: egui::Color32,
    stroke: egui::Stroke,
    point_color: egui::Color32,
) {
    match geom {
        GeoGeometry::Point(p) => {
            let pos = project_xy(projector, p.0.x, p.0.y);
            painter.circle_filled(pos, 5.0, point_color);
        }
        GeoGeometry::MultiPoint(mp) => {
            for p in &mp.0 {
                let pos = project_xy(projector, p.0.x, p.0.y);
                painter.circle_filled(pos, 5.0, point_color);
            }
        }
        GeoGeometry::LineString(ls) => paint_line_string(painter, projector, &ls.0, stroke),
        GeoGeometry::MultiLineString(mls) => {
            for ls in &mls.0 {
                paint_line_string(painter, projector, &ls.0, stroke);
            }
        }
        GeoGeometry::Polygon(p) => paint_polygon(
            painter,
            projector,
            &p.exterior().0,
            p.interiors(),
            fill,
            stroke,
        ),
        GeoGeometry::MultiPolygon(mp) => {
            for poly in &mp.0 {
                paint_polygon(
                    painter,
                    projector,
                    &poly.exterior().0,
                    poly.interiors(),
                    fill,
                    stroke,
                );
            }
        }
        GeoGeometry::GeometryCollection(gc) => {
            for g in &gc.0 {
                paint_geometry(painter, projector, g, fill, stroke, point_color);
            }
        }
        GeoGeometry::Line(l) => {
            let a = project_xy(projector, l.start.x, l.start.y);
            let b = project_xy(projector, l.end.x, l.end.y);
            painter.line_segment([a, b], stroke);
        }
        GeoGeometry::Rect(r) => {
            let lo = r.min();
            let hi = r.max();
            let ring = [
                geo_types::Coord { x: lo.x, y: lo.y },
                geo_types::Coord { x: hi.x, y: lo.y },
                geo_types::Coord { x: hi.x, y: hi.y },
                geo_types::Coord { x: lo.x, y: hi.y },
                geo_types::Coord { x: lo.x, y: lo.y },
            ];
            paint_polygon(painter, projector, &ring, [].iter(), fill, stroke);
        }
        GeoGeometry::Triangle(t) => {
            let v1 = t.v1();
            let ring = [v1, t.v2(), t.v3(), v1];
            paint_polygon(painter, projector, &ring, [].iter(), fill, stroke);
        }
    }
}

fn paint_line_string(
    painter: &egui::Painter,
    projector: &Projector,
    coords: &[geo_types::Coord<f64>],
    stroke: egui::Stroke,
) {
    if coords.len() < 2 {
        return;
    }
    let points: Vec<egui::Pos2> = coords
        .iter()
        .map(|c| project_xy(projector, c.x, c.y))
        .collect();
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], stroke);
    }
}

fn paint_polygon<'a, I>(
    painter: &egui::Painter,
    projector: &Projector,
    exterior: &[geo_types::Coord<f64>],
    interiors: I,
    fill: egui::Color32,
    stroke: egui::Stroke,
) where
    I: IntoIterator<Item = &'a geo_types::LineString<f64>>,
{
    if exterior.len() < 3 {
        return;
    }
    let mut points: Vec<egui::Pos2> = exterior
        .iter()
        .map(|c| project_xy(projector, c.x, c.y))
        .collect();
    // egui's `add_convex_polygon` expects no duplicated closing vertex;
    // GeoJSON rings typically repeat the first coord at the end, so
    // strip it when present to avoid a degenerate triangle on the seam.
    if points.len() >= 2 && points.first() == points.last() {
        points.pop();
    }
    if points.len() < 3 {
        return;
    }
    painter.add(egui::Shape::convex_polygon(points.clone(), fill, stroke));

    // Stroke the inner rings (holes) so they remain visible even though
    // we don't cut them out of the fill. This makes "the polygon has a
    // hole" obvious without needing even-odd fill rules.
    let hole_stroke = egui::Stroke::new(stroke.width, stroke.color);
    for ring in interiors {
        if ring.0.len() < 2 {
            continue;
        }
        let mut hole: Vec<egui::Pos2> = ring
            .0
            .iter()
            .map(|c| project_xy(projector, c.x, c.y))
            .collect();
        for w in hole.windows(2) {
            painter.line_segment([w[0], w[1]], hole_stroke);
        }
        // Close the ring if not already closed.
        if hole.first() != hole.last()
            && let (Some(first), Some(last)) = (hole.first().copied(), hole.last().copied())
        {
            hole.push(first);
            painter.line_segment([last, first], hole_stroke);
        }
    }
}

fn project_xy(projector: &Projector, lon: f64, lat: f64) -> egui::Pos2 {
    let v = projector.project(lon_lat(lon, lat));
    egui::Pos2::new(v.x, v.y)
}
