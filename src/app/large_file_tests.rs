use super::*;

/// A Parquet file is judged on bytes like any other, never on its row count:
/// 50,000 rows with the threshold above the data size open normally. Once the
/// threshold drops to the size the footer states, the same file is large.
#[test]
fn parquet_is_judged_by_its_stated_data_size_not_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.parquet");
    duckdb::Connection::open_in_memory()
        .unwrap()
        .execute_batch(&format!(
            "COPY (SELECT range AS id, 'row ' || range AS name FROM range(50000)) \
             TO '{}' (FORMAT parquet)",
            path.display()
        ))
        .unwrap();
    let on_disk = std::fs::metadata(&path).unwrap().len();
    let stated = stated_data_bytes(&path).unwrap();
    assert!(stated > 0);

    let above = on_disk.max(stated) as usize + 1;
    assert_eq!(large_size(&path, above), None);
    assert_eq!(
        large_size(&path, stated as usize),
        Some(on_disk.max(stated))
    );
}

/// A CSV states nothing, so only its length on disk counts.
#[test]
fn csv_is_judged_by_its_length_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rows.csv");
    std::fs::write(&path, "a,b\n1,2\n").unwrap();
    assert_eq!(stated_data_bytes(&path), None);
    assert_eq!(large_size(&path, 9), None);
    assert_eq!(large_size(&path, 8), Some(8));
}

fn key(offset: usize) -> LargePageKey {
    LargePageKey {
        offset,
        len: LARGE_PAGE_ROWS,
        order: None,
        filter: String::new(),
    }
}

/// One fetch at a time: a request made while one runs waits behind it, the
/// newest replacing any older waiting one, so dragging the scrollbar reads
/// one page at a time instead of one per frame. Asking again for the page
/// being fetched only updates where the viewport lands.
#[test]
fn page_requests_queue_behind_the_running_fetch_newest_first() {
    // Nothing running: start, unless the page is already on screen.
    assert_eq!(
        queue_request(None, Some(&key(0)), key(1900), Landing::Forward),
        Some((key(1900), Landing::Forward))
    );
    assert_eq!(
        queue_request(None, Some(&key(0)), key(0), Landing::Forward),
        None
    );

    let mut running = LargeFetch {
        key: key(1900),
        landing: Landing::Forward,
        carried: None,
        task: crate::app::pushdown::spawn_local_task(|| (octa::data::DataTable::empty(), None)),
        next: None,
    };
    for offset in [3800, 5700, 9000] {
        assert_eq!(
            queue_request(
                Some(&mut running),
                Some(&key(0)),
                key(offset),
                Landing::Row(offset)
            ),
            None
        );
    }
    assert_eq!(running.next, Some((key(9000), Landing::Row(9000))));

    // The page being fetched again: the newest landing, nothing waiting.
    assert_eq!(
        queue_request(
            Some(&mut running),
            Some(&key(0)),
            key(1900),
            Landing::Backward
        ),
        None
    );
    assert_eq!(running.landing, Landing::Backward);
    assert_eq!(running.next, None);
}
