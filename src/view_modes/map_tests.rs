use super::*;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// A one-shot local server answering every request with `status`.
fn serve_once(status: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
    });
    format!("http://{addr}/0/0/0.png")
}

#[test]
fn a_tile_server_that_answers_passes_the_probe() {
    assert_eq!(probe_tile(&serve_once("200 OK"), "octa/test"), Ok(()));
}

#[test]
fn a_refusing_or_failing_tile_server_fails_the_probe() {
    let err = probe_tile(&serve_once("404 Not Found"), "octa/test").unwrap_err();
    assert!(err.contains("404"), "{err}");
    // Port 1 on loopback: nothing listens there.
    assert!(probe_tile("http://127.0.0.1:1/0/0/0.png", "octa/test").is_err());
}

#[test]
fn the_probe_slot_reports_a_failure_once_and_then_clears() {
    let (tx, rx) = mpsc::channel();
    let mut slot = Some(rx);
    assert_eq!(take_probe_failure(&mut slot), None, "no answer yet");
    assert!(slot.is_some(), "still waiting");
    tx.send(Err("offline".to_string())).unwrap();
    assert_eq!(take_probe_failure(&mut slot), Some("offline".to_string()));
    assert!(slot.is_none(), "read once");

    let (tx, rx) = mpsc::channel();
    let mut slot = Some(rx);
    tx.send(Ok(())).unwrap();
    assert_eq!(
        take_probe_failure(&mut slot),
        None,
        "tiles load, no fallback"
    );
    assert!(slot.is_none());
}

#[test]
fn values_format_plainly_and_the_range_spans_matched_countries() {
    assert_eq!(fmt_value(3.0), "3");
    assert_eq!(fmt_value(2.5), "2.5");
    assert_eq!(fmt_value(1.0 / 3.0), "0.33");
    let v = CountryValues {
        by_country: vec![None, Some((4.0, 2)), Some((-1.0, 1))],
        unmatched: Vec::new(),
    };
    assert_eq!(value_range(&v), Some((-1.0, 4.0)));
    let none = CountryValues {
        by_country: vec![None],
        unmatched: Vec::new(),
    };
    assert_eq!(value_range(&none), None);
}

#[test]
fn a_point_inside_germany_hits_germany_and_not_france() {
    let c = country_match::countries();
    let de = c.find("DE").expect("Germany");
    let fr = c.find("FR").expect("France");
    let inside = |i: usize| geometry_contains(c.features[i].geometry.as_ref().unwrap(), 10.0, 51.0);
    assert!(inside(de));
    assert!(!inside(fr));
}

fn tri_area(p: &[[f64; 2]], t: [u32; 3]) -> f64 {
    cross(p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]).abs() / 2.0
}

#[test]
fn a_concave_u_shape_triangulates_without_spilling() {
    // U: 3 wide, 3 tall, notch 1 wide from the top down to y = 1. Area 9 - 2 = 7.
    let u = [
        [0.0, 0.0],
        [3.0, 0.0],
        [3.0, 3.0],
        [2.0, 3.0],
        [2.0, 1.0],
        [1.0, 1.0],
        [1.0, 3.0],
        [0.0, 3.0],
    ];
    let tris = triangulate(&u);
    assert_eq!(tris.len(), u.len() - 2);
    let total: f64 = tris.iter().map(|&t| tri_area(&u, t)).sum();
    assert!((total - 7.0).abs() < 1e-9, "area {total}");
    // No triangle centre lies in the notch.
    for t in tris {
        let c = |k: usize| (u[t[0] as usize][k] + u[t[1] as usize][k] + u[t[2] as usize][k]) / 3.0;
        assert!(
            !(c(0) > 1.0 && c(0) < 2.0 && c(1) > 1.0),
            "triangle in notch"
        );
    }
}

#[test]
fn the_fill_triangles_of_norway_cover_its_outline_area() {
    let c = country_match::countries();
    let no = c.find("NO").expect("Norway");
    let polys = &country_triangles()[no];
    assert!(!polys.is_empty());
    for p in polys {
        assert_eq!(p.tris.len(), p.pts.len() - 2);
    }
}

#[test]
fn the_row_fingerprint_tells_same_sized_filters_apart() {
    assert_ne!(rows_fingerprint(&[0, 1, 2]), rows_fingerprint(&[0, 1, 3]));
    assert_eq!(rows_fingerprint(&[4, 5]), rows_fingerprint(&[4, 5]));
}

/// What the Map view paints for a table naming countries, without a tile server:
/// the named countries in solid colours, the rest in the faint grey.
#[test]
fn a_table_of_countries_paints_them_in_colour() {
    fn walk(shape: &egui::Shape, solid: &mut usize) {
        match shape {
            egui::Shape::Mesh(mesh) => {
                *solid += usize::from(mesh.vertices.iter().any(|v| v.color.a() == 255));
            }
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, solid)),
            _ => {}
        }
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/geo/countries.csv");
    let registry = octa::formats::FormatRegistry::new();
    let mut tab = TabState::new(Default::default());
    tab.table = registry
        .reader_for_path(&path)
        .unwrap()
        .read_file(&path)
        .unwrap();
    tab.filtered_rows = (0..tab.table.row_count()).collect();
    tab.map_mode = MapMode::GeometryOnly;
    let settings = AppSettings::default();
    let ctx = egui::Context::default();

    let mut solid = 0;
    for _ in 0..3 {
        solid = 0;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            render_map_view(&ctx, ui, &mut tab, &settings);
        });
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .for_each(|c| walk(&c.shape, &mut solid));
    }

    assert_eq!(tab.map_source, MapSource::Countries);
    assert!(tab.map_country.is_some());
    assert!(solid >= 11, "only {solid} country outlines coloured");
}
