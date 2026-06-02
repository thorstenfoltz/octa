//! Random row sampling for a `DataTable`.
//!
//! Pure + deterministic (seeded), so the CLI (`--sample`) and the MCP `sample`
//! tool produce reproducible output for a given seed. Sampling is without
//! replacement; the returned rows keep their original ascending order.

use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::data::DataTable;

/// Row indices of a size-`n` sample, sorted ascending. Deterministic for a
/// given `seed`. When `n >= row_count`, every index is returned.
pub fn sample_row_indices(table: &DataTable, n: usize, seed: u64) -> Vec<usize> {
    let len = table.row_count();
    if n >= len {
        return (0..len).collect();
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let mut idx = rand::seq::index::sample(&mut rng, len, n).into_vec();
    idx.sort_unstable();
    idx
}

/// A new `DataTable` containing a size-`n` random sample of `table`'s rows
/// (columns cloned, original row order preserved). Edits are resolved via
/// `get`, so it is safe on tables with a pending edit overlay.
pub fn sample_table(table: &DataTable, n: usize, seed: u64) -> DataTable {
    let indices = sample_row_indices(table, n, seed);
    let mut out = DataTable::empty();
    out.columns = table.columns.clone();
    out.rows = indices
        .iter()
        .map(|&r| {
            (0..table.col_count())
                .map(|c| {
                    table
                        .get(r, c)
                        .cloned()
                        .unwrap_or(crate::data::CellValue::Null)
                })
                .collect()
        })
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo};

    fn table(rows: usize) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "n".to_string(),
            data_type: "Int64".to_string(),
        }];
        t.rows = (0..rows).map(|i| vec![CellValue::Int(i as i64)]).collect();
        t
    }

    #[test]
    fn deterministic_for_same_seed() {
        let t = table(100);
        assert_eq!(
            sample_row_indices(&t, 10, 42),
            sample_row_indices(&t, 10, 42)
        );
    }

    #[test]
    fn returns_sorted_unique_subset() {
        let t = table(100);
        let idx = sample_row_indices(&t, 10, 7);
        assert_eq!(idx.len(), 10);
        assert!(idx.windows(2).all(|w| w[0] < w[1])); // sorted + unique
        assert!(idx.iter().all(|&i| i < 100));
    }

    #[test]
    fn n_at_or_above_len_returns_all() {
        let t = table(5);
        assert_eq!(sample_row_indices(&t, 5, 1), vec![0, 1, 2, 3, 4]);
        assert_eq!(sample_row_indices(&t, 99, 1), vec![0, 1, 2, 3, 4]);
    }
}
