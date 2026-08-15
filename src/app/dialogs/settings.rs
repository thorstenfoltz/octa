//! Drive the shared [`SettingsDialog`] and apply the committed settings:
//! window size/maximize, theme, fonts, icon (incl. Linux desktop refresh).

use std::sync::Arc;

use eframe::egui;

use super::super::init::render_icon;
use super::super::state::OctaApp;
use crate::app::chat::providers::{config_for_profile, make_provider};
use crate::app::chat::types::{ChatEvent, Message};

pub(crate) fn render_settings_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let applied = app.settings_dialog.show(ctx, app.logo_texture.as_ref());

    // The chat profile form asks for a connection test by leaving a request
    // behind; the providers live here, not in the library-side dialog.
    if let Some(req) = app.settings_dialog.chat_test_request.take() {
        spawn_chat_test(req);
    }

    // Clearing a secret already deleted the keyring entry, so the plaintext
    // fallback has to go from the LIVE settings too - the dialog can only
    // reach its own draft, which closing with the `x` throws away, and the
    // user was told the key was cleared.
    let purges = app.settings_dialog.take_secret_purges();
    if !purges.is_empty() {
        for purge in purges {
            match purge {
                octa::ui::settings::SecretPurge::Chat(key_id) => {
                    octa::ui::settings::secrets::delete_key_for(&key_id, &mut app.settings);
                }
                octa::ui::settings::SecretPurge::Cloud(conn_id) => {
                    octa::ui::settings::cloud_secrets::delete_cloud_secret(
                        &conn_id,
                        &mut app.settings,
                    );
                }
                octa::ui::settings::SecretPurge::Db(conn_id) => {
                    octa::ui::settings::db_secrets::delete_db_secret(&conn_id, &mut app.settings);
                }
            }
        }
        app.settings.save();
        app.cloud_browser.secret_cache.clear();
    }

    let Some(mut new_settings) = applied else {
        return;
    };

    // The dialog's draft is a snapshot taken when it opened, and the app kept
    // running behind it. Take back the fields another surface has written
    // since, or Apply silently undoes them.
    app.settings_dialog
        .carry_external_edits(&mut new_settings, &app.settings);

    let icon_changed = app.settings_dialog.icon_changed;
    let font_changed = app.settings_dialog.font_changed;
    let theme_changed = app.settings_dialog.theme_changed;
    let window_size_changed = new_settings.window_size != app.settings.window_size;
    let maximized_changed = new_settings.start_maximized != app.settings.start_maximized;

    app.settings = new_settings;
    app.settings.save();

    // Give auto-save a full fresh interval after any Settings apply (also the
    // moment the user flips the toggle on), so it never fires immediately.
    app.last_auto_save = std::time::Instant::now();

    // A secret added / cleared in Settings changes whether a connection shows
    // Sign in vs Sign out, so drop the memoised secret-presence cache.
    app.cloud_browser.secret_cache.clear();

    // DB connections may have been edited, deleted, or re-secreted: drop the
    // cached live connectors so the next operation reconnects with the new
    // definition.
    app.db_conn_cache.clear();

    // Re-seed the session search behaviour from the (possibly changed) default
    // so the search-bar toggle reflects the new preference immediately.
    app.search_result_mode = app.settings.search_result_mode;

    // Truncate the recent-files list if the user lowered `max_recent_files`.
    // Without this the menu keeps showing the old (longer) list until a new
    // file is opened - confusing because the setting appears to do nothing.
    if app.recent_files.len() > app.settings.max_recent_files {
        app.recent_files.truncate(app.settings.max_recent_files);
        app.save_recent_files();
    }

    // Push the (possibly changed) first-load row cap into the streaming
    // readers' process-wide atomic so any subsequent file open picks up
    // the new value without restart. "Unlimited" overrides the numeric.
    let initial_cap = if app.settings.initial_load_rows_unlimited {
        usize::MAX
    } else {
        app.settings.initial_load_rows
    };
    octa::formats::set_initial_load_rows(initial_cap);

    // Apply the UI language immediately so menus / dialogs re-translate without
    // a restart.
    octa::i18n::set_language(&app.settings.language);

    // Apply the debug-log verbosity immediately (no restart).
    octa::diagnostics::set_debug_level(app.settings.debug_mode);

    // Apply window-size / maximize changes immediately so the user sees the
    // effect without relaunching. `with_inner_size()` at startup is ignored
    // while the window is maximized, which was the source of "the setting
    // does nothing" reports.
    if maximized_changed || window_size_changed {
        if app.settings.start_maximized {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        } else {
            if maximized_changed {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
            }
            let [w, h] = app.settings.window_size.dimensions();
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
        }
    }

    if theme_changed {
        let was_rainbow = app.theme_mode.is_rainbow();
        app.theme_mode = app.settings.default_theme;
        // Leaving Rainbow -> drop the rainbow-rosette logo textures so
        // `ensure_logo_textures` re-renders from the user's configured
        // `resolved_icon` SVG on the next frame.
        if was_rainbow && !app.theme_mode.is_rainbow() {
            app.rainbow_active = false;
            app.logo_texture = None;
            app.welcome_logo_texture = None;
        }
    }
    if font_changed || theme_changed {
        app.apply_zoom(ctx);
    }
    if icon_changed {
        // Re-roll for `Random`; identity for any concrete variant.
        app.resolved_icon = app.settings.icon_variant.resolve();
        let svg_src = app.resolved_icon.svg_source();
        let opt = resvg::usvg::Options::default();
        if let Ok(tree) = resvg::usvg::Tree::from_str(svg_src, &opt) {
            let size = tree.size();
            let (w, h) = (size.width() as u32, size.height() as u32);
            if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) {
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::default(),
                    &mut pixmap.as_mut(),
                );
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    pixmap.data(),
                );
                app.logo_texture =
                    Some(ctx.load_texture("octa_logo", image, egui::TextureOptions::LINEAR));
            }
        }
        // Re-render welcome logo at high resolution on the next frame.
        app.welcome_logo_texture = None;

        let icon = render_icon(svg_src);
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(Arc::new(icon))));

        #[cfg(target_os = "linux")]
        refresh_linux_desktop_icon(svg_src);
    }
}

/// Run one tiny real turn against the profile under test, on a worker thread.
/// It goes through the same provider adapter and the same `ProviderConfig`
/// builder as a normal chat turn, so anything the API rejects about the profile
/// (bad key, unknown model, a `temperature` the model refuses, a thinking value
/// in the wrong dialect, an unreachable Ollama) surfaces here rather than on
/// the user's first question.
fn spawn_chat_test(req: octa::ui::settings::ChatTestRequest) {
    std::thread::spawn(move || {
        let kind = req.profile.kind;
        let res = if kind.needs_api_key() && req.api_key.trim().is_empty() {
            Err(octa::i18n::t("chat.no_key_hint"))
        } else {
            // A small cap keeps the probe cheap; the providers lift it
            // themselves where their API demands more (Anthropic thinking).
            let cfg =
                config_for_profile(&req.profile, &req.fallback_base_url, req.api_key, Some(64));
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut reply = String::new();
            make_provider(kind)
                .stream_turn(
                    &cfg,
                    "Reply with the single word OK.",
                    &[Message::user_text("ping")],
                    &[],
                    &cancel,
                    &mut |ev| match ev {
                        ChatEvent::TextDelta(t) => reply.push_str(&t),
                        ChatEvent::Error(e) => reply = format!("!{e}"),
                        _ => {}
                    },
                )
                .and_then(|()| match reply.strip_prefix('!') {
                    // A mid-stream error event is a failure even though the
                    // HTTP call itself succeeded.
                    Some(e) => Err(e.to_string()),
                    None => Ok(reply.trim().chars().take(60).collect::<String>()),
                })
        };
        if let Ok(mut g) = req.slot.lock() {
            *g = Some(res);
        }
        req.ctx.request_repaint();
    });
}

#[cfg(target_os = "linux")]
fn refresh_linux_desktop_icon(svg_src: &str) {
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);

    // Always write to user-local icon path (create dirs if needed)
    if let Some(ref h) = home {
        let local_icon_path = h.join(".local/share/icons/hicolor/scalable/apps/octa.svg");
        if let Some(parent) = local_icon_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&local_icon_path, svg_src);
    }

    // Also try system paths if they already exist
    for path in &[
        "/usr/share/icons/hicolor/scalable/apps/octa.svg",
        "/usr/local/share/icons/hicolor/scalable/apps/octa.svg",
    ] {
        let p = std::path::Path::new(path);
        if p.exists() {
            let _ = std::fs::write(p, svg_src);
        }
    }

    // Refresh icon caches (GTK, XDG, KDE)
    if let Some(ref h) = home {
        let local_hicolor = h.join(".local/share/icons/hicolor");
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-f", "-t"])
            .arg(&local_hicolor)
            .spawn();
    }
    let _ = std::process::Command::new("xdg-icon-resource")
        .arg("forceupdate")
        .spawn();
    if let Some(ref h) = home {
        let local_apps = h.join(".local/share/applications");
        if local_apps.exists() {
            let _ = std::process::Command::new("update-desktop-database")
                .arg(&local_apps)
                .spawn();
        }
    }
    // KDE Plasma: rebuild sycoca cache so taskbar picks up the new icon.
    for cmd in &["kbuildsycoca6", "kbuildsycoca5"] {
        if std::process::Command::new(cmd)
            .arg("--noincremental")
            .spawn()
            .is_ok()
        {
            break;
        }
    }
}
