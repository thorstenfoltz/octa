//! Unit tests for [`tolerance`](super).

use super::*;

#[test]
fn an_empty_draft_is_exact() {
    assert!(ToleranceDraft::default().options(&[]).is_exact());
}

#[test]
fn the_draft_reads_either_decimal_mark_and_names_ignored_columns() {
    let draft = ToleranceDraft {
        abs: "0,5".into(),
        pct: "1.5".into(),
        secs: " 60 ".into(),
        ignore_case: true,
        ignore_spaces: false,
        ignore_cols: vec![1],
    };
    let o = draft.options(&["id".into(), "updated_at".into()]);
    assert_eq!(o.number_abs, Some(0.5));
    assert_eq!(o.number_pct, Some(1.5));
    assert_eq!(o.seconds, Some(60.0));
    assert!(o.ignore_case);
    assert_eq!(o.ignore_columns, vec!["updated_at".to_string()]);
}

#[test]
fn text_that_is_not_a_number_or_is_negative_means_no_tolerance() {
    let draft = ToleranceDraft {
        abs: "abc".into(),
        pct: "-1".into(),
        ..Default::default()
    };
    let o = draft.options(&[]);
    assert_eq!(o.number_abs, None);
    assert_eq!(o.number_pct, None);
}
