//! **Copy as IN list**: turn a handful of cells into `('a', 'b', 'c')`, ready
//! to paste after `WHERE id IN` in any database tool.

use super::CellValue;

/// The values as one SQL `IN` list, or `None` when there is nothing to list.
///
/// Empty cells are left out and repeats are listed once, in first-seen order.
/// Numbers go in bare when every value is a number; as soon as one is not, all
/// of them are quoted, so a text column holding `007` keeps its leading zero
/// and the list stays one type. Quotes inside a value are doubled.
pub fn sql_in_list<'a>(values: impl IntoIterator<Item = &'a CellValue>) -> Option<String> {
    let values: Vec<&CellValue> = values
        .into_iter()
        .filter(|v| !matches!(v, CellValue::Null))
        .collect();
    let all_numbers = values
        .iter()
        .all(|v| matches!(v, CellValue::Int(_) | CellValue::Float(_)));
    let mut seen = std::collections::HashSet::new();
    let items: Vec<String> = values
        .iter()
        .map(|v| match v {
            CellValue::Int(n) if all_numbers => n.to_string(),
            CellValue::Float(f) if all_numbers => f.to_string(),
            other => format!("'{}'", other.to_string().replace('\'', "''")),
        })
        .filter(|item| seen.insert(item.clone()))
        .collect();
    (!items.is_empty()).then(|| format!("({})", items.join(", ")))
}

#[cfg(test)]
#[path = "in_list_tests.rs"]
mod tests;
