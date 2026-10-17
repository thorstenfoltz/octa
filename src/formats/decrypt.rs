//! Opening password-protected archives.
//!
//! An encrypted zip is a file Octa can see but not read, and the useful
//! response is to ask for the passphrase rather than to report a corrupt
//! archive. That means detecting encryption **before** prompting, which is
//! possible because the encryption flag lives in the central directory and
//! needs no key to read.
//!
//! One rule runs through the whole module: **a passphrase never appears in
//! an error, a log or a message.** Errors here say that the passphrase was
//! rejected, never what was tried.

use std::io::Read;
use std::path::Path;

use anyhow::Context;

/// What to open a protected file with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OpenSecret {
    /// No secret offered: correct for an ordinary archive, and the state a
    /// protected one starts in before the user has been asked.
    #[default]
    None,
    Passphrase(String),
}

impl OpenSecret {
    /// The bytes to hand the zip reader, if any.
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            OpenSecret::None => None,
            OpenSecret::Passphrase(p) => Some(p.as_bytes()),
        }
    }
}

/// Whether any entry in `path` is encrypted.
///
/// Reads the central directory only, so this answers **before** anyone is
/// asked for a passphrase: prompting for a file that turns out not to need
/// one is its own small insult.
///
/// Every file Octa opens is asked this, so anything that does not start like
/// a zip is answered from its first four bytes. Handed to the zip library, a
/// file that is not a zip gets searched end to end for the archive's
/// directory, several times over: 43 GB read for a 5 GB CSV, on the UI
/// thread, before the open even started.
pub fn zip_needs_passphrase(path: &Path) -> anyhow::Result<bool> {
    use std::io::{Read, Seek};
    const LOCAL_HEADER: [u8; 4] = *b"PK\x03\x04";
    const EMPTY_ARCHIVE: [u8; 4] = *b"PK\x05\x06";
    let mut file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut head = [0u8; 4];
    match file.read_exact(&mut head) {
        Ok(()) if head == LOCAL_HEADER || head == EMPTY_ARCHIVE => {}
        Ok(()) => return Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
        Err(e) => {
            return Err(anyhow::Error::new(e).context(format!("reading {}", path.display())));
        }
    }
    file.rewind()
        .with_context(|| format!("reading {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .context("reading zip central directory")?;
    for i in 0..archive.len() {
        // `by_index_raw` reads the header without decrypting or decompressing,
        // which is the whole point: the flag is readable without the key.
        if archive
            .by_index_raw(i)
            .map(|e| e.encrypted())
            .unwrap_or(false)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Read one entry out of a zip, decrypting it when `secret` provides a
/// passphrase.
///
/// The error for a rejected passphrase says exactly that and **never**
/// includes the passphrase: an error string ends up in logs, in a status
/// bar, and in bug reports.
pub fn open_zip_entry(path: &Path, entry: &str, secret: &OpenSecret) -> anyhow::Result<Vec<u8>> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .context("reading zip central directory")?;

    let mut buf = Vec::new();
    match secret.bytes() {
        Some(password) => {
            let mut e = archive
                .by_name_decrypt(entry, password)
                .map_err(map_zip_err)?;
            e.read_to_end(&mut buf)
                .context("reading the decrypted entry")?;
        }
        None => {
            let mut e = archive.by_name(entry).map_err(map_zip_err)?;
            e.read_to_end(&mut buf).context("reading the entry")?;
        }
    }
    Ok(buf)
}

/// Whether `path` is a password-protected workbook.
///
/// A protected xlsx is not a zip at all: it is an OLE compound file holding
/// the encrypted package, which is why an ordinary reader reports it as
/// corrupt rather than as locked. The signature is the first eight bytes,
/// so this answers without a passphrase, exactly like the zip case.
///
/// # Detection only
///
/// Octa can tell you a workbook is protected but cannot open it. The one
/// maintained pure-Rust crate that decrypts ECMA-376 agile encryption,
/// `office-crypto` 0.4, pins `quick-xml ^0.38.4`, which carries
/// RUSTSEC-2026-0194 and RUSTSEC-2026-0195 (a quadratic parse and an
/// unbounded allocation). Both are reachable here: the vulnerable
/// `attributes()` call runs on XML read straight out of the workbook being
/// opened, which is a file someone else sent you. The fix is quick-xml 0.41
/// or later and the pin cannot reach it, so the dependency was dropped
/// rather than shipped with a known DoS on attacker-supplied input.
///
/// Revisit when `office-crypto` bumps its quick-xml, or when another
/// maintained pure-Rust implementation appears. Encrypted **zips** are
/// unaffected and open normally.
pub fn xlsx_needs_passphrase(path: &Path) -> anyhow::Result<bool> {
    use std::io::Read;
    const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    let mut file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut head = [0u8; 8];
    match file.read_exact(&mut head) {
        Ok(()) => Ok(head == OLE_MAGIC),
        // Too short to be either an xlsx or an encrypted container.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(anyhow::Error::new(e).context(format!("reading {}", path.display()))),
    }
}

/// Turn a zip error into one safe to show, log and paste into a bug report.
fn map_zip_err(e: zip::result::ZipError) -> anyhow::Error {
    match e {
        zip::result::ZipError::InvalidPassword => {
            anyhow::anyhow!("the passphrase was rejected for this archive")
        }
        // A zip whose entry is encrypted but was opened without a passphrase
        // reports an unsupported method rather than a missing password.
        other => anyhow::anyhow!("{other}"),
    }
}

#[cfg(test)]
#[path = "decrypt_tests.rs"]
mod tests;
