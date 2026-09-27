use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

fn text_col(name: &str, values: &[CellValue]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: name.into(),
        data_type: "Utf8".into(),
    }];
    t.rows = values.iter().map(|v| vec![v.clone()]).collect();
    t
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

fn mixed_column() -> DataTable {
    text_col(
        "amount",
        &[s("12"), s("not a number"), s("7"), CellValue::Null],
    )
}

#[test]
fn preview_counts_convertible_and_lists_failures_with_row_numbers() {
    let p = preview_retype(&mixed_column(), 0, TargetType::Integer);
    assert_eq!(p.total, 4);
    assert_eq!(
        p.convertible, 2,
        "two integers; null is neither converted nor a failure"
    );
    assert_eq!(p.failed, 1);
    assert_eq!(p.failures.len(), 1);
    assert_eq!(p.failures[0].0, 1, "row index of the bad value");
    assert_eq!(p.failures[0].1, "not a number");
}

#[test]
fn apply_converts_what_parses_and_keeps_the_rest_as_text() {
    let mut t = mixed_column();
    let out = apply_retype(&mut t, 0, TargetType::Integer);
    assert_eq!(out.converted, 2);
    assert_eq!(
        out.kept_as_text,
        vec![(1, 0)],
        "the failure is reported as a problem cell"
    );
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(12)));
    assert_eq!(
        t.get(1, 0),
        Some(&s("not a number")),
        "a value that will not parse keeps its original text, never becomes null"
    );
    assert_eq!(t.get(3, 0), Some(&CellValue::Null), "null stays null");
    assert_eq!(
        t.columns[0].data_type, "Int64",
        "the declared type does change, spelled the way the readers spell it"
    );
}

#[test]
fn dates_go_through_the_same_inference_the_loader_uses() {
    let mut t = text_col("d", &[s("2026-09-18"), s("nope")]);
    let out = apply_retype(&mut t, 0, TargetType::Date);
    assert_eq!(out.converted, 1);
    assert_eq!(t.get(0, 0), Some(&CellValue::Date("2026-09-18".into())));
    assert_eq!(t.get(1, 0), Some(&s("nope")));
    assert_eq!(t.columns[0].data_type, "Date32");
}

/// The layout is chosen for the whole column, so a European column converts
/// to canonical ISO instead of being refused for not already being ISO. This
/// is the difference between re-type and the old all-or-nothing Change Type,
/// whose `can_convert_value` only ever accepted `%Y-%m-%d`.
#[test]
fn a_non_iso_date_column_picks_its_layout_and_canonicalises() {
    let mut t = text_col("d", &[s("18.09.2026"), s("01.01.2020")]);
    let out = apply_retype(&mut t, 0, TargetType::Date);
    assert_eq!(out.converted, 2);
    assert_eq!(t.get(0, 0), Some(&CellValue::Date("2026-09-18".into())));
    assert_eq!(t.get(1, 0), Some(&CellValue::Date("2020-01-01".into())));
}

/// The chosen layout is the one that reads the MOST values, not the first one
/// that reads any. The stray ISO value here is read by `YmdDash`, which comes
/// first in `DateLayout::ALL`; picking it would convert one row and fail
/// three. The European layout reads three, so it wins and the ISO straggler
/// is the one that keeps its text.
#[test]
fn the_layout_that_reads_the_most_values_wins() {
    let mut t = text_col(
        "d",
        &[
            s("18.09.2026"),
            s("02.03.2020"),
            s("04.05.2021"),
            s("2026-01-05"),
        ],
    );
    let out = apply_retype(&mut t, 0, TargetType::Date);
    assert_eq!(out.converted, 3);
    assert_eq!(t.get(1, 0), Some(&CellValue::Date("2020-03-02".into())));
    assert_eq!(
        t.get(3, 0),
        Some(&s("2026-01-05")),
        "the minority layout's value keeps its text rather than being read \
         under the majority layout"
    );
    assert_eq!(out.kept_as_text, vec![(3, 0)]);
}

/// A genuinely ambiguous column (`01/02/2020` reads as both DD/MM and MM/DD)
/// is a tie. It goes to whichever layout `DateLayout::ALL` lists first, which
/// is the European reading, not the US one.
#[test]
fn an_ambiguous_date_column_breaks_its_tie_towards_the_earlier_layout() {
    let mut t = text_col("d", &[s("01/02/2020")]);
    apply_retype(&mut t, 0, TargetType::Date);
    assert_eq!(
        t.get(0, 0),
        Some(&CellValue::Date("2020-02-01".into())),
        "DD/MM/YYYY comes before MM/DD/YYYY in DateLayout::ALL"
    );
}

#[test]
fn datetimes_convert_through_their_own_layouts() {
    let mut t = text_col("ts", &[s("2026-09-18 14:30:00"), s("nope")]);
    let out = apply_retype(&mut t, 0, TargetType::DateTime);
    assert_eq!(out.converted, 1);
    assert_eq!(
        t.get(0, 0),
        Some(&CellValue::DateTime("2026-09-18 14:30:00".into()))
    );
    assert_eq!(t.columns[0].data_type, "Timestamp(Microsecond, None)");
}

/// Text is the escape hatch: every value has a text form, so nothing can fail
/// and nothing is lost, not even a blank that the shared `convert_value`
/// would have turned into a null.
#[test]
fn text_never_fails_and_keeps_blanks_as_blanks() {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "n".into(),
        data_type: "Int64".into(),
    }];
    t.rows = vec![vec![CellValue::Int(12)], vec![s("")], vec![CellValue::Null]];
    let out = apply_retype(&mut t, 0, TargetType::Text);
    assert!(out.kept_as_text.is_empty(), "text can never fail");
    assert_eq!(out.converted, 2, "the null is neither converted nor failed");
    assert_eq!(t.get(0, 0), Some(&s("12")));
    assert_eq!(t.get(1, 0), Some(&s("")), "a blank stays a blank");
    assert_eq!(t.get(2, 0), Some(&CellValue::Null));
    assert_eq!(t.columns[0].data_type, "Utf8");
}

/// A blank is not a failure on a typed target either: it is the same absence
/// a null is, so it becomes one rather than being flagged.
#[test]
fn a_blank_string_becomes_null_on_a_typed_target_without_being_flagged() {
    let mut t = text_col("amount", &[s("12"), s("   ")]);
    let out = apply_retype(&mut t, 0, TargetType::Integer);
    assert_eq!(out.converted, 1);
    assert!(out.kept_as_text.is_empty());
    assert_eq!(t.get(1, 0), Some(&CellValue::Null));
}

#[test]
fn booleans_take_the_spellings_the_rest_of_octa_takes() {
    let mut t = text_col("flag", &[s("yes"), s("0"), s("maybe")]);
    let out = apply_retype(&mut t, 0, TargetType::Boolean);
    assert_eq!(out.converted, 2);
    assert_eq!(out.kept_as_text, vec![(2, 0)]);
    assert_eq!(t.get(0, 0), Some(&CellValue::Bool(true)));
    assert_eq!(t.get(1, 0), Some(&CellValue::Bool(false)));
    assert_eq!(
        t.get(2, 0),
        Some(&s("maybe")),
        "an unrecognised word keeps its text instead of silently reading false"
    );
}

/// The preview must promise exactly what the apply delivers, or the dialog
/// lies. Both run the same per-cell classification over the same effective
/// values, and this pins that they stay in step.
#[test]
fn preview_and_apply_agree_on_the_counts() {
    let mut t = mixed_column();
    let p = preview_retype(&t, 0, TargetType::Integer);
    let out = apply_retype(&mut t, 0, TargetType::Integer);
    assert_eq!(p.convertible, out.converted);
    assert_eq!(p.failed, out.kept_as_text.len());
}

/// A long column reports every failure in its counts but only carries a
/// sample of them, so a million bad rows cannot blow up the dialog.
#[test]
fn the_failure_sample_is_capped_but_the_count_is_not() {
    let values: Vec<CellValue> = (0..FAILURE_SAMPLE + 10).map(|_| s("nope")).collect();
    let t = text_col("amount", &values);
    let p = preview_retype(&t, 0, TargetType::Integer);
    assert_eq!(p.failed, FAILURE_SAMPLE + 10, "the count is the true count");
    assert_eq!(p.failures.len(), FAILURE_SAMPLE, "the sample is capped");
}

/// The spec asks for one undo entry for the whole conversion, not one per
/// cell, and undoing must put back the values that were kept as text too.
#[test]
fn the_whole_conversion_undoes_in_one_step() {
    let mut t = mixed_column();
    let undo_before = t.undo_stack.len();
    apply_retype(&mut t, 0, TargetType::Integer);
    assert_eq!(t.undo_stack.len(), undo_before + 1, "exactly one entry");

    assert!(t.undo());
    assert_eq!(t.get(0, 0), Some(&s("12")), "the converted value came back");
    assert_eq!(t.get(1, 0), Some(&s("not a number")));
    assert_eq!(t.columns[0].data_type, "Utf8", "and so did the type");

    assert!(t.redo());
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(12)));
    assert_eq!(t.columns[0].data_type, "Int64");
}

/// Re-typing a column to what it already is changes nothing, so it must not
/// leave an undo step the user has to press Ctrl+Z through.
#[test]
fn a_conversion_that_changes_nothing_leaves_no_undo_entry() {
    let mut t = text_col("amount", &[s("a"), s("b")]);
    let out = apply_retype(&mut t, 0, TargetType::Text);
    assert_eq!(out.converted, 2);
    assert!(
        t.undo_stack.is_empty(),
        "nothing changed, so nothing to undo"
    );
}

#[test]
fn an_out_of_range_column_is_refused_rather_than_panicking() {
    let mut t = mixed_column();
    let p = preview_retype(&t, 9, TargetType::Integer);
    assert_eq!(p.total, 0);
    assert_eq!(p.convertible, 0);
    let out = apply_retype(&mut t, 9, TargetType::Integer);
    assert_eq!(out.converted, 0);
    assert!(out.kept_as_text.is_empty());
    assert!(t.undo_stack.is_empty());
}

/// A pending edit is what the user sees, so it is what the counts describe
/// and what the conversion rewrites. The underlying row value is converted
/// too, so removing the edit later does not reveal an unconverted string.
#[test]
fn a_pending_edit_is_converted_along_with_the_row_underneath_it() {
    let mut t = text_col("amount", &[s("12"), s("7")]);
    t.edits.insert((0, 0), s("99"));
    let out = apply_retype(&mut t, 0, TargetType::Integer);
    assert_eq!(out.converted, 2);
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(99)), "the edit converted");
    assert_eq!(
        t.rows[0][0],
        CellValue::Int(12),
        "and so did the value underneath it"
    );
}

// --- strictness ------------------------------------------------------

/// Strict mode refuses a column it cannot fully convert, and leaves it
/// EXACTLY as it was: not half-converted, not type-changed, nothing on the
/// undo stack to unwind.
#[test]
fn strict_refuses_a_mixed_column_and_changes_nothing() {
    let mut t = mixed_column();
    let before_type = t.columns[0].data_type.clone();
    let before_cells: Vec<Option<CellValue>> =
        (0..t.row_count()).map(|r| t.get(r, 0).cloned()).collect();

    let out = apply_retype_with(&mut t, 0, TargetType::Integer, Strictness::Strict);

    assert_eq!(out.refused, Some(1), "says how many values blocked it");
    assert_eq!(out.converted, 0);
    assert_eq!(t.columns[0].data_type, before_type, "the type is untouched");
    for (r, before) in before_cells.iter().enumerate() {
        assert_eq!(t.get(r, 0).cloned(), *before, "row {r} is untouched");
    }
    assert!(t.undo_stack.is_empty(), "nothing to undo, nothing happened");
}

/// A column that converts completely is converted under strict mode too:
/// strict is a refusal to go halfway, not a refusal to work.
#[test]
fn strict_converts_a_clean_column_normally() {
    let mut t = text_col("amount", &[s("12"), s("7"), CellValue::Null]);
    let out = apply_retype_with(&mut t, 0, TargetType::Integer, Strictness::Strict);
    assert_eq!(out.refused, None);
    assert_eq!(out.converted, 2);
    assert_eq!(t.get(0, 0), Some(&CellValue::Int(12)));
    assert_eq!(t.columns[0].data_type, "Int64");
}

/// Mixed mode is the default and is unchanged: `apply_retype` and
/// `apply_retype_with(.., Mixed)` are the same conversion.
#[test]
fn mixed_is_the_default_and_still_converts_what_it_can() {
    let mut a = mixed_column();
    let mut b = mixed_column();
    let out_a = apply_retype(&mut a, 0, TargetType::Integer);
    let out_b = apply_retype_with(&mut b, 0, TargetType::Integer, Strictness::Mixed);
    assert_eq!(out_a.converted, out_b.converted);
    assert_eq!(out_a.kept_as_text, out_b.kept_as_text);
    assert_eq!(out_a.refused, None);
    assert_eq!(a.columns[0].data_type, b.columns[0].data_type);
}
