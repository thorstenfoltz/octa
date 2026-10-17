//! Listing and hashing for [`super`]: local folders through `std::fs`, cloud
//! folders through a [`CloudProvider`] the caller resolved (the GUI from a
//! sidebar connection, the CLI from a URL, the MCP tool through its sandbox).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use chrono::{DateTime, Utc};

use super::{FileEntry, FolderListing, compare_table, labels_for, which_need_hash};
use crate::cloud::CloudProvider;
use crate::data::DataTable;
use crate::ui::settings::AppSettings;

pub enum FolderSource {
    Local(PathBuf),
    /// An open provider and the key prefix of the folder (`""` = bucket root).
    Cloud {
        provider: Box<dyn CloudProvider>,
        prefix: String,
    },
}

pub enum ScanProgress<'a> {
    Listing(&'a str),
    Hashing { done: usize, total: usize },
}

/// A folder key as a listing prefix: `""` stays the root, anything else ends in `/`.
pub fn cloud_prefix(key: &str) -> String {
    if key.is_empty() || key.ends_with('/') {
        key.to_string()
    } else {
        format!("{key}/")
    }
}

/// A root typed on the command line or picked in the GUI: a cloud URL
/// resolves through the saved connections (else ambient credentials), the
/// rest is a local path.
pub fn source_for(root: &str, settings: &AppSettings) -> anyhow::Result<FolderSource> {
    if crate::cloud::parse_cloud_url(root).is_some() {
        let (provider, loc) = crate::cloud::provider_for_url(root, settings)?;
        return Ok(FolderSource::Cloud {
            provider,
            prefix: cloud_prefix(&loc.key),
        });
    }
    let path = PathBuf::from(root);
    if !path.is_dir() {
        anyhow::bail!("{root} is not a folder");
    }
    Ok(FolderSource::Local(path))
}

fn check(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }
    Ok(())
}

/// Recursive walk. Symlinked folders are not followed (they can loop);
/// symlinked files are. Returns whether the limit was hit.
fn walk_local(
    dir: &Path,
    base: &Path,
    recursive: bool,
    cap: usize,
    cancel: &AtomicBool,
    out: &mut Vec<FileEntry>,
) -> anyhow::Result<bool> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        check(cancel)?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if recursive && walk_local(&path, base, recursive, cap, cancel, out)? {
                return Ok(true);
            }
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if out.len() >= cap {
            return Ok(true);
        }
        out.push(FileEntry {
            rel_path: path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
            size: meta.len(),
            modified: meta.modified().ok().map(DateTime::<Utc>::from),
            hash: None,
        });
    }
    Ok(false)
}

fn list(
    src: &FolderSource,
    recursive: bool,
    cap: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(Vec<FileEntry>, bool)> {
    match src {
        FolderSource::Local(root) => {
            let mut out = Vec::new();
            let truncated = walk_local(root, root, recursive, cap, cancel, &mut out)?;
            Ok((out, truncated))
        }
        FolderSource::Cloud { provider, prefix } => {
            let (entries, truncated) = if recursive {
                provider.list_recursive_until(prefix, cap, cancel)?
            } else {
                let files: Vec<_> = provider
                    .list(prefix)?
                    .into_iter()
                    .filter(|e| !e.is_prefix)
                    .collect();
                let truncated = files.len() > cap;
                (files.into_iter().take(cap).collect(), truncated)
            };
            // `list_recursive_until` answers a raised flag with a short,
            // "truncated" listing; that is a cancel, not a result.
            check(cancel)?;
            let files = entries
                .into_iter()
                .filter(|e| !e.is_prefix)
                .map(|e| FileEntry {
                    rel_path: e
                        .key
                        .strip_prefix(prefix.as_str())
                        .unwrap_or(&e.key)
                        .to_string(),
                    size: e.size.unwrap_or(0),
                    modified: e.modified,
                    hash: None,
                })
                .collect();
            Ok((files, truncated))
        }
    }
}

fn hash(src: &FolderSource, rel: &str) -> anyhow::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    match src {
        FolderSource::Local(root) => {
            let path = root.join(rel);
            let file = std::fs::File::open(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            hasher
                .update_reader(file)
                .with_context(|| format!("reading {}", path.display()))?;
        }
        FolderSource::Cloud { provider, prefix } => {
            provider
                .get_streaming(&format!("{prefix}{rel}"), &mut |chunk| {
                    hasher.update(chunk);
                    Ok(())
                })
                .with_context(|| format!("reading {prefix}{rel}"))?;
        }
    }
    Ok(*hasher.finalize().as_bytes())
}

/// List every folder, hash what decides the answer, and build the table.
/// `sources` are `(root as the user gave it, source)`; at least two.
///
/// Every file [`which_need_hash`] names is hashed before the table is built,
/// and a cancel or a failed read ends with an error rather than a table:
/// [`compare_table`] reads two unhashed copies of equal size as `same`.
pub fn compare_sources(
    sources: Vec<(String, FolderSource)>,
    recursive: bool,
    cap: usize,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(ScanProgress<'_>),
) -> anyhow::Result<(DataTable, Vec<FolderListing>)> {
    if sources.len() < 2 {
        anyhow::bail!("compare at least two folders");
    }
    let roots: Vec<String> = sources.iter().map(|(r, _)| r.clone()).collect();
    let labels = labels_for(&roots);
    let mut listings = Vec::with_capacity(sources.len());
    for ((root, src), label) in sources.iter().zip(&labels) {
        progress(ScanProgress::Listing(label));
        let (files, truncated) = list(src, recursive, cap, cancel)?;
        listings.push(FolderListing {
            root: root.clone(),
            label: label.clone(),
            files,
            truncated,
        });
    }
    let need = which_need_hash(&listings);
    let index: Vec<HashMap<String, usize>> = listings
        .iter()
        .map(|l| {
            l.files
                .iter()
                .enumerate()
                .map(|(i, f)| (f.rel_path.clone(), i))
                .collect()
        })
        .collect();
    for (done, (folder, rel)) in need.iter().enumerate() {
        check(cancel)?;
        let h = hash(&sources[*folder].1, rel)?;
        if let Some(&i) = index[*folder].get(rel) {
            listings[*folder].files[i].hash = Some(h);
        }
        progress(ScanProgress::Hashing {
            done: done + 1,
            total: need.len(),
        });
    }
    Ok((compare_table(&listings), listings))
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
