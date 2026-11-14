//! A Unix mailbox, `.mbox`: every message one row of the Mail table, their
//! attachments in the Attachments table. Opens through the table picker,
//! like a database. Reads at most the initial-rows setting of messages.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::Result;
use mail_parser::mailbox::mbox::MessageIterator;

use super::{MailBook, mime, with_format};
use crate::data::DataTable;
use crate::formats::{FormatReader, TableInfo};

pub struct MboxReader;

fn book(path: &Path, with_bytes: bool) -> Result<MailBook> {
    let mut book = MailBook::default();
    let messages = MessageIterator::new(BufReader::new(File::open(path)?));
    for msg in messages.take(crate::formats::initial_load_rows()) {
        mime::push_message(&mut book, msg?.contents(), "", with_bytes);
    }
    Ok(book)
}

impl FormatReader for MboxReader {
    fn name(&self) -> &str {
        "Mailbox"
    }

    fn extensions(&self) -> &[&str] {
        &["mbox"]
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
}
