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
