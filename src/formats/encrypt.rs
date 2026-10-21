//! Writing password-protected files: AES-256 zips.
//!
//! The write half of `decrypt.rs`. Octa protects a file by putting it in a
//! zip whose entry is AES-256 encrypted. That is the only protection it
//! writes: a password-protected xlsx is out of reach (see
//! `decrypt::xlsx_needs_passphrase`). Octa, 7-Zip and Keka open these zips;
//! the zip tools built into Windows and macOS do not.
//!
//! Same rule as `decrypt.rs`: **a passphrase never appears in an error, a log
//! or a message.** Nothing here formats one.

use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::data::DataTable;
use crate::formats::write_options::WriteOptions;

/// Name prefix of the private folders holding decrypted copies.
pub const PROTECTED_DIR_PREFIX: &str = "octa-protected-";
const LOCK_FILE: &str = ".lock";
const MIN_STALE_AGE: std::time::Duration = std::time::Duration::from_secs(60);

/// `report.csv` -> `report.csv.zip`: the zip is named after the file inside.
pub fn zip_path_for(plain: &Path) -> PathBuf {
    let mut os = plain.as_os_str().to_owned();
    os.push(".zip");
    PathBuf::from(os)
}

fn aes_options(large: bool, passphrase: &str) -> zip::write::FileOptions<'_, '_, ()> {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(large)
        .with_aes_encryption(zip::AesMode::Aes256, passphrase)
}

fn add_encrypted<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    name: &str,
    src: &Path,
    passphrase: &str,
) -> anyhow::Result<()> {
    let mut file =
        std::fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let large = file.metadata()?.len() >= u64::from(u32::MAX);
    zip.start_file(name, aes_options(large, passphrase))
        .context("starting the encrypted entry")?;
    std::io::copy(&mut file, zip).context("encrypting the entry")?;
    Ok(())
}

/// Write `entries` (name inside the zip, plain file on disk) into an AES-256
/// zip at `dest`. Built beside `dest` and renamed into place, so a failure
/// never leaves half a zip where the old file was.
pub fn write_encrypted_zip(
    dest: &Path,
    entries: &[(&str, &Path)],
    passphrase: &str,
) -> anyhow::Result<()> {
    if passphrase.is_empty() {
        anyhow::bail!("an empty passphrase protects nothing");
    }
    crate::formats::write_atomically(dest, |tmp| {
        let mut zip = zip::ZipWriter::new(create_temp(tmp)?);
        for (name, src) in entries {
            add_encrypted(&mut zip, name, src, passphrase)?;
        }
        zip.finish().context("finishing the zip")?;
        Ok(())
    })
}

fn create_temp(tmp: &Path) -> anyhow::Result<std::fs::File> {
    std::fs::File::create(tmp).with_context(|| format!("creating {}", tmp.display()))
}

/// Put the plain file `new_src` back into the AES zip at `zip_path` as
/// `entry`, encrypted with `passphrase`. Every other entry is copied as it is,
/// still encrypted and never decrypted. Appends `entry` when it is missing.
pub fn replace_entry_encrypted(
    zip_path: &Path,
    entry: &str,
    new_src: &Path,
    passphrase: &str,
) -> anyhow::Result<()> {
    if passphrase.is_empty() {
        anyhow::bail!("an empty passphrase protects nothing");
    }
    crate::formats::write_atomically(zip_path, |tmp| {
        let file = std::fs::File::open(zip_path)
            .with_context(|| format!("opening {}", zip_path.display()))?;
        let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
            .context("reading zip central directory")?;
        let mut out = zip::ZipWriter::new(create_temp(tmp)?);
        out.set_raw_comment(archive.comment().into())
            .context("keeping the zip comment")?;
        let mut replaced = false;
        for i in 0..archive.len() {
            let raw = archive.by_index_raw(i).context("reading an entry")?;
            if raw.name().context("reading an entry name")? == entry {
                drop(raw);
                add_encrypted(&mut out, entry, new_src, passphrase)?;
                replaced = true;
            } else {
                out.raw_copy_file(raw).context("copying an entry")?;
            }
        }
        if !replaced {
            add_encrypted(&mut out, entry, new_src, passphrase)?;
        }
        out.finish().context("finishing the zip")?;
        // `archive` drops at the closure's end, before the rename (Windows
        // cannot replace a file that is still open).
        Ok(())
    })
}

fn check_entry_name(entry_name: &str) -> anyhow::Result<()> {
    if Path::new(entry_name).file_name() != Some(std::ffi::OsStr::new(entry_name)) {
        anyhow::bail!("{entry_name:?} is not a plain file name");
    }
    Ok(())
}

/// Write `table` as `entry_name` (format from its extension) inside an
/// AES-256 zip at `zip_path`. The plain copy exists only in a private folder
/// that is removed before this returns.
pub fn write_table_encrypted(
    table: &DataTable,
    zip_path: &Path,
    entry_name: &str,
    passphrase: &str,
    opts: &WriteOptions,
) -> anyhow::Result<()> {
    check_entry_name(entry_name)?;
    let tmp = ProtectedTempDir::new()?;
    let plain = tmp.path().join(entry_name);
    let registry = crate::formats::FormatRegistry::new();
    let writer = registry
        .reader_for_path(&plain)
        .filter(|r| r.supports_write())
        .ok_or_else(|| anyhow::anyhow!("Octa cannot write {entry_name}: pick another format"))?;
    writer.write_file_with_options(&plain, table, opts)?;
    write_encrypted_zip(zip_path, &[(entry_name, &plain)], passphrase)
}

/// [`write_table_encrypted`] for text written exactly as typed (a raw-text tab).
pub fn write_bytes_encrypted(
    bytes: &[u8],
    zip_path: &Path,
    entry_name: &str,
    passphrase: &str,
) -> anyhow::Result<()> {
    check_entry_name(entry_name)?;
    let tmp = ProtectedTempDir::new()?;
    let plain = tmp.path().join(entry_name);
    std::fs::write(&plain, bytes).context("writing the plain copy")?;
    write_encrypted_zip(zip_path, &[(entry_name, &plain)], passphrase)
}

/// A private folder (owner-only on Unix) for decrypted copies, removed on
/// drop. It holds a locked `.lock` file for its whole life: that is how
/// [`sweep_stale_protected_dirs`] tells a crashed session's leftovers from a
/// running Octa's folder.
pub struct ProtectedTempDir {
    // Field order matters: fields drop in declaration order, and the lock
    // file must be closed before the folder is removed (Windows refuses to
    // delete a folder holding an open file).
    _lock: std::fs::File,
    dir: tempfile::TempDir,
}

impl ProtectedTempDir {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_in(&std::env::temp_dir())
    }

    pub fn new_in(root: &Path) -> anyhow::Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix(PROTECTED_DIR_PREFIX);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let dir = builder
            .tempdir_in(root)
            .context("creating a private temporary folder")?;
        let lock = std::fs::File::create(dir.path().join(LOCK_FILE))
            .context("creating the folder lock")?;
        lock.try_lock()
            .context("locking the private temporary folder")?;
        Ok(Self { _lock: lock, dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Remove the private folders that crashed sessions left in `root`. A
/// folder whose lock can be taken has no living owner; one whose lock is
/// held belongs to a running Octa and stays. Returns how many were removed.
///
/// Runs at startup in a shared temp folder, so it trusts nothing there:
/// symlinks are never followed, the lock is opened without blocking (a FIFO
/// planted as `.lock` must not hang Octa), and anything unreadable is left.
///
/// ponytail: folders younger than `MIN_STALE_AGE` are skipped, which covers
/// the instant between `tempdir_in` and its lock file in another starting
/// Octa; a crash leftover younger than that is swept by the next start.
pub fn sweep_stale_protected_dirs(root: &Path) -> usize {
    sweep_older_than(root, MIN_STALE_AGE)
}

fn sweep_older_than(root: &Path, min_age: std::time::Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(PROTECTED_DIR_PREFIX)
        {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let path = entry.path();
        let young = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_none_or(|age| age < min_age);
        if young {
            continue;
        }
        let stale = match open_lock(&path.join(LOCK_FILE)) {
            // Dropping `f` at the end of the arm releases the lock again.
            Ok(f) => f.metadata().is_ok_and(|m| m.is_file()) && f.try_lock().is_ok(),
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        };
        if stale && std::fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn open_lock(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    o.open(path)
}

#[cfg(test)]
#[path = "encrypt_tests.rs"]
mod tests;
