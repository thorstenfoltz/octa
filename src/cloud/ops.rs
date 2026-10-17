//! Copy / move / delete over cloud object storage, for one object or a whole
//! prefix ("folder").
//!
//! Object stores have no folders and no rename: a "folder" is a shared key
//! prefix, and moving one means copying every object under it and then deleting
//! the originals. This module is the one place that knows that, so the sidebar,
//! the MCP tools and the CLI all behave identically.
//!
//! Two copy paths, picked by `same_store`:
//!
//! - **Within one store** (same connection and bucket): `CloudProvider::copy`,
//!   which the backend performs server-side. No bytes reach this process, so a
//!   100 GB object costs one API call.
//! - **Across stores** (a different connection, possibly a different provider):
//!   [`provider::copy_across`], which streams the object in 8 MiB blocks
//!   straight into a multipart upload. Memory stays flat regardless of size.
//!
//! Everything here is pure plumbing over [`CloudProvider`]; permission checks
//! (the connection's own `allow_writes`) belong to the
//! callers, which know which connection a provider came from.

use anyhow::{Result, bail};

use super::provider::{self, CloudProvider};

/// Ceiling on how many objects one prefix operation touches. A folder move is
/// not resumable, so a half-finished one is worse than a refusal: we stop
/// before starting rather than partway through.
pub const MAX_BULK_OBJECTS: usize = 10_000;

/// What a copy / move / delete actually did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransferReport {
    /// Objects successfully copied / moved / deleted.
    pub objects: usize,
    /// Total bytes, as reported by the listing. `0` for a single-object
    /// operation, where no listing is done.
    pub bytes: u64,
    /// True when the backend did the copy itself and no bytes travelled
    /// through this process.
    pub server_side: bool,
}

/// Whether `key` names a prefix (folder) rather than a single object.
pub fn is_prefix(key: &str) -> bool {
    key.is_empty() || key.ends_with('/')
}

/// Destination key for `src_key` when `src_root` is being copied to `dst_root`.
///
/// Pure so the mapping is testable without a backend: this is where a folder
/// move silently flattening or duplicating a path segment would show up.
pub fn map_key(src_root: &str, dst_root: &str, src_key: &str) -> String {
    let rel = src_key.strip_prefix(src_root).unwrap_or(src_key);
    let dst = dst_root.trim_end_matches('/');
    let rel = rel.trim_start_matches('/');
    if dst.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        dst.to_string()
    } else {
        format!("{dst}/{rel}")
    }
}

/// Destination key for one source among several being copied into the same
/// target folder: the source keeps its own name under `dst_prefix`.
///
/// Separate from [`map_key`], which re-creates a *tree* under the target. When
/// the user picked five files out of different folders, the thing they meant is
/// five files side by side in the destination, not five rebuilt paths.
pub fn dest_in_folder(dst_prefix: &str, src_key: &str) -> String {
    let name = src_key
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(src_key);
    let dst = dst_prefix.trim_end_matches('/');
    if dst.is_empty() {
        name.to_string()
    } else {
        format!("{dst}/{name}")
    }
}

/// Refuse a copy/move that would write into its own source. Copying `a/` into
/// `a/b/` would keep discovering the objects it just wrote on a backend that
/// lists lazily, and even where it terminates the result is not what anyone
/// meant.
fn check_not_nested(same_store: bool, src: &str, dst: &str) -> Result<()> {
    if !same_store {
        return Ok(());
    }
    let (s, d) = (src.trim_end_matches('/'), dst.trim_end_matches('/'));
    if s == d {
        bail!("source and destination are the same object");
    }
    if is_prefix(src) && (d == s || d.starts_with(&format!("{s}/"))) {
        bail!("cannot copy a folder into itself ({src} -> {dst})");
    }
    Ok(())
}

/// List every object under `prefix`, refusing rather than truncating.
fn list_all(provider: &dyn CloudProvider, prefix: &str) -> Result<Vec<(String, u64)>> {
    let (entries, truncated) = provider.list_recursive(prefix, MAX_BULK_OBJECTS)?;
    if truncated {
        bail!(
            "{prefix} holds more than {MAX_BULK_OBJECTS} objects; \
             narrow the selection or use the provider's own bulk tooling"
        );
    }
    Ok(entries
        .into_iter()
        .filter(|e| !e.is_prefix)
        .map(|e| (e.key, e.size.unwrap_or(0)))
        .collect())
}

/// Copy one object or a whole prefix from `src` to `dst`.
///
/// `same_store` must be true only when both providers address the same bucket
/// of the same account; it selects the server-side path and enables the
/// nesting check.
pub fn copy(
    src: &dyn CloudProvider,
    src_key: &str,
    dst: &dyn CloudProvider,
    dst_key: &str,
    same_store: bool,
) -> Result<TransferReport> {
    check_not_nested(same_store, src_key, dst_key)?;

    if !is_prefix(src_key) {
        copy_one(src, src_key, dst, dst_key, same_store)?;
        return Ok(TransferReport {
            objects: 1,
            bytes: 0,
            server_side: same_store,
        });
    }

    let objects = list_all(src, src_key)?;
    let mut report = TransferReport {
        server_side: same_store,
        ..Default::default()
    };
    for (key, size) in objects {
        let target = map_key(src_key, dst_key, &key);
        copy_one(src, &key, dst, &target, same_store)?;
        report.objects += 1;
        report.bytes += size;
    }
    Ok(report)
}

fn copy_one(
    src: &dyn CloudProvider,
    src_key: &str,
    dst: &dyn CloudProvider,
    dst_key: &str,
    same_store: bool,
) -> Result<()> {
    if same_store {
        src.copy(src_key, dst_key)
    } else {
        provider::copy_across(src, src_key, dst, dst_key)
    }
}

/// Move one object or a whole prefix: copy, then delete the source.
///
/// The delete only runs once every copy has succeeded, so a failure partway
/// through leaves the source intact (and some already-written destination
/// objects, which is the recoverable direction). A single object in one store
/// goes through [`CloudProvider::rename`].
pub fn move_(
    src: &dyn CloudProvider,
    src_key: &str,
    dst: &dyn CloudProvider,
    dst_key: &str,
    same_store: bool,
) -> Result<TransferReport> {
    // One object inside one store: the provider's rename. Server-side and
    // instant on SFTP; the object stores' default is the same copy + delete
    // as below.
    if same_store && !is_prefix(src_key) {
        check_not_nested(same_store, src_key, dst_key)?;
        src.rename(src_key, dst_key)?;
        return Ok(TransferReport {
            objects: 1,
            bytes: 0,
            server_side: true,
        });
    }
    let report = copy(src, src_key, dst, dst_key, same_store)?;
    delete(src, src_key)?;
    Ok(report)
}

/// Delete one object, or every object under a prefix.
pub fn delete(provider: &dyn CloudProvider, key: &str) -> Result<TransferReport> {
    if !is_prefix(key) {
        provider.delete(key)?;
        return Ok(TransferReport {
            objects: 1,
            ..Default::default()
        });
    }
    let objects = list_all(provider, key)?;
    let mut report = TransferReport::default();
    for (k, size) in objects {
        provider.delete(&k)?;
        report.objects += 1;
        report.bytes += size;
    }
    Ok(report)
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
