use crate::data::DataTable;
use crate::sql::{SqlWorkspace, TableOrigin};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    /// Left rows that have a partner, left columns only.
    Semi,
    /// Left rows that have no partner, left columns only.
    Anti,
    /// Each left row gets the nearest right row under exactly one inequality
    /// (`>=` nearest earlier, `<=` nearest later) after the equality
    /// conditions; unmatched left rows are kept.
    AsOf,
}

impl JoinType {
    pub const ALL: [JoinType; 7] = [
        JoinType::Inner,
        JoinType::Left,
        JoinType::Right,
        JoinType::Full,
        JoinType::Semi,
        JoinType::Anti,
        JoinType::AsOf,
    ];

    /// The CLI / MCP spelling.
    pub fn id(self) -> &'static str {
        match self {
            JoinType::Inner => "inner",
            JoinType::Left => "left",
            JoinType::Right => "right",
            JoinType::Full => "full",
            JoinType::Semi => "semi",
            JoinType::Anti => "anti",
            JoinType::AsOf => "asof",
        }
    }

    /// Parse the CLI / MCP spelling, case-insensitively.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        let s = s.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|t| t.id() == s || (s == "as-of" && *t == JoinType::AsOf))
            .ok_or_else(|| {
                let all: Vec<&str> = Self::ALL.iter().map(|t| t.id()).collect();
                anyhow::anyhow!(
                    "unknown join type \"{s}\"; expected one of: {}",
                    all.join(", ")
                )
            })
    }
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
        JoinType::Semi => "SEMI JOIN",
        JoinType::Anti => "ANTI JOIN",
        JoinType::AsOf => "ASOF LEFT JOIN",
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

/// Dates and datetimes compare as `TIMESTAMP`, so `2024-09-01` sorts before
/// `2024-10-01` whatever text form either side came in. Matters for as-of,
/// where the nearest row is found by ordering.
fn is_temporal_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("date") || t.contains("timestamp")
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
    if how == JoinType::AsOf {
        let inequalities = conds.iter().filter(|c| c.op != JoinOp::Eq).count();
        if inequalities != 1 {
            anyhow::bail!(
                "an as-of join needs exactly one condition with >=, >, <= or < (the column \
                 to find the nearest row by) and any number of = conditions; this one has {inequalities}"
            );
        }
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
        } else if is_temporal_type(l_ty) && is_temporal_type(r_ty) {
            "TIMESTAMP"
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
#[path = "join_tests.rs"]
mod tests;
