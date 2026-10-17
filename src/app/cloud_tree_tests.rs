use super::{format_entry_meta, human_size};
use chrono::{TimeZone, Utc};

#[test]
fn human_size_scales_units() {
    assert_eq!(human_size(512), "512 B");
    assert_eq!(human_size(1024), "1.0 KB");
    assert_eq!(human_size(1536), "1.5 KB");
    assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
}

#[test]
fn entry_meta_shows_full_timestamp() {
    let m = Utc.with_ymd_and_hms(2026, 7, 6, 14, 8, 33).unwrap();
    // Both size and time -> full timestamp to the second, not date-only.
    assert_eq!(
        format_entry_meta(Some(1024), Some(&m)),
        "  (1.0 KB, 2026-07-06 14:08:33)"
    );
    // Time only.
    assert_eq!(format_entry_meta(None, Some(&m)), "  (2026-07-06 14:08:33)");
    // Size only.
    assert_eq!(format_entry_meta(Some(512), None), "  (512 B)");
    // Neither.
    assert_eq!(format_entry_meta(None, None), "");
}
