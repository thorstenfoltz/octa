use super::*;
use octa::data::ColumnInfo;

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "a".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![vec![CellValue::String("x".into())]];
    t
}

fn cache(key: KeyDiffKey, pass: u64) -> KeyDiffCache {
    KeyDiffCache {
        key,
        pass,
        outcome: Err(String::new()),
    }
}

fn key(t: &DataTable, join: bool) -> KeyDiffKey {
    KeyDiffKey {
        join,
        keys: vec![],
        options: CompareOptions::default(),
        left: generation(t),
    }
}

#[test]
fn same_shape_tables_have_different_generations() {
    let (a, b) = (table(), table());
    assert_ne!(generation(&a), generation(&b));
}

#[test]
fn cache_counts_only_when_used_on_the_previous_pass() {
    let t = table();
    let c = cache(key(&t, false), 10);
    assert!(reusable(&c, &key(&t, false), 11));
    assert!(!reusable(&c, &key(&t, false), 12));
    assert!(!reusable(&c, &key(&t, true), 11));
}
