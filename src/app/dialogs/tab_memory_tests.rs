use super::*;
use octa::data::SearchMode;

fn tab_with_source(path: &str) -> TabState {
    let mut tab = TabState::new(SearchMode::Plain);
    tab.table.source_path = Some(path.to_string());
    tab
}

#[test]
fn a_tab_with_pending_edits_refuses_to_unload() {
    let mut tab = tab_with_source("/tmp/x.csv");
    assert!(can_unload(&tab).is_ok());
    tab.table
        .edits
        .insert((0, 0), octa::data::CellValue::Int(1));
    assert!(matches!(can_unload(&tab), Err(UnloadRefusal::PendingEdits)));
}

/// A structural change is not in the edit overlay, so checking only
/// `edits` would let a deleted column be thrown away silently.
#[test]
fn a_tab_with_structural_changes_refuses_too() {
    let mut tab = tab_with_source("/tmp/x.csv");
    tab.table.structural_changes = true;
    assert!(matches!(can_unload(&tab), Err(UnloadRefusal::PendingEdits)));
}

#[test]
fn a_tab_with_no_file_behind_it_refuses_to_unload() {
    let tab = TabState::new(SearchMode::Plain);
    assert!(
        matches!(can_unload(&tab), Err(UnloadRefusal::NoSourcePath)),
        "an in-memory table cannot be reloaded, so unloading it would lose it"
    );
}

/// Every refusal explains itself on the disabled button, or the user is
/// left with a dead control and no reason.
#[test]
fn every_refusal_has_a_message() {
    for r in [UnloadRefusal::PendingEdits, UnloadRefusal::NoSourcePath] {
        assert!(r.i18n_key().starts_with("tabmem.refuse_"));
    }
}
