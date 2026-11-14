use super::*;

fn t(rows: &[(&str, &str, f64)]) -> DataTable {
    DataTable {
        columns: vec![
            ColumnInfo {
                name: "who".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "when".into(),
                data_type: "Date32".into(),
            },
            ColumnInfo {
                name: "amount".into(),
                data_type: "Float64".into(),
            },
        ],
        rows: rows
            .iter()
            .map(|(w, d, a)| {
                vec![
                    CellValue::String((*w).into()),
                    CellValue::Date((*d).into()),
                    CellValue::Float(*a),
                ]
            })
            .collect(),
        ..DataTable::empty()
    }
}

fn sample() -> DataTable {
    t(&[
        ("a", "2024-01-05", 10.0),
        ("a", "2024-02-10", 20.0),
        ("b", "2024-01-20", 30.0),
        ("b", "2024-03-01", 40.0),
        ("c", "2024-02-02", 50.0),
    ])
}

fn spec(show: Show) -> CohortSpec {
    CohortSpec {
        who: 0,
        when: 1,
        period: Period::Month,
        show,
        sum_col: Some(2),
    }
}

fn row(w: &DataTable, r: usize) -> Vec<CellValue> {
    (0..w.col_count())
        .map(|c| w.get(r, c).cloned().unwrap())
        .collect()
}

#[test]
fn share_count_and_sum() {
    let w = wide(&collect(&sample(), &spec(Show::Share)).unwrap());
    let names: Vec<&str> = w.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["cohort", "size", "month_0", "month_1", "month_2"]);
    use CellValue::*;
    assert_eq!(
        row(&w, 0),
        [
            String("2024-01".into()),
            Int(2),
            Float(100.0),
            Float(50.0),
            Float(50.0)
        ]
    );
    assert_eq!(
        row(&w, 1),
        [
            String("2024-02".into()),
            Int(1),
            Float(100.0),
            Float(0.0),
            Null
        ]
    );
    let w = wide(&collect(&sample(), &spec(Show::Count)).unwrap());
    assert_eq!(row(&w, 0)[2..], [Int(2), Int(1), Int(1)]);
    assert_eq!(row(&w, 1)[2..], [Int(1), Int(0), Null]);
    let w = wide(&collect(&sample(), &spec(Show::Sum)).unwrap());
    assert_eq!(row(&w, 0)[2..], [Float(40.0), Float(20.0), Float(40.0)]);
    assert_eq!(row(&w, 1)[2..], [Float(50.0), Float(0.0), Null]);
}

#[test]
fn weeks_start_on_monday_across_the_year_end() {
    let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    assert_eq!(Period::Week.start(d("2025-01-01")), d("2024-12-30"));
    assert_eq!(Period::Week.label(d("2024-12-30")), "2025-W01");
    assert_eq!(Period::Week.between(d("2024-12-30"), d("2025-01-06")), 1);
}

#[test]
fn quarters_and_years() {
    let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    assert_eq!(Period::Quarter.start(d("2024-05-17")), d("2024-04-01"));
    assert_eq!(Period::Quarter.label(d("2024-04-01")), "2024-Q2");
    assert_eq!(Period::Quarter.between(d("2024-04-01"), d("2025-01-01")), 3);
    assert_eq!(Period::Year.offset(d("2023-01-01"), 2), d("2025-01-01"));
}

#[test]
fn rows_without_who_or_date_are_skipped_and_counted() {
    let mut tb = sample();
    tb.rows.push(vec![
        CellValue::String("".into()),
        CellValue::Date("2024-01-01".into()),
        CellValue::Float(1.0),
    ]);
    tb.rows.push(vec![
        CellValue::String("d".into()),
        CellValue::String("soon".into()),
        CellValue::Float(1.0),
    ]);
    assert_eq!(collect(&tb, &spec(Show::Count)).unwrap().skipped, 2);
}

#[test]
fn the_long_form_has_one_row_per_filled_cell() {
    let w = wide(&collect(&sample(), &spec(Show::Count)).unwrap());
    let l = long_from_wide(&w);
    assert_eq!(l.row_count(), 5);
    assert_eq!(
        l.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["cohort", "period", "value"]
    );
}

#[test]
fn no_usable_rows_is_an_error() {
    assert_eq!(
        collect(&t(&[]), &spec(Show::Count)).err(),
        Some(CohortError::NoRows)
    );
    let s = CohortSpec {
        sum_col: None,
        ..spec(Show::Sum)
    };
    assert_eq!(collect(&sample(), &s).err(), Some(CohortError::NoSumColumn));
}
