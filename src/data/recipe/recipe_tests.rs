//! Unit tests for recipes. Included via `#[path]` from `recipe/mod.rs`.

use super::*;
use crate::data::{CellValue, ColumnInfo};

fn table(headers: &[&str], rows: &[&[&str]]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = headers
        .iter()
        .map(|h| ColumnInfo {
            name: (*h).to_string(),
            data_type: "Utf8".to_string(),
        })
        .collect();
    t.rows = rows
        .iter()
        .map(|r| r.iter().map(|s| CellValue::String(s.to_string())).collect())
        .collect();
    t
}

fn cells(t: &DataTable) -> Vec<Vec<String>> {
    (0..t.row_count())
        .map(|r| {
            (0..t.col_count())
                .map(|c| t.get(r, c).map(|v| v.to_string()).unwrap_or_default())
                .collect()
        })
        .collect()
}

fn headers(t: &DataTable) -> Vec<String> {
    names(t)
}

fn sample() -> Recipe {
    Recipe::new(vec![
        RecipeStep::Rename(Rename {
            renames: vec![RenamePair {
                from: "Kunde".into(),
                to: "customer".into(),
            }],
        }),
        RecipeStep::DropDuplicates(DropDuplicates::new(
            vec!["id".into()],
            crate::data::dedupe::KeepWhich::First,
        )),
        RecipeStep::Sort(Sort {
            by: vec![SortKey {
                column: "customer".into(),
                descending: false,
            }],
        }),
    ])
}

#[test]
fn a_recipe_survives_a_round_trip_through_toml() {
    let r = sample();
    let text = r.to_toml().unwrap();
    assert!(text.contains("step = \"rename\""), "{text}");
    assert_eq!(Recipe::from_toml(&text).unwrap(), r);
}

#[test]
fn replaying_on_next_months_file_goes_by_name_not_position() {
    // Same columns, different order and different rows.
    let mut t = table(
        &["id", "Kunde"],
        &[&["2", "Zed"], &["1", "Anna"], &["2", "Zed"]],
    );
    let out = apply_recipe(&mut t, &sample());
    assert!(out.iter().all(|o| o.error.is_none()), "{out:?}");
    assert_eq!(headers(&t), ["id", "customer"]);
    assert_eq!(cells(&t), vec![vec!["1", "Anna"], vec!["2", "Zed"]]);
}

#[test]
fn a_step_whose_column_is_gone_is_skipped_and_the_rest_still_runs() {
    let mut t = table(&["id", "name"], &[&["1", "b"], &["2", "a"]]);
    let out = apply_recipe(&mut t, &sample());
    assert!(
        out[0]
            .error
            .as_deref()
            .unwrap()
            .contains("`Kunde` not found")
    );
    assert!(out[1].error.is_none());
    // The sort names `customer`, which only the skipped rename would create.
    assert!(out[2].error.is_some());
    assert_eq!(cells(&t), vec![vec!["1", "b"], vec!["2", "a"]]);
}

#[test]
fn missing_columns_ignores_names_an_earlier_rename_in_the_step_creates() {
    let step = RecipeStep::Rename(Rename {
        renames: vec![
            RenamePair {
                from: "a".into(),
                to: "b".into(),
            },
            RenamePair {
                from: "b".into(),
                to: "c".into(),
            },
        ],
    });
    let t = table(&["a"], &[]);
    assert!(step.missing_columns(&t).is_empty());
    let mut t = t;
    step.apply(&mut t).unwrap();
    assert_eq!(headers(&t), ["c"]);
}

#[test]
fn transform_steps_name_and_place_new_columns_like_the_dialog() {
    let mut t = table(&["full", "x"], &[&["a-b", "1"]]);
    let recipe = Recipe::new(vec![
        RecipeStep::Split(Split {
            column: "full".into(),
            by: "delimiter".into(),
            value: "-".into(),
            new_name: "part".into(),
            position: None,
        }),
        RecipeStep::Extract(Extract {
            column: "full".into(),
            pattern: "^(.)".into(),
            new_name: String::new(),
            position: None,
        }),
        RecipeStep::Merge(Merge {
            columns: vec!["part_2".into(), "part_1".into()],
            separator: "+".into(),
            new_name: String::new(),
            position: Some(1),
        }),
    ]);
    let out = apply_recipe(&mut t, &recipe);
    assert!(out.iter().all(|o| o.error.is_none()), "{out:?}");
    assert_eq!(
        headers(&t),
        ["merged", "full", "full_extracted", "part_1", "part_2", "x"]
    );
    assert_eq!(cells(&t)[0], ["b+a", "a-b", "a", "a", "b", "1"]);
}

#[test]
fn a_replay_is_one_undo_step_when_the_caller_coalesces() {
    let mut t = table(
        &["id", "Kunde"],
        &[&["2", "Zed"], &["1", "Anna"], &["2", "Zed"]],
    );
    let before = cells(&t);
    let start = t.undo_stack.len();
    apply_recipe(&mut t, &sample());
    t.coalesce_undo_since(start);
    assert_eq!(t.undo_stack.len(), start + 1);
    t.undo();
    t.apply_edits();
    assert_eq!(cells(&t), before);
    assert_eq!(headers(&t), ["id", "Kunde"]);
}

#[test]
fn a_newer_format_is_refused_with_a_reason() {
    let err = Recipe::from_toml("version = 99\n").unwrap_err();
    assert!(format!("{err:#}").contains("newer Octa"));
}

fn set(key: &[&str], edits: &[(&[&str], &str, Option<&str>)]) -> RecipeStep {
    RecipeStep::SetCells(SetCells {
        key: key.iter().map(|s| s.to_string()).collect(),
        cells: edits
            .iter()
            .map(|(row, column, value)| CellEdit {
                row: row.iter().map(|s| s.to_string()).collect(),
                column: column.to_string(),
                value: value.map(str::to_string),
            })
            .collect(),
    })
}

#[test]
fn a_hand_edit_finds_its_row_by_id_wherever_it_moved() {
    let step = set(&["id"], &[(&["7"], "price", Some("9.99"))]);
    // Next month: row 7 is now the last row, and there are more rows.
    let mut t = table(
        &["id", "price"],
        &[&["3", "1.00"], &["5", "2.00"], &["7", "3.00"]],
    );
    step.apply(&mut t).unwrap();
    assert_eq!(cells(&t)[2], ["7", "9.99"]);
    assert_eq!(cells(&t)[0], ["3", "1.00"]);
}

#[test]
fn a_hand_edit_whose_row_is_gone_writes_the_rest_and_names_the_missing_one() {
    let step = set(
        &["id"],
        &[
            (&["1"], "v", Some("new")),
            (&["99"], "v", Some("lost")),
            (&["2"], "v", None),
        ],
    );
    let mut t = table(&["id", "v"], &[&["1", "a"], &["2", "b"]]);
    let err = format!("{:#}", step.apply(&mut t).unwrap_err());
    assert!(err.contains("1 of 3") && err.contains("id = 99"), "{err}");
    assert_eq!(cells(&t), vec![vec!["1", "new"], vec!["2", ""]]);
}

#[test]
fn a_two_column_key_matches_on_both() {
    let step = set(&["store", "day"], &[(&["B", "mon"], "sales", Some("5"))]);
    let mut t = table(
        &["store", "day", "sales"],
        &[&["A", "mon", "1"], &["B", "mon", "2"], &["B", "tue", "3"]],
    );
    step.apply(&mut t).unwrap();
    let got: Vec<String> = cells(&t).into_iter().map(|r| r[2].clone()).collect();
    assert_eq!(got, ["1", "5", "3"]);
}

#[test]
fn the_row_key_is_guessed_only_when_it_plainly_is_one() {
    // An id-named unique column wins even when it is not first.
    let t = table(&["name", "customer_id"], &[&["a", "1"], &["a", "2"]]);
    assert_eq!(guess_row_key(&t).as_deref(), Some("customer_id"));
    // No id name: a unique first column is taken.
    let t = table(&["sku", "price"], &[&["x1", "1"], &["x2", "1"]]);
    assert_eq!(guess_row_key(&t).as_deref(), Some("sku"));
    // A unique column that is not first and not named like an ID is not a
    // guess worth making: ask.
    let t = table(&["city", "price"], &[&["Oslo", "1.50"], &["Oslo", "2.75"]]);
    assert_eq!(guess_row_key(&t), None);
    // An "id" with repeats is no ID either.
    let t = table(&["id", "v"], &[&["1", "a"], &["1", "b"]]);
    assert_eq!(guess_row_key(&t), None);
}

#[test]
fn a_set_cells_step_survives_toml() {
    let r = Recipe::new(vec![set(
        &["id"],
        &[(&["7"], "price", Some("9.99")), (&["8"], "price", None)],
    )]);
    assert_eq!(Recipe::from_toml(&r.to_toml().unwrap()).unwrap(), r);
}

#[test]
fn tidy_id_rewrites_valid_ids_and_never_touches_invalid_ones() {
    let mut t = table(
        &["iban"],
        &[
            &["de89 3704 0044 0532 0130 00"],
            &["DE89370400440532013001"],
            &[""],
        ],
    );
    let r = Recipe::new(vec![RecipeStep::TidyId(TidyId {
        column: "iban".into(),
        kind: "iban".into(),
    })]);
    let back = Recipe::from_toml(&r.to_toml().unwrap()).unwrap();
    assert_eq!(back, r);
    assert!(
        apply_recipe(&mut t, &back)
            .iter()
            .all(|o| o.error.is_none())
    );
    assert_eq!(
        cells(&t),
        vec![
            vec!["DE89 3704 0044 0532 0130 00"],
            vec!["DE89370400440532013001"],
            vec![""],
        ]
    );
    let bad = Recipe::new(vec![RecipeStep::TidyId(TidyId {
        column: "iban".into(),
        kind: "phone".into(),
    })]);
    assert!(apply_recipe(&mut t, &bad)[0].error.is_some());
}
