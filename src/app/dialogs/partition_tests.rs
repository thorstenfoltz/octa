use super::*;
use octa::data::partition::PartitionLayout;
use octa::data::{CellValue, ColumnInfo, DataTable};

fn table(values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "city".into(),
            data_type: "Utf8".into(),
        },
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
    ];
    t.rows = values
        .iter()
        .enumerate()
        .map(|(i, v)| vec![CellValue::String((*v).into()), CellValue::Int(i as i64)])
        .collect();
    t
}

fn flat() -> PartitionState {
    PartitionState {
        col: Some(0),
        layout: PartitionLayout::Flat,
        ..PartitionState::new()
    }
}

/// The preview is only useful if it is the user's own data, and short
/// enough to read at a glance.
#[test]
fn preview_takes_up_to_three_real_values_skipping_repeats() {
    let t = table(&["Berlin", "Berlin", "Hamburg", "Munich", "Koeln"]);
    assert_eq!(
        preview_lines(&t, &flat(), "csv"),
        vec!["berlin.csv", "hamburg.csv", "munich.csv"]
    );
}

/// Naming itself is `partition_path`'s job and is tested there; what
/// matters here is that the preview calls it rather than reimplementing
/// it, so the two cannot drift.
#[test]
fn preview_matches_the_shared_path_builder() {
    let t = table(&["New York", "Berlin"]);
    for layout in PartitionLayout::ALL {
        let shown = preview_lines(
            &t,
            &PartitionState {
                col: Some(0),
                layout: *layout,
                ..PartitionState::new()
            },
            "parquet",
        );
        let expected: Vec<String> = ["New York", "Berlin"]
            .iter()
            .map(|v| octa::data::partition::partition_path(*layout, "city", v, "parquet"))
            .collect();
        assert_eq!(shown, expected, "{layout:?}");
    }
}

/// The counter that disambiguates colliding flat stems has to be visible
/// in the preview too, or the preview promises names the writer will not
/// produce.
#[test]
fn preview_shows_the_flat_collision_counter() {
    let t = table(&["New York", "new-york", "NEW_YORK"]);
    assert_eq!(
        preview_lines(&t, &flat(), "csv"),
        vec!["new_york.csv", "new_york_2.csv", "new_york_3.csv"]
    );
}

#[test]
fn no_values_means_no_preview_and_the_dialog_says_so() {
    assert!(preview_lines(&table(&[]), &flat(), "csv").is_empty());
    let blanks = table(&["", ""]);
    assert!(preview_lines(&blanks, &flat(), "csv").is_empty());
}

#[test]
fn preview_ext_prefers_the_override_then_the_source() {
    let mut t = table(&["a"]);
    t.source_path = Some("/tmp/sales.parquet".into());
    assert_eq!(preview_ext(&t, ""), "parquet");
    assert_eq!(preview_ext(&t, ".tsv"), "tsv");
    assert_eq!(preview_ext(&t, "json"), "json");
    t.source_path = None;
    assert_eq!(
        preview_ext(&t, ""),
        "csv",
        "a tab with no file still shows something"
    );
}

/// The dialog's preview comes from the same function the writer uses, so a
/// row limit shows part numbers before Apply.
#[test]
fn preview_shows_part_numbers_under_a_row_limit() {
    let t = table(&["Berlin", "Tokyo"]);
    let st = PartitionState {
        col: Some(0),
        max_rows_buf: "10".into(),
        ..PartitionState::new()
    };
    let lines = preview_lines(&t, &st, "csv");
    assert_eq!(lines, ["berlin_part_0001.csv", "tokyo_part_0001.csv"]);
}

#[test]
fn no_column_and_no_limit_cannot_apply() {
    let st = PartitionState {
        col: None,
        ..PartitionState::new()
    };
    assert!(st.spec(PartitionLayout::Flat).is_err());
}

#[test]
fn limits_parse_commas_and_decimal_megabytes() {
    let st = PartitionState {
        col: None,
        max_rows_buf: "10,000".into(),
        max_mb_buf: "2,5".into(),
        ..PartitionState::new()
    };
    let spec = st.spec(PartitionLayout::Flat).unwrap();
    assert_eq!(spec.max_rows, Some(10_000));
    assert_eq!(spec.max_bytes, Some(2_500_000));
    for bad in ["0", "abc", "-1"] {
        let st = PartitionState {
            col: None,
            max_mb_buf: bad.into(),
            ..PartitionState::new()
        };
        assert!(st.spec(PartitionLayout::Flat).is_err(), "{bad}");
    }
}
