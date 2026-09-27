//! How much memory one open tab is holding.
//!
//! Every number here is an **estimate**, and deliberately so: the exact
//! figure is not observable from inside the process. A `String` may hold
//! more capacity than its length, the allocator pads and rounds, and a
//! `HashMap` keeps spare buckets whose count it does not publish. Reporting
//! a precise-looking byte total would be a lie with decimal places on it.
//!
//! What this is for is comparison: which of eleven open tabs is holding the
//! memory, and roughly how much. A figure that is consistently within a
//! factor of a little answers that; a figure that is exact answers nothing
//! more.

use std::collections::HashMap;

use crate::data::{CellValue, DataTable};

/// Heap bytes a single cell's payload holds, on top of the enum itself.
fn payload_bytes(value: &CellValue) -> usize {
    match value {
        CellValue::String(s) | CellValue::Date(s) | CellValue::DateTime(s) => s.len(),
        CellValue::Nested(s) => s.len(),
        CellValue::Binary(b) => b.len(),
        CellValue::Null | CellValue::Bool(_) | CellValue::Int(_) | CellValue::Float(_) => 0,
    }
}

/// Bytes held by a `(row, col) -> CellValue` overlay map, including a rough
/// allowance for the map's own buckets.
fn overlay_bytes(map: &HashMap<(usize, usize), CellValue>) -> usize {
    const ENTRY_OVERHEAD: usize = size_of::<(usize, usize)>() + size_of::<CellValue>() + 16;
    map.len() * ENTRY_OVERHEAD + map.values().map(payload_bytes).sum::<usize>()
}

/// Estimated bytes held by `table`.
///
/// Counts the cells, their heap payloads, the pending-edit overlay, the
/// colour marks, the undo and redo stacks and the column metadata. The undo
/// stacks matter more than they look: they hold whole-column snapshots after
/// a type conversion, so a tab that feels idle can be holding two more
/// copies of a column.
pub fn estimate_bytes(table: &DataTable) -> u64 {
    let mut total = size_of::<DataTable>();

    // Cells. One pass, not one per column: a wide table is the case that
    // matters and walking it twice would double the cost of the dialog.
    for row in &table.rows {
        total += size_of::<Vec<CellValue>>() + row.capacity() * size_of::<CellValue>();
        for cell in row {
            total += payload_bytes(cell);
        }
    }
    total += table.rows.capacity().saturating_sub(table.rows.len()) * size_of::<Vec<CellValue>>();

    // Column metadata.
    for col in &table.columns {
        total += size_of::<crate::data::ColumnInfo>() + col.name.len() + col.data_type.len();
    }

    total += overlay_bytes(&table.edits);
    total += table.marks.len() * 64;
    // An undo entry's size depends on what it captured; 128 bytes is a
    // placeholder for the entry itself, and the column snapshots inside a
    // `ConvertColumn` are what actually dominate, counted below.
    total += (table.undo_stack.len() + table.redo_stack.len()) * 128;
    for action in table.undo_stack.iter().chain(table.redo_stack.iter()) {
        total += undo_payload_bytes(action);
    }

    total as u64
}

/// Heap held by one undo entry's captured values. Only the variants that
/// capture whole columns are worth counting; the rest are a handful of
/// bytes already covered by the flat per-entry allowance.
fn undo_payload_bytes(action: &crate::data::UndoAction) -> usize {
    match action {
        crate::data::UndoAction::ConvertColumn {
            old_values,
            new_values,
            ..
        } => {
            (old_values.len() + new_values.len()) * size_of::<CellValue>()
                + old_values.iter().map(payload_bytes).sum::<usize>()
                + new_values.iter().map(payload_bytes).sum::<usize>()
        }
        _ => 0,
    }
}

/// Human-readable size. Deliberately one decimal: the number is an estimate
/// and a second decimal would suggest a precision it does not have.
pub fn format_estimate(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

#[cfg(test)]
#[path = "tab_memory_tests.rs"]
mod tests;
