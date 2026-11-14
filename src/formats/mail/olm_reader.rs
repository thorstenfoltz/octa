//! Outlook for Mac export, `.olm`: a zip of XML files, one per message plus
//! one per calendar and address book, with attachments as files beside them.
//!
//! Microsoft does not document the format; this follows public descriptions
//! of what Outlook for Mac 2011 to 2019 writes. Element and attribute names
//! are matched ignoring case, and anything unknown is skipped, never an
//! error. Opens through the table picker; rows carry their folder.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::Result;

use super::mapi::person;
use super::{Attachment, Contact, Event, Mail, MailBook, join, with_format};
use crate::data::DataTable;
use crate::formats::xml_reader::{XmlElement, parse_tree};
use crate::formats::{FormatReader, TableInfo};

pub struct OlmReader;

/// The part after a namespace prefix.
fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn child<'a>(e: &'a XmlElement, name: &str) -> Option<&'a XmlElement> {
    e.children
        .iter()
        .find(|c| local(&c.name).eq_ignore_ascii_case(name))
}

fn text(e: &XmlElement, name: &str) -> String {
    child(e, name)
        .map(|c| c.text.trim().to_string())
        .unwrap_or_default()
}

fn attr<'a>(e: &'a XmlElement, name: &str) -> &'a str {
    e.attributes
        .iter()
        .find(|(k, _)| local(k).eq_ignore_ascii_case(name))
        .map_or("", |(_, v)| v.as_str())
}

/// `2024-11-05T09:00:00` (a zone or fraction after it ignored) or a bare
/// date, as `YYYY-MM-DD HH:MM:SS`.
fn time(e: &XmlElement, name: &str) -> Option<String> {
    let t = text(e, name);
    let fmt = |d: chrono::NaiveDateTime| d.format("%Y-%m-%d %H:%M:%S").to_string();
    if let Some(d) = t
        .get(..19)
        .and_then(|s| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
    {
        return Some(fmt(d));
    }
    let d = chrono::NaiveDate::parse_from_str(t.get(..10)?, "%Y-%m-%d").ok()?;
    Some(fmt(d.and_hms_opt(0, 0, 0)?))
}

/// The `emailAddress` children of a list element, as `Name <address>`.
fn addresses(e: &XmlElement, name: &str) -> String {
    child(e, name).map_or_else(String::new, |list| {
        join(list.children.iter().map(|a| {
            person(
                attr(a, "OPFContactEmailAddressName"),
                attr(a, "OPFContactEmailAddressAddress"),
            )
        }))
    })
}

/// The folder an entry sits in: the path after the `com.microsoft.__...`
/// part, else that part's own name (`Calendar`).
fn folder_of(entry: &str) -> String {
    let parts: Vec<&str> = entry.split('/').collect();
    let Some(i) = parts.iter().position(|p| p.starts_with("com.microsoft.__")) else {
        return String::new();
    };
    let inner = &parts[i + 1..parts.len().saturating_sub(1)];
    if inner.is_empty() {
        parts[i].trim_start_matches("com.microsoft.__").to_string()
    } else {
        inner.join("/")
    }
}

fn mail(email: &XmlElement, folder: String) -> Mail {
    let body = text(email, "OPFMessageCopyBody");
    let body = if body.is_empty() {
        mail_parser::decoders::html::html_to_text(&text(email, "OPFMessageCopyHTMLBody"))
    } else {
        body
    };
    Mail {
        folder,
        sent: time(email, "OPFMessageCopySentTime"),
        received: time(email, "OPFMessageCopyReceivedTime"),
        from: addresses(email, "OPFMessageCopyFromAddresses"),
        to: addresses(email, "OPFMessageCopyToAddresses"),
        cc: addresses(email, "OPFMessageCopyCCAddresses"),
        subject: text(email, "OPFMessageCopySubject"),
        body: body.trim().to_string(),
        message_id: text(email, "OPFMessageCopyMessageID")
            .trim_start_matches('<')
            .trim_end_matches('>')
            .to_string(),
    }
}

fn event(a: &XmlElement, folder: String) -> Event {
    let all_day = text(a, "OPFCalendarEventCopyIsAllDayEvent");
    Event {
        folder,
        title: text(a, "OPFCalendarEventCopySummary"),
        start: time(a, "OPFCalendarEventCopyStartTime"),
        end: time(a, "OPFCalendarEventCopyEndTime"),
        all_day: all_day == "1" || all_day.eq_ignore_ascii_case("true"),
        location: text(a, "OPFCalendarEventCopyLocation"),
        organiser: text(a, "OPFCalendarEventCopyOrganizer"),
        attendees: addresses(a, "OPFCalendarEventCopyAttendeeList"),
        recurrence: String::new(),
        description: mail_parser::decoders::html::html_to_text(&text(
            a,
            "OPFCalendarEventCopyDescriptionPlain",
        )),
    }
}

fn contact(c: &XmlElement, folder: String) -> Contact {
    let phone = |name: &str, what: &str| {
        let n = text(c, name);
        if n.is_empty() {
            n
        } else {
            format!("{n} ({what})")
        }
    };
    Contact {
        folder,
        name: text(c, "OPFContactCopyDisplayName"),
        given_name: text(c, "OPFContactCopyFirstName"),
        family_name: text(c, "OPFContactCopyLastName"),
        company: text(c, "OPFContactCopyBusinessCompany"),
        job_title: text(c, "OPFContactCopyJobTitle"),
        emails: text(c, "OPFContactCopyDefaultEmailAddress"),
        phones: join([
            phone("OPFContactCopyBusinessPhone", "work"),
            phone("OPFContactCopyHomePhone", "home"),
            phone("OPFContactCopyCellPhone", "mobile"),
        ]),
        address: String::new(),
        birthday: time(c, "OPFContactCopyBirthday").and_then(|t| t.get(..10).map(str::to_string)),
        note: text(c, "OPFContactCopyNotesPlain"),
    }
}

/// Everything in the export. `with_bytes` reads attachment contents (only
/// the Attachments table needs them); sizes come from the zip either way.
fn book(path: &Path, with_bytes: bool) -> Result<MailBook> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| Some(zip.by_index_raw(i).ok()?.name().ok()?.into_owned()))
        .filter(|n| n.to_ascii_lowercase().ends_with(".xml"))
        .collect();
    names.sort();
    let cap = crate::formats::initial_load_rows();
    let mut book = MailBook {
        has_folders: true,
        ..MailBook::default()
    };
    for name in names {
        let mut xml = String::new();
        if zip.by_name(&name)?.read_to_string(&mut xml).is_err() {
            continue;
        }
        let Ok(tree) = parse_tree(&xml) else {
            continue;
        };
        let folder = folder_of(&name);
        for root in &tree {
            let kind = local(&root.name).to_ascii_lowercase();
            for item in &root.children {
                match kind.as_str() {
                    "emails" if book.mail.len() < cap => {
                        let atts = attachments(&mut zip, item, with_bytes);
                        book.push_mail(mail(item, folder.clone()), atts);
                    }
                    "appointments" => book.events.push(event(item, folder.clone())),
                    "contacts" => book.contacts.push(contact(item, folder.clone())),
                    _ => {}
                }
            }
        }
    }
    Ok(book)
}

/// A message's attachments; their files sit in the zip at the URL given.
fn attachments(
    zip: &mut zip::ZipArchive<BufReader<File>>,
    email: &XmlElement,
    with_bytes: bool,
) -> Vec<Attachment> {
    let Some(list) = child(email, "OPFMessageCopyAttachmentList") else {
        return Vec::new();
    };
    list.children
        .iter()
        .map(|a| {
            let url = attr(a, "OPFAttachmentURL");
            let (size, content) = match zip.by_name(url) {
                Ok(mut f) => {
                    let size = f.size() as i64;
                    let mut bytes = Vec::new();
                    let content =
                        (with_bytes && f.read_to_end(&mut bytes).is_ok()).then_some(bytes);
                    (size, content)
                }
                Err(_) => (0, None),
            };
            Attachment {
                mail_id: 0,
                file_name: attr(a, "OPFAttachmentName").to_string(),
                size,
                mime: attr(a, "OPFAttachmentContentType").to_string(),
                content,
            }
        })
        .collect()
}

impl FormatReader for OlmReader {
    fn name(&self) -> &str {
        "Outlook for Mac export"
    }

    fn extensions(&self) -> &[&str] {
        &["olm"]
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

#[cfg(test)]
#[path = "olm_reader_tests.rs"]
mod tests;
