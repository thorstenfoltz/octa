//! Offline tests for the lakehouse history. Included via `#[path]`.

use super::*;
use crate::formats::lakehouse_reader::PartsFamily;
use serde_json::json;

/// A `_delta_log` with one commit per entry of `commit_infos`.
fn delta_dir(commit_infos: &[serde_json::Value]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("_delta_log");
    std::fs::create_dir(&log).unwrap();
    for (n, info) in commit_infos.iter().enumerate() {
        let body = format!(
            "{}\n{}\n",
            json!({"commitInfo": info}),
            json!({"add": {"path": format!("p{n}.parquet"), "size": 1}})
        );
        std::fs::write(log.join(format!("{n:020}.json")), body).unwrap();
    }
    dir
}

#[test]
fn delta_lists_newest_first_with_operation_and_rows() {
    let dir = delta_dir(&[
        json!({"timestamp": 1_700_000_000_000i64, "operation": "WRITE", "operationMetrics": {"num_added_rows": 8}}),
        json!({"timestamp": 1_700_000_100_000i64, "operation": "MERGE", "operationMetrics": {
            "numTargetRowsInserted": "3", "numTargetRowsUpdated": "2", "numTargetRowsDeleted": "1"}}),
    ]);
    let h = list(dir.path(), LakehouseKind::Delta, LIST_LIMIT).unwrap();
    assert_eq!(h.total, 2);
    let v1 = &h.versions[0];
    assert_eq!(v1.id, VersionId::Delta(1));
    assert_eq!(v1.operation.as_deref(), Some("MERGE"));
    assert_eq!(
        (v1.added, v1.removed, v1.updated),
        (Some(3), Some(1), Some(2))
    );
    assert_eq!(v1.previous, Some(VersionId::Delta(0)));
    let v0 = &h.versions[1];
    assert_eq!(v0.added, Some(8));
    assert_eq!(v0.previous, None);
    assert_eq!(v0.when_ms, Some(1_700_000_000_000));
}

#[test]
fn delta_spark_write_counts_output_rows_but_merge_does_not() {
    let dir = delta_dir(&[
        json!({"timestamp": 1, "operation": "WRITE", "operationMetrics": {"numOutputRows": "5"}}),
        json!({"timestamp": 2, "operation": "MERGE", "operationMetrics": {"numOutputRows": "900"}}),
    ]);
    let h = list(dir.path(), LakehouseKind::Delta, LIST_LIMIT).unwrap();
    assert_eq!(h.versions[1].added, Some(5));
    assert_eq!(
        h.versions[0].added, None,
        "a MERGE's output rows include copied ones"
    );
}

#[test]
fn delta_prefers_in_commit_timestamp() {
    let dir = delta_dir(&[json!({"timestamp": 1, "inCommitTimestamp": 2, "operation": "WRITE"})]);
    let h = list(dir.path(), LakehouseKind::Delta, LIST_LIMIT).unwrap();
    assert_eq!(h.versions[0].when_ms, Some(2));
}

#[test]
fn delta_without_commit_info_still_lists_the_version() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("_delta_log");
    std::fs::create_dir(&log).unwrap();
    std::fs::write(
        log.join(format!("{:020}.json", 0)),
        "{\"protocol\":{\"minReaderVersion\":1}}\n",
    )
    .unwrap();
    let h = list(dir.path(), LakehouseKind::Delta, LIST_LIMIT).unwrap();
    assert_eq!(h.versions.len(), 1);
    assert_eq!(h.versions[0].operation, None);
    assert!(
        h.versions[0].when_ms.is_some(),
        "falls back to the file time"
    );
}

#[test]
fn delta_caps_the_list_but_counts_everything() {
    let infos: Vec<_> = (0..5)
        .map(|i| json!({"timestamp": i, "operation": "WRITE"}))
        .collect();
    let dir = delta_dir(&infos);
    let h = list(dir.path(), LakehouseKind::Delta, 2).unwrap();
    assert_eq!(h.total, 5);
    let ids: Vec<VersionId> = h.versions.iter().map(|v| v.id.clone()).collect();
    assert_eq!(ids, [VersionId::Delta(4), VersionId::Delta(3)]);
    assert_eq!(
        h.versions[1].previous,
        Some(VersionId::Delta(2)),
        "previous works past the cap"
    );
}

#[test]
fn delta_ignores_checkpoints_and_other_files() {
    let dir = delta_dir(&[json!({"timestamp": 1, "operation": "WRITE"})]);
    let log = dir.path().join("_delta_log");
    std::fs::write(log.join(format!("{:020}.checkpoint.parquet", 0)), b"x").unwrap();
    std::fs::write(log.join("_last_checkpoint"), b"{}").unwrap();
    assert_eq!(
        list(dir.path(), LakehouseKind::Delta, LIST_LIMIT)
            .unwrap()
            .total,
        1
    );
}

fn iceberg_dir(hint: &str, file_name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let meta = dir.path().join("metadata");
    std::fs::create_dir(&meta).unwrap();
    std::fs::write(meta.join("version-hint.text"), hint).unwrap();
    let body = json!({"format-version": 2, "snapshots": [
        {"snapshot-id": 11, "sequence-number": 1, "timestamp-ms": 1000,
         "summary": {"operation": "append", "added-records": "3"}},
        {"snapshot-id": 22, "parent-snapshot-id": 11, "sequence-number": 2, "timestamp-ms": 2000,
         "summary": {"operation": "delete", "deleted-records": "1"}}
    ]});
    std::fs::write(meta.join(file_name), body.to_string()).unwrap();
    dir
}

#[test]
fn iceberg_reads_snapshots_through_a_numeric_hint() {
    let dir = iceberg_dir("2", "v2.metadata.json");
    let h = list(dir.path(), LakehouseKind::Iceberg, LIST_LIMIT).unwrap();
    assert_eq!(h.total, 2);
    let newest = &h.versions[0];
    assert_eq!(
        newest.id,
        VersionId::Iceberg {
            snapshot_id: 22,
            sequence: 2
        }
    );
    assert_eq!(newest.operation.as_deref(), Some("delete"));
    assert_eq!(newest.removed, Some(1));
    assert_eq!(
        newest.previous,
        Some(VersionId::Iceberg {
            snapshot_id: 11,
            sequence: 1
        })
    );
    assert_eq!(h.versions[1].added, Some(3));
    assert_eq!(h.versions[1].previous, None);
}

#[test]
fn iceberg_hint_may_name_the_file() {
    let dir = iceberg_dir("00001-abc", "00001-abc.metadata.json");
    assert_eq!(
        list(dir.path(), LakehouseKind::Iceberg, LIST_LIMIT)
            .unwrap()
            .total,
        2
    );
}

#[test]
fn iceberg_without_a_hint_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("metadata")).unwrap();
    let err = list(dir.path(), LakehouseKind::Iceberg, LIST_LIMIT).unwrap_err();
    assert!(format!("{err:#}").contains("version-hint.text"), "{err:#}");
}

#[test]
fn a_parts_folder_has_no_history() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        list(
            dir.path(),
            LakehouseKind::Parts(PartsFamily::Parquet),
            LIST_LIMIT
        )
        .is_err()
    );
}
