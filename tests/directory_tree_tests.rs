use std::collections::HashSet;
use std::fs;

use octa::ui::directory_tree::{DirectoryTreeState, read_sorted_dir};

#[test]
fn directories_sort_before_files() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("zoo")).unwrap();
    fs::write(tmp.path().join("alpha.txt"), "").unwrap();
    fs::write(tmp.path().join("beta.txt"), "").unwrap();

    let entries = read_sorted_dir(tmp.path()).unwrap();
    let names: Vec<String> = entries
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["zoo", "alpha.txt", "beta.txt"]);
}

#[test]
fn state_starts_with_root_expanded() {
    let tmp = tempfile::tempdir().unwrap();
    let state = DirectoryTreeState::new(tmp.path().to_path_buf());
    let want: HashSet<_> = [tmp.path().to_path_buf()].into_iter().collect();
    assert_eq!(state.expanded, want);
    assert_eq!(state.root, tmp.path());
}

#[test]
fn sort_is_case_insensitive() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Banana"), "").unwrap();
    fs::write(tmp.path().join("apple"), "").unwrap();
    fs::write(tmp.path().join("cherry"), "").unwrap();

    let entries = read_sorted_dir(tmp.path()).unwrap();
    let names: Vec<String> = entries
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["apple", "Banana", "cherry"]);
}

#[test]
fn nonexistent_dir_errors() {
    let result = read_sorted_dir(std::path::Path::new("/definitely/not/a/real/path"));
    assert!(result.is_err());
}
