use super::super::join_keys::tests::text_table;
use super::super::test_support::{DuckConn, source_named};
use super::*;
use crate::data::join_diag::diagnose;

fn both(
    left: crate::data::DataTable,
    right: crate::data::DataTable,
) -> (JoinDiagnosis, JoinDiagnosis) {
    let want = diagnose(&left, 0, &right, 0, usize::MAX);
    let mut c = DuckConn::with_tables(left, vec![("other", right)]);
    let (got, not_checked) = run(
        &mut c,
        &source_named("data"),
        "k",
        &source_named("other"),
        "id",
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(not_checked.is_empty());
    (got, want)
}

/// Every fix, regex ones included, against the in-memory engine.
#[test]
fn the_server_diagnoses_like_the_in_memory_engine() {
    let left = text_table(
        &["k"],
        &[
            &["007"],
            &["A1"],
            &["b2 "],
            &["c3"],
            &["c3"],
            &["x9"],
            &[""],
            &["NULL"],
            &["a  b"],
            &["x-1"],
            &["a  b  c"],
            &["x-1-2"],
        ],
    );
    let right = text_table(
        &["id"],
        &[
            &["7"],
            &["a1"],
            &["b2"],
            &["c3"],
            &["zz"],
            &["a b"],
            &["x 1"],
            &["a b c"],
            &["x 1 2"],
        ],
    );
    let (got, want) = both(left, right);
    assert_eq!(got, want);
    for kind in [
        FixKind::CollapseWhitespace,
        FixKind::StripPunctuation,
        FixKind::StripLeadingZeros,
    ] {
        assert!(got.fixes.iter().any(|f| f.kind == kind), "{kind:?}");
    }
}

/// `LTRIM('000', '0')` is `''` on most engines (NULL on Oracle); in memory
/// an all-zero key becomes "0", so it matches "0" and never a blank.
#[test]
fn an_all_zero_key_strips_to_zero_not_blank() {
    let left = text_table(&["k"], &[&["000"], &["5"]]);
    let right = text_table(&["id"], &[&["0"], &["05"], &[" "]]);
    let (got, want) = both(left, right);
    assert_eq!(got, want);
    let zeros = got
        .fixes
        .iter()
        .find(|f| f.kind == FixKind::StripLeadingZeros);
    assert_eq!(zeros.map(|f| f.would_match), Some(2));
}

#[test]
fn unchecked_fixes_name_what_the_engine_cannot_spell() {
    assert_eq!(
        unchecked_fixes(DbEngine::Mssql),
        [
            FixKind::CollapseWhitespace,
            FixKind::StripPunctuation,
            FixKind::StripLeadingZeros,
        ]
    );
    assert!(unchecked_fixes(DbEngine::Postgres).is_empty());
}

#[test]
fn sql_server_leaves_the_regex_and_zero_fixes_to_the_loaded_rows() {
    let missing: Vec<FixKind> = ALL_FIXES
        .into_iter()
        .filter(|&k| fix_expr(DbEngine::Mssql, k, "x").is_none())
        .collect();
    assert_eq!(
        missing,
        [
            FixKind::CollapseWhitespace,
            FixKind::StripPunctuation,
            FixKind::StripLeadingZeros
        ]
    );
    assert!(
        ALL_FIXES
            .into_iter()
            .all(|k| fix_expr(DbEngine::Postgres, k, "x").is_some())
    );
}
