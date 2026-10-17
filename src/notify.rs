//! Desktop notification for a long task that finished while Octa was in the
//! background. This module decides *whether* and sends; the app's per-frame
//! watcher (`src/app/notify_watch.rs`) decides *when*.

use std::time::Duration;

/// The id Windows shows the zip / installer copy's toasts under, registered
/// by [`register_windows_identity`]. The Store copy uses its package's own.
#[cfg(windows)]
const WINDOWS_APP_ID: &str = "Octa.Octa";

/// Notify only when switched on, the task ran at least `threshold_secs`, and
/// Octa is not the window in front, unless `in_front_too` says that does not
/// matter. Unknown focus counts as in front: no pop-up rather than one over
/// the window the user is looking at.
pub fn should_notify(
    enabled: bool,
    threshold_secs: u32,
    elapsed: Duration,
    focused: Option<bool>,
    in_front_too: bool,
) -> bool {
    enabled
        && elapsed >= Duration::from_secs(u64::from(threshold_secs))
        && (in_front_too || focused == Some(false))
}

/// The "Notify after (seconds)" field: a whole number of 1 or more.
pub fn parse_after_secs(text: &str) -> Option<u32> {
    let n: u32 = text.trim().parse().ok()?;
    (n >= 1).then_some(n)
}

/// `38 s`, `4 min 12 s`, `1 h 2 min`.
pub fn format_elapsed(d: Duration) -> String {
    let s = d.as_secs();
    match (s / 3600, s % 3600 / 60, s % 60) {
        (0, 0, sec) => format!("{sec} s"),
        (0, min, sec) => format!("{min} min {sec} s"),
        (h, min, _) => format!("{h} h {min} min"),
    }
}

/// Show one notification on a short-lived thread, so a slow or missing
/// notification service never stalls a frame. A failure goes to the debug
/// log only.
pub fn send(title: String, body: String) {
    std::thread::spawn(move || {
        if let Err(e) = show(&title, &body) {
            tracing::warn!("notification not shown: {e:#}");
        }
    });
}

fn show(title: &str, body: &str) -> anyhow::Result<()> {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body);
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // The desktop-entry hint is what KDE files a notification under:
        // Octa's own page in System Settings > Notifications, its icon, and
        // its place in the history. It matches the installed octa.desktop.
        n.appname("Octa")
            .icon("octa")
            .hint(notify_rust::Hint::DesktopEntry("octa".into()));
    }
    #[cfg(target_os = "macos")]
    {
        // Matches CFBundleIdentifier in the release workflow's Info.plist.
        let _ = notify_rust::set_application("dev.octa.Octa");
    }
    #[cfg(windows)]
    {
        let id = crate::platform::package_app_id().unwrap_or_else(|| WINDOWS_APP_ID.to_string());
        n.app_id(&id);
    }
    let _ = n.show()?;
    Ok(())
}

/// Give the zip / installer copy a name and icon in Windows notifications:
/// one HKCU registry entry, no admin, nothing to install. The Store copy has
/// its package identity and skips this. No-op off Windows.
pub fn register_windows_identity(icon_svg: &str) {
    #[cfg(windows)]
    {
        if crate::platform::package_app_id().is_some() {
            return;
        }
        let icon =
            crate::ui::settings::AppSettings::config_dir().map(|d| d.join("octa-notify.png"));
        let icon = icon.filter(|path| match write_icon_png(icon_svg, path) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("notification icon not written: {e:#}");
                false
            }
        });
        if let Err(e) =
            crate::platform::register_app_user_model_id(WINDOWS_APP_ID, "Octa", icon.as_deref())
        {
            tracing::warn!("notification identity not registered: {e:#}");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = icon_svg;
    }
}

#[cfg(windows)]
fn write_icon_png(svg: &str, path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(256, 256).context("allocating the icon")?;
    let size = tree.size();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(256.0 / size.width(), 256.0 / size.height()),
        &mut pixmap.as_mut(),
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    pixmap.save_png(path)?;
    Ok(())
}

#[cfg(test)]
#[path = "notify_tests.rs"]
mod tests;
