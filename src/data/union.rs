use crate::data::{CellValue, ColumnInfo, DataTable};

#[derive(Debug, Clone, PartialEq)]
pub struct UnionColumnPlan {
    pub name: String,
    pub include: bool,
    pub target_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnionPlan {
    pub columns: Vec<UnionColumnPlan>,
}

fn is_int(ty: &str) -> bool {
    matches!(
        ty,
        "Int8" | "Int16" | "Int32" | "Int64" | "UInt8" | "UInt16" | "UInt32" | "UInt64"
    )
}

fn is_float(ty: &str) -> bool {
    matches!(ty, "Float16" | "Float32" | "Float64")
}

/// Widen two Arrow type-name strings: all-int -> Int64; int+float -> Float64; otherwise Utf8.
fn widen(a: &str, b: &str) -> String {
    if a == b {
        return a.to_string();
    }
    let (ai, af, bi, bf) = (is_int(a), is_float(a), is_int(b), is_float(b));
    if ai && bi {
        "Int64".to_string()
    } else if (ai || af) && (bi || bf) {
        "Float64".to_string()
    } else {
        "Utf8".to_string()
    }
}

/// Smart-merge default plan: union of all columns (first-seen order), each typed
/// to the widened common type across sources that carry it, everything included.
pub fn plan_union(schemas: &[&[ColumnInfo]]) -> UnionPlan {
    let mut order: Vec<String> = Vec::new();
    let mut types: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for schema in schemas {
        for col in *schema {
            match types.get(&col.name) {
                None => {
                    order.push(col.name.clone());
                    types.insert(col.name.clone(), col.data_type.clone());
                }
                Some(existing) => {
                    let merged = widen(existing, &col.data_type);
                    types.insert(col.name.clone(), merged);
                }
            }
        }
    }
    UnionPlan {
        columns: order
            .into_iter()
            .map(|name| {
                let target_type = types
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| "Utf8".to_string());
                UnionColumnPlan {
                    name,
                    include: true,
                    target_type,
                }
            })
            .collect(),
    }
}

/// Coerce a cell toward an Arrow target type (numeric widening; otherwise string form for Utf8 targets).
fn coerce(value: &CellValue, target: &str) -> CellValue {
    match value {
        CellValue::Null => CellValue::Null,
        CellValue::Int(i) if is_float(target) => CellValue::Float(*i as f64),
        CellValue::Int(_) | CellValue::Float(_) | CellValue::Bool(_) if target == "Utf8" => {
            CellValue::String(value.to_string())
        }
        other if target == "Utf8" => match other {
            CellValue::String(_)
            | CellValue::Date(_)
            | CellValue::DateTime(_)
            | CellValue::Nested(_) => other.clone(),
            _ => CellValue::String(other.to_string()),
        },
        other => other.clone(),
    }
}

/// Stacks all input tables row-by-row according to `plan`.
///
/// Columns marked `include = false` are omitted from the output.
/// Columns present in the plan but absent from a particular source table
/// are filled with `CellValue::Null`. Values are coerced to the plan's
/// `target_type` (e.g. `Int64` widened to `Float64`).
pub fn union_tables(tables: &[&DataTable], plan: &UnionPlan) -> anyhow::Result<DataTable> {
    let kept: Vec<&UnionColumnPlan> = plan.columns.iter().filter(|c| c.include).collect();
    let mut out = DataTable::empty();
    out.columns = kept
        .iter()
        .map(|c| ColumnInfo {
            name: c.name.clone(),
            data_type: c.target_type.clone(),
        })
        .collect();
    for table in tables {
        let src_idx: Vec<Option<usize>> = kept
            .iter()
            .map(|c| table.columns.iter().position(|sc| sc.name == c.name))
            .collect();
        for row in 0..table.row_count() {
            let new_row: Vec<CellValue> = kept
                .iter()
                .zip(&src_idx)
                .map(|(c, idx)| match idx {
                    Some(i) => table
                        .get(row, *i)
                        .map(|v| coerce(v, &c.target_type))
                        .unwrap_or(CellValue::Null),
                    None => CellValue::Null,
                })
                .collect();
            out.rows.push(new_row);
        }
    }
    out.structural_changes = true;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(names: &[(&str, &str)]) -> Vec<ColumnInfo> {
        names
            .iter()
            .map(|(n, t)| ColumnInfo {
                name: n.to_string(),
                data_type: t.to_string(),
            })
            .collect()
    }

    fn tbl(columns: &[(&str, &str)], rows: Vec<Vec<CellValue>>) -> DataTable {
        let mut t = DataTable::empty();
        t.columns = cols(columns);
        t.rows = rows;
        t
    }

    #[test]
    fn plan_widens_and_unions_columns() {
        let a = cols(&[("id", "Int64"), ("region", "Utf8"), ("amt", "Int64")]);
        let b = cols(&[
            ("id", "Int64"),
            ("region", "Utf8"),
            ("amt", "Float64"),
            ("note", "Utf8"),
        ]);
        let plan = plan_union(&[&a, &b]);
        let names: Vec<_> = plan
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.target_type.as_str(), c.include))
            .collect();
        assert_eq!(
            names,
            vec![
                ("id", "Int64", true),
                ("region", "Utf8", true),
                ("amt", "Float64", true),
                ("note", "Utf8", true),
            ]
        );
    }

    #[test]
    fn plan_disagreement_falls_back_to_utf8() {
        let a = cols(&[("v", "Int64")]);
        let b = cols(&[("v", "Date")]);
        let plan = plan_union(&[&a, &b]);
        assert_eq!(plan.columns[0].target_type, "Utf8");
    }

    #[test]
    fn union_fills_missing_with_null_and_widens() {
        let a = tbl(
            &[("id", "Int64"), ("amt", "Int64")],
            vec![vec![CellValue::Int(1), CellValue::Int(10)]],
        );
        let b = tbl(
            &[("id", "Int64"), ("amt", "Float64"), ("note", "Utf8")],
            vec![vec![
                CellValue::Int(2),
                CellValue::Float(2.5),
                CellValue::String("hi".into()),
            ]],
        );
        let plan = plan_union(&[&a.columns, &b.columns]);
        let out = union_tables(&[&a, &b], &plan).unwrap();
        assert_eq!(
            out.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "amt", "note"]
        );
        assert_eq!(out.row_count(), 2);
        assert_eq!(out.get(0, 1), Some(&CellValue::Float(10.0)));
        assert_eq!(out.get(0, 2), Some(&CellValue::Null));
        assert_eq!(out.get(1, 2), Some(&CellValue::String("hi".into())));
    }

    #[test]
    fn union_respects_dropped_columns() {
        let a = tbl(
            &[("id", "Int64"), ("drop_me", "Utf8")],
            vec![vec![CellValue::Int(1), CellValue::String("x".into())]],
        );
        let mut plan = plan_union(&[&a.columns]);
        plan.columns
            .iter_mut()
            .find(|c| c.name == "drop_me")
            .unwrap()
            .include = false;
        let out = union_tables(&[&a], &plan).unwrap();
        assert_eq!(
            out.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id"]
        );
    }
}
