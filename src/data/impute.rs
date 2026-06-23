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

fn numeric(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) => Some(*f),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
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
                s.sort_by(|a, b| a.partial_cmp(b).unwrap());
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
        ImputeStrategy::Constant(s) => match s.parse::<i64>() {
            Ok(i) => CellValue::Int(i),
            Err(_) => match s.parse::<f64>() {
                Ok(f) => CellValue::Float(f),
                Err(_) => CellValue::String(s.clone()),
            },
        },
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
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    fn col(vals: Vec<CellValue>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "v".into(),
            data_type: "Float64".into(),
        }];
        t.rows = vals.into_iter().map(|v| vec![v]).collect();
        t
    }

    #[test]
    fn mean_fills_nulls() {
        let t = col(vec![
            CellValue::Float(2.0),
            CellValue::Null,
            CellValue::Float(4.0),
        ]);
        let out = impute_column(&t, 0, &ImputeStrategy::Mean).unwrap();
        assert_eq!(out[1], CellValue::Float(3.0));
        assert_eq!(out[0], CellValue::Float(2.0));
    }

    #[test]
    fn constant_fills_nulls() {
        let t = col(vec![CellValue::Null, CellValue::Float(1.0)]);
        let out = impute_column(&t, 0, &ImputeStrategy::Constant("9".into())).unwrap();
        assert_eq!(out[0], CellValue::Int(9));
    }

    #[test]
    fn mean_on_text_errors() {
        let mut t = col(vec![CellValue::String("x".into())]);
        t.columns[0].data_type = "Utf8".into();
        assert!(impute_column(&t, 0, &ImputeStrategy::Mean).is_err());
    }

    #[test]
    fn median_even_count() {
        let t = col(vec![
            CellValue::Float(1.0),
            CellValue::Float(3.0),
            CellValue::Float(5.0),
            CellValue::Float(7.0),
        ]);
        let out = impute_column(&t, 0, &ImputeStrategy::Median).unwrap();
        // No nulls, values unchanged
        assert_eq!(out[0], CellValue::Float(1.0));
    }

    #[test]
    fn median_fills_null_with_middle_value() {
        let t = col(vec![
            CellValue::Float(1.0),
            CellValue::Null,
            CellValue::Float(3.0),
        ]);
        let out = impute_column(&t, 0, &ImputeStrategy::Median).unwrap();
        // median of [1,3] = 2.0
        assert_eq!(out[1], CellValue::Float(2.0));
    }

    #[test]
    fn mode_fills_null_with_most_frequent() {
        let t = col(vec![
            CellValue::Float(5.0),
            CellValue::Float(5.0),
            CellValue::Float(3.0),
            CellValue::Null,
        ]);
        let out = impute_column(&t, 0, &ImputeStrategy::Mode).unwrap();
        assert_eq!(out[3], CellValue::Float(5.0));
    }

    #[test]
    fn constant_string_fills_null() {
        let t = col(vec![CellValue::Null, CellValue::Float(1.0)]);
        let out = impute_column(&t, 0, &ImputeStrategy::Constant("hello".into())).unwrap();
        assert_eq!(out[0], CellValue::String("hello".into()));
    }

    #[test]
    fn forward_fill_delegates_to_fill_down() {
        let t = col(vec![
            CellValue::Float(1.0),
            CellValue::Null,
            CellValue::Null,
        ]);
        let out = impute_column(&t, 0, &ImputeStrategy::ForwardFill).unwrap();
        assert_eq!(out[1], CellValue::Float(1.0));
        assert_eq!(out[2], CellValue::Float(1.0));
    }

    #[test]
    fn backward_fill_delegates_to_fill_up() {
        let t = col(vec![
            CellValue::Null,
            CellValue::Null,
            CellValue::Float(9.0),
        ]);
        let out = impute_column(&t, 0, &ImputeStrategy::BackwardFill).unwrap();
        assert_eq!(out[0], CellValue::Float(9.0));
        assert_eq!(out[1], CellValue::Float(9.0));
    }
}
