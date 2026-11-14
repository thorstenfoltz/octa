//! A finished run becomes two columns in one undo step, and answers never
//! land on rows that moved while the run was out.

use std::sync::{Arc, Mutex};

use octa::data::transform::classify::{Answer, ClassifyMode, ClassifySpec, Progress, RunReport};
use octa::data::{CellValue, ColumnInfo, DataTable};

use super::Run;
use crate::app::state::OctaApp;

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

fn app() -> OctaApp {
    let mut app = OctaApp::new(
        vec![],
        octa::ui::settings::AppSettings::default(),
        octa::ui::settings::IconVariant::Rose,
    );
    let mut t = DataTable::empty();
    t.columns = vec![ColumnInfo {
        name: "msg".into(),
        data_type: "Utf8".into(),
    }];
    t.rows = vec![
        vec![s("invoice")],
        vec![CellValue::Null],
        vec![s("hello")],
        vec![s("invoice")],
    ];
    app.tabs[0].table = t;
    app
}

fn run_for(app: &OctaApp) -> Run {
    Run {
        tab: app.tabs[0].id,
        spec: ClassifySpec {
            columns: vec![0],
            mode: ClassifyMode::Categories,
            ..ClassifySpec::default()
        },
        name: "category".into(),
        undo_len: app.tabs[0].table.undo_stack.len(),
        // Rows 0 and 3 share a state, row 1 had nothing to send.
        rows: vec![Some(0), None, Some(1), Some(0)],
        progress: Arc::new(Progress::default()),
        slot: Arc::new(Mutex::new(None)),
    }
}

fn answers() -> Vec<Option<Answer>> {
    vec![
        Some(Answer {
            value: s("billing"),
            certainty: CellValue::Float(0.9),
        }),
        // The second value got no answer.
        None,
    ]
}

#[test]
fn answers_become_two_columns_and_one_undo_step() {
    let mut app = app();
    let run = run_for(&app);
    app.apply_classify(&run, &answers(), &RunReport::default())
        .unwrap();
    let t = &mut app.tabs[0].table;
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["msg", "category", "category_confidence"]);
    assert_eq!(t.get(0, 1), Some(&s("billing")));
    assert_eq!(t.get(3, 1), Some(&s("billing")));
    assert_eq!(t.get(3, 2), Some(&CellValue::Float(0.9)));
    for row in [1, 2] {
        assert_eq!(t.get(row, 1), Some(&CellValue::Null), "row {row}");
    }
    t.undo();
    assert_eq!(t.col_count(), 1, "one Ctrl+Z takes both columns away");
}

#[test]
fn answers_are_refused_once_the_table_changed() {
    let mut app = app();
    let run = run_for(&app);
    app.tabs[0].table.set(0, 0, s("edited meanwhile"));
    assert!(
        app.apply_classify(&run, &answers(), &RunReport::default())
            .is_err()
    );
    assert_eq!(app.tabs[0].table.col_count(), 1);
}

#[test]
fn a_saved_classifier_comes_back_and_names_the_columns_it_misses() {
    let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut st = super::ClassifyState::default();
    st.spec.columns = vec![1, 0];
    st.spec.mode = ClassifyMode::Categories;
    st.spec.instructions = "What is it about?".into();
    st.spec.categories[0].name = "billing".into();
    st.spec.categories[1].name = "other".into();
    st.name = "topic".into();
    st.name_edited = true;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.json");
    st.to_saved(&names(&["msg", "subject"]))
        .save(&path)
        .unwrap();
    let saved = octa::data::transform::classify::SavedClassifier::load(&path).unwrap();

    // Another table: `subject` sits elsewhere, `msg` is gone.
    let mut back = super::ClassifyState::default();
    let missing = back.apply_saved(saved, &names(&["id", "date", "subject"]));
    assert_eq!(missing, ["msg"]);
    assert_eq!(back.spec.columns, [2]);
    assert_eq!(back.spec.categories, st.spec.categories);
    assert_eq!(back.spec.instructions, st.spec.instructions);
    assert_eq!((back.name.as_str(), back.name_edited), ("topic", true));
    // The levels nobody filled in come back as the two rows to fill in.
    assert_eq!(back.spec.levels.len(), 2);

    std::fs::write(&path, "not json").unwrap();
    assert!(octa::data::transform::classify::SavedClassifier::load(&path).is_err());
}
