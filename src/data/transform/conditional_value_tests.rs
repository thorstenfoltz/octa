use super::*;
use crate::data::ColumnInfo;

fn table() -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "amount".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "region".into(),
                data_type: "Utf8".into(),
            },
        ],
        rows: vec![
            vec![CellValue::Int(150), CellValue::String("west".into())],
            vec![CellValue::Int(60), CellValue::String("east".into())],
            vec![CellValue::Int(10), CellValue::String("west".into())],
        ],
        ..DataTable::empty()
    }
}

fn rule(conditions: Vec<CaseCond>, match_all: bool, output: &str) -> CaseRule {
    CaseRule {
        conditions,
        match_all,
        output: output.into(),
    }
}

#[test]
fn several_conditions_combine_with_and_or_or() {
    // amount > 50 AND region = west: only row 0 (150, west).
    let both = vec![
        CaseCond::on(0, CondOp::Gt, &["50"]),
        CaseCond::on(1, CondOp::Eq, &["west"]),
    ];
    let spec = CaseSpec {
        rules: vec![rule(both.clone(), true, "yes")],
        else_output: "no".into(),
    };
    let yes = CellValue::String("yes".into());
    let no = CellValue::String("no".into());
    assert_eq!(
        build_case_column(&table(), &spec),
        vec![yes.clone(), no.clone(), no.clone()]
    );
    // OR: rows 0 (both), 1 (amount), 2 (region).
    let spec = CaseSpec {
        rules: vec![rule(both, false, "yes")],
        else_output: "no".into(),
    };
    assert_eq!(
        build_case_column(&table(), &spec),
        vec![yes.clone(), yes.clone(), yes]
    );
    // A condition without a column is ignored, not a failed test.
    let spec = CaseSpec {
        rules: vec![rule(
            vec![CaseCond::on(1, CondOp::Eq, &["east"]), CaseCond::new()],
            true,
            "e",
        )],
        else_output: "no".into(),
    };
    assert_eq!(
        build_case_column(&table(), &spec),
        vec![no.clone(), CellValue::String("e".into()), no]
    );
}

#[test]
fn several_values_mean_any_of_them_and_none_of_them_when_negated() {
    let x = CellValue::String("x".into());
    let no = CellValue::String("no".into());
    let run = |op, values: &[&str]| {
        let spec = CaseSpec {
            rules: vec![rule(vec![CaseCond::on(0, op, values)], true, "x")],
            else_output: "no".into(),
        };
        build_case_column(&table(), &spec)
    };
    // amount is 150, 60, 10.
    assert_eq!(
        run(CondOp::Eq, &["150", "", "10"]),
        vec![x.clone(), no.clone(), x.clone()]
    );
    assert_eq!(
        run(CondOp::Ne, &["150", "10"]),
        vec![no.clone(), x.clone(), no.clone()]
    );
    // Every entry blank still compares against "", as a single blank did.
    assert_eq!(run(CondOp::Eq, &["", ""]), vec![no.clone(), no.clone(), no]);
}

#[test]
fn numeric_if_elseif_else() {
    let spec = CaseSpec {
        rules: vec![
            rule(vec![CaseCond::on(0, CondOp::Gt, &["100"])], true, "high"),
            rule(vec![CaseCond::on(0, CondOp::Gt, &["50"])], true, "medium"),
        ],
        else_output: "low".into(),
    };
    let col = build_case_column(&table(), &spec);
    assert_eq!(
        col,
        vec![
            CellValue::String("high".into()),
            CellValue::String("medium".into()),
            CellValue::String("low".into()),
        ]
    );
}

#[test]
fn string_condition_and_numeric_output() {
    let spec = CaseSpec {
        rules: vec![rule(
            vec![CaseCond::on(1, CondOp::Eq, &["west"])],
            true,
            "1",
        )],
        else_output: "0".into(),
    };
    let col = build_case_column(&table(), &spec);
    assert_eq!(
        col,
        vec![CellValue::Int(1), CellValue::Int(0), CellValue::Int(1)]
    );
    assert_eq!(infer_case_column_type(&col), "Int64");
}

#[test]
fn rules_without_a_column_are_skipped() {
    let spec = CaseSpec {
        rules: vec![rule(vec![CaseCond::new()], true, "x")],
        else_output: "fallback".into(),
    };
    let col = build_case_column(&table(), &spec);
    assert!(
        col.iter()
            .all(|c| *c == CellValue::String("fallback".into()))
    );
}
