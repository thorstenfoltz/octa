//! Headless frames over a notebook holding a picture: it is drawn at its own
//! size in either layout, and a click on it hands back its bytes.

use super::*;
use crate::app::state::TabState;
use base64::Engine as _;
use octa::formats::FormatReader as _;
use octa::formats::jupyter_reader::{JupyterReader, read_output_parts};
use std::io::Write as _;

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn notebook_tab(picture: &[u8]) -> (TabState, tempfile::NamedTempFile) {
    let b64 = base64::engine::general_purpose::STANDARD.encode(picture);
    let json = format!(
        r#"{{"nbformat": 4, "nbformat_minor": 5, "metadata": {{}}, "cells": [
          {{"cell_type": "code", "execution_count": 1, "metadata": {{}}, "source": ["plot()"],
            "outputs": [
              {{"output_type": "stream", "name": "stdout", "text": "drawing\n"}},
              {{"output_type": "display_data", "metadata": {{}},
                "data": {{"image/png": "{b64}", "text/plain": "<Figure>"}}}}]}}]}}"#
    );
    let mut file = tempfile::Builder::new()
        .suffix(".ipynb")
        .tempfile()
        .unwrap();
    file.write_all(json.as_bytes()).unwrap();
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    tab.table = JupyterReader.read_file(file.path()).unwrap();
    tab.notebook_outputs = read_output_parts(file.path()).unwrap();
    (tab, file)
}

/// The bounds of every shape painted with texture `id`, nested ones included.
fn textured(shapes: &[egui::epaint::ClippedShape], id: egui::TextureId) -> Vec<egui::Rect> {
    fn walk(shape: &egui::Shape, id: egui::TextureId, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, id, out)),
            s if s.texture_id() == id => out.push(s.visual_bounding_rect()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|c| walk(&c.shape, id, &mut out));
    out
}

/// Draw a few frames, then click the picture and check the click hands back
/// its row and bytes. Returns where the picture was drawn.
fn draw_and_click(layout: NotebookOutputLayout) -> egui::Rect {
    let picture = png(400, 200);
    let (mut tab, _file) = notebook_tab(&picture);
    let ctx = egui::Context::default();
    let frame = |events: Vec<egui::Event>, tab: &mut TabState| {
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1400.0, 900.0),
            )),
            ..Default::default()
        };
        let mut clicked = None;
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            clicked = render_notebook_view(&ctx, ui, tab, ThemeMode::Light, layout, false);
        });
        out.textures_delta.clear();
        (out.shapes, clicked)
    };
    let mut shapes = Vec::new();
    for _ in 0..3 {
        shapes = frame(Vec::new(), &mut tab).0;
    }
    let Some(Ok(tex)) = tab.notebook_textures.get(&(1, 1)) else {
        panic!("picture not decoded: {:?}", tab.notebook_textures.keys());
    };
    let rects = textured(&shapes, tex.id());
    assert_eq!(rects.len(), 1, "the picture is drawn once");
    let at = rects[0].center();
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    frame(vec![egui::Event::PointerMoved(at)], &mut tab);
    frame(vec![press(true)], &mut tab);
    let (_, clicked) = frame(vec![press(false)], &mut tab);
    assert_eq!(clicked, Some((0, picture)));
    rects[0]
}

#[test]
fn a_picture_beneath_its_cell_is_drawn_at_its_own_size_and_opens_on_click() {
    let rect = draw_and_click(NotebookOutputLayout::Beneath);
    assert_eq!(rect.size(), egui::vec2(400.0, 200.0));
}

#[test]
fn a_picture_beside_its_cell_is_on_screen_at_its_own_size_and_opens_on_click() {
    // The source editor fills the row unless told otherwise, which once put
    // every Beside output past the right edge of the window.
    let rect = draw_and_click(NotebookOutputLayout::Beside);
    assert_eq!(rect.size(), egui::vec2(400.0, 200.0));
    assert!(rect.max.x <= 1400.0, "off screen: {rect:?}");
}
