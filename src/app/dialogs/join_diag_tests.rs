use octa::data::join_diag::FixKind;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn col(vals: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "k".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vals
        .iter()
        .map(|v| vec![CellValue::String((*v).into())])
        .collect();
    t
}

/// Only the kinds the server could not check are taken from the loaded
/// rows, so the two lists never repeat a fix.
#[test]
fn local_fixes_keep_only_the_unchecked_kinds() {
    let l = col(&["a  b", "x-1", " c"]);
    let r = col(&["a b", "x 1", "c"]);
    let got = super::local_fixes_for(&l, 0, &r, 0, usize::MAX, &[FixKind::StripPunctuation]);
    assert_eq!(
        got.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [FixKind::StripPunctuation]
    );
    assert!(super::local_fixes_for(&l, 0, &r, 0, usize::MAX, &[]).is_empty());
}
