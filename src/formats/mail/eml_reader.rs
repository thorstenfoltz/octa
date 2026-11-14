//! One saved email, `.eml`: the Mail table (one row) and, when it has any,
//! the Attachments table. Both open at once.

use std::path::Path;

use anyhow::Result;

use super::{MailBook, mime, with_format};
use crate::data::DataTable;
use crate::formats::{FormatReader, TableInfo};

pub struct EmlReader;

/// Attachment bytes are read only for the Attachments table itself.
fn book(path: &Path, with_bytes: bool) -> Result<MailBook> {
    let mut book = MailBook::default();
    mime::push_message(&mut book, &std::fs::read(path)?, "", with_bytes);
    Ok(book)
}

impl FormatReader for EmlReader {
    fn name(&self) -> &str {
        "Email"
    }

    fn extensions(&self) -> &[&str] {
        &["eml"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        Ok(with_format(book(path, false)?.single(), self.name()))
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        Ok(Some(book(path, false)?.table_infos()))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        let with_bytes = table.eq_ignore_ascii_case(super::Kind::Attachments.name());
        Ok(with_format(
            book(path, with_bytes)?.table(table)?,
            self.name(),
        ))
    }

    fn opens_all_tables(&self) -> bool {
        true
    }
}
