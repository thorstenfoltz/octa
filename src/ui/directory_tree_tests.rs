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
fn band_selects_rows_between_two_ys_either_order() {
    use super::indices_in_band;
    let centers = [10.0, 30.0, 50.0, 70.0];
    // Downward drag.
    assert_eq!(indices_in_band(&centers, 25.0, 55.0), vec![1, 2]);
    // Upward drag (unordered args) picks the same rows.
    assert_eq!(indices_in_band(&centers, 55.0, 25.0), vec![1, 2]);
    // Zero-height band picks nothing.
    assert_eq!(indices_in_band(&centers, 40.0, 40.0), Vec::<usize>::new());
    // Full sweep picks all.
    assert_eq!(indices_in_band(&centers, 0.0, 100.0), vec![0, 1, 2, 3]);
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

#[test]
fn a_band_selects_the_same_rows_however_far_the_list_is_scrolled() {
    // The bug this pins: the band anchor used to be a *screen* position while
    // the row rects move as the list scrolls. Auto-scrolling mid-drag then slid
    // the band off the rows it started on, so a long selection picked the wrong
    // files (and, dragging past the bottom, no files at all).
    //
    // Rows are 20pt apart in content space. The same drag, expressed in screen
    // coordinates at two different scroll offsets, must select the same rows.
    let row_content_y = [10.0_f32, 30.0, 50.0, 70.0, 90.0];

    let selection_at = |scroll: f32| -> Vec<usize> {
        // Scrolling down by `scroll` moves the content top up by that much.
        let content_top = -scroll;
        let screen_centers: Vec<f32> = row_content_y.iter().map(|y| y + content_top).collect();
        let frame = MarqueeFrame {
            // Anchor and pointer are content-space, so they do not move.
            start_y: 25.0,
            current_y: 75.0,
            content_top,
        };
        frame.contains_row(&screen_centers)
    };

    let expected = vec![1, 2, 3];
    assert_eq!(selection_at(0.0), expected, "unscrolled");
    assert_eq!(selection_at(40.0), expected, "scrolled a little");
    assert_eq!(selection_at(400.0), expected, "scrolled far past the rows");
}

#[test]
fn the_painted_band_follows_the_content_it_anchors_to() {
    // The band is drawn in screen space, so scrolling has to move it: it marks
    // rows, not a fixed region of the panel.
    let x = egui::Rangef::new(0.0, 100.0);
    let unscrolled = MarqueeFrame {
        start_y: 20.0,
        current_y: 60.0,
        content_top: 0.0,
    }
    .band_rect(x);
    let scrolled = MarqueeFrame {
        start_y: 20.0,
        current_y: 60.0,
        content_top: -30.0,
    }
    .band_rect(x);

    assert_eq!(unscrolled.top(), 20.0);
    assert_eq!(unscrolled.bottom(), 60.0);
    assert_eq!(scrolled.top(), -10.0, "moved up with the content");
    assert_eq!(scrolled.height(), unscrolled.height(), "same size");
}

#[test]
fn an_upward_band_works_like_a_downward_one() {
    let centers = [10.0_f32, 30.0, 50.0];
    let down = MarqueeFrame {
        start_y: 5.0,
        current_y: 55.0,
        content_top: 0.0,
    };
    let up = MarqueeFrame {
        start_y: 55.0,
        current_y: 5.0,
        content_top: 0.0,
    };
    assert_eq!(down.contains_row(&centers), vec![0, 1, 2]);
    assert_eq!(up.contains_row(&centers), down.contains_row(&centers));
}
