use crate::data::transform::fill::{fill_down, fill_up};
use crate::data::{CellValue, DataTable};

#[derive(Debug, Clone, PartialEq)]
pub enum ImputeStrategy {
    Mean,
    Median,
    Mode,
    Constant(String),
    ForwardFill,
    BackwardFill,
}

fn is_missing(v: Option<&CellValue>) -> bool {
    matches!(v, None | Some(CellValue::Null))
        || matches!(v, Some(CellValue::String(s)) if s.is_empty())
}

/// A cell as a number, or `None` when it is not one. `NaN` and the infinities
/// are treated as missing (see the same helper in
/// [`outliers`](crate::data::outliers)): they are how R and pandas spell a
/// missing number in a CSV, and averaging or sorting them is meaningless.
fn numeric(v: &CellValue) -> Option<f64> {
    let n = match v {
        CellValue::Int(i) => *i as f64,
        CellValue::Float(f) => *f,
        CellValue::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// Return `col`'s values with missing cells (null / empty string) filled per strategy.
pub fn impute_column(
    table: &DataTable,
    col: usize,
    strategy: &ImputeStrategy,
) -> anyhow::Result<Vec<CellValue>> {
    let n = table.row_count();
    let cur: Vec<CellValue> = (0..n)
        .map(|r| table.get(r, col).cloned().unwrap_or(CellValue::Null))
        .collect();
    match strategy {
        ImputeStrategy::ForwardFill => return Ok(fill_down(table, col)),
        ImputeStrategy::BackwardFill => return Ok(fill_up(table, col)),
        _ => {}
    }
    let fill: CellValue = match strategy {
        ImputeStrategy::Mean | ImputeStrategy::Median => {
            let nums: Vec<f64> = cur
                .iter()
                .filter(|v| !is_missing(Some(v)))
                .filter_map(numeric)
                .collect();
            if nums.is_empty() {
                anyhow::bail!("no numeric values to compute {:?}", strategy);
            }
            let value = if matches!(strategy, ImputeStrategy::Mean) {
                nums.iter().sum::<f64>() / nums.len() as f64
            } else {
                let mut s = nums.clone();
                s.sort_by(|a, b| a.total_cmp(b));
                let m = s.len() / 2;
                if s.len() % 2 == 1 {
                    s[m]
                } else {
                    (s[m - 1] + s[m]) / 2.0
                }
            };
            CellValue::Float(value)
        }
        ImputeStrategy::Mode => {
            use std::collections::HashMap;
            let mut counts: HashMap<String, (usize, CellValue)> = HashMap::new();
            for v in cur.iter().filter(|v| !is_missing(Some(v))) {
                let e = counts.entry(v.to_string()).or_insert((0, v.clone()));
                e.0 += 1;
            }
            counts
                .into_values()
                .max_by_key(|(c, _)| *c)
                .map(|(_, v)| v)
                .ok_or_else(|| anyhow::anyhow!("no values to compute mode"))?
        }
        // A time column reads the value as its own type, as a number column
        // reads a number.
        ImputeStrategy::Constant(s) => {
            match crate::data::times::typed_cell(&table.columns[col].data_type, s) {
                Some(cell) => cell,
                None => match s.parse::<i64>() {
                    Ok(i) => CellValue::Int(i),
                    Err(_) => match s.parse::<f64>() {
                        Ok(f) => CellValue::Float(f),
                        Err(_) => CellValue::String(s.clone()),
                    },
                },
            }
        }
        ImputeStrategy::ForwardFill | ImputeStrategy::BackwardFill => unreachable!(),
    };
    Ok(cur
        .into_iter()
        .map(|v| {
            if is_missing(Some(&v)) {
                fill.clone()
            } else {
                v
            }
        })
        .collect())
}

#[cfg(test)]
#[path = "impute_tests.rs"]
mod tests;
