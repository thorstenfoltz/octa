use super::*;

fn options() -> Options {
    Options {
        question: "What is this about?".into(),
        columns: Some("msg".into()),
        categories: Vec::new(),
        levels: None,
        name: None,
    }
}

#[test]
fn the_flags_pick_the_kind_of_question() {
    assert_eq!(spec_from(&options()).unwrap().mode, ClassifyMode::YesNo);

    let mut o = options();
    o.categories = vec!["billing=Invoices, payments".into(), " other ".into()];
    let spec = spec_from(&o).unwrap();
    assert_eq!(spec.mode, ClassifyMode::Categories);
    assert_eq!(spec.categories[0].name, "billing");
    // A comma in a description survives: one category per flag.
    assert_eq!(spec.categories[0].description, "Invoices, payments");
    assert_eq!(spec.categories[1].name, "other");

    let mut o = options();
    o.levels = Some("low, medium ,high".into());
    let spec = spec_from(&o).unwrap();
    assert_eq!(spec.mode, ClassifyMode::Scale);
    assert_eq!(spec.levels, ["low", "medium", "high"]);

    o.categories = vec!["a".into()];
    assert!(spec_from(&o).is_err());
}

#[test]
fn the_report_names_what_got_no_answer() {
    let clean = RunReport {
        sent: 3,
        input_tokens: 30,
        ..RunReport::default()
    };
    assert_eq!(
        report_line(&clean),
        "3 distinct values sent to TypeSafe, 30 input tokens"
    );
    let partial = RunReport {
        sent: 3,
        failed: 1,
        first_error: Some("HTTP 500".into()),
        input_tokens: 20,
    };
    let line = report_line(&partial);
    assert!(line.contains("1 got no answer"), "{line}");
    assert!(line.contains("HTTP 500"), "{line}");
}
