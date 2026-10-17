use super::*;

#[test]
fn matching_ignores_case_and_surrounding_space() {
    let n = needle("  Sales ").unwrap();
    assert!(matches("fact_SALES_2024", &n));
    assert!(!matches("orders", &n));
    assert_eq!(needle("   "), None);
}

#[test]
fn a_row_opens_only_for_a_match_below_it() {
    let f = Some("sales");
    assert_eq!(RowFilter::of(None, "public", |_| false), RowFilter::Show);
    assert_eq!(RowFilter::of(f, "sales_mart", |_| false), RowFilter::Show);
    assert_eq!(
        RowFilter::of(f, "public", |_| true),
        RowFilter::OpenForMatch
    );
    assert_eq!(RowFilter::of(f, "public", |_| false), RowFilter::Hide);
    // A folder whose own name matches shows its contents unfiltered.
    assert_eq!(RowFilter::Show.child_filter(f), None);
    assert_eq!(RowFilter::OpenForMatch.child_filter(f), f);
}

fn done(hit: u8) -> TreeSearch<u8> {
    TreeSearch::Done {
        hits: vec![hit],
        stopped_at: None,
    }
}

#[test]
fn a_stale_search_never_overwrites_a_newer_one() {
    let mut slot: SearchSlot<u8> = SearchSlot::default();
    let old = slot.start("old");
    let new = slot.start("new");
    slot.finish(&old, done(1));
    assert!(matches!(slot.state, TreeSearch::Running));
    slot.finish(&new, done(2));
    assert!(matches!(slot.state, TreeSearch::Done { ref hits, .. } if hits == &[2]));
    // Editing the box drops results that answer something else.
    slot.forget_unless("newer");
    assert!(matches!(slot.state, TreeSearch::Idle));
}

#[test]
fn cancel_stops_the_worker_and_only_that_worker() {
    let mut slot: SearchSlot<u8> = SearchSlot::default();
    let first = slot.start("a");
    slot.cancel();
    assert!(first.load(Ordering::Relaxed));
    assert!(matches!(slot.state, TreeSearch::Idle));
    // Searching the same text again: the stopped worker's late result
    // must not land in the new run.
    let again = slot.start("a");
    slot.finish(&first, done(1));
    assert!(matches!(slot.state, TreeSearch::Running));
    assert!(!again.load(Ordering::Relaxed));
    // Starting another search stops the one before it.
    let next = slot.start("b");
    assert!(again.load(Ordering::Relaxed));
    assert!(!next.load(Ordering::Relaxed));
}
