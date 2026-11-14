//! Unit tests for [`sniff`](sniff). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn parquet_magic() {
    assert_eq!(sniff_magic(b"PAR1\x00\x00"), Some("Parquet"));
}

#[test]
fn sqlite_magic() {
    assert_eq!(sniff_magic(b"SQLite format 3\0"), Some("SQLite"));
}

#[test]
fn duckdb_magic() {
    // 8-byte checksum then "DUCK".
    let head = b"\x06\x31\x87\xb8\x9f\x14\x11\x6cDUCK";
    assert_eq!(sniff_magic(head), Some("DuckDB"));
}

#[test]
fn arrow_avro_orc_hdf5_magic() {
    assert_eq!(sniff_magic(b"ARROW1\0\0"), Some("Arrow IPC"));
    assert_eq!(sniff_magic(b"Obj\x01\x00"), Some("Avro"));
    assert_eq!(sniff_magic(b"ORC\x00"), Some("ORC"));
    assert_eq!(sniff_magic(b"\x89HDF\r\n\x1a\n"), Some("HDF5"));
}

#[test]
fn a_sas_transport_file_is_known_by_its_header() {
    assert_eq!(sniff_magic(b"HEADER RECORD***"), Some("SAS transport"));
}

#[test]
fn an_access_database_is_known_by_its_header() {
    assert_eq!(sniff_magic(b"\0\x01\0\0Standard Jet DB"), Some("Access"));
    assert_eq!(sniff_magic(b"\0\x01\0\0Standard ACE DB"), Some("Access"));
}

#[test]
fn zip_magic_routes_to_archive() {
    assert_eq!(sniff_magic(b"PK\x03\x04abcd"), Some("Archive"));
}

#[test]
fn prose_is_not_a_format() {
    // No magic, no structural delimiter -> None.
    assert_eq!(sniff_magic(b"Hello, world"), None);
}
