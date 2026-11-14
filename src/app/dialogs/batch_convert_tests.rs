//! The dialog has closed and the status line fades, so the report tab is where a short
//! copy has to stay visible.

use std::path::PathBuf;

use octa::data::batch_convert::{BatchItem, BatchReport, BatchStatus};

use crate::app::state::{BatchConvertState, OctaApp};

fn finished_run(partial: &[bool]) -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let items: Vec<BatchItem> = partial
        .iter()
        .enumerate()
        .map(|(i, partial)| BatchItem {
            input: PathBuf::from(format!("/in/{i}.csv")),
            output: PathBuf::from(format!("/out/{i}.tsv")),
            status: BatchStatus::Done { rows: 3 },
            partial: *partial,
        })
        .collect();
    let state = BatchConvertState::new(vec![PathBuf::from("/in/0.csv")]);
    *state.result.lock().unwrap() = Some(BatchReport {
        converted: items.len(),
        failed: 0,
        skipped: 0,
        items,
    });
    app.batch_convert_dialog = Some(state);
    app.drain_batch_convert();
    app
}

#[test]
fn a_run_with_short_copies_leaves_a_banner_on_its_report() {
    let app = finished_run(&[true, false, true]);
    assert_eq!(
        app.tabs[app.active_tab].parse_error_banner.as_deref(),
        Some(octa::i18n::t("batch.partial").replace("{n}", "2").as_str()),
    );
}

#[test]
fn a_run_of_whole_copies_leaves_none() {
    let app = finished_run(&[false, false]);
    assert_eq!(app.tabs[app.active_tab].parse_error_banner, None);
}
