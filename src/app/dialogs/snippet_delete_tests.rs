use super::*;
use crate::app::sql_snippets::SqlSnippet;

fn app_with_one_snippet() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.sql_snippets = vec![SqlSnippet {
        name: "s1".into(),
        description: String::new(),
        query: "select 1".into(),
    }];
    app
}

#[test]
fn a_delete_asks_first_while_the_setting_is_on() {
    let mut app = app_with_one_snippet();
    assert!(app.settings.confirm_snippet_delete);
    app.request_snippet_delete("s1".into());
    assert_eq!(app.sql_snippets.len(), 1);
    assert_eq!(app.pending_snippet_delete.as_deref(), Some("s1"));
}

#[test]
fn with_the_setting_off_a_delete_happens_at_once() {
    let mut app = app_with_one_snippet();
    app.settings.confirm_snippet_delete = false;
    app.request_snippet_delete("s1".into());
    assert!(app.sql_snippets.is_empty());
    assert_eq!(app.pending_snippet_delete, None);
}
