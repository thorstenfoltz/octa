//! Compare the column schemas of two tables and surface what differs.
//!
//! Pure function so integration tests can hit it without I/O. The MCP
//! tool (`mcp/tools/compare_schemas.rs`), CLI handler
//! (`cli/compare_schemas.rs`), and the validate_against_schema tool
//! all delegate here.
//!
//! Match rule: columns are paired by exact, case-sensitive name. Order
//! in the result preserves the order of side A; `only_in_b` follows the
//! order of side B.

use crate::data::ColumnInfo;

/// What two schemas have in common and where they differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiff {
    /// Columns present on both sides with the same `data_type`.
    pub common: Vec<ColumnInfo>,
    /// Columns whose name appears only on side A.
    pub only_in_a: Vec<ColumnInfo>,
    /// Columns whose name appears only on side B.
    pub only_in_b: Vec<ColumnInfo>,
    /// Columns present on both sides but with different `data_type`.
    pub type_mismatches: Vec<TypeMismatch>,
    /// `true` when every input column lines up identically. Convenience:
    /// equivalent to `common.len() == a.len() == b.len()` and all the
    /// other vectors being empty.
    pub identical: bool,
}

/// A column name shared by both schemas with a differing `data_type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMismatch {
    pub name: String,
    pub type_a: String,
    pub type_b: String,
}

/// Compare two column lists.
pub fn compare_schemas(a: &[ColumnInfo], b: &[ColumnInfo]) -> SchemaDiff {
    use std::collections::HashMap;

    let mut b_index: HashMap<&str, &ColumnInfo> = HashMap::with_capacity(b.len());
    for col in b {
        b_index.insert(col.name.as_str(), col);
    }

    let mut common = Vec::new();
    let mut type_mismatches = Vec::new();
    let mut only_in_a = Vec::new();

    for col_a in a {
        match b_index.get(col_a.name.as_str()) {
            Some(col_b) => {
                if crate::data::times::same_type(&col_a.data_type, &col_b.data_type) {
                    common.push(col_a.clone());
                } else {
                    type_mismatches.push(TypeMismatch {
                        name: col_a.name.clone(),
                        type_a: col_a.data_type.clone(),
                        type_b: col_b.data_type.clone(),
                    });
                }
            }
            None => only_in_a.push(col_a.clone()),
        }
    }

    let a_names: std::collections::HashSet<&str> = a.iter().map(|c| c.name.as_str()).collect();
    let only_in_b: Vec<ColumnInfo> = b
        .iter()
        .filter(|c| !a_names.contains(c.name.as_str()))
        .cloned()
        .collect();

    let identical = only_in_a.is_empty() && only_in_b.is_empty() && type_mismatches.is_empty();

    SchemaDiff {
        common,
        only_in_a,
        only_in_b,
        type_mismatches,
        identical,
    }
}
