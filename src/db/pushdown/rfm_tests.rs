use super::*;
use crate::data::rfm::{Names, RfmSpec, collect, table};
use crate::data::{ColumnInfo, DataTable};

fn spec(order: Option<&'static str>) -> ServerRfm<'static> {
    ServerRfm {
        customer: "customer",
        date: "at",
        amount: "amount",
        amount_type: "Decimal128(8, 2)",
        order,
        as_of: None,
    }
}

#[test]
fn one_group_by_with_an_exact_sum() {
    let [main, skipped] = sql(DbEngine::Postgres, "\"t\"", &spec(Some("order")));
    assert!(main.contains("GROUP BY \"customer\""), "{main}");
    assert!(main.contains("COUNT(DISTINCT \"order\")"), "{main}");
    assert!(main.contains("EXTRACT(EPOCH FROM \"at\")"), "{main}");
    // A decimal column goes into SUM as it is.
    assert!(main.contains("SUM(\"amount\")"), "{main}");
    assert!(
        skipped.contains("\"customer\" IS NULL OR \"at\" IS NULL"),
        "{skipped}"
    );
    assert!(
        main.contains("SUM(CASE WHEN \"order\" IS NULL THEN 1 ELSE 0 END)"),
        "a row without an order number is an order: {main}"
    );
    let [rows, _] = sql(DbEngine::Postgres, "\"t\"", &spec(None));
    assert!(rows.contains("COUNT(*) AS octa_n"), "{rows}");
    // As of 2024-03-02: before 2024-03-03 00:00 UTC.
    let snapshot = ServerRfm {
        as_of: chrono::NaiveDate::from_ymd_opt(2024, 3, 2),
        ..spec(None)
    };
    let [cut, _] = sql(DbEngine::Postgres, "\"t\"", &snapshot);
    assert!(cut.contains("< 1709424000 GROUP BY"), "{cut}");
}

fn shop() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = [
        ("customer", "Utf8"),
        ("at", "Timestamp(Microsecond, None)"),
        ("amount", "Decimal128(8, 2)"),
        ("order", "Utf8"),
    ]
    .iter()
    .map(|(n, d)| ColumnInfo {
        name: (*n).into(),
        data_type: (*d).into(),
    })
    .collect();
    t.rows = [
        ("ann", "2024-03-01 10:00:00", Some(10.1), "o1"),
        ("ann", "2024-03-01 10:00:00", Some(0.2), "o1"),
        ("ann", "2024-03-20 09:00:00", Some(5.0), "o2"),
        ("bob", "2024-01-05 23:59:00", Some(100.0), "o3"),
        ("cy", "2024-03-30 08:00:00", None, "o4"),
        ("eve", "2024-02-10 00:00:00", Some(0.1), "o7"),
        ("eve", "2024-02-11 00:00:00", Some(0.2), "o8"),
        ("eve", "2024-02-12 00:00:00", Some(0.3), "o9"),
        ("fay", "2024-03-02 12:00:00", Some(7.5), "o10"),
        // No order number: an order of its own on both paths.
        ("fay", "2024-03-03 12:00:00", Some(2.5), ""),
    ]
    .iter()
    .map(|(c, at, a, o)| {
        vec![
            CellValue::String((*c).into()),
            CellValue::DateTime((*at).into()),
            a.map_or(CellValue::Null, CellValue::Float),
            if o.is_empty() {
                CellValue::Null
            } else {
                CellValue::String((*o).into())
            },
        ]
    })
    .collect();
    t
}

/// The SQL, run by DuckDB, adds up what the local engine adds up, so the
/// marks and names of both paths are the same table.
#[test]
fn duckdb_agrees_with_the_local_engine() {
    use super::super::test_support::{DuckConn, source};
    let t = shop();
    // The day before fay's last purchase leaves it out; ann's 03-20 too.
    let as_ofs = [None, chrono::NaiveDate::from_ymd_opt(2024, 3, 2)];
    for (order, as_of) in [None, Some("order")]
        .into_iter()
        .flat_map(|o| as_ofs.map(|a| (o, a)))
    {
        let local = collect(
            &t,
            &RfmSpec {
                customer: 0,
                date: 1,
                amount: 2,
                order: order.map(|_| 3),
                as_of,
            },
        );
        let mut conn = DuckConn::new(t.clone());
        let server_spec = ServerRfm {
            as_of,
            ..spec(order)
        };
        let server = run(&mut conn, &source(), &server_spec, &AtomicBool::new(false)).unwrap();
        for names in Names::ALL {
            let (a, _) = table(&local, as_of, names).unwrap();
            let (b, _) = table(&server, as_of, names).unwrap();
            assert_eq!(a.rows, b.rows, "{order:?} {as_of:?} {names:?}");
        }
        assert_eq!(server.skipped, 0);
    }
}
