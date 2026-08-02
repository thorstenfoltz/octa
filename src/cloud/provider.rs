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
                    if out.len() >= cap {
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
        runtime()
            .block_on(self.store.delete(&path))
            .with_context(|| format!("deleting cloud object {key}"))?;
        Ok(())
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
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn seed_store() -> Arc<dyn ObjectStore> {
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        // Seed through our own provider so the test never calls object_store's
        // trait methods directly (their `put`/`get` live on an extension trait
        // that would otherwise need importing here).
        let provider = ObjectStoreProvider::new(store.clone());
        provider.put("top.csv", b"a,b\n1,2\n".to_vec()).unwrap();
        provider.put("sub/inner.txt", b"hi".to_vec()).unwrap();
        store
    }

    #[test]
    fn lists_root_folders_and_files() {
        let provider = ObjectStoreProvider::new(seed_store());
        let mut entries = provider.list("").unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        // One folder "sub" and one file "top.csv".
        assert_eq!(entries.len(), 2);
        let folder = entries.iter().find(|e| e.is_prefix).unwrap();
        assert_eq!(folder.name, "sub");
        assert_eq!(folder.key, "sub/");
        let file = entries.iter().find(|e| !e.is_prefix).unwrap();
        assert_eq!(file.name, "top.csv");
        assert_eq!(file.size, Some(8));
        assert!(file.modified.is_some());
    }

    #[test]
    fn list_recursive_flattens_everything() {
        let provider = ObjectStoreProvider::new(seed_store());
        provider.put("sub/deep/c.csv", b"x\n1\n".to_vec()).unwrap();
        let (entries, truncated) = provider.list_recursive("", 10).unwrap();
        assert!(!truncated);
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|e| !e.is_prefix));
        assert!(entries.iter().all(|e| e.size.is_some()));
        assert!(entries.iter().any(|e| e.key == "sub/deep/c.csv"));
    }

    #[test]
    fn list_recursive_cap_truncates() {
        let provider = ObjectStoreProvider::new(seed_store());
        provider.put("sub/deep/c.csv", b"x".to_vec()).unwrap();
        let (entries, truncated) = provider.list_recursive("", 2).unwrap();
        assert!(truncated);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn list_recursive_scopes_to_prefix() {
        let provider = ObjectStoreProvider::new(seed_store());
        let (entries, _) = provider.list_recursive("sub", 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "inner.txt");
    }

    #[test]
    fn get_roundtrips_object_bytes() {
        let provider = ObjectStoreProvider::new(seed_store());
        let bytes = provider.get("top.csv").unwrap();
        assert_eq!(bytes, b"a,b\n1,2\n");
    }

    #[test]
    fn put_then_get_roundtrips() {
        let provider = ObjectStoreProvider::new(Arc::new(InMemory::new()));
        provider.put("x/y.json", b"{\"k\":1}".to_vec()).unwrap();
        assert_eq!(provider.get("x/y.json").unwrap(), b"{\"k\":1}");
    }
}
