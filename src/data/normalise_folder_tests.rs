use super::*;
use std::sync::atomic::AtomicBool;

fn write_csv(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).expect("fixture written");
    p
}

fn opts_for(dir: &std::path::Path) -> CombineOptions {
    CombineOptions {
        root: dir.to_path_buf(),
        recursive: false,
        ignore_case: true,
        source_column: DEFAULT_SOURCE_COLUMN.to_string(),
    }
}

fn names(t: &DataTable) -> Vec<String> {
    t.columns.iter().map(|c| c.name.clone()).collect()
}

#[test]
fn drifted_files_combine_into_the_union_schema_with_provenance() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_csv(dir.path(), "jan.csv", "id,amount\n1,10\n2,20\n");
    write_csv(dir.path(), "feb.csv", "id,amount,note\n3,30,late\n");

    let r = combine_folder(&opts_for(dir.path()), &|_, _| {}, &AtomicBool::new(false))
        .expect("combine succeeds");

    assert_eq!(r.files_read, 2);
    assert_eq!(r.table.row_count(), 3, "every row from every file");
    let names = names(&r.table);
    assert!(
        names.iter().any(|n| n == "note"),
        "union schema keeps the extra column"
    );
    assert!(
        names.iter().any(|n| n == "source_file"),
        "provenance column added"
    );

    // Files are folded in SCAN order, which is sorted, so the result is the
    // same every run. `feb` before `jan` is alphabetical, not chronological:
    // a folder of parts has no calendar and sorting is the only order that
    // does not depend on how the filesystem felt that morning.
    let src_col = names
        .iter()
        .position(|n| n == "source_file")
        .expect("present");
    let stamped: Vec<String> = (0..r.table.row_count())
        .map(|row| {
            r.table
                .get(row, src_col)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        stamped,
        vec!["feb.csv", "jan.csv", "jan.csv"],
        "got {stamped:?}"
    );
}

/// A file may genuinely have a column called `source_file`, typed by a
/// person who had the same idea. Overwriting it would destroy data.
#[test]
fn a_column_called_source_file_already_present_is_not_overwritten() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_csv(dir.path(), "a.csv", "id,source_file\n1,typed by hand\n");

    let r = combine_folder(&opts_for(dir.path()), &|_, _| {}, &AtomicBool::new(false))
        .expect("combine succeeds");

    let names = names(&r.table);
    assert!(
        names.iter().any(|n| n == "source_file"),
        "the original column survives"
    );
    assert!(
        names.iter().any(|n| n.starts_with("source_file_")),
        "the provenance column is suffixed instead: {names:?}"
    );
    let original = names
        .iter()
        .position(|n| n == "source_file")
        .expect("present");
    assert_eq!(
        r.table
            .get(0, original)
            .map(|c| c.to_string())
            .unwrap_or_default(),
        "typed by hand",
        "the user's own value is untouched"
    );
}

/// The suffix is chosen across ALL inputs, not per file, or the rows from
/// the colliding file would land in a different column from the rest.
#[test]
fn the_provenance_column_is_one_column_across_every_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_csv(dir.path(), "a.csv", "id,source_file\n1,hand\n");
    write_csv(dir.path(), "b.csv", "id\n2\n");

    let r = combine_folder(&opts_for(dir.path()), &|_, _| {}, &AtomicBool::new(false))
        .expect("combine succeeds");

    let names = names(&r.table);
    let provenance: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("source_file"))
        .collect();
    assert_eq!(
        provenance.len(),
        2,
        "the original plus exactly one new: {names:?}"
    );
    let col = names
        .iter()
        .position(|n| n == "source_file_2")
        .expect("suffixed");
    let stamped: Vec<String> = (0..r.table.row_count())
        .map(|row| {
            r.table
                .get(row, col)
                .map(|c| c.to_string())
                .unwrap_or_default()
        })
        .collect();
    assert!(
        stamped.iter().all(|s| !s.is_empty()),
        "every row is stamped, whichever file it came from: {stamped:?}"
    );
}

#[test]
fn an_unreadable_file_is_named_and_skipped_not_fatal() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_csv(dir.path(), "good.csv", "id\n1\n");
    std::fs::write(dir.path().join("bad.parquet"), b"not parquet at all").expect("written");

    let r = combine_folder(&opts_for(dir.path()), &|_, _| {}, &AtomicBool::new(false))
        .expect("one bad file does not abort the run");

    assert_eq!(r.files_read, 1);
    assert_eq!(r.skipped.len(), 1);
    assert!(r.skipped[0].0.ends_with("bad.parquet"));
    assert!(
        !r.skipped[0].1.is_empty(),
        "the reason is reported, not swallowed"
    );
}

/// Cancelling stops the walk rather than running it to the end and
/// discarding the result, which is the whole point of a cancel button on a
/// folder of hundreds of files.
#[test]
fn a_cancelled_run_stops_instead_of_finishing() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_csv(dir.path(), "a.csv", "id\n1\n");
    let cancelled = AtomicBool::new(true);
    assert!(combine_folder(&opts_for(dir.path()), &|_, _| {}, &cancelled).is_err());
}

#[test]
fn an_empty_folder_is_an_error_not_an_empty_table() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(combine_folder(&opts_for(dir.path()), &|_, _| {}, &AtomicBool::new(false)).is_err());
}
