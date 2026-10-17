use crate::app::state::LookupAct;

/// After a server result the actions ask first; otherwise they act.
#[test]
fn server_results_ask_before_acting() {
    assert_eq!(
        super::route(LookupAct::Split(2), true),
        super::Route::Ask(LookupAct::Split(2))
    );
    assert_eq!(
        super::route(LookupAct::Breaking(0), false),
        super::Route::Now(LookupAct::Breaking(0))
    );
}
