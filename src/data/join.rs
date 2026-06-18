use crate::data::DataTable;
use crate::sql::{SqlWorkspace, TableOrigin};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

/// Comparison operator for a join condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinOp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl JoinOp {
    fn sql(self) -> &'static str {
        match self {
            JoinOp::Eq => "=",
            JoinOp::Lt => "<",
            JoinOp::Le => "<=",
            JoinOp::Gt => ">",
            JoinOp::Ge => ">=",
        }
    }
}

/// One join condition: `left.left_col <op> right.right_col`. Column names need
/// not match and the column types need not agree; both sides are cast to a
/// common type before comparing (numeric when both columns are numeric, else
/// text).
#[derive(Debug, Clone, PartialEq)]
pub struct JoinCond {
    pub left_col: String,
    pub op: JoinOp,
    pub right_col: String,
}

fn keyword(how: JoinType) -> &'static str {
    match how {
        JoinType::Inner => "JOIN",
        JoinType::Left => "LEFT JOIN",
        JoinType::Right => "RIGHT JOIN",
        JoinType::Full => "FULL JOIN",
    }
}

fn is_numeric_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("int")
        || t.contains("float")
        || t.contains("double")
        || t.contains("decimal")
        || t.contains("real")
}

fn col_type<'a>(table: &'a DataTable, name: &str) -> Option<&'a str> {
    table
        .columns
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.data_type.as_str())
}

/// Join exactly two named in-memory tables on one or more conditions, casting
/// each condition's columns to a common type so mismatched types still compare.
/// Output keeps every column of both tables (`SELECT *`). Reuses DuckDB via
/// [`SqlWorkspace`].
pub fn join_two(
    left: (&str, &DataTable),
    right: (&str, &DataTable),
    conds: &[JoinCond],
    how: JoinType,
) -> anyhow::Result<DataTable> {
    if conds.is_empty() {
        anyhow::bail!("join needs at least one condition");
    }
    let (lname, lt) = left;
    let (rname, rt) = right;

    let mut ws = SqlWorkspace::new()?;
    ws.add_table(lname, lt, TableOrigin::ActiveTab)?;
    ws.add_table(rname, rt, TableOrigin::ActiveTab)?;

    let mut on_parts = Vec::with_capacity(conds.len());
    for c in conds {
        let l_ty = col_type(lt, &c.left_col)
            .ok_or_else(|| anyhow::anyhow!("left table has no column \"{}\"", c.left_col))?;
        let r_ty = col_type(rt, &c.right_col)
            .ok_or_else(|| anyhow::anyhow!("right table has no column \"{}\"", c.right_col))?;
        let cast = if is_numeric_type(l_ty) && is_numeric_type(r_ty) {
            "DOUBLE"
        } else {
            "VARCHAR"
        };
        on_parts.push(format!(
            "TRY_CAST(\"{lname}\".\"{}\" AS {cast}) {} TRY_CAST(\"{rname}\".\"{}\" AS {cast})",
            c.left_col,
            c.op.sql(),
            c.right_col,
        ));
    }

    let sql = format!(
        "SELECT * FROM \"{lname}\" {} \"{rname}\" ON {}",
        keyword(how),
        on_parts.join(" AND "),
    );
    Ok(ws.execute(&sql)?.table)
}

/// Join N named in-memory tables left-to-right on shared key columns, reusing
/// DuckDB via `SqlWorkspace`. `USING (keys)` collapses the key columns and
/// disambiguates the rest. Returns the result table.
pub fn join_tables(
    tables: &[(&str, &DataTable)],
    keys: &[String],
    how: JoinType,
) -> anyhow::Result<DataTable> {
    if tables.len() < 2 {
        anyhow::bail!("join needs at least two tables");
    }
    if keys.is_empty() {
        anyhow::bail!("join needs at least one key column");
    }
    let mut ws = SqlWorkspace::new()?;
    for (name, t) in tables {
        ws.add_table(name, t, TableOrigin::ActiveTab)?;
    }
    let using = keys
        .iter()
        .map(|k| format!("\"{k}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("SELECT * FROM \"{}\"", tables[0].0);
    for (name, _) in &tables[1..] {
        sql.push_str(&format!(" {} \"{}\" USING ({})", keyword(how), name, using));
    }
    Ok(ws.execute(&sql)?.table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{CellValue, ColumnInfo};

    fn tbl(cols: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
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

    #[test]
    fn left_join_keeps_unmatched_left_rows() {
        let a = tbl(
            &[("id", "Int64"), ("name", "Utf8")],
            vec![
                vec![CellValue::Int(1), CellValue::String("a".into())],
                vec![CellValue::Int(2), CellValue::String("b".into())],
            ],
        );
        let b = tbl(
            &[("id", "Int64"), ("amt", "Int64")],
            vec![vec![CellValue::Int(1), CellValue::Int(100)]],
        );
        let out = join_tables(&[("t0", &a), ("t1", &b)], &["id".into()], JoinType::Left).unwrap();
        assert_eq!(out.row_count(), 2);
    }

    #[test]
    fn inner_join_drops_unmatched() {
        let a = tbl(
            &[("id", "Int64")],
            vec![vec![CellValue::Int(1)], vec![CellValue::Int(2)]],
        );
        let b = tbl(&[("id", "Int64")], vec![vec![CellValue::Int(1)]]);
        let out = join_tables(&[("t0", &a), ("t1", &b)], &["id".into()], JoinType::Inner).unwrap();
        assert_eq!(out.row_count(), 1);
    }

    #[test]
    fn join_two_matches_different_names_and_types() {
        // Left key is Int64 "id"; right key is Utf8 "ref" holding "1"/"3".
        let a = tbl(
            &[("id", "Int64"), ("name", "Utf8")],
            vec![
                vec![CellValue::Int(1), CellValue::String("a".into())],
                vec![CellValue::Int(2), CellValue::String("b".into())],
            ],
        );
        let b = tbl(
            &[("ref", "Utf8"), ("amt", "Int64")],
            vec![vec![CellValue::String("1".into()), CellValue::Int(100)]],
        );
        let conds = vec![JoinCond {
            left_col: "id".into(),
            op: JoinOp::Eq,
            right_col: "ref".into(),
        }];
        let out = join_two(("l", &a), ("r", &b), &conds, JoinType::Inner).unwrap();
        // Only id=1 matches "1".
        assert_eq!(out.row_count(), 1);
    }

    #[test]
    fn join_two_supports_inequality() {
        let a = tbl(
            &[("v", "Int64")],
            vec![vec![CellValue::Int(1)], vec![CellValue::Int(5)]],
        );
        let b = tbl(&[("threshold", "Int64")], vec![vec![CellValue::Int(3)]]);
        // v > threshold -> only the row with v=5 matches.
        let conds = vec![JoinCond {
            left_col: "v".into(),
            op: JoinOp::Gt,
            right_col: "threshold".into(),
        }];
        let out = join_two(("l", &a), ("r", &b), &conds, JoinType::Inner).unwrap();
        assert_eq!(out.row_count(), 1);
    }
}
