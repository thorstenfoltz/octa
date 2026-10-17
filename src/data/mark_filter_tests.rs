use super::*;

fn marks(items: &[MarkKey]) -> HashMap<MarkKey, MarkColor> {
    items
        .iter()
        .cloned()
        .map(|k| (k, MarkColor::Yellow))
        .collect()
}

#[test]
fn row_marks_kept() {
    let m = marks(&[MarkKey::Row(2), MarkKey::Row(5)]);
    let ks = mark_keep_set(&m, MarkFilterCellMode::RowsOnly);
    assert_eq!(ks.rows, HashSet::from([2, 5]));
    assert!(ks.cols.is_empty());
}

#[test]
fn column_marks_kept() {
    let m = marks(&[MarkKey::Column(1)]);
    let ks = mark_keep_set(&m, MarkFilterCellMode::RowsOnly);
    assert_eq!(ks.cols, HashSet::from([1]));
    assert!(ks.rows.is_empty());
}

#[test]
fn cell_rows_only() {
    let m = marks(&[MarkKey::Cell(3, 2)]);
    let ks = mark_keep_set(&m, MarkFilterCellMode::RowsOnly);
    assert_eq!(ks.rows, HashSet::from([3]));
    assert!(ks.cols.is_empty());
}

#[test]
fn cell_columns_only() {
    let m = marks(&[MarkKey::Cell(3, 2)]);
    let ks = mark_keep_set(&m, MarkFilterCellMode::Columns);
    assert_eq!(ks.cols, HashSet::from([2]));
    assert!(ks.rows.is_empty());
}

#[test]
fn cell_intersect() {
    let m = marks(&[MarkKey::Cell(3, 2)]);
    let ks = mark_keep_set(&m, MarkFilterCellMode::Intersect);
    assert_eq!(ks.rows, HashSet::from([3]));
    assert_eq!(ks.cols, HashSet::from([2]));
}

#[test]
fn union_of_kinds() {
    let m = marks(&[MarkKey::Row(1), MarkKey::Column(4), MarkKey::Cell(2, 0)]);
    let ks = mark_keep_set(&m, MarkFilterCellMode::Intersect);
    assert_eq!(ks.rows, HashSet::from([1, 2]));
    assert_eq!(ks.cols, HashSet::from([0, 4]));
}
