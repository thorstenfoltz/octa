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

/// A column name comes from the file, so a planted one has to stay a name. Each
/// payload closes the identifier and needs the plain column `a` for the statement
/// before its COPY to run.
#[test]
fn a_column_name_cannot_run_sql() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out.csv");
    let copy = format!("COPY (SELECT 1 AS x) TO '{}'; --", target.display());
    let table = |evil: &str| {
        tbl(
            &[("a", "Int64"), (evil, "Int64")],
            vec![vec![CellValue::Int(1), CellValue::Int(2)]],
        )
    };

    let key = format!("a\"); {copy}");
    let t = table(&key);
    let joined = join_tables(
        &[("l", &t), ("r", &t)],
        std::slice::from_ref(&key),
        JoinType::Inner,
    )
    .unwrap();
    assert!(!target.exists(), "a key of join_tables ran SQL");
    assert_eq!(joined.row_count(), 1);

    let col = format!("a\" AS VARCHAR) = '1'; {copy}");
    let t = table(&col);
    let cond = JoinCond {
        left_col: col.clone(),
        op: JoinOp::Eq,
        right_col: col.clone(),
    };
    let joined = join_two(("l", &t), ("r", &t), &[cond], JoinType::Inner).unwrap();
    assert!(!target.exists(), "a condition of join_two ran SQL");
    assert_eq!(joined.row_count(), 1);
}

#[test]
fn a_table_name_with_a_quote_still_joins() {
    let t = tbl(&[("id", "Int64")], vec![vec![CellValue::Int(1)]]);
    let keys = ["id".to_string()];
    let joined = join_tables(&[("o\"ne", &t), ("two", &t)], &keys, JoinType::Inner).unwrap();
    assert_eq!(joined.row_count(), 1);
}

#[test]
fn join_tables_matches_a_number_key_against_text_that_is_not_all_numbers() {
    // DuckDB's own cast turned the text into a number and `abc` failed the join.
    let a = tbl(
        &[("id", "Int64")],
        vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]],
    );
    let b = tbl(
        &[("id", "Utf8"), ("v", "Utf8")],
        vec![
            vec![
                CellValue::String("abc".into()),
                CellValue::String("p".into()),
            ],
            vec![CellValue::String("2".into()), CellValue::String("q".into())],
        ],
    );
    let c = tbl(&[("id", "Int64")], vec![vec![CellValue::Int(2)]]);
    let out = join_tables(
        &[("a", &a), ("b", &b), ("c", &c)],
        &["id".into()],
        JoinType::Inner,
    )
    .unwrap();
    assert_eq!(out.row_count(), 1);
    assert_eq!(out.columns.len(), 2, "the key stays one column");
    assert_eq!(out.rows[0][1], CellValue::String("q".into()));
}

#[test]
fn join_tables_leaves_number_keys_of_different_widths_as_numbers() {
    let a = tbl(&[("id", "Int64")], vec![vec![CellValue::Int(1)]]);
    let b = tbl(&[("id", "Float64")], vec![vec![CellValue::Float(1.0)]]);
    assert!(!key_needs_text(&[("a", &a), ("b", &b)], "id"));
    let out = join_tables(&[("a", &a), ("b", &b)], &["id".into()], JoinType::Inner).unwrap();
    assert_eq!(out.row_count(), 1);
}
