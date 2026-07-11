//! Unit tests for [`directory_tree`](directory_tree). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn sort_puts_directories_first() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("zdir")).unwrap();
    std::fs::write(tmp.path().join("afile.txt"), "").unwrap();
    std::fs::write(tmp.path().join("bfile.txt"), "").unwrap();
    let out = read_sorted_dir(tmp.path()).unwrap();
    let names: Vec<String> = out
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["zdir", "afile.txt", "bfile.txt"]);
}

#[test]
fn state_has_root_expanded_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let s = DirectoryTreeState::new(tmp.path().to_path_buf());
    assert!(s.expanded.contains(&tmp.path().to_path_buf()));
}

#[test]
fn dockerfile_is_listed_even_with_filter() {
    let mut set = std::collections::HashSet::new();
    set.insert("csv".to_string());
    let allowed = Some(&set);
    // A known filename with no extension is shown despite the filter.
    assert!(file_is_listed(
        std::path::Path::new("/x/Dockerfile"),
        allowed
    ));
    assert!(file_is_listed(
        std::path::Path::new("/x/Dockerfile.dev"),
        allowed
    ));
    // A genuinely unknown extension-less file stays hidden.
    assert!(!file_is_listed(
        std::path::Path::new("/x/randomfile"),
        allowed
    ));
    // Normal extension filtering still works.
    assert!(file_is_listed(std::path::Path::new("/x/data.csv"), allowed));
    assert!(!file_is_listed(
        std::path::Path::new("/x/data.parquet"),
        allowed
    ));
}

#[test]
fn hidden_and_filtered_files_are_not_range_selectable() {
    // Shift-range selection walks the raw directory listing, which still holds
    // dotfiles and filtered-out files that the draw loop skips. Only rows the
    // user can actually see may be swept into a selection.
    let tmp = tempfile::tempdir().unwrap();
    let visible = tmp.path().join("data.csv");
    let hidden = tmp.path().join(".secret.csv");
    let filtered = tmp.path().join("notes.xyz");
    let dir = tmp.path().join("sub");
    std::fs::write(&visible, "a\n").unwrap();
    std::fs::write(&hidden, "a\n").unwrap();
    std::fs::write(&filtered, "a\n").unwrap();
    std::fs::create_dir(&dir).unwrap();

    let allowed: HashSet<String> = ["csv".to_string()].into_iter().collect();
    let exts = Some(&allowed);

    assert!(file_row_visible(&visible, exts));
    assert!(
        !file_row_visible(&hidden, exts),
        "a dotfile is never selectable"
    );
    assert!(
        !file_row_visible(&filtered, exts),
        "a filtered-out file is never selectable"
    );
    assert!(
        !file_row_visible(&dir, exts),
        "a directory is not a file row"
    );
}

#[test]
fn unfiltered_tree_still_hides_dotfiles_from_selection() {
    // With no extension filter every file is listed, but dotfiles stay hidden.
    let tmp = tempfile::tempdir().unwrap();
    let visible = tmp.path().join("anything.bin");
    let hidden = tmp.path().join(".gitignore");
    std::fs::write(&visible, "a\n").unwrap();
    std::fs::write(&hidden, "a\n").unwrap();

    assert!(file_row_visible(&visible, None));
    assert!(!file_row_visible(&hidden, None));
}
