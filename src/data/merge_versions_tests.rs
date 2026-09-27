//! Unit tests for [`merge_versions`](super). Included via `#[path]`.

use super::*;

fn table(headers: &[&str], rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = headers
        .iter()
        .map(|h| ColumnInfo {
            name: (*h).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
        .collect();
    t
}

fn rows(t: &DataTable) -> Vec<Vec<String>> {
    t.rows
        .iter()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect()
}

fn id() -> Vec<String> {
    vec!["id".to_string()]
}

#[test]
fn with_an_original_each_versions_own_change_is_taken() {
    let base = table(&["id", "name", "qty", "note"], &[&["1", "a", "1", "x"]]);
    let v1 = table(&["id", "name", "qty", "note"], &[&["1", "A", "1", "x"]]);
    let v2 = table(&["id", "name", "qty", "note"], &[&["1", "a", "9", "x"]]);
    let v3 = table(&["id", "name", "qty", "note"], &[&["1", "a", "1", "y"]]);
    let r = merge_versions(Some(&base), &[&v1, &v2, &v3], &id()).unwrap();
    assert!(r.conflicts.is_empty());
    assert_eq!(r.status, vec![RowStatus::Changed]);
    assert_eq!(rows(&r.finish().unwrap())[0], ["1", "A", "9", "y"]);
}

#[test]
fn the_same_change_in_several_versions_is_not_a_conflict() {
    let base = table(&["id", "v"], &[&["1", "x"]]);
    let same = table(&["id", "v"], &[&["1", "y"]]);
    let untouched = base.clone();
    let r = merge_versions(Some(&base), &[&same, &untouched, &same], &id()).unwrap();
    assert!(r.conflicts.is_empty());
    assert_eq!(rows(&r.table)[0], ["1", "y"]);
}

#[test]
fn different_changes_offer_every_distinct_value_with_its_versions() {
    let base = table(&["id", "v"], &[&["1", "x"]]);
    let a = table(&["id", "v"], &[&["1", "a"]]);
    let b = table(&["id", "v"], &[&["1", "b"]]);
    let keep = base.clone();
    let mut r = merge_versions(Some(&base), &[&a, &b, &keep, &a], &id()).unwrap();
    assert_eq!(r.unresolved(), 1);
    let ConflictKind::Cell { options, .. } = &r.conflicts[0].kind else {
        panic!("cell conflict expected")
    };
    let seen: Vec<(String, Vec<usize>)> = options
        .iter()
        .map(|o| (o.value.to_string(), o.from.clone()))
        .collect();
    assert_eq!(
        seen,
        vec![
            ("a".to_string(), vec![0, 3]),
            ("b".to_string(), vec![1]),
            ("x".to_string(), vec![2])
        ]
    );
    assert!(r.finish().is_err());
    r.prefer_all(1);
    assert_eq!(rows(&r.finish().unwrap())[0], ["1", "b"]);
}

#[test]
fn without_an_original_every_disagreement_asks_and_rows_are_unioned() {
    let v1 = table(&["id", "v"], &[&["1", "same"], &["2", "p"]]);
    let v2 = table(&["id", "v"], &[&["1", "same"], &["2", "q"], &["3", "new"]]);
    let mut r = merge_versions(None, &[&v1, &v2], &id()).unwrap();
    assert_eq!(
        r.status,
        vec![RowStatus::Unchanged, RowStatus::Conflict, RowStatus::Added]
    );
    r.prefer_all(0);
    assert_eq!(
        rows(&r.finish().unwrap()),
        vec![vec!["1", "same"], vec!["2", "p"], vec!["3", "new"]]
    );
}

#[test]
fn delete_by_some_and_edit_by_another_asks_and_untouched_deletes() {
    let base = table(&["id", "v"], &[&["1", "x"], &["2", "y"]]);
    let deleter = table(&["id", "v"], &[]);
    let editor = table(&["id", "v"], &[&["1", "x"], &["2", "Y"]]);
    let mut r = merge_versions(Some(&base), &[&deleter, &editor], &id()).unwrap();
    assert_eq!(rows(&r.table), vec![vec!["2", "Y"]]);
    assert_eq!(
        r.conflicts[0].kind,
        ConflictKind::DeleteVsEdit {
            deleted_by: vec![0],
            edited_by: vec![1]
        }
    );
    let mut keep = r.clone();
    keep.prefer_all(1);
    assert_eq!(keep.finish().unwrap().row_count(), 1);
    r.prefer_all(0);
    assert_eq!(r.finish().unwrap().row_count(), 0);
}

#[test]
fn added_columns_are_kept_and_one_dropped_by_a_version_is_dropped() {
    let base = table(&["id", "old"], &[&["1", "o"]]);
    let v1 = table(&["id", "old", "new"], &[&["1", "o", "n"]]);
    let v2 = table(&["id"], &[&["1"]]);
    let r = merge_versions(Some(&base), &[&v1, &v2], &id()).unwrap();
    let names: Vec<&str> = r.table.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "new"]);
    assert_eq!(rows(&r.table), vec![vec!["1", "n"]]);
}

#[test]
fn position_is_the_key_without_one_and_one_version_alone_is_refused() {
    let v1 = table(&["v"], &[&["a"], &["b"]]);
    let v2 = table(&["v"], &[&["a"], &["b"]]);
    let r = merge_versions(None, &[&v1, &v2], &[]).unwrap();
    assert!(r.conflicts.is_empty());
    assert!(merge_versions(None, &[&v1], &[]).is_err());
    let err = merge_versions(None, &[&v1, &v2], &id()).unwrap_err();
    assert!(format!("{err:#}").contains("key column `id` not found"));
}

#[test]
fn the_suggested_key_is_the_identifying_column_all_share() {
    let a = table(
        &["kind", "id", "v"],
        &[&["a", "1", "x"], &["a", "2", "y"], &["b", "3", "z"]],
    );
    let b = table(
        &["kind", "id", "v"],
        &[&["a", "1", "X"], &["a", "2", "y"], &["b", "3", "z"]],
    );
    assert_eq!(suggest_key(&[&a, &b, &a]).as_deref(), Some("id"));
}

#[test]
fn the_conflict_table_has_one_column_per_version() {
    let a = table(&["id", "v"], &[&["1", "a"]]);
    let b = table(&["id", "v"], &[&["1", "b"]]);
    let c = table(&["id", "v"], &[&["1", "c"]]);
    let r = merge_versions(None, &[&a, &b, &c], &id()).unwrap();
    let t = conflict_table(&r);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "row",
            "column",
            "original",
            "version_1",
            "version_2",
            "version_3"
        ]
    );
    assert_eq!(rows(&t)[0], ["1", "v", "", "a", "b", "c"]);
}
