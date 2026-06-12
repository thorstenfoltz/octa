//! Unit tests for [`sample`](sample). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

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
