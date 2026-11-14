use super::*;
use crate::formats::{ThreadRowCap, read_table_auto};

fn long_csv(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "id\n1\n2\n3\n4\n5\n").unwrap();
    path
}

#[test]
fn a_read_the_cap_stopped_is_remembered_once() {
    take();
    let dir = tempfile::tempdir().unwrap();
    let path = long_csv(dir.path(), "long.csv");
    let _cap = ThreadRowCap::new(2);
    read_table_auto(&path, None, u64::MAX).unwrap();
    read_table_auto(&path, None, u64::MAX).unwrap();
    let reads = take();
    assert_eq!(reads.len(), 1, "{reads:?}");
    assert_eq!(reads[0].path, path);
    assert_eq!(reads[0].rows, 2);
    assert!(take().is_empty(), "taken once");
}

#[test]
fn a_whole_read_leaves_nothing() {
    take();
    let dir = tempfile::tempdir().unwrap();
    let path = long_csv(dir.path(), "long.csv");
    let _cap = ThreadRowCap::new(5);
    read_table_auto(&path, None, u64::MAX).unwrap();
    assert!(take().is_empty());
}

/// A GUI session reads for as long as it runs and nobody ever takes these.
#[test]
fn the_list_stays_small() {
    take();
    let dir = tempfile::tempdir().unwrap();
    let _cap = ThreadRowCap::new(2);
    for i in 0..MAX_REMEMBERED + 3 {
        let path = long_csv(dir.path(), &format!("f{i}.csv"));
        read_table_auto(&path, None, u64::MAX).unwrap();
    }
    assert_eq!(take().len(), MAX_REMEMBERED);
}
