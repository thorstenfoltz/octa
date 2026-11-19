use super::{format_entry_meta, human_size};
use chrono::{TimeZone, Utc};
use eframe::egui;

#[test]
fn human_size_scales_units() {
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(1024), "1.0 KB");
    assert_eq!(human_size(1536), "1.5 KB");
    assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
}

#[test]
fn entry_meta_shows_full_timestamp() {
    let m = Utc.with_ymd_and_hms(2026, 7, 6, 14, 8, 33).unwrap();
    // Both size and time -> full timestamp to the second, not date-only.
    assert_eq!(
        format_entry_meta(Some(1024), Some(&m)),
        "  (1.0 KB, 2026-07-06 14:08:33)"
    );
    // Time only.
    assert_eq!(format_entry_meta(None, Some(&m)), "  (2026-07-06 14:08:33)");
    // Size only.
    assert_eq!(format_entry_meta(Some(512), None), "  (512 B)");
    // Neither.
    assert_eq!(format_entry_meta(None, None), "");
}

/// One open connection holding `n` objects, as the sidebar hands it over.
struct Bucket {
    conn: octa::cloud::CloudConnection,
    listings: std::collections::HashMap<super::ConnPrefix, super::ListState>,
    expanded: std::collections::HashSet<super::ConnPrefix>,
}

fn bucket(n: usize) -> Bucket {
    let conn = octa::cloud::CloudConnection {
        id: "c".into(),
        name: "bucket".into(),
        bucket: "b".into(),
        anonymous: true,
        ..Default::default()
    };
    let entries = (0..n)
        .map(|i| octa::cloud::ObjectEntry {
            name: format!("part-{i:06}.parquet"),
            key: format!("part-{i:06}.parquet"),
            is_prefix: false,
            size: Some(1024 * i as u64),
            modified: None,
            etag: None,
            version: None,
        })
        .collect();
    let root = (conn.id.clone(), String::new());
    Bucket {
        listings: [(root.clone(), super::ListState::Ready(entries))].into(),
        expanded: [root].into(),
        conn,
    }
}

/// The rest of what the sidebar hands the tree: nothing signed in, nothing
/// selected, no search.
#[derive(Default)]
struct Quiet {
    sign_in: std::collections::HashMap<String, super::SignInState>,
    cli: std::collections::HashMap<octa::cloud::CloudKind, bool>,
    secrets: std::collections::HashMap<String, bool>,
    selected: std::collections::HashSet<super::CloudSelection>,
    search: octa::ui::tree_filter::TreeSearch<super::CloudHit>,
}

fn tree_ctx<'a>(b: &'a Bucket, q: &'a Quiet) -> super::TreeCtx<'a> {
    super::TreeCtx {
        listings: &b.listings,
        expanded: &b.expanded,
        sign_in: &q.sign_in,
        cli_avail: &q.cli,
        secret_present: &q.secrets,
        sign_out_confirm: None,
        sort: super::CloudSort::NameAsc,
        selected: &q.selected,
        search: &q.search,
        can_upload: false,
    }
}

fn screen(height: f32) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(300.0, height),
        )),
        ..Default::default()
    }
}

/// Draws the tree in a 300 x 600 sidebar, `frames` times; returns the time
/// and the text shapes the last frame painted.
fn draw(b: &Bucket, frames: usize) -> (std::time::Duration, usize) {
    let ctx = egui::Context::default();
    let quiet = Quiet::default();
    let tree_ctx = tree_ctx(b, &quiet);
    let mut query = String::new();
    let started = std::time::Instant::now();
    let mut texts = 0;
    for _ in 0..frames {
        let mut out = ctx.run_ui(screen(600.0), |ui| {
            super::render_cloud_tree(ui, std::slice::from_ref(&b.conn), &tree_ctx, &mut query);
        });
        out.textures_delta.clear();
        texts = out
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Text(_)))
            .count();
    }
    (started.elapsed() / frames as u32, texts)
}

/// A folder's rows out of view are passed over by their height, not drawn.
/// The list must still come out exactly as tall as with every row drawn, or
/// the scroll bar jumps as rows come into view, and the rubber band must
/// find each row where a drawn one would be.
#[test]
fn rows_out_of_view_take_the_room_drawn_rows_take() {
    let b = bucket(300);
    let quiet = Quiet::default();
    let lay_out = |height: f32| {
        let ctx = egui::Context::default();
        let tree_ctx = tree_ctx(&b, &quiet);
        let mut found = (0.0, Vec::new(), 0);
        let mut out = ctx.run_ui(screen(height), |ui| {
            let mut rows = Vec::new();
            let mut action = super::CloudTreeAction::default();
            let node = super::Node {
                conn_id: "c",
                prefix: "",
                depth: 1,
                filter: None,
            };
            super::draw_listing(ui, &tree_ctx, node, &mut action, &mut rows);
            let centers: Vec<f32> = rows.iter().map(|(r, _, _)| r.center().y).collect();
            found = (ui.min_rect().height(), centers, 0);
        });
        out.textures_delta.clear();
        found.2 = out
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Text(_)))
            .count();
        found
    };
    let (all_height, all_rows, all_drawn) = lay_out(100_000.0);
    let (some_height, some_rows, some_drawn) = lay_out(200.0);
    assert!(some_drawn < 20, "only the rows in view: {some_drawn}");
    assert_eq!(all_drawn, 300, "every row in a tall window");
    assert!(
        (all_height - some_height).abs() < 0.01,
        "{all_height} vs {some_height}"
    );
    assert_eq!(all_rows.len(), 300);
    assert_eq!(some_rows.len(), 300);
    for (i, (a, s)) in all_rows.iter().zip(&some_rows).enumerate() {
        assert!(
            (a - s).abs() < 0.01,
            "row {i}: drawn at {a}, passed over at {s}"
        );
    }
}

#[test]
#[ignore = "timing, run by hand"]
fn time_a_big_folder() {
    for n in [100, 2_000, 20_000, 200_000] {
        let (per_frame, texts) = draw(&bucket(n), 10);
        eprintln!("{n:>6} objects: {per_frame:?} per frame, {texts} texts painted");
    }
}
