//! Pure display model over `DataTable.edits`: the pending cell edits as an
//! ordered list with before and after. The panel in `src/app/edit_audit.rs`
//! renders this and nothing else; the same entries are rendered as UPDATE
//! statements by `db::write_back::edits_as_update_sql`, which lives there so
//! it can reuse the dialect-correct statement builder.

use crate::data::DataTable;

/// One pending edit, resolved against the table it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub row: usize,
    pub col: usize,
    pub column_name: String,
    pub before: String,
    pub after: String,
}

/// Every pending edit, sorted by row then column so the list reads in the
/// order the grid does. Edits whose indices no longer resolve (a structural
/// change that outran its index shift) are dropped rather than rendered as a
/// phantom row.
pub fn audit_entries(table: &DataTable) -> Vec<AuditEntry> {
    let mut keys: Vec<(usize, usize)> = table.edits.keys().copied().collect();
    keys.sort_unstable();
    keys.into_iter()
        .filter_map(|(row, col)| {
            let column_name = table.columns.get(col)?.name.clone();
            let before = table.original_value(row, col)?.to_string();
            let after = table.edits.get(&(row, col))?.to_string();
            Some(AuditEntry {
                row,
                col,
                column_name,
                before,
                after,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "edit_audit_tests.rs"]
mod tests;
