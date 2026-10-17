use super::{binary_on_path, executable_in};
use std::ffi::OsString;

/// The probe's one subtle bit: a *file* named `zenity` that nobody can run
/// must not count, or the missing-dialog warning is suppressed on exactly
/// the systems that need it.
#[test]
fn only_an_executable_counts_as_on_path() {
    assert!(binary_on_path("sh"), "POSIX guarantees /bin/sh");
    assert!(!binary_on_path("octa-no-such-binary-2f9c"));

    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("zenity"), "not executable").unwrap();
    let only_this_dir = OsString::from(dir.path());
    assert!(!executable_in(&only_this_dir, "zenity"));
    assert!(!executable_in(&only_this_dir, "nothing-here"));
    assert!(executable_in(&OsString::from("/bin:/usr/bin"), "sh"));
}
