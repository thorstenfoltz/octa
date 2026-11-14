//! An export from a tab that holds part of its source says so where it stays
//! visible, on the tab. The file pickers of PDF and workbook need a desktop, so those
//! two are pinned through the helper they both call.

use octa::data::{CellValue, ColumnInfo, DataTable};

use crate::app::state::{OctaApp, ReportState};

fn two_rows() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "id".into(),
        data_type: "Int64".into(),
    }];
    t.rows = vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]];
    t
}

fn app_with(total: Option<usize>) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    app.tabs[0].table = two_rows();
    app.tabs[0].table.total_rows = total;
    app
}

fn banner() -> String {
    octa::i18n::t("dialog.swb_partial").replace("{n}", "2")
}

#[test]
fn an_export_from_a_partly_loaded_tab_leaves_a_banner() {
    let mut app = app_with(Some(10));
    app.warn_partial_export(0, 2);
    assert_eq!(
        app.tabs[0].parse_error_banner.as_deref(),
        Some(banner().as_str())
    );
}

#[test]
fn an_export_from_a_whole_tab_leaves_nothing() {
    let mut app = app_with(None);
    app.warn_partial_export(0, 2);
    assert_eq!(app.tabs[0].parse_error_banner, None);
}

/// A report builds on a worker. The tab it came from gets the banner, whichever tab
/// is active by the time it lands, and however the tabs have moved.
#[test]
fn a_report_that_lands_later_warns_on_the_tab_it_came_from() {
    let mut app = app_with(Some(10));
    let source = app.tabs[0].id;
    let mode = app.settings.default_search_mode;
    app.tabs.push(crate::app::state::TabState::new(mode));
    app.tabs.swap(0, 1);
    app.active_tab = 0;

    let mut state = ReportState::new("/tmp/out.html".to_string());
    state.partial_source = Some((source, 2));
    *state.result.lock().unwrap() = Some(Ok("/tmp/out.html".into()));
    app.report_dialog = Some(state);
    app.drain_report();

    assert_eq!(app.tabs[0].parse_error_banner, None, "the other tab");
    assert_eq!(
        app.tabs[1].parse_error_banner.as_deref(),
        Some(banner().as_str())
    );
}
