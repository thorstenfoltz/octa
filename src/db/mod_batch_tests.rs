use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// 4-line test-only parser: pull (limit, offset) back out of a paged sql.
fn parse_limit_offset(sql: &str, engine: DbEngine) -> (usize, usize) {
    let grab = |kw: &str| -> usize {
        sql.split(kw)
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };
    match engine {
        DbEngine::Mssql => (grab("FETCH NEXT"), grab("OFFSET")),
        _ => (grab("LIMIT"), grab("OFFSET")),
    }
}

struct FakeConn {
    rows: usize,
    engine: DbEngine,
}
impl DbConnector for FakeConn {
    fn list_schemas(&mut self, _: Option<&str>) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn list_tables(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    fn engine(&self) -> DbEngine {
        self.engine
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
        unimplemented!()
    }
    fn query(&mut self, sql: &str) -> anyhow::Result<DataTable> {
        let (limit, offset) = parse_limit_offset(sql, self.engine);
        let n = self.rows.saturating_sub(offset).min(limit);
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        }];
        t.rows = (0..n)
            .map(|i| vec![CellValue::Int((offset + i) as i64)])
            .collect();
        Ok(t)
    }
}

#[test]
fn fetch_batches_streams_all_without_cap() {
    let mut c = FakeConn {
        rows: 250,
        engine: DbEngine::Postgres,
    };
    let mut total = 0;
    c.fetch_batches("SELECT * FROM t", 100, &mut |b| {
        total += b.row_count();
        Ok(())
    })
    .unwrap();
    assert_eq!(total, 250); // 100 + 100 + 50, no cap
}
