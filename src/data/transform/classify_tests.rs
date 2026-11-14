use super::*;
use std::sync::atomic::AtomicUsize;

fn table(cols: &[&str], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).into(),
            data_type: "Utf8".into(),
        })
        .collect();
    t.rows = rows;
    t
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

fn categories() -> ClassifySpec {
    ClassifySpec {
        columns: vec![0],
        mode: ClassifyMode::Categories,
        instructions: "What is this message about?".into(),
        categories: vec![
            Category {
                name: "billing".into(),
                description: "Invoices and payments".into(),
            },
            Category {
                name: "other".into(),
                description: String::new(),
            },
        ],
        levels: Vec::new(),
    }
}

/// Answers "billing" for a state mentioning an invoice, "other" otherwise,
/// and counts the calls.
fn fake_choice(calls: &AtomicUsize) -> impl Fn(&Value) -> anyhow::Result<Value> + Sync + '_ {
    move |body| {
        calls.fetch_add(1, Ordering::Relaxed);
        let choice = if body["state"].to_string().contains("invoice") {
            "billing"
        } else {
            "other"
        };
        Ok(json!({
            "model": "jev-1.14.0",
            "answers": {"q": {"type": "choice", "choice": choice, "confidence": 0.9}},
            "usage": {"input_tokens": 10, "output_tokens": 0}
        }))
    }
}

#[test]
fn request_bodies_match_the_documented_shapes() {
    let state = json!({"msg": "hi"});
    let body = request_body("jev-latest", &state, &categories());
    assert_eq!(
        body,
        json!({
            "model": "jev-latest",
            "state": {"msg": "hi"},
            "questions": {"q": {
                "type": "choice",
                "instructions": "What is this message about?",
                // An empty description falls back to the name.
                "criteria": {"billing": "Invoices and payments", "other": "other"}
            }}
        })
    );

    let yes_no = ClassifySpec {
        mode: ClassifyMode::YesNo,
        ..categories()
    };
    assert_eq!(
        request_body("m", &state, &yes_no)["questions"]["q"],
        json!({"type": "noul", "instructions": "What is this message about?"})
    );

    let scale = ClassifySpec {
        mode: ClassifyMode::Scale,
        levels: vec!["Can wait".into(), "Today".into()],
        ..categories()
    };
    assert_eq!(
        request_body("m", &state, &scale)["questions"]["q"]["criteria"],
        json!(["Can wait", "Today"])
    );
}

#[test]
fn each_distinct_value_is_sent_once_and_fans_back_to_its_rows() {
    let mut t = table(
        &["msg"],
        vec![
            vec![s("my invoice is wrong")],
            vec![s("hello")],
            vec![s("my invoice is wrong")],
            vec![CellValue::Null],
            vec![s("hello")],
        ],
    );
    let calls = AtomicUsize::new(0);
    let (name, report) = add_classify_columns(
        &mut t,
        &["msg".to_string()],
        categories(),
        None,
        "jev-latest",
        3,
        &fake_choice(&calls),
    )
    .unwrap();

    assert_eq!(name, "category");
    // Two distinct values; the NULL row sends nothing.
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(report.sent, 2);
    assert_eq!(report.failed, 0);
    assert_eq!(report.input_tokens, 20);
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["msg", "category", "category_confidence"]);
    let got: Vec<String> = t.rows.iter().map(|r| r[1].to_string()).collect();
    assert_eq!(got, ["billing", "other", "billing", "", "other"]);
    assert_eq!(t.rows[0][2], CellValue::Float(0.9));
    assert_eq!(t.rows[3][2], CellValue::Null);
}

#[test]
fn yes_no_and_scale_answers_land_typed() {
    let yes_no = ClassifySpec {
        mode: ClassifyMode::YesNo,
        ..categories()
    };
    let a = parse_answer(
        &json!({"answers": {"q": {"type": "noul", "noul": 0.2}}}),
        &yes_no,
    )
    .unwrap();
    assert_eq!(a.value, CellValue::Bool(false));
    assert_eq!(a.certainty, CellValue::Float(0.2));
    let [answer, sure] = result_columns(ClassifyMode::YesNo, "urgent");
    assert_eq!(
        (answer.name.as_str(), answer.data_type.as_str()),
        ("urgent", "Boolean")
    );
    assert_eq!(sure.name, "urgent_probability");

    let scale = ClassifySpec {
        mode: ClassifyMode::Scale,
        levels: vec!["low".into(), "high".into()],
        ..categories()
    };
    let a = parse_answer(
        &json!({"answers": {"q": {"type": "score", "score": 0.7, "confidence": 0.6}}}),
        &scale,
    )
    .unwrap();
    assert_eq!(a.value, CellValue::Float(0.7));
    assert_eq!(a.certainty, CellValue::Float(0.6));

    // A body without the expected field is an error, never a silent NULL.
    assert!(parse_answer(&json!({"answers": {"q": {}}}), &scale).is_err());
}

#[test]
fn a_failed_value_stays_empty_and_is_reported() {
    let mut t = table(
        &["msg"],
        vec![vec![s("invoice")], vec![s("boom")], vec![s("invoice")]],
    );
    let calls = AtomicUsize::new(0);
    let ok = fake_choice(&calls);
    let send = |body: &Value| {
        if body["state"].to_string().contains("boom") {
            anyhow::bail!("HTTP 529: overloaded");
        }
        ok(body)
    };
    let (_, report) = add_classify_columns(
        &mut t,
        &["msg".to_string()],
        categories(),
        Some("topic"),
        "m",
        1,
        &send,
    )
    .unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.first_error.as_deref(), Some("HTTP 529: overloaded"));
    assert_eq!(t.rows[0][1], s("billing"));
    assert_eq!(t.rows[1][1], CellValue::Null);
}

#[test]
fn a_run_that_only_fails_gives_up_early_and_adds_nothing() {
    let rows = (0..50).map(|i| vec![s(&format!("v{i}"))]).collect();
    let mut t = table(&["msg"], rows);
    let calls = AtomicUsize::new(0);
    let send = |_: &Value| -> anyhow::Result<Value> {
        calls.fetch_add(1, Ordering::Relaxed);
        anyhow::bail!("HTTP 401: invalid API key")
    };
    let err = add_classify_columns(
        &mut t,
        &["msg".to_string()],
        categories(),
        None,
        "m",
        1,
        &send,
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("invalid API key"), "{err:#}");
    assert_eq!(calls.load(Ordering::Relaxed), 5);
    assert_eq!(t.columns.len(), 1);
}

#[test]
fn cancel_stops_the_run() {
    let t = table(&["msg"], vec![vec![s("a")], vec![s("b")]]);
    let calls = AtomicUsize::new(0);
    let progress = Progress::default();
    progress.cancel.store(true, Ordering::Relaxed);
    let err =
        classify_table(&t, &categories(), "m", 2, &fake_choice(&calls), &progress).unwrap_err();
    assert_eq!(err.to_string(), "cancelled");
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn bad_specs_and_taken_names_are_refused_before_anything_is_sent() {
    let calls = AtomicUsize::new(0);
    let send = fake_choice(&calls);
    let mut t = table(&["msg", "category"], vec![vec![s("a"), s("b")]]);
    let cols = ["msg".to_string()];

    let err = add_classify_columns(&mut t, &cols, categories(), None, "m", 1, &send).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");

    let one = ClassifySpec {
        categories: categories().categories[..1].to_vec(),
        ..categories()
    };
    assert_eq!(one.problem(), Some("classify.need_categories"));
    assert!(add_classify_columns(&mut t, &cols, one, Some("x"), "m", 1, &send).is_err());

    let mut twice = categories();
    twice.categories[1].name = " billing ".into();
    assert_eq!(twice.problem(), Some("classify.duplicate_category"));

    let mut no_question = categories();
    no_question.instructions = "  ".into();
    assert_eq!(no_question.problem(), Some("classify.need_question"));

    let scale = ClassifySpec {
        mode: ClassifyMode::Scale,
        levels: vec!["x".into(); MAX_LEVELS + 1],
        ..categories()
    };
    assert_eq!(scale.problem(), Some("classify.too_many_levels"));

    assert!(
        add_classify_columns(
            &mut t,
            &["nope".to_string()],
            categories(),
            None,
            "m",
            1,
            &send
        )
        .is_err()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn mode_names_parse_as_the_cli_spells_them() {
    assert_eq!(
        ClassifyMode::parse("categories"),
        Some(ClassifyMode::Categories)
    );
    assert_eq!(ClassifyMode::parse("yes-no"), Some(ClassifyMode::YesNo));
    assert_eq!(ClassifyMode::parse("Scale"), Some(ClassifyMode::Scale));
    assert_eq!(ClassifyMode::parse("chat"), None);
}
