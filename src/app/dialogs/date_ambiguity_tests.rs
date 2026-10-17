use super::*;

fn q(name: &str, dates: &[DateLayout], datetimes: &[DateTimeLayout]) -> DateAmbiguity {
    DateAmbiguity {
        tab_idx: 0,
        col_idx: 0,
        col_name: name.to_string(),
        samples: Vec::new(),
        date_candidates: dates.to_vec(),
        datetime_candidates: datetimes.to_vec(),
    }
}

/// "Use this for all remaining" must only settle columns that actually
/// offer the chosen layout. Forcing it on a column with a different set
/// of candidates would answer a question nobody asked, and silently
/// reinterpret its values.
#[test]
fn apply_to_all_skips_columns_that_do_not_offer_the_layout() {
    let queue: VecDeque<DateAmbiguity> = [
        q("a", &[DateLayout::DmySlash, DateLayout::MdySlash], &[]),
        q("b", &[DateLayout::MdySlash], &[]),
        q("c", &[], &[DateTimeLayout::DmySlashSpace]),
    ]
    .into_iter()
    .collect();

    assert_eq!(
        takes_same_answer(&queue, Choice::Date(DateLayout::DmySlash)),
        [0]
    );
    assert_eq!(
        takes_same_answer(&queue, Choice::Date(DateLayout::MdySlash)),
        [0, 1]
    );
    assert_eq!(
        takes_same_answer(&queue, Choice::DateTime(DateTimeLayout::DmySlashSpace)),
        [2]
    );
}

/// Leaving a column as text changes nothing, so every queued column can
/// take that answer: it is the one that clears the whole queue.
#[test]
fn leaving_as_text_settles_every_remaining_column() {
    let queue: VecDeque<DateAmbiguity> = [
        q("a", &[DateLayout::DmySlash], &[]),
        q("b", &[], &[DateTimeLayout::MdySlashSpace]),
    ]
    .into_iter()
    .collect();
    assert_eq!(takes_same_answer(&queue, Choice::Skip), [0, 1]);
}
