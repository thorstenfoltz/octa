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

fn path_is_packaged(path: &Path) -> bool {
    // Substring match (not Path::components) so the test is portable: on Linux
    // a backslash path is a single component, but a lowercased substring scan
    // works regardless of separator, and on Windows current_exe() yields a real
    // `...\WindowsApps\...` path.
    let lower = path.to_string_lossy().to_ascii_lowercase();
    lower.contains(r"\windowsapps\") || lower.contains("/windowsapps/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_windowsapps_install() {
        assert!(path_is_packaged(Path::new(
            r"C:\Program Files\WindowsApps\Octa_1.2.3.0_x64__abc123\octa.exe"
        )));
    }

    #[test]
    fn ignores_non_store_paths() {
        assert!(!path_is_packaged(Path::new(
            r"C:\Program Files\Octa\octa.exe"
        )));
        assert!(!path_is_packaged(Path::new(
            r"C:\Users\me\Downloads\octa.exe"
        )));
        assert!(!path_is_packaged(Path::new("/usr/local/bin/octa")));
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(path_is_packaged(Path::new(
            r"C:\Program Files\windowsapps\Octa\octa.exe"
        )));
    }
}
