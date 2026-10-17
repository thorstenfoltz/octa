use super::*;
use crate::data::ColumnInfo;
use crate::data::validation::{ValidationKind, ValidationRule};

fn cols() -> Vec<ColumnInfo> {
    vec![
        ColumnInfo {
            name: "order_id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "amount".into(),
            data_type: "Float64".into(),
        },
    ]
}

#[test]
fn round_trips_through_toml() {
    let rules = vec![
        ValidationRule {
            column: Some(0),
            kind: ValidationKind::Unique,
        },
        ValidationRule {
            column: Some(1),
            kind: ValidationKind::Range {
                min: Some(0.0),
                max: Some(100.0),
            },
        },
        ValidationRule {
            column: None,
            kind: ValidationKind::NotNull,
        },
        ValidationRule {
            column: Some(0),
            kind: ValidationKind::Id(crate::data::id_checks::IdKind::VatId),
        },
    ];
    let file = to_named(&rules, &cols());
    let text = toml::to_string_pretty(&file).unwrap();
    let back: RulesFile = toml::from_str(&text).unwrap();
    let (resolved, unknown) = resolve(&back, &cols());
    assert!(unknown.is_empty());
    assert_eq!(resolved, rules);
}

#[test]
fn an_unknown_column_is_reported_not_skipped() {
    let text = r#"
[[rule]]
column = "nope"
kind = "unique"
"#;
    let file: RulesFile = toml::from_str(text).unwrap();
    let (resolved, unknown) = resolve(&file, &cols());
    assert!(resolved.is_empty());
    assert_eq!(unknown, vec!["nope".to_string()]);
}

#[test]
fn a_rule_without_a_column_applies_to_every_column() {
    let text = r#"
[[rule]]
kind = "not_null"
"#;
    let file: RulesFile = toml::from_str(text).unwrap();
    let (resolved, unknown) = resolve(&file, &cols());
    assert!(unknown.is_empty());
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].column, None);
}

#[test]
fn an_unknown_kind_resolves_to_nothing_and_is_reported() {
    let text = r#"
[[rule]]
column = "amount"
kind = "sparkles"
"#;
    let file: RulesFile = toml::from_str(text).unwrap();
    let (resolved, unknown) = resolve(&file, &cols());
    assert!(resolved.is_empty());
    assert!(
        unknown.iter().any(|u| u.contains("sparkles")),
        "got {unknown:?}"
    );
}

use crate::data::conditional_format::CondOp;

fn cols_of(names: &[&str]) -> Vec<ColumnInfo> {
    names
        .iter()
        .map(|n| ColumnInfo {
            name: (*n).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect()
}

#[test]
fn every_comparison_round_trips_through_the_file() {
    let columns = cols_of(&["end", "start"]);
    for &op in CondOp::COMPARISONS {
        let rules = vec![ValidationRule {
            column: Some(0),
            kind: ValidationKind::CompareColumn { op, other: 1 },
        }];
        let file = to_named(&rules, &columns);
        let text = toml::to_string_pretty(&file).unwrap();
        let back: RulesFile = toml::from_str(&text).unwrap();
        let (resolved, problems) = resolve(&back, &columns);
        assert!(problems.is_empty(), "{op:?}: {problems:?}");
        assert_eq!(resolved, rules, "{op:?}");
    }
}

#[test]
fn one_of_round_trips_through_the_file() {
    let columns = cols_of(&["status"]);
    let rules = vec![ValidationRule {
        column: Some(0),
        kind: ValidationKind::OneOf(vec!["open".into(), "closed".into()]),
    }];
    let text = toml::to_string_pretty(&to_named(&rules, &columns)).unwrap();
    assert!(text.contains("one_of"), "{text}");
    let (resolved, problems) = resolve(&toml::from_str(&text).unwrap(), &columns);
    assert!(problems.is_empty());
    assert_eq!(resolved, rules);
}

#[test]
fn a_compare_rule_that_cannot_run_is_reported() {
    let columns = cols_of(&["end", "start"]);
    let file: RulesFile = toml::from_str(
        "[[rule]]\ncolumn = \"end\"\nkind = \"compare\"\nop = \">=\"\nother_column = \"begin\"\n\
         [[rule]]\ncolumn = \"end\"\nkind = \"compare\"\nop = \"=>\"\nother_column = \"start\"\n\
         [[rule]]\nkind = \"compare\"\nop = \">=\"\nother_column = \"start\"\n\
         [[rule]]\ncolumn = \"end\"\nkind = \"one_of\"\n",
    )
    .unwrap();
    let (resolved, problems) = resolve(&file, &columns);
    assert!(resolved.is_empty());
    assert_eq!(problems.len(), 4, "{problems:?}");
    assert!(problems[0].contains("begin"));
}
