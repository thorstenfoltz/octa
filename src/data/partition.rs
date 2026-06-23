use crate::data::{CellValue, DataTable};
use std::collections::BTreeMap;

/// One DataTable per distinct value of `col`. Null groups under "__null__".
/// Row order within a group is preserved; groups are returned value-sorted.
pub fn partition_table(table: &DataTable, col: usize) -> Vec<(String, DataTable)> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for row in 0..table.row_count() {
        let key = match table.get(row, col) {
            Some(CellValue::Null) | None => "__null__".to_string(),
            Some(v) => v.to_string(),
        };
        groups.entry(key).or_default().push(row);
    }
    groups
        .into_iter()
        .map(|(value, idxs)| {
            let mut out = DataTable::empty();
            out.columns = table.columns.clone();
            out.rows = idxs
                .iter()
                .map(|&r| {
                    (0..table.col_count())
                        .map(|c| table.get(r, c).cloned().unwrap_or(CellValue::Null))
                        .collect()
                })
                .collect();
            out.structural_changes = true;
            (value, out)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ColumnInfo;

    #[test]
    fn splits_rows_by_value() {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "region".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "amt".into(),
                data_type: "Int64".into(),
            },
        ];
        t.rows = vec![
            vec![CellValue::String("US".into()), CellValue::Int(1)],
            vec![CellValue::String("EU".into()), CellValue::Int(2)],
            vec![CellValue::String("US".into()), CellValue::Int(3)],
        ];
        let parts = partition_table(&t, 0);
        let mut labels: Vec<_> = parts
            .iter()
            .map(|(v, tbl)| (v.clone(), tbl.row_count()))
            .collect();
        labels.sort();
        assert_eq!(labels, vec![("EU".to_string(), 1), ("US".to_string(), 2)]);
        assert_eq!(parts[0].1.columns.len(), 2);
    }

    #[test]
    fn null_groups_under_null_key() {
        let mut t = DataTable::empty();
        t.columns = vec![ColumnInfo {
            name: "k".into(),
            data_type: "Utf8".into(),
        }];
        t.rows = vec![
            vec![CellValue::Null],
            vec![CellValue::String("A".into())],
            vec![CellValue::Null],
        ];
        let parts = partition_table(&t, 0);
        let null_group = parts.iter().find(|(k, _)| k == "__null__");
        assert!(null_group.is_some());
        assert_eq!(null_group.unwrap().1.row_count(), 2);
    }

    #[test]
    fn preserves_row_order_within_group() {
        let mut t = DataTable::empty();
        t.columns = vec![
            ColumnInfo {
                name: "g".into(),
                data_type: "Utf8".into(),
            },
            ColumnInfo {
                name: "n".into(),
                data_type: "Int64".into(),
            },
        ];
        t.rows = vec![
            vec![CellValue::String("X".into()), CellValue::Int(10)],
            vec![CellValue::String("X".into()), CellValue::Int(20)],
            vec![CellValue::String("X".into()), CellValue::Int(30)],
        ];
        let parts = partition_table(&t, 0);
        assert_eq!(parts.len(), 1);
        let rows = &parts[0].1;
        assert_eq!(rows.get(0, 1), Some(&CellValue::Int(10)));
        assert_eq!(rows.get(1, 1), Some(&CellValue::Int(20)));
        assert_eq!(rows.get(2, 1), Some(&CellValue::Int(30)));
    }
}
