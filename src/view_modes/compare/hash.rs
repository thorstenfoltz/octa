//! Row hashing for the Compare view's RowHashDiff mode.
//!
//! Stable across runs (BLAKE3 has no per-process state) so two Octa sessions
//! comparing the same files always produce the same digests. The hash sees
//! only the *string representation* of cells via `CellValue::to_string`, so
//! comparison is cross-format: a CSV row and a Parquet row containing the
//! same logical values hash identically.

use octa::data::DataTable;

/// Hash the selected columns of one row into a 32-byte BLAKE3 digest.
/// `cols.is_empty()` means "hash every column" — the typical default until
/// the user picks specific columns in the UI.
///
/// Column ordering matters: hashing `(a=1, b=2)` produces a different digest
/// from `(b=2, a=1)`. Callers that need order-insensitive comparison should
/// pre-sort `cols` consistently between left and right.
pub fn hash_row(table: &DataTable, row_idx: usize, cols: &[usize]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    // Separator between columns prevents accidental collisions where two
    // different column splits join to the same byte sequence.
    const SEP: &[u8] = b"\x1F"; // ASCII unit separator
    if cols.is_empty() {
        for col_idx in 0..table.col_count() {
            append_cell(&mut hasher, table, row_idx, col_idx);
            hasher.update(SEP);
        }
    } else {
        for &col_idx in cols {
            append_cell(&mut hasher, table, row_idx, col_idx);
            hasher.update(SEP);
        }
    }
    *hasher.finalize().as_bytes()
}

fn append_cell(hasher: &mut blake3::Hasher, table: &DataTable, row: usize, col: usize) {
    match table.get(row, col) {
        Some(v) => hasher.update(v.to_string().as_bytes()),
        None => hasher.update(b""),
    };
}

/// Format a 32-byte digest as a short hex preview (first 8 bytes = 16 hex
/// chars). Used in the diff result UI so users can spot identical hashes at
/// a glance without scrolling through 64 characters.
pub fn short_hex(digest: &[u8; 32]) -> String {
    let mut s = String::with_capacity(16);
    for b in &digest[..8] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
