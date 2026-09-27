//! Unit tests for [`test_data`](super). Included via `#[path]`.

use super::*;

fn table(cols: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
    let mut t = DataTable::empty();
    t.columns = cols
        .iter()
        .map(|(n, ty)| ColumnInfo {
            name: n.to_string(),
            data_type: ty.to_string(),
        })
        .collect();
    t.rows = rows;
    t
}

fn s(v: &str) -> CellValue {
    CellValue::String(v.into())
}

/// 200 customers: id 1..200, a skewed amount, a segment category with
/// empties, an order code, a signup date.
fn customers() -> DataTable {
    let rows = (1..=200)
        .map(|i| {
            vec![
                CellValue::Int(i),
                CellValue::Float(if i % 10 == 0 {
                    1000.5
                } else {
                    10.25 + i as f64
                }),
                if i % 4 == 0 {
                    CellValue::Null
                } else {
                    s(["retail", "retail", "wholesale"][i as usize % 3])
                },
                s(&format!("ORD-{i:05}")),
                CellValue::Date(format!("2025-{:02}-{:02}", i % 12 + 1, i % 28 + 1)),
            ]
        })
        .collect();
    table(
        &[
            ("id", "Int64"),
            ("amount", "Float64"),
            ("segment", "Utf8"),
            ("code", "Utf8"),
            ("signup", "Date32"),
        ],
        rows,
    )
}

fn orders() -> DataTable {
    let rows = (1..=500)
        .map(|i| vec![CellValue::Int(i), CellValue::Int((i * 7) % 200 + 1)])
        .collect();
    table(&[("order_id", "Int64"), ("customer_id", "Int64")], rows)
}

fn col(t: &DataTable, name: &str) -> Vec<CellValue> {
    let c = t.columns.iter().position(|c| c.name == name).unwrap();
    t.rows.iter().map(|r| r[c].clone()).collect()
}

#[test]
fn the_same_seed_gives_the_same_rows_and_another_seed_does_not() {
    let plan = vec![profile_table(&customers(), "customers")];
    let a = generate(&plan, 7).unwrap();
    let b = generate(&plan, 7).unwrap();
    let c = generate(&plan, 8).unwrap();
    assert_eq!(a[0].rows, b[0].rows);
    assert_ne!(a[0].rows, c[0].rows);
}

#[test]
fn each_column_gets_a_generator_that_fits_it() {
    let p = profile_table(&customers(), "customers");
    let ids: Vec<&str> = p.columns.iter().map(|c| c.generator.id()).collect();
    assert_eq!(
        ids,
        ["running_number", "number", "category", "pattern", "date"]
    );
    assert!(p.columns[2].generator.keeps_real_values());
    assert!((p.columns[2].null_share - 0.25).abs() < 1e-9);
}

#[test]
fn numbers_dates_and_codes_stay_in_shape() {
    let mut plan = vec![profile_table(&customers(), "customers")];
    plan[0].rows = 2000;
    let t = &generate(&plan, 1).unwrap()[0];
    assert_eq!(t.row_count(), 2000);

    let ids: HashSet<String> = col(t, "id").iter().map(|v| v.to_string()).collect();
    assert_eq!(ids.len(), 2000, "a running number stays unique");

    for v in col(t, "amount") {
        let x = as_f64(&v).unwrap();
        assert!((11.25..=1000.5).contains(&x), "{x} outside the real range");
    }
    for v in col(t, "code") {
        let v = v.to_string();
        assert!(v.starts_with("ORD-") && v.len() == 9, "{v}");
    }
    for v in col(t, "signup") {
        let d = parse_date(&v.to_string()).unwrap();
        assert_eq!(d.year(), 2025);
    }
}

#[test]
fn empty_cells_come_back_at_about_the_real_rate() {
    let mut plan = vec![profile_table(&customers(), "customers")];
    plan[0].rows = 4000;
    let t = &generate(&plan, 3).unwrap()[0];
    let seg = col(t, "segment");
    let empty = seg.iter().filter(|v| is_blank(v)).count() as f64 / seg.len() as f64;
    assert!((0.21..0.29).contains(&empty), "{empty}");
    for v in seg.iter().filter(|v| !is_blank(v)) {
        assert!(["retail", "wholesale"].contains(&v.to_string().as_str()));
    }
}

#[test]
fn a_renamed_category_hides_the_real_values() {
    let mut plan = vec![profile_table(&customers(), "customers")];
    let Generator::Category { values } = &plan[0].columns[2].generator else {
        panic!("category expected")
    };
    plan[0].columns[2].generator = renamed_category(values);
    assert!(!plan[0].columns[2].generator.keeps_real_values());
    let t = &generate(&plan, 3).unwrap()[0];
    for v in col(t, "segment").iter().filter(|v| !is_blank(v)) {
        assert!(v.to_string().starts_with("value_"));
    }
}

#[test]
fn linked_tables_still_join() {
    let (c, o) = (customers(), orders());
    let links = suggest_links(&[&c, &o]);
    assert_eq!(
        links,
        vec![Link {
            parent: (0, 0),
            child: (1, 1)
        }]
    );
    let mut plans = vec![profile_table(&c, "customers"), profile_table(&o, "orders")];
    apply_links(&mut plans, &links);
    plans[0].rows = 50;
    plans[1].rows = 300;
    let out = generate(&plans, 9).unwrap();
    let parent: HashSet<String> = col(&out[0], "id").iter().map(|v| v.to_string()).collect();
    for v in col(&out[1], "customer_id") {
        assert!(parent.contains(&v.to_string()), "{v} has no parent");
    }
}

#[test]
fn fake_ibans_and_cards_pass_their_check_digits() {
    let mut rng = StdRng::seed_from_u64(1);
    for _ in 0..200 {
        assert!(IdKind::Iban.check(&fake_value(&mut rng, Fake::Iban)));
        assert!(IdKind::CardNumber.check(&fake_value(&mut rng, Fake::CardNumber)));
        assert!(IdKind::Email.check(&fake_value(&mut rng, Fake::Email)));
    }
}

#[test]
fn a_unique_text_column_stays_unique() {
    let rows = (0..100)
        .map(|i| vec![s(&format!("user {i} of many"))])
        .collect();
    let mut plan = vec![profile_table(&table(&[("label", "Utf8")], rows), "t")];
    plan[0].rows = 3000;
    let t = &generate(&plan, 2).unwrap()[0];
    let vals: HashSet<String> = col(t, "label").iter().map(|v| v.to_string()).collect();
    assert_eq!(vals.len(), 3000);
}

#[test]
fn a_link_loop_is_refused_in_words() {
    let mut plans = vec![profile_table(&orders(), "a"), profile_table(&orders(), "b")];
    plans[0].columns[1].generator = Generator::Link {
        table: 1,
        column: 1,
    };
    plans[1].columns[1].generator = Generator::Link {
        table: 0,
        column: 1,
    };
    let err = generate(&plans, 1).unwrap_err();
    assert!(format!("{err:#}").contains("circle"));
}
