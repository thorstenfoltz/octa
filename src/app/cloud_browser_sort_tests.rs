use super::{CloudSort, sorted_entries};
use chrono::{TimeZone, Utc};
use octa::cloud::ObjectEntry;

fn file(name: &str, size: u64, day: u32) -> ObjectEntry {
    ObjectEntry {
        name: name.to_string(),
        key: name.to_string(),
        is_prefix: false,
        size: Some(size),
        modified: Some(Utc.with_ymd_and_hms(2026, 1, day, 0, 0, 0).unwrap()),
        etag: None,
        version: None,
    }
}
fn folder(name: &str) -> ObjectEntry {
    ObjectEntry {
        name: name.to_string(),
        key: format!("{name}/"),
        is_prefix: true,
        size: None,
        modified: None,
        etag: None,
        version: None,
    }
}

#[test]
fn folders_first_then_files_by_key() {
    let entries = vec![file("b.csv", 10, 2), folder("zzz"), file("a.csv", 30, 1)];
    // Size largest: folder still first, then a.csv (30) before b.csv (10).
    let by_size = sorted_entries(&entries, CloudSort::SizeLargest);
    assert_eq!(by_size[0].name, "zzz");
    assert_eq!(by_size[1].name, "a.csv");
    assert_eq!(by_size[2].name, "b.csv");
    // Newest first: day 2 (b.csv) before day 1 (a.csv).
    let by_date = sorted_entries(&entries, CloudSort::ModifiedNewest);
    assert_eq!(by_date[0].name, "zzz");
    assert_eq!(by_date[1].name, "b.csv");
    assert_eq!(by_date[2].name, "a.csv");
}
