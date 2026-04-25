//! Drive the shared [`SettingsDialog`] and apply the committed settings:
//! window size/maximize, theme, fonts, icon (incl. Linux desktop refresh).

use std::sync::Arc;

use eframe::egui;

use super::super::init::render_icon;
use super::super::state::OctaApp;

pub(crate) fn render_settings_dialog(app: &mut OctaApp, ctx: &egui::Context) {
    let Some(new_settings) = app.settings_dialog.show(ctx, app.logo_texture.as_ref()) else {
        return;
    };
    let icon_changed = app.settings_dialog.icon_changed;
    let font_changed = app.settings_dialog.font_changed;
    let theme_changed = app.settings_dialog.theme_changed;
    let window_size_changed = new_settings.window_size != app.settings.window_size;
    let maximized_changed = new_settings.start_maximized != app.settings.start_maximized;

    app.settings = new_settings;
    app.settings.save();

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
        app.theme_mode = app.settings.default_theme;
    }
    if font_changed || theme_changed {
        app.apply_zoom(ctx);
    }
    if icon_changed {
        let svg_src = app.settings.icon_variant.svg_source();
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
