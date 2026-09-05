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
            // For --union and --join the positional files are intentional (they
            // form the first sources); --impute, --outliers, --partition-by,
            // --resample, --rolling, --batch-convert, --to-workbook and
            // --db-write-table take positional FILEs too.
            // All other actions ignore them, so warn.
            if !cli.files.is_empty()
                && !matches!(
                    action,
                    cli::Action::Union { .. }
                        | cli::Action::Join { .. }
                        | cli::Action::Impute { .. }
                        | cli::Action::Outliers { .. }
                        | cli::Action::Partition { .. }
                        | cli::Action::Resample { .. }
                        | cli::Action::Rolling { .. }
                        | cli::Action::BatchConvert { .. }
                        | cli::Action::Report { .. }
                        | cli::Action::ToWorkbook { .. }
                        | cli::Action::FuzzyJoin(_)
                        | cli::Action::DbWrite { .. }
                )
            {
                eprintln!(
                    "warning: ignoring trailing files when an action flag is set: {}",
                    cli.files
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            // Only --sql can scan a file in place. Saying so beats a flag
            // that quietly does nothing.
            if cli.stream && !matches!(action, cli::Action::Sql { .. }) {
                eprintln!(
                    "warning: --stream only applies to --sql; this action reads the rows themselves"
                );
            }
            // `--mcp` is handled here rather than in `cli::dispatch` because
            // it needs a tokio runtime - the GUI path never builds one and
            // we don't want to pay the cost there.
            if matches!(action, cli::Action::Mcp) {
                return run_mcp(cli.mcp_read_only, &cli.mcp_tools, &cli.mcp_without);
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
    // it can be removed. Best-effort - the next update would surface a clear
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

    let mut settings = AppSettings::load();
    // `OCTA_DEBUG=1 octa` turns diagnostics on without going through Settings.
    // That matters when the GUI is the thing being diagnosed: a user whose
    // dialog buttons do not respond cannot reach the checkbox that enables it.
    if std::env::var_os("OCTA_DEBUG").is_some() {
        settings.debug_mode = true;
    }

    // GUI diagnostics: file logging (size-capped) + a panic hook that records
    // crashes. The MCP path keeps its own stderr subscriber and is unaffected.
    octa::diagnostics::init_logging(settings.debug_mode);
    octa::diagnostics::crash::install_panic_hook();

    let resolved_icon = settings.icon_variant.resolve();
    let icon_svg = resolved_icon.svg_source();
    let icon = render_icon(icon_svg);
    let default_theme = settings.default_theme;

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(settings.window_size.dimensions())
        // Deliberately small. 800x600 used to be the floor and made the window
        // impossible to shrink far enough to park beside another one. The chrome
        // adapts down to this size: the toolbar and tab bar scroll horizontally,
        // side panels clamp to a share of the window (`ui::panel_fit`), and
        // dialogs are clamped to the viewport by `ui::size_dialog_window`.
        .with_min_inner_size([400.0, 300.0])
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
            // The Settings dialog lists the assistant's tools and what each
            // costs per request; the definitions live in this binary, so hand
            // them over before any UI runs.
            app::chat::tool_groups::publish_to_settings();
            ui::theme::apply_theme(
                &cc.egui_ctx,
                default_theme,
                ui::theme::FontSettings {
                    size: settings.font_size,
                    body: settings.body_font,
                    custom_path: Some(settings.custom_font_path.as_str()),
                },
            );
            // Suppress egui's id-clash warning (the red outline it paints
            // around widgets in debug builds). The virtual table briefly
            // reuses a widget id at a new rect during a search/filter reflow;
            // it's benign (interaction stays correct) but the red flash is
            // distracting. This is a no-op in release builds, where the
            // warning is already off (`warn_on_id_clash = cfg!(debug_assertions)`).
            cc.egui_ctx.options_mut(|o| o.warn_on_id_clash = false);
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
/// stdio loop. Logs to stderr - JSON-RPC traffic owns stdout.
fn run_mcp(read_only: bool, only: &[String], without: &[String]) -> ExitCode {
    // Resolve the tool filter before anything else: a typo in a group name
    // must stop the server, not start one advertising a surface the user did
    // not intend.
    let hidden = match mcp::tool_groups::hidden_tools(only, without) {
        Ok(hidden) => hidden,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::FAILURE;
        }
    };
    let settings = AppSettings::load();
    let row_limit = settings.mcp_default_row_limit;
    let cell_cap = settings.mcp_default_cell_bytes;
    let allow_schema_changes = !settings.write_protection;
    let backup_before_modify = settings.backup_before_modify;
    // Push the user's GUI-configured file-loader cap into the streaming
    // readers' process-wide atomic so MCP tools without `unlimited` use the
    // same default the GUI does. "Unlimited" in Settings -> Performance
    // overrides the numeric value with usize::MAX.
    let initial_cap = if settings.initial_load_rows_unlimited {
        usize::MAX
    } else {
        settings.initial_load_rows
    };
    octa::formats::set_initial_load_rows(initial_cap);
    // Route rmcp's internal tracing output to stderr; the JSON-RPC channel
    // owns stdout. Failing here is non-fatal - the server still works, we
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
    match rt.block_on(mcp::run(
        row_limit,
        cell_cap,
        read_only,
        allow_schema_changes,
        backup_before_modify,
        settings.large_file_min_bytes,
        &hidden,
    )) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: MCP server exited with error: {e}");
            ExitCode::FAILURE
        }
    }
}
