//! Platform / packaging detection.

use std::path::Path;
use std::sync::OnceLock;

/// True when Octa is running as an MSIX package installed from the Microsoft
/// Store. Store packages always live under a `WindowsApps` directory; nothing
/// else does, so a path check is sufficient and needs no Windows API. Cached
/// for the process lifetime (packaging never changes mid-run).
///
// ponytail: path heuristic; swap to GetCurrentPackageFullName (windows-sys)
// if it ever misfires.
pub fn is_store_packaged() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::current_exe()
            .map(|p| path_is_packaged(&p))
            .unwrap_or(false)
    })
}

/// Octa's own page in the Microsoft Store app. It shows an **Update** button
/// when a newer version is out, so a Store copy can be updated at once rather
/// than whenever the Store gets round to it. Opening it sends no request from
/// Octa, unlike the GitHub check a Store copy has to do without.
pub const STORE_PAGE_URI: &str = "ms-windows-store://pdp/?ProductId=9PF9BVRT9PX4";

fn path_is_packaged(path: &Path) -> bool {
    // Substring match (not Path::components) so the test is portable: on Linux
    // a backslash path is a single component, but a lowercased substring scan
    // works regardless of separator, and on Windows current_exe() yields a real
    // `...\WindowsApps\...` path.
    let lower = path.to_string_lossy().to_ascii_lowercase();
    lower.contains(r"\windowsapps\") || lower.contains("/windowsapps/")
}

/// `<PackageFamilyName>!Octa` when running from the Store package, the id its
/// toasts and taskbar entry carry. `None` for the zip / installer copy.
#[cfg(windows)]
pub fn package_app_id() -> Option<String> {
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    let mut len: u32 = 0;
    // SAFETY: a null buffer with length 0 asks for the required length; an
    // unpackaged process gets APPMODEL_ERROR_NO_PACKAGE instead.
    let rc = unsafe { GetCurrentPackageFamilyName(&mut len, std::ptr::null_mut()) };
    if rc != ERROR_INSUFFICIENT_BUFFER || len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: `buf` holds `len` UTF-16 units, the size the first call asked for.
    let rc = unsafe { GetCurrentPackageFamilyName(&mut len, buf.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let family = String::from_utf16_lossy(&buf[..(len as usize).saturating_sub(1)]);
    Some(format!("{family}!Octa"))
}

/// Register an app identity for toasts from an unpackaged exe:
/// `HKCU\Software\Classes\AppUserModelId\<id>` with `DisplayName` and
/// `IconUri`. Per user, no admin.
#[cfg(windows)]
pub fn register_app_user_model_id(
    id: &str,
    display_name: &str,
    icon: Option<&Path>,
) -> std::io::Result<()> {
    let key = format!(r"Software\Classes\AppUserModelId\{id}");
    reg_set_sz(&key, "DisplayName", display_name)?;
    if let Some(icon) = icon {
        reg_set_sz(&key, "IconUri", &icon.to_string_lossy())?;
    }
    Ok(())
}

#[cfg(windows)]
fn reg_set_sz(subkey: &str, name: &str, value: &str) -> std::io::Result<()> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (k, n, v) = (wide(subkey), wide(name), wide(value));
    // SAFETY: three NUL-terminated UTF-16 buffers that outlive the call; the
    // size is in bytes and includes the terminator, as REG_SZ requires.
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            REG_SZ,
            v.as_ptr().cast(),
            (v.len() * 2) as u32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(rc as i32))
    }
}

/// Set the window's *large* icon, the one the Windows taskbar and Alt+Tab
/// show. egui sets only the small (title-bar) one, so without this the
/// taskbar keeps the icon baked into octa.exe whatever colour the user picked.
/// Windows ignores it for the Store copy, whose taskbar icon comes from the
/// package. No-op on other platforms.
pub fn set_taskbar_icon(
    window: &impl raw_window_handle::HasWindowHandle,
    rgba: &[u8],
    width: u32,
    height: u32,
) {
    #[cfg(windows)]
    {
        use raw_window_handle::RawWindowHandle;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateIcon, ICON_BIG, SendMessageW, WM_SETICON,
        };
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(win) = handle.as_raw() else {
            return;
        };
        // winit's own recipe (`RgbaIcon::into_windows_icon`): BGRA colour
        // bits plus an AND mask derived from the alpha.
        let mut bgra = rgba.to_vec();
        let mut and_mask = Vec::with_capacity(bgra.len() / 4);
        for px in bgra.as_chunks_mut::<4>().0 {
            and_mask.push(px[3].wrapping_sub(u8::MAX));
            px.swap(0, 2);
        }
        // SAFETY: both buffers live across the call and hold width * height
        // pixels, as CreateIcon expects for 32 bits per pixel.
        let icon = unsafe {
            CreateIcon(
                std::ptr::null_mut(),
                width as i32,
                height as i32,
                1,
                32,
                and_mask.as_ptr(),
                bgra.as_ptr(),
            )
        };
        if icon.is_null() {
            return;
        }
        // ponytail: the previous big icon is not destroyed, one small HICON
        // per colour change for the life of the process.
        // SAFETY: a valid HWND from the live window and an HICON just created.
        unsafe {
            SendMessageW(
                win.hwnd.get() as _,
                WM_SETICON,
                ICON_BIG as usize,
                icon as isize,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, rgba, width, height);
    }
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
