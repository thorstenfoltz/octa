//! One file per recipe step kind. Each is a plain struct that knows how to
//! apply itself by column name (`apply`), say what it does (`describe`), and
//! list the columns it needs (`columns`), which is how a replay warns before
//! it runs.

mod change_type;
mod delete_columns;
mod drop_duplicates;
mod extract;
mod fill;
mod fill_missing;
mod merge;
mod rename;
mod repair_encoding;
mod replace;
mod set_cells;
mod sort;
mod split;
mod tidy_id;

pub use change_type::ChangeType;
pub use delete_columns::DeleteColumns;
pub use drop_duplicates::DropDuplicates;
pub use extract::Extract;
pub use fill::Fill;
pub use fill_missing::FillMissing;
pub use merge::Merge;
pub use rename::{Rename, RenamePair};
pub use repair_encoding::RepairEncoding;
pub use replace::Replace;
pub use set_cells::{CellEdit, SetCells, duplicate_key_rows, guess_row_key};
pub use sort::{Sort, SortKey};
pub use split::Split;
pub use tidy_id::TidyId;

/// Write `values` into column `col`, one undoable `set` per row.
pub(crate) fn write_column(
    table: &mut crate::data::DataTable,
    col: usize,
    values: Vec<crate::data::CellValue>,
) {
    for (r, v) in values.into_iter().enumerate() {
        table.set(r, col, v);
    }
}

/// Insert a new text column at `idx` and fill it.
pub(crate) fn insert_filled(
    table: &mut crate::data::DataTable,
    idx: usize,
    name: String,
    values: Vec<crate::data::CellValue>,
) {
    table.insert_column(idx, name, "Utf8".to_string());
    write_column(table, idx, values);
}
