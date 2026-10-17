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
    let tmp = decompress_to_temp(f.path(), Codec::Gzip, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    assert_eq!(std::fs::read(tmp.path()).unwrap(), b"a,b\n1,2\n");
    assert!(tmp.path().to_string_lossy().ends_with(".csv"));
}

#[test]
fn zstd_compress_then_decompress() {
    let src = temp_with_suffix(".csv");
    std::fs::write(src.path(), b"x\n1\n").unwrap();
    let dest = temp_with_suffix(".csv.zst");
    compress_file(src.path(), dest.path(), Codec::Zstd).unwrap();
    let tmp = decompress_to_temp(dest.path(), Codec::Zstd, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
    assert_eq!(std::fs::read(tmp.path()).unwrap(), b"x\n1\n");
}

#[test]
fn gzip_compress_file_roundtrip() {
    let src = temp_with_suffix(".csv");
    std::fs::write(src.path(), b"h\nv\n").unwrap();
    let dest = temp_with_suffix(".csv.gz");
    compress_file(src.path(), dest.path(), Codec::Gzip).unwrap();
    let tmp = decompress_to_temp(dest.path(), Codec::Gzip, DEFAULT_MAX_DECOMPRESSED_BYTES).unwrap();
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
