use super::*;
use crate::data::value_frequency::compute_value_frequency;
use crate::data::{CellValue, DataTable};
use crate::db::pushdown::test_support::{DuckConn, source};
use crate::db::{DbEngine, DbWriteMode, DbWriteReport};

fn table() -> DataTable {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "n".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "s".into(),
            data_type: "Utf8".into(),
        },
    ];
    t.rows = (0..50)
        .map(|i| {
            vec![
                if i % 11 == 0 {
                    CellValue::Null
                } else {
                    CellValue::Int((i * 7) % 23)
                },
                if i % 6 == 0 {
                    CellValue::String(String::new())
                } else {
                    CellValue::String(format!("k{}", i % 4))
                },
            ]
        })
        .collect();
    t
}

fn same(a: &ValueFrequency, b: &ValueFrequency) {
    assert_eq!(
        (a.nulls, a.total_non_null, a.unique_count, a.binned),
        (b.nulls, b.total_non_null, b.unique_count, b.binned)
    );
    let rows = |v: &ValueFrequency| {
        v.rows
            .iter()
            .map(|r| (r.label.clone(), r.count))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(a), rows(b));
}

#[test]
fn raw_values_match_the_in_memory_count() {
    let t = table();
    for col in 0..2 {
        for top in [None, Some(3)] {
            let local = compute_value_frequency(&t, col, top, BinningMode::None).unwrap();
            let mut c = DuckConn::new(t.clone());
            let server = run(
                &mut c,
                &source(),
                &t.columns[col],
                top,
                BinningMode::None,
                &AtomicBool::new(false),
            )
            .unwrap();
            same(&local, &server);
        }
    }
}

#[test]
fn bins_match_the_in_memory_count() {
    let t = table();
    for binning in [BinningMode::Sturges, BinningMode::Custom(4)] {
        let local = compute_value_frequency(&t, 0, None, binning).unwrap();
        let mut c = DuckConn::new(t.clone());
        let server = run(
            &mut c,
            &source(),
            &t.columns[0],
            None,
            binning,
            &AtomicBool::new(false),
        )
        .unwrap();
        same(&local, &server);
    }
}

/// Records every statement and answers with a canned one-row table.
struct Recorder(DbEngine, Vec<String>);

impl DbConnector for Recorder {
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        self.1.push(sql.to_string());
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "a".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "b".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "c".into(),
                data_type: "Int64".into(),
            },
        ];
        t.rows = vec![vec![
            CellValue::Int(1),
            CellValue::Int(1),
            CellValue::Int(1),
        ]];
        Ok(t)
    }
    fn engine(&self) -> DbEngine {
        self.0
    }
    fn execute(&mut self, _: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    fn write_table(
        &mut self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: DbWriteMode,
        _: &DataTable,
    ) -> anyhow::Result<DbWriteReport> {
        anyhow::bail!("recorder")
    }
}

#[test]
fn sql_server_top_values_use_fetch_next() {
    let mut src = source();
    src.conn.engine = DbEngine::Mssql;
    let col = ColumnInfo {
        name: "s".into(),
        data_type: "Utf8".into(),
    };
    let mut c = Recorder(DbEngine::Mssql, vec![]);
    run(
        &mut c,
        &src,
        &col,
        Some(7),
        BinningMode::None,
        &AtomicBool::new(false),
    )
    .unwrap();
    let last = c.1.last().unwrap();
    assert!(
        last.ends_with(" OFFSET 0 ROWS FETCH NEXT 7 ROWS ONLY"),
        "{last}"
    );
}

/// MySQL's and SQL Server's default collations fold case and accents, so
/// grouping by the plain text would merge `Apple` and `apple` into one
/// label, which a value filter (exact) then splits again.
#[test]
fn values_are_counted_by_their_exact_text() {
    let col = ColumnInfo {
        name: "s".into(),
        data_type: "Utf8".into(),
    };
    for (e, ex) in [
        (DbEngine::MySql, "CAST("),
        (DbEngine::Mssql, "COLLATE Latin1_General_100_BIN2"),
    ] {
        let mut src = source();
        src.conn.engine = e;
        let mut c = Recorder(e, vec![]);
        run(
            &mut c,
            &src,
            &col,
            Some(7),
            BinningMode::None,
            &AtomicBool::new(false),
        )
        .unwrap();
        let (totals, list) = (&c.1[0], c.1.last().unwrap());
        assert!(
            totals.contains("COUNT(DISTINCT") && totals.contains(ex),
            "{e:?}: {totals}"
        );
        let group = &list[list.find("GROUP BY").unwrap()..];
        assert!(group.contains(ex), "{e:?}: {list}");
        assert!(list.starts_with("SELECT MIN("), "{e:?}: {list}");
    }
}
