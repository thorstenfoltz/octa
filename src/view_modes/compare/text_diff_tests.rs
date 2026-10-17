use super::*;

// (tag, text): 'e'=Equal, 'd'=Delete (left-only), 'i'=Insert (right-only)
fn changes(spec: &[(char, &str)]) -> Vec<(char, String)> {
    spec.iter().map(|(t, s)| (*t, s.to_string())).collect()
}

#[test]
fn modified_line_pairs_on_same_row() {
    // line2 changed: delete old, insert new -> one row, old left / new right.
    let (left, right) = pair_changes(&changes(&[
        ('e', "line1"),
        ('d', "old2"),
        ('i', "new2"),
        ('e', "line3"),
    ]));
    assert_eq!(left.len(), right.len());
    assert_eq!(left.len(), 3); // line1, changed, line3 -> no drift
    assert_eq!(left[1].2, RowKind::Del);
    assert_eq!(left[1].1, "old2");
    assert_eq!(right[1].2, RowKind::Add);
    assert_eq!(right[1].1, "new2");
    assert_eq!(left[2].1, "line3");
    assert_eq!(right[2].1, "line3");
}

#[test]
fn unequal_run_lengths_pad_the_shorter_side() {
    // 1 delete, 2 inserts -> max(1,2)=2 rows; left[1] is a blank pad.
    let (left, right) = pair_changes(&changes(&[('d', "a"), ('i', "x"), ('i', "y")]));
    assert_eq!(left.len(), 2);
    assert_eq!(right.len(), 2);
    assert_eq!(left[1].2, RowKind::Blank);
    assert_eq!(right[1].1, "y");
}
