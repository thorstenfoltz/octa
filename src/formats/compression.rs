//! Transparent decompression for `.gz` / `.zst` single files (not archives:
//! `.tgz` / `.tar.gz` stay with the archive reader). Detection is by outer
//! extension; the decompressed bytes land in a temp file whose suffix is the
//! *inner* extension so the format registry dispatches normally. A byte cap
//! guards against decompression bombs (a tiny `.gz` can inflate to terabytes).

use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};

/// Default decompressed-size cap: 4 GiB, what [`max_decompressed`] returns
/// until something sets it.
pub const DEFAULT_MAX_DECOMPRESSED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Process-wide decompressed-size cap, built like `formats::initial_load_rows`:
/// the GUI sets it from Settings (start and Apply), the CLI and MCP from
/// `--max-decompressed`, else from settings.toml. `u64::MAX` = unlimited.
static MAX_DECOMPRESSED: AtomicU64 = AtomicU64::new(DEFAULT_MAX_DECOMPRESSED_BYTES);

/// The decompressed-size cap every read in this process uses.
pub fn max_decompressed() -> u64 {
    MAX_DECOMPRESSED.load(Ordering::Relaxed)
}

/// Updates the cap returned by [`max_decompressed`].
pub fn set_max_decompressed(bytes: u64) {
    MAX_DECOMPRESSED.store(bytes, Ordering::Relaxed);
}

/// Compression codec detected from the outer file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Gzip,
    Zstd,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::Gzip => "gzip",
            Codec::Zstd => "zstd",
        }
    }
}

/// `Some(codec)` when the outer extension is `.gz` / `.zst`. `.tgz` and
/// `.tar.gz` are archives and stay with the archive reader.
pub fn detect_codec(path: &Path) -> Option<Codec> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".tgz") || name.ends_with(".tar.gz") {
        return None;
    }
    if name.ends_with(".gz") {
        return Some(Codec::Gzip);
    }
    if name.ends_with(".zst") {
        return Some(Codec::Zstd);
    }
    None
}

/// File name with the one compression suffix stripped:
/// `data.csv.gz` -> `data.csv`, `x.zst` -> `x`.
pub fn inner_file_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_lowercase();
    for suf in [".gz", ".zst"] {
        if lower.ends_with(suf) {
            return Some(name[..name.len() - suf.len()].to_string());
        }
    }
    None
}

/// Copy `reader` into `writer`, refusing to write more than `max` bytes.
fn copy_capped(mut reader: impl Read, mut writer: impl Write, max: u64) -> Result<()> {
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            writer.flush()?;
            return Ok(());
        }
        total += n as u64;
        if total > max {
            bail!("decompressed size exceeds the {max}-byte cap (see Settings > Performance)");
        }
        writer.write_all(&buf[..n])?;
    }
}

/// Decompress `path` into a temp file whose suffix is the inner extension
/// (so the registry can dispatch on it).
pub fn decompress_to_temp(
    path: &Path,
    codec: Codec,
    max_bytes: u64,
) -> Result<tempfile::NamedTempFile> {
    let inner = inner_file_name(path).unwrap_or_else(|| "decompressed".into());
    let suffix = Path::new(&inner)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let tmp = tempfile::Builder::new()
        .suffix(&suffix)
        .tempfile()
        .context("creating a temp file for decompression")?;
    let src = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let out = io::BufWriter::new(tmp.reopen()?);
    match codec {
        Codec::Gzip => copy_capped(flate2::read::GzDecoder::new(src), out, max_bytes),
        Codec::Zstd => copy_capped(zstd::stream::read::Decoder::new(src)?, out, max_bytes),
    }
    .with_context(|| format!("decompressing {} ({})", path.display(), codec.label()))?;
    Ok(tmp)
}

/// Compress `src` onto `dest` (overwrites). Used by save-back so a file
/// opened from `data.csv.gz` saves back to `data.csv.gz`.
pub fn compress_file(src: &Path, dest: &Path, codec: Codec) -> Result<()> {
    // Through a temp + rename: `dest` is the user's real `data.csv.gz`, and a
    // failure part-way used to leave it truncated with the data gone.
    crate::formats::write_atomically(dest, |tmp| {
        let mut input =
            std::fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
        let out =
            std::fs::File::create(tmp).with_context(|| format!("writing {}", dest.display()))?;
        match codec {
            Codec::Gzip => {
                let mut enc = flate2::write::GzEncoder::new(out, flate2::Compression::default());
                io::copy(&mut input, &mut enc)?;
                enc.finish()?;
            }
            Codec::Zstd => {
                let mut enc = zstd::stream::write::Encoder::new(out, 3)?;
                io::copy(&mut input, &mut enc)?;
                enc.finish()?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "compression_tests.rs"]
mod tests;
