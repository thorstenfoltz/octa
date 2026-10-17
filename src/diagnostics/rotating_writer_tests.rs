use super::*;

#[test]
fn rotates_when_cap_exceeded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("octa.log");
    let mut w = RotatingWriter::with_cap(&path, 100).unwrap();

    w.write_all(&[b'x'; 60]).unwrap();
    assert!(
        !path.with_extension("log.1").exists(),
        "should not rotate before the cap"
    );

    w.write_all(&[b'y'; 60]).unwrap(); // crosses 100 -> rotate
    assert!(
        path.with_extension("log.1").exists(),
        "rotated file should exist after crossing the cap"
    );
    let live = std::fs::metadata(&path).unwrap().len();
    assert!(live < 100, "live file should restart small, got {live}");
}

#[test]
fn total_size_stays_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("octa.log");
    let mut w = RotatingWriter::with_cap(&path, 100).unwrap();
    for _ in 0..50 {
        w.write_all(&[b'z'; 40]).unwrap();
    }
    let live = std::fs::metadata(&path).unwrap().len();
    let bak = std::fs::metadata(path.with_extension("log.1"))
        .map(|m| m.len())
        .unwrap_or(0);
    assert!(
        live + bak <= 2 * 100 + 40,
        "total {} exceeded bound",
        live + bak
    );
}
