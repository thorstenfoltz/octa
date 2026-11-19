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
//!   - Point / MultiPoint -> small filled circle.
//!   - LineString / MultiLineString -> connected line segments.
//!   - Polygon / MultiPolygon -> outer ring filled, outline stroked. Holes
//!     are deliberately *not* cut out - egui's painter doesn't expose
//!     even-odd / winding fills natively, and the v1 visual is "good
//!     enough" for the common case.
//!   - GeometryCollection -> recurse.

use eframe::egui;
use geo_types::Geometry as GeoGeometry;
use walkers::sources::{Attribution, TileSource};
use walkers::{HttpTiles, Map, MapMemory, Plugin, Position, Projector, TileId, lon_lat};

use crate::app::state::{CountryPick, MapSource, TabState};
use crate::ui::settings::AppSettings;
use octa::data::MapMode;
use octa::data::chart::{Aggregation, heat_rgb};
use octa::data::country_match::{self, CountryValues};
use octa::formats::geojson_reader::MapFeature;

/// Public entry point. Driven by `central_panel::render_central_panel`
/// when the active tab's `view_mode == ViewMode::Map`. Returns a note for the
/// status bar when the tiles could not be loaded and the tab fell back to
/// geometry only.
pub fn render_map_view(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tab: &mut TabState,
    settings: &AppSettings,
) -> Option<String> {
    // GeoJSON and Shapefile tabs carry their geometry from the file. Any other
    // table is plotted from its latitude/longitude columns (detected on first
    // entry).
    let geometry_from_file = matches!(
        tab.table.format_name.as_deref(),
        Some("GeoJSON") | Some("Shapefile")
    );
    if !geometry_from_file {
        seed_point_features(tab);
        seed_country_mode(tab);
    }
    let countries_mode =
        !geometry_from_file && tab.map_source == MapSource::Countries && tab.map_country.is_some();
    if countries_mode {
        refresh_country_cache(tab);
    }

    if !countries_mode && tab.geojson_features.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(octa::i18n::t("view.map_no_features")).weak());
        });
        return None;
    }

    draw_toolbar(ui, tab, geometry_from_file);
    ui.separator();

    // Lazy-init the map widget state. Both `HttpTiles` and `MapMemory`
    // need an `&egui::Context`, which we only have during a frame.
    ensure_map_state(ctx, tab, settings, countries_mode);
    let note = take_probe_failure(&mut tab.map_tile_probe).map(|reason| {
        tab.map_mode = MapMode::GeometryOnly;
        octa::i18n::t("view.map_tiles_failed").replace("{reason}", &reason)
    });

    // Centre on the bounding box of all features the first time the
    // memory is created (when the user has not yet dragged). After that,
    // user pans/zooms take over.
    let initial_center = if countries_mode {
        lon_lat(10.0, 25.0)
    } else {
        compute_centroid(&tab.geojson_features).unwrap_or_else(|| lon_lat(0.0, 0.0))
    };

    let mode = tab.map_mode;
    let country_plugin = countries_mode.then(|| country_plugin(tab, mode == MapMode::Tiles));
    if countries_mode {
        draw_country_legend(ui, tab);
    }
    let geojson_plugin = GeoJsonPlugin {
        features: if countries_mode {
            Vec::new()
        } else {
            tab.geojson_features.clone()
        },
    };
    let memory = tab.map_memory.as_mut().expect("ensured above");
    let tiles_handle: Option<&mut HttpTiles> = if mode == MapMode::Tiles {
        tab.map_tiles.as_deref_mut()
    } else {
        None
    };

    let map = Map::new(
        tiles_handle.map(|t| t as &mut dyn walkers::Tiles),
        memory,
        initial_center,
    );
    let map = match country_plugin {
        Some(p) => map.with_plugin(p),
        None => map.with_plugin(geojson_plugin),
    }
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
    note
}

/// Seed `geojson_features` from the table's coordinate columns the first time
/// a plain (non-GeoJSON) table opens the Map view. Detection runs once; the
/// chosen columns are remembered in `tab.map_coord_cols` and can be changed
/// via the toolbar dropdown (which rebuilds directly).
fn seed_point_features(tab: &mut TabState) {
    if tab.map_coord_cols.is_none()
        && let Some((lat, lon)) = octa::data::geo_detect::detect_lat_lon(&tab.table)
    {
        tab.map_coord_cols = Some((lat, lon));
        tab.geojson_features =
            octa::formats::geojson_reader::points_to_features(&tab.table, lat, lon);
    }
}

fn draw_toolbar(ui: &mut egui::Ui, tab: &mut TabState, geometry_from_file: bool) {
    octa::ui::control_row::control_line(ui, |ui| {
        let shown = match (&tab.country_cache, tab.map_source, geometry_from_file) {
            (Some((_, v)), MapSource::Countries, false) => v.by_country.iter().flatten().count(),
            _ => tab.geojson_features.len(),
        };
        ui.label(
            egui::RichText::new(format!("{} {}", shown, octa::i18n::t("view.map_features")))
                .strong(),
        );
        ui.separator();
        let mut mode = tab.map_mode;
        for &m in MapMode::ALL {
            if ui.radio(mode == m, m.label_t()).clicked() {
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
        if ui.button(octa::i18n::t("view.map_reset")).clicked() {
            // Drop the map memory so the next frame re-creates it with
            // the default zoom, then re-centres on the feature centroid.
            tab.map_memory = None;
        }
        // Coordinate-column picker for point-from-table maps (where the
        // geometry comes from lat/lon columns, not a GeoJSON/Shapefile file).
        if !geometry_from_file {
            draw_source_picker(ui, tab);
            if tab.map_source == MapSource::Countries && tab.map_country.is_some() {
                draw_country_picker(ui, tab);
            } else {
                draw_coord_picker(ui, tab);
            }
        }
    });
}

/// The country column's mode defaults: opens in Countries mode when the table
/// names countries but has no latitude/longitude pair, and keeps the picks
/// valid when columns disappear.
fn seed_country_mode(tab: &mut TabState) {
    let n = tab.table.col_count();
    if tab
        .map_country
        .is_some_and(|p| p.col >= n || p.value.is_some_and(|v| v >= n))
    {
        tab.map_country = None;
        tab.country_cache = None;
    }
    let detected = if tab.map_country.is_none() {
        country_match::detect_country_column(&tab.table)
    } else {
        None
    };
    if tab.map_coord_cols.is_none() && tab.map_country.is_none() && detected.is_some() {
        tab.map_source = MapSource::Countries;
    }
    if tab.map_source == MapSource::Countries && tab.map_country.is_none() && n > 0 {
        let col = detected.unwrap_or(0);
        let value = (0..n).find(|&c| {
            c != col && octa::data::is_numeric_data_type(&tab.table.columns[c].data_type)
        });
        tab.map_country = Some(CountryPick {
            col,
            value,
            agg: Aggregation::Sum,
        });
    }
}

/// Hash of the visible rows, so a filter that keeps the same number of
/// different rows still invalidates the cache. O(n), far cheaper than `aggregate`.
fn rows_fingerprint(rows: &[usize]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rows.hash(&mut h);
    h.finish()
}

/// Re-aggregate only when the pick, the visible rows or the cells changed.
fn refresh_country_cache(tab: &mut TabState) {
    let Some(pick) = tab.map_country else {
        return;
    };
    let key = (
        pick,
        rows_fingerprint(&tab.filtered_rows),
        tab.table.edits.len(),
        tab.table.undo_stack.len(),
    );
    if tab.country_cache.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    let values = country_match::aggregate(
        &tab.table,
        &tab.filtered_rows,
        pick.col,
        pick.value,
        pick.agg,
    );
    tab.country_cache = Some((key, values));
}

/// Points / Countries radios. Points is greyed while no lat/lon pair exists.
fn draw_source_picker(ui: &mut egui::Ui, tab: &mut TabState) {
    ui.separator();
    let has_points = tab.map_coord_cols.is_some();
    let hint = octa::i18n::t("mapcoord.mode_points_hint");
    let points = ui
        .add_enabled(
            has_points,
            egui::RadioButton::new(
                tab.map_source == MapSource::Points,
                octa::i18n::t("mapcoord.mode_points"),
            ),
        )
        .on_hover_text(&hint)
        .on_disabled_hover_text(octa::ui::menu_entry::disabled_tip(
            &hint,
            &octa::i18n::t("greyed.view_map"),
        ));
    let countries = ui
        .radio(
            tab.map_source == MapSource::Countries,
            octa::i18n::t("mapcoord.mode_countries"),
        )
        .on_hover_text(octa::i18n::t("mapcoord.mode_countries_hint"));
    let next = if points.clicked() {
        MapSource::Points
    } else if countries.clicked() {
        MapSource::Countries
    } else {
        return;
    };
    if next != tab.map_source {
        tab.map_source = next;
        // Re-centre and re-zoom for the other kind of map.
        tab.map_memory = None;
    }
}

/// Country, Value and Combine dropdowns for the Countries mode.
fn draw_country_picker(ui: &mut egui::Ui, tab: &mut TabState) {
    let Some(mut pick) = tab.map_country else {
        return;
    };
    let t = octa::i18n::t;
    let cols: Vec<(String, bool)> = tab
        .table
        .columns
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                octa::data::is_numeric_data_type(&c.data_type),
            )
        })
        .collect();
    let name = |i: usize| cols.get(i).map(|c| c.0.clone()).unwrap_or_default();
    ui.separator();
    ui.label(t("mapcoord.country"));
    egui::ComboBox::from_id_salt("map_country_col")
        .selected_text(name(pick.col))
        .show_ui(ui, |ui| {
            for (i, (n, _)) in cols.iter().enumerate() {
                ui.selectable_value(&mut pick.col, i, n);
            }
        })
        .response
        .on_hover_text(t("mapcoord.country_hint"));
    ui.label(t("mapcoord.value"));
    egui::ComboBox::from_id_salt("map_country_value")
        .selected_text(pick.value.map_or_else(|| t("mapcoord.count_rows"), name))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut pick.value, None, t("mapcoord.count_rows"));
            for (i, (n, numeric)) in cols.iter().enumerate() {
                if *numeric {
                    ui.selectable_value(&mut pick.value, Some(i), n);
                }
            }
        })
        .response
        .on_hover_text(t("mapcoord.value_hint"));
    ui.label(t("mapcoord.combine"));
    let combine_hint = t("mapcoord.combine_hint");
    ui.add_enabled_ui(pick.value.is_some(), |ui| {
        egui::ComboBox::from_id_salt("map_country_agg")
            .selected_text(pick.agg.display_name())
            .show_ui(ui, |ui| {
                for a in [Aggregation::Sum, Aggregation::Avg, Aggregation::Count] {
                    ui.selectable_value(&mut pick.agg, a, a.display_name());
                }
            })
            .response
            .on_hover_text(&combine_hint)
            .on_disabled_hover_text(octa::ui::menu_entry::disabled_tip(
                &combine_hint,
                &t("mapcoord.combine_off"),
            ));
    });
    tab.map_country = Some(pick);
}

/// Lat/Lon column dropdowns for a table-driven map. Changing either rebuilds
/// the point features and recentres the map.
fn draw_coord_picker(ui: &mut egui::Ui, tab: &mut TabState) {
    let Some((mut lat, mut lon)) = tab.map_coord_cols else {
        return;
    };
    let col_names: Vec<String> = tab.table.columns.iter().map(|c| c.name.clone()).collect();
    let mut changed = false;
    ui.separator();
    ui.label(octa::i18n::t("mapcoord.lat"));
    egui::ComboBox::from_id_salt("map_lat_col")
        .selected_text(col_names.get(lat).cloned().unwrap_or_default())
        .show_ui(ui, |ui| {
            for (i, name) in col_names.iter().enumerate() {
                if ui.selectable_label(lat == i, name).clicked() && lat != i {
                    lat = i;
                    changed = true;
                }
            }
        });
    ui.label(octa::i18n::t("mapcoord.lon"));
    egui::ComboBox::from_id_salt("map_lon_col")
        .selected_text(col_names.get(lon).cloned().unwrap_or_default())
        .show_ui(ui, |ui| {
            for (i, name) in col_names.iter().enumerate() {
                if ui.selectable_label(lon == i, name).clicked() && lon != i {
                    lon = i;
                    changed = true;
                }
            }
        });
    if changed {
        tab.map_coord_cols = Some((lat, lon));
        tab.geojson_features =
            octa::formats::geojson_reader::points_to_features(&tab.table, lat, lon);
        // Recentre on the new points.
        tab.map_memory = None;
    }
}

/// Initialise (or rebuild) the per-tab tile cache + map memory if needed.
/// Called every frame; cheap when both are already populated.
fn ensure_map_state(
    ctx: &egui::Context,
    tab: &mut TabState,
    settings: &AppSettings,
    countries_mode: bool,
) {
    if tab.map_memory.is_none() {
        let mut memory = MapMemory::default();
        if countries_mode {
            // The default zoom (16) is street level; 2 shows the world.
            let _ = memory.set_zoom(2.0);
        }
        tab.map_memory = Some(Box::new(memory));
    }
    if tab.map_mode == MapMode::Tiles && tab.map_tiles.is_none() {
        let source = TemplatedTileSource::new(settings.map_tile_url_template.clone());
        let agent = format!("octa/{}", env!("CARGO_PKG_VERSION"));
        // walkers only logs a failed download, so Octa asks the server for one
        // tile itself, with the same user agent the map sends.
        if settings.map_fallback_to_geometry {
            let url = source.tile_url(TileId {
                x: 0,
                y: 0,
                zoom: 0,
            });
            let (tx, rx) = std::sync::mpsc::channel();
            let (ui_ctx, agent) = (ctx.clone(), agent.clone());
            std::thread::spawn(move || {
                let _ = tx.send(probe_tile(&url, &agent));
                ui_ctx.request_repaint();
            });
            tab.map_tile_probe = Some(rx);
        }
        let options = walkers::HttpOptions {
            user_agent: walkers::HeaderValue::from_str(&agent).ok(),
            ..Default::default()
        };
        tab.map_tiles = Some(Box::new(HttpTiles::with_options(
            source,
            options,
            ctx.clone(),
        )));
    }
}

/// Fetch one tile to learn whether the server answers. `Err` says why not.
fn probe_tile(url: &str, agent: &str) -> Result<(), String> {
    let client: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into();
    let response = client
        .get(url)
        .header("User-Agent", agent)
        .call()
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", response.status()))
    }
}

/// The probe's reason once it has failed; clears the slot when it has
/// answered either way.
fn take_probe_failure(
    slot: &mut Option<std::sync::mpsc::Receiver<Result<(), String>>>,
) -> Option<String> {
    let answer = match slot.as_ref()?.try_recv() {
        Ok(answer) => answer,
        Err(std::sync::mpsc::TryRecvError::Empty) => return None,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => Ok(()),
    };
    *slot = None;
    answer.err()
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

/// Smallest and largest matched value, `None` when no country matched.
fn value_range(values: &CountryValues) -> Option<(f64, f64)> {
    values
        .by_country
        .iter()
        .flatten()
        .map(|(v, _)| *v)
        .fold(None, |r, v| match r {
            None => Some((v, v)),
            Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
        })
}

/// A value for the tooltip and legend: whole numbers plain, others to two
/// decimals with trailing zeros dropped.
fn fmt_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

/// Fill and tooltip per outline from the cached aggregate.
fn country_plugin(tab: &TabState, tiles: bool) -> CountryPlugin {
    let c = country_match::countries();
    let mut fills = vec![None; c.features.len()];
    let mut hover = vec![None; c.features.len()];
    if let Some((_, values)) = &tab.country_cache
        && let Some((lo, hi)) = value_range(values)
    {
        let alpha = if tiles { 200 } else { 255 };
        let template = octa::i18n::t("mapcoord.hover");
        for (i, v) in values.by_country.iter().enumerate().take(fills.len()) {
            if let Some((x, n)) = v {
                let [r, g, b] = heat_rgb(*x, lo, hi);
                fills[i] = Some(egui::Color32::from_rgba_unmultiplied(r, g, b, alpha));
                hover[i] = Some(
                    template
                        .replace("{country}", &c.names[i])
                        .replace("{value}", &fmt_value(*x))
                        .replace("{n}", &n.to_string()),
                );
            }
        }
    }
    CountryPlugin {
        features: &c.features,
        fills,
        hover,
    }
}

/// Under the map: the colour scale between the smallest and largest value,
/// then the values that named no country.
fn draw_country_legend(ui: &mut egui::Ui, tab: &TabState) {
    let Some((_, values)) = &tab.country_cache else {
        return;
    };
    let range = value_range(values);
    if range.is_none() && values.unmatched.is_empty() {
        return;
    }
    egui::Panel::bottom("map_country_legend")
        .resizable(false)
        .show(ui, |ui| {
            octa::ui::control_row::control_line(ui, |ui| {
                let Some((lo, hi)) = range else {
                    return;
                };
                ui.label(fmt_value(lo));
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(120.0, 14.0), egui::Sense::hover());
                const STEPS: usize = 10;
                let w = rect.width() / STEPS as f32;
                for i in 0..STEPS {
                    let v = lo + (hi - lo) * (i as f64 + 0.5) / STEPS as f64;
                    let [r, g, b] = heat_rgb(v, lo, hi);
                    let x = rect.left() + w * i as f32;
                    let cell = egui::Rect::from_min_max(
                        egui::pos2(x, rect.top()),
                        egui::pos2(x + w, rect.bottom()),
                    );
                    ui.painter()
                        .rect_filled(cell, 0.0, egui::Color32::from_rgb(r, g, b));
                }
                ui.label(fmt_value(hi));
            });
            if values.unmatched.is_empty() {
                return;
            }
            let shown = 20;
            octa::ui::control_row::control_row(ui, |ui| {
                ui.label(octa::i18n::t("mapcoord.unmatched"))
                    .on_hover_text(octa::i18n::t("mapcoord.unmatched_hint"));
                let row = octa::i18n::t("mapcoord.unmatched_row");
                for (value, n) in values.unmatched.iter().take(shown) {
                    ui.label(row.replace("{value}", value).replace("{n}", &n.to_string()));
                }
                if values.unmatched.len() > shown {
                    ui.label(
                        octa::i18n::t("banner.and_more")
                            .replace("{n}", &(values.unmatched.len() - shown).to_string()),
                    );
                }
            });
        });
}

/// Is the point (lon, lat) inside the geometry? Polygons only.
fn geometry_contains(g: &GeoGeometry<f64>, x: f64, y: f64) -> bool {
    match g {
        GeoGeometry::Polygon(p) => octa::data::spatial_join::contains(p, x, y),
        GeoGeometry::MultiPolygon(mp) => {
            mp.0.iter()
                .any(|p| octa::data::spatial_join::contains(p, x, y))
        }
        GeoGeometry::GeometryCollection(gc) => gc.0.iter().any(|g| geometry_contains(g, x, y)),
        _ => false,
    }
}

/// One outline polygon, ready to fill: vertices as (lon, lat) with latitude
/// clamped to the Web Mercator limit, and the triangles that cover it.
struct FillPoly {
    pts: Vec<(f64, f64)>,
    tris: Vec<[u32; 3]>,
}

/// Unit Web Mercator (x right, y down), the plane walkers' screen position is
/// an affine function of at any zoom, so triangles made here stay valid.
fn mercator_unit(lon: f64, lat: f64) -> [f64; 2] {
    let lat = lat.clamp(-85.05, 85.05).to_radians();
    [
        (lon + 180.0) / 360.0,
        0.5 - (std::f64::consts::FRAC_PI_4 + lat / 2.0).tan().ln() / std::f64::consts::TAU,
    ]
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Ear-clipping triangulation of a simple polygon (no holes). Returns
/// `n - 2` index triples into `pts`, any winding. ponytail: O(n^2), run once
/// per outline; a ring with no ear (degenerate input) clips its first vertex
/// so it always terminates.
fn triangulate(pts: &[[f64; 2]]) -> Vec<[u32; 3]> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    if n < 3 {
        return out;
    }
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let mut idx: Vec<usize> = (0..n).collect();
    if area < 0.0 {
        idx.reverse();
    }
    while idx.len() > 3 {
        let m = idx.len();
        let ear = (0..m).find(|&i| {
            let (a, b, c) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            if cross(pts[a], pts[b], pts[c]) <= 0.0 {
                return false;
            }
            !idx.iter().any(|&p| {
                p != a && p != b && p != c && {
                    let q = pts[p];
                    cross(pts[a], pts[b], q) >= 0.0
                        && cross(pts[b], pts[c], q) >= 0.0
                        && cross(pts[c], pts[a], q) >= 0.0
                }
            })
        });
        let i = ear.unwrap_or(0);
        out.push([
            idx[(i + m - 1) % m] as u32,
            idx[i] as u32,
            idx[(i + 1) % m] as u32,
        ]);
        idx.remove(i);
    }
    out.push([idx[0] as u32, idx[1] as u32, idx[2] as u32]);
    out
}

/// The fill triangles of every country outline, built once.
fn country_triangles() -> &'static [Vec<FillPoly>] {
    static T: std::sync::OnceLock<Vec<Vec<FillPoly>>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let fill_of = |p: &geo_types::Polygon<f64>| {
            let mut ring: Vec<(f64, f64)> = p
                .exterior()
                .0
                .iter()
                .map(|c| (c.x, c.y.clamp(-85.05, 85.05)))
                .collect();
            if ring.len() > 1 && ring.first() == ring.last() {
                ring.pop();
            }
            let plane: Vec<[f64; 2]> = ring.iter().map(|&(x, y)| mercator_unit(x, y)).collect();
            FillPoly {
                tris: triangulate(&plane),
                pts: ring,
            }
        };
        country_match::countries()
            .features
            .iter()
            .map(|f| match f.geometry.as_ref() {
                Some(GeoGeometry::Polygon(p)) => vec![fill_of(p)],
                Some(GeoGeometry::MultiPolygon(mp)) => mp.0.iter().map(fill_of).collect(),
                _ => Vec::new(),
            })
            .collect()
    })
}

/// Country outlines filled by value. `fills[i]` is `None` for an outline no
/// row names (faint fill); `hover[i]` is its tooltip.
struct CountryPlugin {
    features: &'static [MapFeature],
    fills: Vec<Option<egui::Color32>>,
    hover: Vec<Option<String>>,
}

impl Plugin for CountryPlugin {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &Projector,
        _memory: &MapMemory,
    ) {
        let border = ui.visuals().widgets.noninteractive.bg_stroke.color;
        let stroke = egui::Stroke::new(1.0_f32, border);
        let faint = egui::Color32::from_rgba_unmultiplied(128, 128, 128, 40);
        let painter = ui.painter().clone();
        let tris = country_triangles();
        // Fills first (one mesh per outline), then every border on top.
        for (polys, fill) in tris.iter().zip(&self.fills) {
            let colour = fill.unwrap_or(faint);
            for poly in polys {
                let mut mesh = egui::Mesh::default();
                for &(lon, lat) in &poly.pts {
                    mesh.colored_vertex(project_xy(projector, lon, lat), colour);
                }
                for t in &poly.tris {
                    mesh.add_triangle(t[0], t[1], t[2]);
                }
                painter.add(egui::Shape::mesh(mesh));
            }
        }
        for f in self.features {
            if let Some(g) = f.geometry.as_ref() {
                paint_geometry(
                    &painter,
                    projector,
                    g,
                    egui::Color32::TRANSPARENT,
                    stroke,
                    border,
                );
            }
        }
        if let Some(p) = response.hover_pos() {
            let at = projector.unproject(p.to_vec2());
            let hit = self.features.iter().enumerate().find(|(i, f)| {
                self.hover[*i].is_some()
                    && f.geometry
                        .as_ref()
                        .is_some_and(|g| geometry_contains(g, at.x(), at.y()))
            });
            if let Some((i, _)) = hit
                && let Some(text) = self.hover[i].clone()
            {
                response.clone().on_hover_ui_at_pointer(|ui| {
                    ui.label(text);
                });
            }
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
        let stroke = egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(0x1f, 0x3d, 0x5c));
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

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;
