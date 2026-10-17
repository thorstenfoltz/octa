use super::*;
use crate::data::{CellValue, ColumnInfo};

fn tbl(cols: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: n.to_string(),
            data_type: ty.to_string(),
        })
        .collect();
    t.rows = rows;
    t
}

#[test]
fn left_join_keeps_unmatched_left_rows() {
    let a = tbl(
        &[("id", "Int64"), ("name", "Utf8")],
        vec![
            vec![CellValue::Int(1), CellValue::String("a".into())],
            vec![CellValue::Int(2), CellValue::String("b".into())],
        ],
    );
    let b = tbl(
        &[("id", "Int64"), ("amt", "Int64")],
        vec![vec![CellValue::Int(1), CellValue::Int(100)]],
    );
    let out = join_tables(&[("t0", &a), ("t1", &b)], &["id".into()], JoinType::Left).unwrap();
    assert_eq!(out.row_count(), 2);
}

#[test]
fn inner_join_drops_unmatched() {
    let a = tbl(
        &[("id", "Int64")],
        vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]],
    );
    let b = tbl(&[("id", "Int64")], vec![vec![CellValue::Int(1)]]);
    let out = join_tables(&[("t0", &a), ("t1", &b)], &["id".into()], JoinType::Inner).unwrap();
    assert_eq!(out.row_count(), 1);
}

#[test]
fn join_two_matches_different_names_and_types() {
    // Left key is Int64 "id"; right key is Utf8 "ref" holding "1"/"3".
    let a = tbl(
        &[("id", "Int64"), ("name", "Utf8")],
        vec![
            vec![CellValue::Int(1), CellValue::String("a".into())],
            vec![CellValue::Int(2), CellValue::String("b".into())],
        ],
    );
    let b = tbl(
        &[("ref", "Utf8"), ("amt", "Int64")],
        vec![vec![CellValue::String("1".into()), CellValue::Int(100)]],
    );
    let conds = vec![JoinCond {
        left_col: "id".into(),
        op: JoinOp::Eq,
        right_col: "ref".into(),
    }];
    let out = join_two(("l", &a), ("r", &b), &conds, JoinType::Inner).unwrap();
    // Only id=1 matches "1".
    assert_eq!(out.row_count(), 1);
}

#[test]
fn join_two_supports_inequality() {
    let a = tbl(
        &[("v", "Int64")],
        vec![vec![CellValue::Int(1)], vec![CellValue::Int(5)]],
    );
    let b = tbl(&[("threshold", "Int64")], vec![vec![CellValue::Int(3)]]);
    // v > threshold -> only the row with v=5 matches.
    let conds = vec![JoinCond {
        left_col: "v".into(),
        op: JoinOp::Gt,
        right_col: "threshold".into(),
    }];
    let out = join_two(("l", &a), ("r", &b), &conds, JoinType::Inner).unwrap();
    assert_eq!(out.row_count(), 1);
}

fn ints(name: &str, v: &[i64]) -> DataTable {
    tbl(
        &[(name, "Int64")],
        v.iter().map(|&n| vec![CellValue::Int(n)]).collect(),
    )
}

fn eq(l: &str, r: &str) -> JoinCond {
    JoinCond {
        left_col: l.into(),
        op: JoinOp::Eq,
        right_col: r.into(),
    }
}

#[test]
fn semi_keeps_matched_left_rows_once_and_left_columns_only() {
    let a = ints("id", &[1, 2, 3]);
    // id 1 twice on the right: a semi join still yields it once.
    let b = tbl(
        &[("id", "Int64"), ("x", "Int64")],
        vec![
            vec![CellValue::Int(1), CellValue::Int(9)],
            vec![CellValue::Int(1), CellValue::Int(8)],
            vec![CellValue::Int(3), CellValue::Int(7)],
        ],
    );
    let out = join_two(("l", &a), ("r", &b), &[eq("id", "id")], JoinType::Semi).unwrap();
    assert_eq!(out.row_count(), 2);
    assert_eq!(out.col_count(), 1);
}

#[test]
fn anti_keeps_unmatched_left_rows() {
    let a = ints("id", &[1, 2, 3]);
    let b = ints("id", &[1, 3]);
    let out = join_two(("l", &a), ("r", &b), &[eq("id", "id")], JoinType::Anti).unwrap();
    assert_eq!(out.row_count(), 1);
    assert_eq!(out.rows[0][0].to_string(), "2");
    let many = join_tables(&[("t0", &a), ("t1", &b)], &["id".into()], JoinType::Anti).unwrap();
    assert_eq!(many.row_count(), 1);
}

#[test]
fn asof_takes_the_nearest_earlier_row_per_key_and_keeps_the_unmatched() {
    let dt = |s: &str| CellValue::DateTime(s.into());
    let text = |s: &str| CellValue::String(s.into());
    let trades = tbl(
        &[("t", "Timestamp(Microsecond, None)"), ("sym", "Utf8")],
        vec![
            vec![dt("2026-09-24 08:59:00"), text("A")],
            vec![dt("2026-09-24 09:03:00"), text("A")],
            vec![dt("2026-09-24 09:03:00"), text("B")],
        ],
    );
    let quotes = tbl(
        &[
            ("qt", "Timestamp(Microsecond, None)"),
            ("sym", "Utf8"),
            ("bid", "Int64"),
        ],
        vec![
            vec![dt("2026-09-24 09:00:00"), text("A"), CellValue::Int(10)],
            vec![dt("2026-09-24 09:02:00"), text("A"), CellValue::Int(11)],
            vec![dt("2026-09-24 09:05:00"), text("A"), CellValue::Int(12)],
            vec![dt("2026-09-24 09:01:00"), text("B"), CellValue::Int(50)],
        ],
    );
    let conds = vec![
        eq("sym", "sym"),
        JoinCond {
            left_col: "t".into(),
            op: JoinOp::Ge,
            right_col: "qt".into(),
        },
    ];
    let out = join_two(("l", &trades), ("r", &quotes), &conds, JoinType::AsOf).unwrap();
    assert_eq!(out.row_count(), 3);
    let bid = out.columns.iter().position(|c| c.name == "bid").unwrap();
    let mut bids: Vec<String> = out.rows.iter().map(|r| r[bid].to_string()).collect();
    bids.sort();
    // 08:59 has no earlier quote (kept, empty); 09:03 A -> 09:02; B -> 09:01.
    assert_eq!(bids, ["", "11", "50"]);
}

#[test]
fn asof_without_exactly_one_inequality_is_refused_plainly() {
    let a = ints("id", &[1]);
    let err = join_two(("l", &a), ("r", &a), &[eq("id", "id")], JoinType::AsOf).unwrap_err();
    assert!(format!("{err:#}").contains("exactly one condition"));
}

#[test]
fn join_type_parses_every_spelling_it_prints() {
    for t in JoinType::ALL {
        assert_eq!(JoinType::parse(t.id()).unwrap(), t);
    }
    assert_eq!(JoinType::parse("As-Of").unwrap(), JoinType::AsOf);
    assert!(JoinType::parse("cross").is_err());
}
