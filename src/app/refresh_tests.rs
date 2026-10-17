use super::*;

#[test]
fn a_cloud_tab_refreshes_the_object_not_its_downloaded_copy() {
    let mut tab = TabState::new(octa::data::SearchMode::default());
    tab.table.source_path = Some("/tmp/octa-download-123.csv".into());
    tab.cloud_origin = Some(crate::app::state::CloudOrigin {
        conn_id: "s3".into(),
        key: "data/sales.csv".into(),
    });
    assert_eq!(
        tab.refresh_source().as_deref(),
        Some("cloud:s3:data/sales.csv")
    );
}

#[test]
fn a_file_tab_refreshes_its_path_and_a_computed_tab_nothing() {
    let mut tab = TabState::new(octa::data::SearchMode::default());
    assert_eq!(tab.refresh_source(), None);
    tab.table.source_path = Some("/data/a.csv".into());
    assert_eq!(tab.refresh_source().as_deref(), Some("/data/a.csv"));
}

#[test]
fn only_the_awaited_source_claims_the_tab() {
    let target = ReloadTarget {
        tab: 2,
        source: "/data/a.csv".into(),
    };
    assert!(target.claims("/data/a.csv", 3));
    // Another file opened meanwhile gets a tab of its own.
    assert!(!target.claims("/data/b.csv", 3));
    // The tab was closed in between: nothing to land in.
    assert!(!target.claims("/data/a.csv", 2));
}

#[test]
fn closing_a_tab_keeps_the_indices_on_the_same_tabs() {
    assert_eq!(shifted(1, 3), Some(1));
    assert_eq!(shifted(3, 3), None);
    assert_eq!(shifted(4, 3), Some(3));
}
