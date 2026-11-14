//! Mail, calendar and contact files as tables: one table per kind of item
//! (Mail, Calendar, Contacts, Tasks, Notes) plus Attachments, offered only
//! when the file holds that kind. Every reader in this folder fills a
//! [`MailBook`]; the book turns into tables.

pub mod content_line;
pub mod eml_reader;
pub mod ics_reader;
pub mod mapi;
pub mod mbox_reader;
pub mod mime;
pub mod msg_reader;
pub mod olm_reader;
pub mod pst_reader;
pub mod vcf_reader;

use crate::data::{CellValue, ColumnInfo, DataTable};
use crate::formats::TableInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Mail,
    Calendar,
    Contacts,
    Tasks,
    Notes,
    Attachments,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Self::Mail,
        Self::Calendar,
        Self::Contacts,
        Self::Tasks,
        Self::Notes,
        Self::Attachments,
    ];
    /// The table name: English and stable, like a sheet name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mail => "Mail",
            Self::Calendar => "Calendar",
            Self::Contacts => "Contacts",
            Self::Tasks => "Tasks",
            Self::Notes => "Notes",
            Self::Attachments => "Attachments",
        }
    }
}

/// Timestamps are `YYYY-MM-DD HH:MM:SS`, dates `YYYY-MM-DD`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mail {
    pub folder: String,
    pub sent: Option<String>,
    pub received: Option<String>,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub message_id: String,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Event {
    pub folder: String,
    pub title: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub all_day: bool,
    pub location: String,
    pub organiser: String,
    pub attendees: String,
    pub recurrence: String,
    pub description: String,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contact {
    pub folder: String,
    pub name: String,
    pub given_name: String,
    pub family_name: String,
    pub company: String,
    pub job_title: String,
    pub emails: String,
    pub phones: String,
    pub address: String,
    pub birthday: Option<String>,
    pub note: String,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Task {
    pub folder: String,
    pub subject: String,
    pub start: Option<String>,
    pub due: Option<String>,
    pub status: String,
    pub percent_done: Option<f64>,
    pub priority: String,
    pub body: String,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Note {
    pub folder: String,
    pub text: String,
    pub created: Option<String>,
    pub modified: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attachment {
    pub mail_id: i64,
    pub file_name: String,
    pub size: i64,
    pub mime: String,
    pub content: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct MailBook {
    /// Mailboxes (.pst, .olm) have folders; a single file does not.
    pub has_folders: bool,
    pub mail: Vec<(Mail, i64)>,
    pub events: Vec<Event>,
    pub contacts: Vec<Contact>,
    pub tasks: Vec<Task>,
    pub notes: Vec<Note>,
    pub attachments: Vec<Attachment>,
}

fn s(v: &str) -> CellValue {
    if v.is_empty() {
        CellValue::Null
    } else {
        CellValue::String(v.to_string())
    }
}
fn ts(v: &Option<String>) -> CellValue {
    v.clone().map_or(CellValue::Null, CellValue::DateTime)
}
fn d(v: &Option<String>) -> CellValue {
    v.clone().map_or(CellValue::Null, CellValue::Date)
}
const TS: &str = "Timestamp(Microsecond, None)";

/// Name the format on a table built from a book, as every reader does.
pub(crate) fn with_format(mut t: DataTable, name: &str) -> DataTable {
    t.format_name = Some(name.to_string());
    t
}

/// The file as text: UTF-8 (a BOM is honoured), invalid bytes replaced.
pub(crate) fn read_text(path: &std::path::Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(encoding_rs::UTF_8.decode(&bytes).0.into_owned())
}

/// `a; b; c`, empties dropped: several values in one cell.
pub fn join(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

impl MailBook {
    /// Add a mail and its attachments; returns its `mail_id` (1-based row).
    pub fn push_mail(&mut self, mail: Mail, attachments: Vec<Attachment>) -> i64 {
        let id = self.mail.len() as i64 + 1;
        let count = attachments.len() as i64;
        self.attachments.extend(
            attachments
                .into_iter()
                .map(|a| Attachment { mail_id: id, ..a }),
        );
        self.mail.push((mail, count));
        id
    }

    fn rows(&self, kind: Kind) -> usize {
        match kind {
            Kind::Mail => self.mail.len(),
            Kind::Calendar => self.events.len(),
            Kind::Contacts => self.contacts.len(),
            Kind::Tasks => self.tasks.len(),
            Kind::Notes => self.notes.len(),
            Kind::Attachments => self.attachments.len(),
        }
    }

    /// The columns of `kind`'s table, for a picker listing a file it has not
    /// read yet.
    pub fn columns_of(kind: Kind, has_folders: bool) -> Vec<ColumnInfo> {
        MailBook {
            has_folders,
            ..MailBook::default()
        }
        .build(kind)
        .columns
    }

    /// The kinds this file holds, for the table picker.
    pub fn table_infos(&self) -> Vec<TableInfo> {
        Kind::ALL
            .into_iter()
            .filter(|k| self.rows(*k) > 0)
            .map(|k| {
                let t = self.build(k);
                TableInfo {
                    name: k.name().to_string(),
                    schema: None,
                    columns: t.columns,
                    row_count: Some(t.rows.len()),
                }
            })
            .collect()
    }

    pub fn table(&self, name: &str) -> anyhow::Result<DataTable> {
        let k = Kind::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| anyhow::anyhow!("no table \"{name}\" in this file"))?;
        Ok(self.build(k))
    }

    /// The first non-empty table (what `read_file` returns).
    pub fn single(&self) -> DataTable {
        Kind::ALL
            .into_iter()
            .find(|k| self.rows(*k) > 0)
            .map_or_else(DataTable::empty, |k| self.build(k))
    }

    fn build(&self, kind: Kind) -> DataTable {
        let folder = self.has_folders;
        let mut cols: Vec<(&str, &str)> = Vec::new();
        if folder && kind != Kind::Attachments {
            cols.push(("folder", "Utf8"));
        }
        let rows: Vec<Vec<CellValue>> = match kind {
            Kind::Mail => {
                cols.splice(0..0, [("mail_id", "Int64")]);
                cols.extend([
                    ("sent", TS),
                    ("received", TS),
                    ("from", "Utf8"),
                    ("to", "Utf8"),
                    ("cc", "Utf8"),
                    ("subject", "Utf8"),
                    ("attachments", "Int64"),
                    ("body", "Utf8"),
                    ("message_id", "Utf8"),
                ]);
                self.mail
                    .iter()
                    .enumerate()
                    .map(|(i, (m, n))| {
                        let mut r = vec![CellValue::Int(i as i64 + 1)];
                        if folder {
                            r.push(s(&m.folder));
                        }
                        r.extend([
                            ts(&m.sent),
                            ts(&m.received),
                            s(&m.from),
                            s(&m.to),
                            s(&m.cc),
                            s(&m.subject),
                            CellValue::Int(*n),
                            s(&m.body),
                            s(&m.message_id),
                        ]);
                        r
                    })
                    .collect()
            }
            Kind::Calendar => {
                cols.extend([
                    ("title", "Utf8"),
                    ("start", TS),
                    ("end", TS),
                    ("all_day", "Boolean"),
                    ("location", "Utf8"),
                    ("organiser", "Utf8"),
                    ("attendees", "Utf8"),
                    ("recurrence", "Utf8"),
                    ("description", "Utf8"),
                ]);
                self.events
                    .iter()
                    .map(|e| {
                        let mut r = if folder { vec![s(&e.folder)] } else { vec![] };
                        r.extend([
                            s(&e.title),
                            ts(&e.start),
                            ts(&e.end),
                            CellValue::Bool(e.all_day),
                            s(&e.location),
                            s(&e.organiser),
                            s(&e.attendees),
                            s(&e.recurrence),
                            s(&e.description),
                        ]);
                        r
                    })
                    .collect()
            }
            Kind::Contacts => {
                cols.extend([
                    ("name", "Utf8"),
                    ("given_name", "Utf8"),
                    ("family_name", "Utf8"),
                    ("company", "Utf8"),
                    ("job_title", "Utf8"),
                    ("emails", "Utf8"),
                    ("phones", "Utf8"),
                    ("address", "Utf8"),
                    ("birthday", "Date32"),
                    ("note", "Utf8"),
                ]);
                self.contacts
                    .iter()
                    .map(|c| {
                        let mut r = if folder { vec![s(&c.folder)] } else { vec![] };
                        r.extend([
                            s(&c.name),
                            s(&c.given_name),
                            s(&c.family_name),
                            s(&c.company),
                            s(&c.job_title),
                            s(&c.emails),
                            s(&c.phones),
                            s(&c.address),
                            d(&c.birthday),
                            s(&c.note),
                        ]);
                        r
                    })
                    .collect()
            }
            Kind::Tasks => {
                cols.extend([
                    ("subject", "Utf8"),
                    ("start", TS),
                    ("due", TS),
                    ("status", "Utf8"),
                    ("percent_done", "Float64"),
                    ("priority", "Utf8"),
                    ("body", "Utf8"),
                ]);
                self.tasks
                    .iter()
                    .map(|t| {
                        let mut r = if folder { vec![s(&t.folder)] } else { vec![] };
                        r.extend([
                            s(&t.subject),
                            ts(&t.start),
                            ts(&t.due),
                            s(&t.status),
                            t.percent_done.map_or(CellValue::Null, CellValue::Float),
                            s(&t.priority),
                            s(&t.body),
                        ]);
                        r
                    })
                    .collect()
            }
            Kind::Notes => {
                cols.extend([("text", "Utf8"), ("created", TS), ("modified", TS)]);
                self.notes
                    .iter()
                    .map(|n| {
                        let mut r = if folder { vec![s(&n.folder)] } else { vec![] };
                        r.extend([s(&n.text), ts(&n.created), ts(&n.modified)]);
                        r
                    })
                    .collect()
            }
            Kind::Attachments => {
                cols.extend([
                    ("mail_id", "Int64"),
                    ("file_name", "Utf8"),
                    ("size", "Int64"),
                    ("type", "Utf8"),
                    ("content", "Binary"),
                ]);
                self.attachments
                    .iter()
                    .map(|a| {
                        vec![
                            CellValue::Int(a.mail_id),
                            s(&a.file_name),
                            CellValue::Int(a.size),
                            s(&a.mime),
                            a.content.clone().map_or(CellValue::Null, CellValue::Binary),
                        ]
                    })
                    .collect()
            }
        };
        DataTable {
            columns: cols
                .into_iter()
                .map(|(n, t)| ColumnInfo {
                    name: n.into(),
                    data_type: t.into(),
                })
                .collect(),
            rows,
            ..DataTable::empty()
        }
    }
}
