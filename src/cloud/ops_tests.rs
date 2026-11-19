use super::*;
use crate::cloud::ObjectStoreProvider;
use object_store::{ObjectStore, memory::InMemory};
use std::sync::Arc;

fn provider() -> ObjectStoreProvider {
    let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
    ObjectStoreProvider::new(store)
}

fn seeded() -> ObjectStoreProvider {
    let p = provider();
    p.put("data/a.csv", b"a".to_vec()).unwrap();
    p.put("data/nested/b.csv", b"bb".to_vec()).unwrap();
    p.put("other.txt", b"o".to_vec()).unwrap();
    p
}

fn keys(p: &ObjectStoreProvider) -> Vec<String> {
    let (mut e, _) = p.list_recursive("", 100).unwrap();
    e.sort_by(|a, b| a.key.cmp(&b.key));
    e.into_iter().map(|e| e.key).collect()
}

#[test]
fn map_key_keeps_the_folder_shape() {
    assert_eq!(map_key("data/", "backup/", "data/a.csv"), "backup/a.csv");
    assert_eq!(
        map_key("data/", "backup/", "data/nested/b.csv"),
        "backup/nested/b.csv"
    );
    // Destination without a trailing slash behaves the same.
    assert_eq!(map_key("data/", "backup", "data/a.csv"), "backup/a.csv");
    // Copying to the bucket root drops the source prefix.
    assert_eq!(map_key("data/", "", "data/nested/b.csv"), "nested/b.csv");
}

#[test]
fn dest_in_folder_keeps_each_name_side_by_side() {
    // Several picked files land next to each other under the target,
    // whatever folders they came from: this is a multi-selection, not a
    // tree copy.
    assert_eq!(dest_in_folder("backup/", "data/a.csv"), "backup/a.csv");
    assert_eq!(
        dest_in_folder("backup/", "other/deep/b.csv"),
        "backup/b.csv"
    );
    // A target without the trailing slash behaves the same.
    assert_eq!(dest_in_folder("backup", "data/a.csv"), "backup/a.csv");
    // Bucket root.
    assert_eq!(dest_in_folder("", "data/a.csv"), "a.csv");
    // Two sources with the same basename collide, and that is the honest
    // outcome: the caller warns, it does not silently rename.
    assert_eq!(
        dest_in_folder("backup/", "x/report.csv"),
        dest_in_folder("backup/", "y/report.csv")
    );
}

#[test]
fn copy_one_object_within_a_store() {
    let p = seeded();
    let r = copy(&p, "other.txt", &p, "copied.txt", true, 1).unwrap();
    assert_eq!(r.objects, 1);
    assert!(r.server_side);
    assert_eq!(p.get("copied.txt").unwrap(), b"o");
    // The original survives.
    assert_eq!(p.get("other.txt").unwrap(), b"o");
}

#[test]
fn copy_a_folder_recreates_the_tree() {
    let p = seeded();
    let r = copy(&p, "data/", &p, "backup/", true, 1).unwrap();
    assert_eq!(r.objects, 2);
    assert_eq!(r.bytes, 3);
    assert!(keys(&p).contains(&"backup/a.csv".to_string()));
    assert!(keys(&p).contains(&"backup/nested/b.csv".to_string()));
}

#[test]
fn move_deletes_the_source_only_after_copying() {
    let p = seeded();
    move_(&p, "data/", &p, "archive/", true, 1).unwrap();
    let k = keys(&p);
    assert!(k.contains(&"archive/a.csv".to_string()));
    assert!(k.contains(&"archive/nested/b.csv".to_string()));
    assert!(!k.iter().any(|k| k.starts_with("data/")));
}

#[test]
fn moving_one_object_in_one_store_goes_through_rename() {
    let p = seeded();
    let r = move_(&p, "data/a.csv", &p, "archive/a.csv", true, 1).unwrap();
    assert_eq!(r.objects, 1);
    assert_eq!(p.get("archive/a.csv").unwrap(), b"a");
    assert!(!keys(&p).contains(&"data/a.csv".to_string()));
}

#[test]
fn copy_across_two_stores_streams_the_bytes() {
    // Different providers: no server-side copy available, so this goes
    // through the streaming path.
    let src = seeded();
    let dst = provider();
    let r = copy(&src, "data/", &dst, "landing/", false, 1).unwrap();
    assert_eq!(r.objects, 2);
    assert!(!r.server_side);
    assert_eq!(dst.get("landing/a.csv").unwrap(), b"a");
    assert_eq!(dst.get("landing/nested/b.csv").unwrap(), b"bb");
    // Source untouched by a copy.
    assert_eq!(src.get("data/a.csv").unwrap(), b"a");
}

#[test]
fn a_large_object_survives_the_chunked_copy() {
    // Bigger than one COPY_CHUNK_BYTES block, so the re-blocking in
    // `copy_across` has to stitch it back together exactly.
    let src = provider();
    let dst = provider();
    let big: Vec<u8> = (0..(provider::COPY_CHUNK_BYTES * 2 + 12345))
        .map(|i| (i % 251) as u8)
        .collect();
    src.put("big.bin", big.clone()).unwrap();
    copy(&src, "big.bin", &dst, "big.bin", false, 1).unwrap();
    assert_eq!(dst.get("big.bin").unwrap(), big);
}

#[test]
fn delete_removes_one_object_or_a_whole_folder() {
    let p = seeded();
    assert_eq!(delete(&p, "other.txt", 1).unwrap().objects, 1);
    assert!(!keys(&p).contains(&"other.txt".to_string()));

    assert_eq!(delete(&p, "data/", 1).unwrap().objects, 2);
    assert!(keys(&p).is_empty());
}

#[test]
fn copying_a_folder_into_itself_is_refused() {
    let p = seeded();
    assert!(copy(&p, "data/", &p, "data/inner/", true, 1).is_err());
    assert!(copy(&p, "data/", &p, "data/", true, 1).is_err());
    assert!(copy(&p, "other.txt", &p, "other.txt", true, 1).is_err());
    // The same key on a *different* store is fine: that is a real transfer.
    let dst = provider();
    assert!(copy(&p, "other.txt", &dst, "other.txt", false, 1).is_ok());
}

/// A folder copy, move and delete, many objects at a time: every object
/// arrives once, and the report counts them as the one-by-one loop did.
#[test]
fn a_folder_is_copied_moved_and_deleted_in_parallel() {
    let src = provider();
    for i in 0..40 {
        src.put(&format!("data/f-{i:02}.csv"), vec![b'x'; i + 1])
            .unwrap();
    }
    let dst = provider();
    let r = copy(&src, "data/", &dst, "landing/", false, 8).unwrap();
    assert_eq!((r.objects, r.bytes), (40, (1..=40).sum::<usize>() as u64));
    assert_eq!(keys(&dst).len(), 40);
    assert_eq!(dst.get("landing/f-07.csv").unwrap(), vec![b'x'; 8]);

    move_(&dst, "landing/", &dst, "archive/", true, 8).unwrap();
    assert!(keys(&dst).iter().all(|k| k.starts_with("archive/")));
    assert_eq!(keys(&dst).len(), 40);

    assert_eq!(delete(&dst, "archive/", 8).unwrap().objects, 40);
    assert!(keys(&dst).is_empty());
}

fn objects(n: usize) -> Vec<(String, u64)> {
    (0..n).map(|i| (format!("k{i:02}"), i as u64)).collect()
}

/// The pool: never more than `parallel` objects at once, but more than one.
#[test]
fn objects_overlap_up_to_the_parallel_number() {
    use std::sync::atomic::AtomicUsize;
    let (now, peak) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let (count, bytes) = each_object(&objects(24), 4, |_| {
        let n = now.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(n, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(5));
        now.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    assert_eq!((count, bytes), (24, (0..24).sum()));
    let peak = peak.load(Ordering::SeqCst);
    assert!((2..=4).contains(&peak), "peak {peak}");
}

/// A failure is the first one in listing order, and with 1 at a time nothing
/// after it is started: the one-by-one loop, exactly.
#[test]
fn the_first_failure_stops_the_rest() {
    let started = std::sync::Mutex::new(Vec::new());
    let err = each_object(&objects(10), 1, |k| {
        started.lock().unwrap().push(k.to_string());
        if k == "k03" || k == "k07" {
            bail!("refused {k}");
        }
        Ok(())
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "refused k03");
    assert_eq!(started.into_inner().unwrap(), ["k00", "k01", "k02", "k03"]);

    // In parallel the earliest failure is still the one reported.
    let err = each_object(&objects(30), 6, |k| match k {
        "k05" | "k20" => bail!("refused {k}"),
        _ => Ok(()),
    })
    .unwrap_err();
    assert_eq!(err.to_string(), "refused k05");
}
