//! The history a Delta Lake or Apache Iceberg table keeps of itself: one
//! entry per version, newest first, read from the table's own log.
//!
//! DuckDB's extensions read any version (`delta_scan(dir, version = n)`,
//! `iceberg_scan(dir, snapshot_from_id = id)`) but none lists which versions
//! exist, so this reads the files the two formats define. No DuckDB, no
//! network. Reading a version is `lakehouse_reader::read_version`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::formats::lakehouse_reader::LakehouseKind;

/// Which version of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionId {
    /// A Delta commit number.
    Delta(i64),
    /// An Iceberg snapshot: its id (what DuckDB reads by) and its sequence
    /// number (what people count by).
    Iceberg { snapshot_id: i64, sequence: i64 },
}

/// One entry of a table's history.
#[derive(Debug, Clone, PartialEq)]
pub struct TableVersion {
    pub id: VersionId,
    /// When it was committed, UTC epoch milliseconds.
    pub when_ms: Option<i64>,
    /// `WRITE`, `MERGE`, `DELETE` (Delta), `append`, `overwrite` (Iceberg).
    pub operation: Option<String>,
    /// Row counts, where the writer recorded them.
    pub added: Option<u64>,
    pub removed: Option<u64>,
    pub updated: Option<u64>,
    /// The version "What changed" compares against: Delta n-1, Iceberg the
    /// parent snapshot. `None` for the first one.
    pub previous: Option<VersionId>,
}

/// A table's history, newest first, capped; `total` counts every version.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    pub versions: Vec<TableVersion>,
    pub total: usize,
}

/// Versions [`list`] returns at most: a streaming job commits thousands.
pub const LIST_LIMIT: usize = 500;

/// The history of the table in `dir`, newest first, at most `limit` entries.
pub fn list(dir: &Path, kind: LakehouseKind, limit: usize) -> Result<History> {
    match kind {
        LakehouseKind::Delta => delta(dir, limit),
        LakehouseKind::Iceberg => iceberg(dir, limit),
        LakehouseKind::Parts(_) => bail!("a folder of part files keeps no version history"),
    }
}

fn delta(dir: &Path, limit: usize) -> Result<History> {
    let log = dir.join("_delta_log");
    let mut commits: Vec<(i64, PathBuf)> = std::fs::read_dir(&log)
        .with_context(|| format!("reading {}", log.display()))?
        .flatten()
        .filter_map(|entry| {
            let version = delta_version_of(&entry.file_name().to_string_lossy())?;
            Some((version, entry.path()))
        })
        .collect();
    commits.sort_by_key(|c| std::cmp::Reverse(c.0));
    let total = commits.len();
    let versions = commits
        .iter()
        .take(limit)
        .map(|(n, path)| delta_entry(*n, path))
        .collect();
    Ok(History { versions, total })
}

/// `00000000000000000012.json` -> 12. Checkpoints and other files -> `None`.
fn delta_version_of(name: &str) -> Option<i64> {
    let stem = name.strip_suffix(".json")?;
    if stem.len() != 20 || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    stem.parse().ok()
}

fn delta_entry(version: i64, path: &Path) -> TableVersion {
    let info = commit_info(path).unwrap_or(Value::Null);
    let operation = info
        .get("operation")
        .and_then(Value::as_str)
        .map(str::to_string);
    let metric = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| info.get("operationMetrics")?.get(*k).and_then(json_u64))
    };
    // Spark's `numOutputRows` is every row written; only for a plain write
    // is that the rows added (a MERGE also rewrites the rows it copies).
    let appends = operation.as_deref().is_some_and(|op| {
        op.starts_with("WRITE")
            || op.starts_with("CREATE")
            || op.starts_with("REPLACE")
            || op == "STREAMING UPDATE"
    });
    let added = metric(&[
        "num_added_rows",
        "num_target_rows_inserted",
        "numTargetRowsInserted",
    ])
    .or_else(|| {
        if appends {
            metric(&["numOutputRows"])
        } else {
            None
        }
    });
    TableVersion {
        id: VersionId::Delta(version),
        when_ms: info
            .get("inCommitTimestamp")
            .and_then(Value::as_i64)
            .or_else(|| info.get("timestamp").and_then(Value::as_i64))
            .or_else(|| file_time_ms(path)),
        added,
        removed: metric(&[
            "num_deleted_rows",
            "num_target_rows_deleted",
            "numTargetRowsDeleted",
            "numDeletedRows",
        ]),
        updated: metric(&[
            "num_updated_rows",
            "num_target_rows_updated",
            "numTargetRowsUpdated",
            "numUpdatedRows",
        ]),
        previous: (version > 0).then(|| VersionId::Delta(version - 1)),
        operation,
    }
}

/// The `commitInfo` action of one commit file. Read line by line and stop
/// there: the `add` lines after it can be large.
fn commit_info(path: &Path) -> Option<Value> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    for line in std::io::BufReader::new(file).lines() {
        let line = line.ok()?;
        if !line.contains("\"commitInfo\"") {
            continue;
        }
        if let Ok(mut v) = serde_json::from_str::<Value>(&line)
            && let Some(info) = v.get_mut("commitInfo")
        {
            return Some(info.take());
        }
    }
    None
}

fn file_time_ms(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    i64::try_from(ms).ok()
}

/// A metric as a number: Spark writes `"120"`, delta-rs and Iceberg vary.
fn json_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn iceberg(dir: &Path, limit: usize) -> Result<History> {
    let path = iceberg_metadata_file(dir)?;
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let meta: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let snapshots: &[Value] = meta
        .get("snapshots")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let sequence_of = |id: i64| {
        snapshots
            .iter()
            .enumerate()
            .find(|(_, s)| s.get("snapshot-id").and_then(Value::as_i64) == Some(id))
            .map(|(i, s)| sequence(s, i))
    };
    let mut versions: Vec<TableVersion> = snapshots
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let snapshot_id = s.get("snapshot-id")?.as_i64()?;
            let summary = s.get("summary");
            let count = |k: &str| summary?.get(k).and_then(json_u64);
            let parent = s.get("parent-snapshot-id").and_then(Value::as_i64);
            Some(TableVersion {
                id: VersionId::Iceberg {
                    snapshot_id,
                    sequence: sequence(s, i),
                },
                when_ms: s.get("timestamp-ms").and_then(Value::as_i64),
                operation: summary
                    .and_then(|m| m.get("operation"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                added: count("added-records"),
                removed: count("deleted-records"),
                updated: None,
                previous: parent.and_then(|p| {
                    sequence_of(p).map(|seq| VersionId::Iceberg {
                        snapshot_id: p,
                        sequence: seq,
                    })
                }),
            })
        })
        .collect();
    versions.sort_by_key(|v| {
        std::cmp::Reverse(match v.id {
            VersionId::Iceberg { sequence, .. } => sequence,
            VersionId::Delta(n) => n,
        })
    });
    let total = versions.len();
    versions.truncate(limit);
    Ok(History { versions, total })
}

/// A snapshot's sequence number; format v1 tables have none, so the
/// snapshot's 1-based position stands in.
fn sequence(snapshot: &Value, index: usize) -> i64 {
    snapshot
        .get("sequence-number")
        .and_then(Value::as_i64)
        .unwrap_or(index as i64 + 1)
}

/// The metadata file DuckDB's `iceberg_scan` reads for a table folder:
/// `metadata/version-hint.text` names it, tried as `v<hint>.metadata.json`,
/// then `<hint>.metadata.json`, then `<hint>` itself.
fn iceberg_metadata_file(dir: &Path) -> Result<PathBuf> {
    let meta = dir.join("metadata");
    let hint_path = meta.join("version-hint.text");
    let hint = std::fs::read_to_string(&hint_path).with_context(|| {
        format!(
            "reading {} (an Iceberg table folder needs it to say which metadata file is current)",
            hint_path.display()
        )
    })?;
    let hint = hint.trim();
    [
        format!("v{hint}.metadata.json"),
        format!("{hint}.metadata.json"),
        hint.to_string(),
    ]
    .into_iter()
    .map(|name| meta.join(name))
    .find(|p| p.is_file())
    .with_context(|| {
        format!(
            "no metadata file for version hint `{hint}` in {}",
            meta.display()
        )
    })
}

#[cfg(test)]
#[path = "lakehouse_history_tests.rs"]
mod tests;
