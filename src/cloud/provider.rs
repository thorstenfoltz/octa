//! The sync `CloudProvider` trait and an `object_store`-backed implementation.
//!
//! `object_store`'s trait is async; [`ObjectStoreProvider`] runs each call to
//! completion on the shared [`crate::cloud::runtime`] so the rest of octa stays
//! sync. `list` uses `list_with_delimiter` (one directory level: sub-prefixes
//! become folders, objects become files with size + last-modified), which is
//! what the browser needs per node expansion.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
// `ObjectStoreExt` provides the `dyn`-compatible `get`/`put` over a trait
// object; the core `ObjectStore` trait's versions are RPITIT and not callable
// through `Arc<dyn ObjectStore>`.
use object_store::{ObjectStore, ObjectStoreExt, PutPayload, path::Path as ObjPath};

use super::runtime;

/// One entry in a cloud listing: a folder (prefix) or a file (object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectEntry {
    /// Display name (last path segment), e.g. `file.parquet` or `subdir`.
    pub name: String,
    /// Full key from the bucket root, e.g. `path/to/file.parquet`. For a
    /// folder this ends with `/`.
    pub key: String,
    /// True for a folder (common prefix), false for a file (object).
    pub is_prefix: bool,
    /// Object size in bytes (files only).
    pub size: Option<u64>,
    /// Last-modified timestamp (files only).
    pub modified: Option<DateTime<Utc>>,
    /// Provider ETag (files only, where the backend returns one).
    pub etag: Option<String>,
    /// Object version id (files only; versioned buckets only).
    pub version: Option<String>,
}

/// Sync interface every cloud provider implements. `prefix` and `key` are
/// keys relative to the bucket root (`""` = bucket root).
pub trait CloudProvider: Send + Sync {
    /// List one directory level under `prefix`.
    fn list(&self, prefix: &str) -> Result<Vec<ObjectEntry>>;
    /// List *everything* under `prefix` (recursive, flat: no folder entries),
    /// stopping after `cap` objects. The bool is true when the cap was hit.
    fn list_recursive(&self, prefix: &str, cap: usize) -> Result<(Vec<ObjectEntry>, bool)> {
        let _ = (prefix, cap);
        anyhow::bail!("recursive listing is not supported by this provider")
    }
    /// [`Self::list_recursive`] that also stops, reporting truncated, as soon
    /// as `stop` is raised. The sidebar search's Cancel uses it: a big bucket
    /// is one long stream, and waiting for it to end is not cancelling.
    fn list_recursive_until(
        &self,
        prefix: &str,
        cap: usize,
        stop: &std::sync::atomic::AtomicBool,
    ) -> Result<(Vec<ObjectEntry>, bool)> {
        let _ = stop;
        self.list_recursive(prefix, cap)
    }
    /// Download an object's full bytes.
    fn get(&self, key: &str) -> Result<Vec<u8>>;
    /// Upload bytes to `key` (overwrites).
    fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()>;
    /// Delete one object. Deleting a key that is not there is **not** an error
    /// on most backends, and we do not make it one.
    fn delete(&self, key: &str) -> Result<()>;
    /// Copy `from` to `to` **inside this store**, server-side where the backend
    /// supports it (S3/Azure/GCS all do), so the bytes never travel through
    /// this process. Overwrites `to`.
    fn copy(&self, from: &str, to: &str) -> Result<()>;

    /// Move `from` to `to` inside this store, overwriting `to`. The default is
    /// the copy + delete every object store needs; SFTP overrides it with the
    /// server's own rename, which moves no bytes.
    fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.copy(from, to)?;
        self.delete(from)
    }

    /// Read `key` in blocks, handing each to `on_chunk`, so a large object
    /// never has to exist in memory in one piece.
    ///
    /// The default implementation is the honest non-streaming one (a single
    /// chunk holding the whole object); [`ObjectStoreProvider`] overrides it.
    fn get_streaming(
        &self,
        key: &str,
        on_chunk: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        on_chunk(&self.get(key)?)
    }

    /// Begin an upload that accepts the object in blocks. Mirrors
    /// [`Self::get_streaming`] on the write side.
    ///
    /// No default: a buffered fallback would have to hold `&dyn Self`, and
    /// silently reverting to "read it all into memory" is exactly the failure
    /// mode streaming exists to avoid.
    fn put_streaming(&self, key: &str) -> Result<Box<dyn CloudUpload + '_>>;
}

/// An in-progress streaming upload. Dropping one without calling
/// [`CloudUpload::finish`] abandons it; backends that charge for orphaned
/// multipart parts (S3, GCS) clean them up on their own lifecycle rules.
pub trait CloudUpload {
    /// Append one block. Blocks should be at least 5 MiB for S3-compatible
    /// stores, which is what [`COPY_CHUNK_BYTES`] is sized for.
    fn write_chunk(&mut self, bytes: Vec<u8>) -> Result<()>;
    /// Finish the upload and make the object visible.
    fn finish(self: Box<Self>) -> Result<()>;
}

/// Block size for streaming copies: 8 MiB, comfortably over the 5 MiB
/// minimum part size S3-compatible stores impose on all but the last part.
pub const COPY_CHUNK_BYTES: usize = 8 * 1024 * 1024;

/// Copy one object between two (possibly different) providers without holding
/// it in memory: the source is read in blocks and each block is handed straight
/// to a multipart upload on the destination.
///
/// For a copy *within* one store call [`CloudProvider::copy`] instead; that is
/// server-side and moves no bytes at all.
pub fn copy_across(
    src: &dyn CloudProvider,
    src_key: &str,
    dst: &dyn CloudProvider,
    dst_key: &str,
) -> Result<()> {
    let mut upload = dst.put_streaming(dst_key)?;
    // Re-block the source stream to COPY_CHUNK_BYTES: a provider may hand us
    // whatever chunk size its transport produced, and S3 rejects non-final
    // parts under 5 MiB.
    let mut pending: Vec<u8> = Vec::with_capacity(COPY_CHUNK_BYTES);
    let mut sink = |chunk: &[u8]| -> Result<()> {
        pending.extend_from_slice(chunk);
        while pending.len() >= COPY_CHUNK_BYTES {
            let rest = pending.split_off(COPY_CHUNK_BYTES);
            let part = std::mem::replace(&mut pending, rest);
            upload.write_chunk(part)?;
        }
        Ok(())
    };
    src.get_streaming(src_key, &mut sink)
        .with_context(|| format!("copying {src_key} to {dst_key}"))?;
    if !pending.is_empty() {
        upload.write_chunk(pending)?;
    }
    upload.finish()
}

/// Recursive listing for stores that can only list one folder at a time
/// (SFTP, and WebDAV, whose servers often refuse `Depth: infinity`). Walks
/// folders depth-first, keeps files only, stops at `cap` or when `stop` rises.
///
/// ponytail: folder symlinks are followed, so a link back up the tree would
/// loop; the depth limit stops it. Track visited real paths if that ever
/// hides files.
pub(crate) fn walk_recursive(
    list: &dyn Fn(&str) -> Result<Vec<ObjectEntry>>,
    prefix: &str,
    cap: usize,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<(Vec<ObjectEntry>, bool)> {
    const MAX_DEPTH: usize = 64;
    let mut out = Vec::new();
    let mut folders = vec![prefix.to_string()];
    while let Some(folder) = folders.pop() {
        for entry in list(&folder)? {
            if out.len() >= cap || stop.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok((out, true));
            }
            if entry.is_prefix {
                if entry.key.matches('/').count() <= MAX_DEPTH {
                    folders.push(entry.key);
                }
            } else {
                out.push(entry);
            }
        }
    }
    Ok((out, false))
}

/// A [`CloudProvider`] backed by any `object_store` implementation
/// (S3/Azure/GCS in production, InMemory/LocalFileSystem in tests).
pub struct ObjectStoreProvider {
    store: Arc<dyn ObjectStore>,
}

impl ObjectStoreProvider {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }
}

/// Last path segment of a `/`-delimited key (folder or file name).
fn last_segment(key: &str) -> String {
    key.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

impl CloudProvider for ObjectStoreProvider {
    fn list(&self, prefix: &str) -> Result<Vec<ObjectEntry>> {
        let p = if prefix.is_empty() {
            None
        } else {
            Some(ObjPath::from(prefix))
        };
        let result = runtime()
            .block_on(self.store.list_with_delimiter(p.as_ref()))
            .with_context(|| format!("listing cloud prefix {prefix:?}"))?;

        let mut entries = Vec::new();
        for cp in result.common_prefixes {
            let key = format!("{}/", cp.as_ref().trim_end_matches('/'));
            entries.push(ObjectEntry {
                name: last_segment(cp.as_ref()),
                key,
                is_prefix: true,
                size: None,
                modified: None,
                etag: None,
                version: None,
            });
        }
        for meta in result.objects {
            let key = meta.location.as_ref().to_string();
            entries.push(ObjectEntry {
                name: last_segment(&key),
                key,
                is_prefix: false,
                size: Some(meta.size),
                modified: Some(meta.last_modified),
                etag: meta.e_tag.clone(),
                version: meta.version.clone(),
            });
        }
        Ok(entries)
    }

    fn list_recursive(&self, prefix: &str, cap: usize) -> Result<(Vec<ObjectEntry>, bool)> {
        self.list_recursive_until(prefix, cap, &std::sync::atomic::AtomicBool::new(false))
    }

    fn list_recursive_until(
        &self,
        prefix: &str,
        cap: usize,
        stop: &std::sync::atomic::AtomicBool,
    ) -> Result<(Vec<ObjectEntry>, bool)> {
        use futures_util::TryStreamExt;
        let p = if prefix.is_empty() {
            None
        } else {
            Some(ObjPath::from(prefix))
        };
        runtime()
            .block_on(async {
                let mut stream = self.store.list(p.as_ref());
                let mut out: Vec<ObjectEntry> = Vec::new();
                while let Some(meta) = stream.try_next().await.map_err(anyhow::Error::from)? {
                    if out.len() >= cap || stop.load(std::sync::atomic::Ordering::Relaxed) {
                        return Ok((out, true));
                    }
                    let key = meta.location.as_ref().to_string();
                    out.push(ObjectEntry {
                        name: last_segment(&key),
                        key,
                        is_prefix: false,
                        size: Some(meta.size),
                        modified: Some(meta.last_modified),
                        etag: meta.e_tag.clone(),
                        version: meta.version.clone(),
                    });
                }
                anyhow::Ok((out, false))
            })
            .with_context(|| format!("recursively listing cloud prefix {prefix:?}"))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = ObjPath::from(key);
        let bytes = runtime()
            .block_on(async {
                let res = self.store.get(&path).await?;
                res.bytes().await
            })
            .with_context(|| format!("downloading cloud object {key}"))?;
        Ok(bytes.to_vec())
    }

    fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        let path = ObjPath::from(key);
        runtime()
            .block_on(self.store.put(&path, PutPayload::from(bytes)))
            .with_context(|| format!("uploading cloud object {key}"))?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let path = ObjPath::from(key);
        match runtime().block_on(self.store.delete(&path)) {
            // The trait's promise: a missing key is not an error. S3 already
            // answers so; WebDAV says 404.
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("deleting cloud object {key}")),
        }
    }

    fn copy(&self, from: &str, to: &str) -> Result<()> {
        let (from_p, to_p) = (ObjPath::from(from), ObjPath::from(to));
        runtime()
            .block_on(self.store.copy(&from_p, &to_p))
            .with_context(|| format!("copying cloud object {from} to {to}"))?;
        Ok(())
    }

    fn get_streaming(
        &self,
        key: &str,
        on_chunk: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        use futures_util::TryStreamExt;
        let path = ObjPath::from(key);
        let rt = runtime();
        // One `block_on` per await, never one around the whole loop: `on_chunk`
        // is free to block on the runtime itself (a cross-cloud copy writes
        // each block with a multipart upload), and a `block_on` nested inside
        // another one panics with "Cannot start a runtime from within a
        // runtime". Entering and leaving per chunk keeps the callback outside
        // any runtime context.
        let mut stream = rt
            .block_on(async { self.store.get(&path).await })
            .with_context(|| format!("opening cloud object {key}"))?
            .into_stream();
        loop {
            let next = rt
                .block_on(stream.try_next())
                .with_context(|| format!("streaming cloud object {key}"))?;
            match next {
                Some(bytes) => on_chunk(&bytes)?,
                None => return Ok(()),
            }
        }
    }

    fn put_streaming(&self, key: &str) -> Result<Box<dyn CloudUpload + '_>> {
        let path = ObjPath::from(key);
        let upload = runtime()
            .block_on(self.store.put_multipart(&path))
            .with_context(|| format!("starting a multipart upload to {key}"))?;
        Ok(Box::new(MultipartCloudUpload {
            upload,
            key: key.to_string(),
        }))
    }
}

/// Streaming upload backed by `object_store`'s multipart API. Each
/// `write_chunk` is one part; `finish` completes the upload.
struct MultipartCloudUpload {
    upload: Box<dyn object_store::MultipartUpload>,
    key: String,
}

impl CloudUpload for MultipartCloudUpload {
    fn write_chunk(&mut self, bytes: Vec<u8>) -> Result<()> {
        runtime()
            .block_on(self.upload.put_part(PutPayload::from(bytes)))
            .with_context(|| format!("uploading a part of {}", self.key))?;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        runtime()
            .block_on(self.upload.complete())
            .with_context(|| format!("completing the upload of {}", self.key))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
