#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod mcp;
mod view_modes;

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use eframe::egui;

use octa::ui;
use ui::settings::AppSettings;

use app::OctaApp;
use app::init::render_icon;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

fn main() -> ExitCode {
    // Parse arguments via clap. When an action flag is given (--schema /
    // --head / --convert / --sql) the CLI handler runs and we exit without
    // ever touching eframe. When only file paths are present (or no args),
    // fall through to the GUI.
    let cli = cli::Cli::parse();

    match cli.detect_action() {
        Ok(Some(action)) => {
            if !cli.files.is_empty() {
                eprintln!(
                    "warning: ignoring trailing files when an action flag is set: {}",
                    cli.files
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            // `--mcp` is handled here rather than in `cli::dispatch` because
            // it needs a tokio runtime — the GUI path never builds one and
            // we don't want to pay the cost there.
            if matches!(action, cli::Action::Mcp) {
                return run_mcp();
            }
            // Resolve `--rows N|all` into an optional cap. Invalid input
            // fails fast before the action runs.
            let rows_override = match cli.rows.as_deref() {
                Some(s) => match cli::parse_rows_flag(s) {
                    Ok(n) => Some(n),
                    Err(msg) => {
                        eprintln!("error: {msg}");
                        return ExitCode::FAILURE;
                    }
                },
                None => None,
            };
            return cli::dispatch(action, cli.format, rows_override);
        }
        Ok(None) => {}
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::FAILURE;
        }
    }

    // Windows: clean up leftovers from any previous self-update. Once this new
    // exe is running, the previous-version `.old.exe` is no longer locked, so
    // it can be removed. Best-effort — the next update would surface a clear
    // error if the file is still around.
    #[cfg(target_os = "windows")]
    if let Ok(current_exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(current_exe.with_extension("old.exe"));
        let _ = std::fs::remove_file(current_exe.with_extension("update.exe"));
    }

    let initial_files: Vec<std::path::PathBuf> =
        cli.files.into_iter().filter(|p| p.exists()).collect();
    // Reference VERSION / AUTHORS / REPOSITORY so the consts stay used now
    // that clap owns --version / --help output.
    let _ = (VERSION, AUTHORS, REPOSITORY);

    let title = match initial_files.first() {
        Some(p) if initial_files.len() == 1 => format!(
            "Octa - {}",
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
        Some(_) => format!("Octa - {} files", initial_files.len()),
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
    if settings.use_custom_title_bar {
        // Drop system decorations so the custom title bar in
        // `ui::title_bar` is the only one visible. `with_resizable(true)`
        // keeps WM-level resize edges on most compositors even without a
        // title bar.
        viewport = viewport.with_decorations(false).with_resizable(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    match eframe::run_native(
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
                initial_files,
                settings,
                resolved_icon,
            )))
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eframe failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Spin up the MCP server. Reads the user's row/cell caps from `AppSettings`
/// so a user who lowers them in the GUI sees the same defaults the next time
/// they launch `octa --mcp`. Builds a single-thread tokio runtime so we don't
/// drag the multi-thread scheduler in for what is fundamentally a one-client
/// stdio loop. Logs to stderr — JSON-RPC traffic owns stdout.
fn run_mcp() -> ExitCode {
    let settings = AppSettings::load();
    let row_limit = settings.mcp_default_row_limit;
    let cell_cap = settings.mcp_default_cell_bytes;
    // Push the user's GUI-configured file-loader cap into the streaming
    // readers' process-wide atomic so MCP tools without `unlimited` use the
    // same default the GUI does. "Unlimited" in Settings → Performance
    // overrides the numeric value with usize::MAX.
    let initial_cap = if settings.initial_load_rows_unlimited {
        usize::MAX
    } else {
        settings.initial_load_rows
    };
    octa::formats::set_initial_load_rows(initial_cap);
    // Route rmcp's internal tracing output to stderr; the JSON-RPC channel
    // owns stdout. Failing here is non-fatal — the server still works, we
    // just lose structured logs.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: could not build tokio runtime for --mcp: {e}");
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(mcp::run(row_limit, cell_cap)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: MCP server exited with error: {e}");
            ExitCode::FAILURE
        }
    }
}
