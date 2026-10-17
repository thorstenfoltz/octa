use super::*;

/// The pane as the user sees it: `file` padded with a blank row wherever
/// the right side had a line of its own. Returns (padded text, flags).
fn pane(rows: &[Option<&str>]) -> (String, Vec<bool>) {
    let text = rows
        .iter()
        .map(|r| r.unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    (text, rows.iter().map(|r| r.is_none()).collect())
}

#[test]
fn an_untouched_pane_maps_back_to_the_file_it_came_from() {
    let (padded, pad) = pane(&[Some("a"), None, Some("b"), None, Some("c")]);
    assert_eq!(strip_padding(&padded, &pad, &padded, false), "a\nb\nc");
}

#[test]
fn editing_a_real_line_keeps_the_edit_and_drops_the_padding() {
    let (padded, pad) = pane(&[Some("a"), None, Some("b"), Some("c")]);
    let edited = padded.replace("b", "bZ");
    assert_eq!(strip_padding(&padded, &pad, &edited, false), "a\nbZ\nc");
}

#[test]
fn typing_on_a_padding_row_turns_it_into_a_real_line() {
    let (padded, pad) = pane(&[Some("a"), None, Some("b")]);
    let edited = "a\nNEW\nb";
    assert_eq!(strip_padding(&padded, &pad, edited, false), "a\nNEW\nb");
}

#[test]
fn deleting_a_real_line_removes_it_from_the_file() {
    let (padded, pad) = pane(&[Some("a"), None, Some("b"), Some("c")]);
    let edited = "a\n\nc";
    assert_eq!(strip_padding(&padded, &pad, edited, false), "a\nc");
}

#[test]
fn a_blank_line_the_user_adds_survives_next_to_a_padding_row() {
    // The file legitimately gains an empty line right where padding sits;
    // the count must come out right even though both lines read "".
    let (padded, pad) = pane(&[Some("a"), None, Some("b")]);
    let edited = "a\n\n\nb";
    assert_eq!(strip_padding(&padded, &pad, edited, false), "a\n\nb");
}

#[test]
fn a_file_that_ended_in_a_newline_still_does() {
    // The pane never shows the final terminator, so an edit must not be
    // the thing that removes it.
    let (padded, pad) = pane(&[Some("a"), None, Some("b")]);
    let edited = padded.replace("b", "bZ");
    assert_eq!(strip_padding(&padded, &pad, &edited, true), "a\nbZ\n");
}

#[test]
fn a_pane_with_no_padding_is_returned_unchanged() {
    let (padded, pad) = pane(&[Some("a"), Some("b")]);
    assert_eq!(strip_padding(&padded, &pad, "a\nbb", false), "a\nbb");
}
