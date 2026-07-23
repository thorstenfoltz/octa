//! Transparent decompression for `.gz` / `.zst` single files (not archives:
//! `.tgz` / `.tar.gz` stay with the archive reader). Detection is by outer
//! extension; the decompressed bytes land in a temp file whose suffix is the
//! *inner* extension so the format registry dispatches normally. A byte cap
//! guards against decompression bombs (a tiny `.gz` can inflate to terabytes).

use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Default decompressed-size cap: 4 GiB. The GUI overrides this from the
/// `max_decompressed_bytes` setting; CLI/MCP use it as-is.
pub const DEFAULT_MAX_DECOMPRESSED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

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
            bail!("decompressed size exceeds the {max}-byte cap (see Settings -> Files)");
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
    let mut input =
        std::fs::File::open(src).with_context(|| format!("opening {}", src.display()))?;
    let out = std::fs::File::create(dest).with_context(|| format!("writing {}", dest.display()))?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn gz_bytes(payload: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(payload).unwrap();
        enc.finish().unwrap()
    }

    fn temp_with_suffix(suffix: &str) -> tempfile::NamedTempFile {
        tempfile::Builder::new().suffix(suffix).tempfile().unwrap()
    }

    #[test]
    fn detects_gz_and_zst_only() {
        assert_eq!(detect_codec(Path::new("a.csv.gz")), Some(Codec::Gzip));
        assert_eq!(detect_codec(Path::new("a.jsonl.zst")), Some(Codec::Zstd));
        assert_eq!(detect_codec(Path::new("a.tgz")), None);
        assert_eq!(detect_codec(Path::new("a.tar.gz")), None);
        assert_eq!(detect_codec(Path::new("a.csv")), None);
        // A bare .gz with no inner extension is still a codec hit; content
        // sniffing decides the inner format later.
        assert_eq!(detect_codec(Path::new("dump.gz")), Some(Codec::Gzip));
        // Case-insensitive.
        assert_eq!(detect_codec(Path::new("A.CSV.GZ")), Some(Codec::Gzip));
    }

    #[test]
    fn inner_name_strips_one_suffix() {
        assert_eq!(
            inner_file_name(Path::new("d/data.csv.gz")).unwrap(),
            "data.csv"
        );
        assert_eq!(inner_file_name(Path::new("x.zst")).unwrap(), "x");
        assert_eq!(inner_file_name(Path::new("plain.csv")), None);
    }

    #[test]
    fn gzip_roundtrip_through_temp() {
        let f = temp_with_suffix(".csv.gz");
        std::fs::write(f.path(), gz_bytes(b"a,b\n1,2\n")).unwrap();
        let tmp =
            decompress_to_temp(f.path(), Codec::Gzip, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
        assert_eq!(std::fs::read(tmp.path()).unwrap(), b"a,b\n1,2\n");
        assert!(tmp.path().to_string_lossy().ends_with(".csv"));
    }

    #[test]
    fn zstd_compress_then_decompress() {
        let src = temp_with_suffix(".csv");
        std::fs::write(src.path(), b"x\n1\n").unwrap();
        let dest = temp_with_suffix(".csv.zst");
        compress_file(src.path(), dest.path(), Codec::Zstd).unwrap();
        let tmp =
            decompress_to_temp(dest.path(), Codec::Zstd, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
        assert_eq!(std::fs::read(tmp.path()).unwrap(), b"x\n1\n");
    }

    #[test]
    fn gzip_compress_file_roundtrip() {
        let src = temp_with_suffix(".csv");
        std::fs::write(src.path(), b"h\nv\n").unwrap();
        let dest = temp_with_suffix(".csv.gz");
        compress_file(src.path(), dest.path(), Codec::Gzip).unwrap();
        let tmp =
            decompress_to_temp(dest.path(), Codec::Gzip, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
        assert_eq!(std::fs::read(tmp.path()).unwrap(), b"h\nv\n");
    }

    #[test]
    fn bomb_cap_errors_clearly() {
        let f = temp_with_suffix(".txt.gz");
        std::fs::write(f.path(), gz_bytes(&vec![0u8; 10_000])).unwrap();
        let err = decompress_to_temp(f.path(), Codec::Gzip, 1024)
            .unwrap_err()
            .to_string();
        // anyhow context chain: the cap message is in the root cause.
        assert!(err.contains("decompressing"), "{err}");
    }
}
