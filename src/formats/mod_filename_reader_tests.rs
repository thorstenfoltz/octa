use super::*;

#[test]
fn dockerfile_matches_text_reader_by_filename() {
    let reg = FormatRegistry::new();
    // No extension, no file on disk needed: the filename step returns before sniff.
    let r = reg
        .reader_for_path(std::path::Path::new("Dockerfile"))
        .unwrap();
    assert_eq!(r.name(), "Text");
    let r2 = reg
        .reader_for_path(std::path::Path::new("Dockerfile.dev"))
        .unwrap();
    assert_eq!(r2.name(), "Text");
    let r3 = reg
        .reader_for_path(std::path::Path::new("Containerfile"))
        .unwrap();
    assert_eq!(r3.name(), "Text");
}

#[test]
fn filename_reader_name_is_case_insensitive_on_stem() {
    assert_eq!(filename_reader_name("dockerfile"), Some("Text"));
    assert_eq!(filename_reader_name("Dockerfile.prod"), Some("Text"));
    assert_eq!(filename_reader_name("notes.txt"), None);
    assert_eq!(filename_reader_name("access.log.1"), Some("Log"));
    assert_eq!(filename_reader_name("syslog"), Some("Log"));
}
