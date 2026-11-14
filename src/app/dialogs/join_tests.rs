use super::*;

#[test]
fn join_calls_a_cloud_tab_what_the_tab_bar_calls_it() {
    // Opened from a bucket: the path is the temp download, the label the object.
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    tab.table.source_path = Some("/tmp/octa-cloud-a8F3k2.csv".into());
    tab.custom_tab_label = Some("sales_2024.csv".into());
    assert_eq!(tab_label(&tab), tab.title_display());
    assert_eq!(tab_label(&tab), "sales_2024.csv");
}
