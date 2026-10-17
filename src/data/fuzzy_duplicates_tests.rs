use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn table(cols: &[&str], rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    for c in cols {
        t.columns.push(ColumnInfo {
            name: c.to_string(),
            data_type: "Utf8".to_string(),
        });
    }
    t.rows = rows
        .iter()
        .map(|r| r.iter().map(|v| CellValue::String(v.to_string())).collect())
        .collect();
    t
}

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn edit_ratio_scores() {
    assert_eq!(edit_ratio("abc", "abc"), 1.0);
    assert!(edit_ratio("John Smith", "Jon Smith") > 0.85);
    assert!(edit_ratio("apple", "zzzzz") < 0.3);
}

#[test]
fn jaro_winkler_scores() {
    assert_eq!(jaro_winkler("abc", "abc"), 1.0);
    assert!(jaro_winkler("Martha", "Marhta") > 0.9);
    assert!(jaro_winkler("abcd", "wxyz") < 0.3);
}

#[test]
fn token_set_ignores_order() {
    assert!((token_set("john smith", "smith john") - 1.0).abs() < 1e-9);
    assert!(token_set("acme inc", "acme llc") > 0.0);
    assert!(token_set("acme inc", "acme inc") == 1.0);
}

#[test]
fn normalisation_aligns_punctuation_and_case() {
    let opts = NormalizeOpts::default();
    assert_eq!(normalize("ACME, Inc.", &opts), "acme inc");
    assert_eq!(normalize("  a   b ", &opts), "a b");
}

#[test]
fn threshold_gates_matches() {
    let t = table(&["name"], &[&["John Smith"], &["Jon Smith"], &["Zorbax"]]);
    let cfg = FuzzyDupConfig {
        key_cols: vec![0],
        threshold: 0.8,
        ..Default::default()
    };
    let res = find_fuzzy_duplicates(&t, &cfg, &never());
    assert_eq!(res.clusters.len(), 1);
    assert_eq!(res.clusters[0].rows, vec![0, 1]);

    // A very high threshold finds nothing.
    let cfg_hi = FuzzyDupConfig {
        key_cols: vec![0],
        threshold: 0.99,
        ..Default::default()
    };
    assert!(
        find_fuzzy_duplicates(&t, &cfg_hi, &never())
            .clusters
            .is_empty()
    );
}

#[test]
fn averages_across_columns() {
    // First column identical, second column differs a lot -> average
    // drops below a high threshold so the pair does NOT match.
    let t = table(&["a", "b"], &[&["same", "apple"], &["same", "zzzzz"]]);
    let cfg = FuzzyDupConfig {
        key_cols: vec![0, 1],
        threshold: 0.8,
        ..Default::default()
    };
    assert!(
        find_fuzzy_duplicates(&t, &cfg, &never())
            .clusters
            .is_empty()
    );
}

#[test]
fn transitive_clustering() {
    // A~B and B~C (chain of single-char typos) -> one cluster of three.
    let t = table(&["n"], &[&["aaaa"], &["aaab"], &["aabb"]]);
    let cfg = FuzzyDupConfig {
        key_cols: vec![0],
        threshold: 0.7,
        ..Default::default()
    };
    let res = find_fuzzy_duplicates(&t, &cfg, &never());
    assert_eq!(res.clusters.len(), 1);
    assert_eq!(res.clusters[0].rows, vec![0, 1, 2]);
    // Worst-case score is the lowest linking edge (aaaa~aabb = 0.5 < thr,
    // but the chain links via 0.75 edges), so score is the min edge used.
    assert!(res.clusters[0].score >= 0.7);
}

#[test]
fn blocking_partitions_comparisons() {
    // Same name in two different blocks must NOT cluster together.
    let t = table(&["name", "region"], &[&["Acme", "US"], &["Acme", "EU"]]);
    let cfg = FuzzyDupConfig {
        key_cols: vec![0],
        threshold: 0.9,
        block_col: Some(1),
        ..Default::default()
    };
    assert!(
        find_fuzzy_duplicates(&t, &cfg, &never())
            .clusters
            .is_empty()
    );
    // Without blocking they cluster.
    let cfg2 = FuzzyDupConfig {
        block_col: None,
        ..cfg
    };
    assert_eq!(find_fuzzy_duplicates(&t, &cfg2, &never()).clusters.len(), 1);
}

#[test]
fn cap_sets_capped_flag() {
    let rows: Vec<[&str; 1]> = vec![["x"]; 5];
    let row_refs: Vec<&[&str]> = rows.iter().map(|r| r.as_slice()).collect();
    let t = table(&["n"], &row_refs);
    let cfg = FuzzyDupConfig {
        key_cols: vec![0],
        max_rows: 3,
        ..Default::default()
    };
    let res = find_fuzzy_duplicates(&t, &cfg, &never());
    assert!(res.capped);
    assert_eq!(res.compared_rows, 3);
}

#[test]
fn cancel_short_circuits() {
    let t = table(&["n"], &[&["aaaa"], &["aaab"]]);
    let cfg = FuzzyDupConfig {
        key_cols: vec![0],
        threshold: 0.5,
        ..Default::default()
    };
    let cancelled = AtomicBool::new(true);
    let res = find_fuzzy_duplicates(&t, &cfg, &cancelled);
    // Cancelled before any comparison -> no clusters.
    assert!(res.clusters.is_empty());
}
