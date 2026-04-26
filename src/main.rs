#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod view_modes;

use std::sync::Arc;

use eframe::egui;

use octa::ui;
use ui::settings::AppSettings;

use app::OctaApp;
use app::init::render_icon;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

fn main() -> eframe::Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("octa {}", VERSION);
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!(
                    "octa {} - A modular multi-format data viewer and editor",
                    VERSION
                );
                println!();
                println!("Usage: octa [OPTIONS] [FILE]");
                println!();
                println!("Arguments:");
                println!("  [FILE]  File to open on startup");
                println!();
                println!("Options:");
                println!("  -V, --version  Print version");
                println!("  -h, --help     Print help");
                println!();
                println!("Author:  {}", AUTHORS);
                println!("Repo:    {}", REPOSITORY);
                std::process::exit(0);
            }
            _ => {}
        }
    }

    let initial_file = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists());

    let title = match &initial_file {
        Some(p) => format!(
            "Octa - {}",
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        None => "Octa".to_string(),
    };

    let settings = AppSettings::load();
    let resolved_icon = settings.icon_variant.resolve();
    let icon_svg = resolved_icon.svg_source();
    let icon = render_icon(icon_svg);
    let default_theme = settings.default_theme;

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(settings.window_size.dimensions())
        .with_min_inner_size([800.0, 600.0])
        .with_title(&title)
        .with_icon(Arc::new(icon));
    if settings.start_maximized {
        viewport = viewport.with_maximized(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "octa",
        options,
        Box::new(move |cc| {
            ui::theme::apply_theme(
                &cc.egui_ctx,
                default_theme,
                ui::theme::FontSettings {
                    size: settings.font_size,
                    body: settings.body_font,
                    custom_path: Some(settings.custom_font_path.as_str()),
                },
            );
            Ok(Box::new(OctaApp::new(
                initial_file,
                settings,
                resolved_icon,
            )))
        }),
    )
}
