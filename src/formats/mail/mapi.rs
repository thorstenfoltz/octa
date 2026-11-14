//! Outlook's MAPI properties as rows. `.msg` and `.pst` both store an item
//! as a bag of tagged properties; this maps the tags (and the named
//! properties appointments, contacts and tasks keep their fields in) to the
//! MailBook columns, so both readers agree. Ids from MS-OXPROPS.

use std::collections::HashMap;

use chrono::Datelike;

use super::{Attachment, Contact, Event, Kind, Mail, MailBook, Note, Task, join};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    /// `YYYY-MM-DD HH:MM:SS`, UTC.
    Time(String),
    Bin(Vec<u8>),
}

/// A GUID as stored on disk: Data1, Data2 and Data3 little-endian.
pub type Guid = [u8; 16];

/// PSETID_Appointment {00062002-0000-0000-C000-000000000046}.
pub const PSETID_APPOINTMENT: Guid = [
    0x02, 0x20, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];
/// PSETID_Task {00062003-0000-0000-C000-000000000046}.
pub const PSETID_TASK: Guid = [
    0x03, 0x20, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];
/// PSETID_Address {00062004-0000-0000-C000-000000000046}.
pub const PSETID_ADDRESS: Guid = [
    0x04, 0x20, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

/// The textual form, `00062002-0000-0000-C000-000000000046`.
pub fn guid_text(g: &Guid) -> String {
    let rest: String = g[10..].iter().map(|b| format!("{b:02X}")).collect();
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{rest}",
        u32::from_le_bytes([g[0], g[1], g[2], g[3]]),
        u16::from_le_bytes([g[4], g[5]]),
        u16::from_le_bytes([g[6], g[7]]),
        g[8],
        g[9],
    )
}

/// Property ids (the high 16 bits of a tag).
pub mod tag {
    pub const IMPORTANCE: u16 = 0x0017;
    pub const MESSAGE_CLASS: u16 = 0x001A;
    pub const SUBJECT: u16 = 0x0037;
    pub const CLIENT_SUBMIT_TIME: u16 = 0x0039;
    pub const START_DATE: u16 = 0x0060;
    pub const END_DATE: u16 = 0x0061;
    pub const SENDER_NAME: u16 = 0x0C1A;
    pub const SENDER_EMAIL: u16 = 0x0C1F;
    pub const DISPLAY_CC: u16 = 0x0E03;
    pub const DISPLAY_TO: u16 = 0x0E04;
    pub const DELIVERY_TIME: u16 = 0x0E06;
    pub const BODY: u16 = 0x1000;
    pub const BODY_HTML: u16 = 0x1013;
    pub const INTERNET_MESSAGE_ID: u16 = 0x1035;
    pub const DISPLAY_NAME: u16 = 0x3001;
    pub const EMAIL_ADDRESS: u16 = 0x3003;
    pub const CREATION_TIME: u16 = 0x3007;
    pub const LAST_MODIFICATION_TIME: u16 = 0x3008;
    pub const ATTACH_DATA: u16 = 0x3701;
    pub const ATTACH_FILENAME: u16 = 0x3704;
    pub const ATTACH_METHOD: u16 = 0x3705;
    pub const ATTACH_LONG_FILENAME: u16 = 0x3707;
    pub const ATTACH_MIME_TAG: u16 = 0x370E;
    pub const SMTP_ADDRESS: u16 = 0x39FE;
    pub const RECIPIENT_TYPE: u16 = 0x0C15;
    pub const GIVEN_NAME: u16 = 0x3A06;
    pub const BUSINESS_PHONE: u16 = 0x3A08;
    pub const HOME_PHONE: u16 = 0x3A09;
    pub const SURNAME: u16 = 0x3A11;
    pub const COMPANY: u16 = 0x3A16;
    pub const TITLE: u16 = 0x3A17;
    pub const MOBILE: u16 = 0x3A1C;
    pub const COUNTRY: u16 = 0x3A26;
    pub const CITY: u16 = 0x3A27;
    pub const STREET: u16 = 0x3A29;
    pub const POSTAL_CODE: u16 = 0x3A2A;
    pub const BIRTHDAY: u16 = 0x3A42;
    pub const INTERNET_CPID: u16 = 0x3FDE;
    pub const MESSAGE_CODEPAGE: u16 = 0x3FFD;
    pub const SENDER_SMTP: u16 = 0x5D01;
}

/// Named-property ids (dispids) inside their sets.
pub mod named {
    pub const LOCATION: u32 = 0x8208;
    pub const APPOINTMENT_START: u32 = 0x820D;
    pub const APPOINTMENT_END: u32 = 0x820E;
    pub const ALL_DAY: u32 = 0x8215;
    pub const RECURRENCE_PATTERN: u32 = 0x8232;
    pub const TASK_STATUS: u32 = 0x8101;
    pub const PERCENT_COMPLETE: u32 = 0x8102;
    pub const TASK_START: u32 = 0x8104;
    pub const TASK_DUE: u32 = 0x8105;
    pub const EMAIL1: u32 = 0x8083;
    pub const EMAIL2: u32 = 0x8093;
    pub const EMAIL3: u32 = 0x80A3;
}

/// A Windows FILETIME (100 ns ticks since 1601) as `YYYY-MM-DD HH:MM:SS` UTC.
/// `None` for 0 and for Outlook's "no date" (4501-01-01).
pub fn filetime(ft: u64) -> Option<String> {
    if ft == 0 {
        return None;
    }
    let secs = i64::try_from(ft / 10_000_000).ok()? - 11_644_473_600;
    let t = chrono::DateTime::from_timestamp(secs, 0)?;
    (t.year() < 4500).then(|| t.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// 8-bit text in the item's Windows code page (default windows-1252).
pub fn decode_8bit(bytes: &[u8], code_page: Option<i64>) -> String {
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    let enc = code_page
        .and_then(|cp| u16::try_from(cp).ok())
        .and_then(codepage::to_encoding)
        .unwrap_or(encoding_rs::WINDOWS_1252);
    enc.decode(bytes).0.into_owned()
}

/// UTF-16LE text without its trailing NUL.
pub fn decode_utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
        .trim_end_matches('\0')
        .to_string()
}

/// Which table an item of this message class belongs to. Meeting requests
/// and anything unknown are mail.
pub fn kind_of_class(class: &str) -> Kind {
    let class = class.trim().to_ascii_lowercase();
    let is = |p: &str| class == p || class.starts_with(&format!("{p}."));
    if is("ipm.appointment") {
        Kind::Calendar
    } else if is("ipm.contact") || is("ipm.distlist") {
        Kind::Contacts
    } else if is("ipm.task") {
        Kind::Tasks
    } else if is("ipm.stickynote") {
        Kind::Notes
    } else {
        Kind::Mail
    }
}

/// A subject without the prefix marker Outlook may store in front of it
/// (`\u{1}` and one character saying how long the `RE: ` prefix is).
pub fn clean_subject(s: &str) -> String {
    match s.strip_prefix('\u{1}') {
        Some(rest) => rest.chars().skip(1).collect(),
        None => s.to_string(),
    }
}

#[derive(Debug, Default)]
pub struct Props {
    pub tags: HashMap<u16, Value>,
    /// (set, dispid) -> value, resolved from each format's name map.
    pub named: HashMap<(Guid, u32), Value>,
    /// (1 to, 2 cc, 3 bcc; "Name <address>").
    pub recipients: Vec<(i64, String)>,
}

/// `Name <address>`, or whichever of the two there is.
pub fn person(name: &str, address: &str) -> String {
    match (name.trim(), address.trim()) {
        ("", a) => a.to_string(),
        (n, "") => n.to_string(),
        (n, a) if n == a => a.to_string(),
        (n, a) => format!("{n} <{a}>"),
    }
}

impl Props {
    fn str(&self, id: u16) -> String {
        match self.tags.get(&id) {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        }
    }

    fn time(&self, id: u16) -> Option<String> {
        match self.tags.get(&id) {
            Some(Value::Time(t)) => Some(t.clone()),
            _ => None,
        }
    }

    fn int(&self, id: u16) -> Option<i64> {
        match self.tags.get(&id) {
            Some(Value::Int(i)) => Some(*i),
            _ => None,
        }
    }

    fn named(&self, set: Guid, id: u32) -> Option<&Value> {
        self.named.get(&(set, id))
    }

    fn named_str(&self, set: Guid, id: u32) -> String {
        match self.named(set, id) {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        }
    }

    fn named_time(&self, set: Guid, id: u32) -> Option<String> {
        match self.named(set, id) {
            Some(Value::Time(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// The code page 8-bit strings of this item are written in.
    pub fn code_page(&self) -> Option<i64> {
        self.int(tag::INTERNET_CPID)
            .or_else(|| self.int(tag::MESSAGE_CODEPAGE))
    }

    /// Which table the item belongs to, by its message class.
    pub fn kind(&self) -> Kind {
        kind_of_class(&self.str(tag::MESSAGE_CLASS))
    }

    /// The plain body, else the HTML body as text. A body stored only as
    /// compressed RTF stays empty.
    fn body(&self) -> String {
        let plain = self.str(tag::BODY);
        if !plain.trim().is_empty() {
            return plain;
        }
        match self.tags.get(&tag::BODY_HTML) {
            Some(Value::Str(h)) => mail_parser::decoders::html::html_to_text(h),
            Some(Value::Bin(b)) => {
                mail_parser::decoders::html::html_to_text(&String::from_utf8_lossy(b))
            }
            _ => String::new(),
        }
    }

    fn sender(&self) -> String {
        let smtp = self.str(tag::SENDER_SMTP);
        let email = self.str(tag::SENDER_EMAIL);
        let address = if smtp.is_empty() && email.contains('@') {
            email
        } else {
            smtp
        };
        person(&self.str(tag::SENDER_NAME), &address)
    }

    /// Recipients of one type, else the display line Outlook keeps.
    fn recipients_of(&self, kind: i64, display: u16) -> String {
        let listed = join(
            self.recipients
                .iter()
                .filter(|(k, _)| *k == kind)
                .map(|(_, r)| r.clone()),
        );
        if listed.is_empty() {
            self.str(display)
        } else {
            listed
        }
    }

    fn priority(&self) -> String {
        match self.int(tag::IMPORTANCE) {
            Some(0) => "low".into(),
            Some(1) => "normal".into(),
            Some(2) => "high".into(),
            _ => String::new(),
        }
    }

    /// Add the item to the table its message class belongs to. Only a mail
    /// keeps its attachments (the Attachments table links to Mail).
    pub fn push_into(&self, book: &mut MailBook, folder: &str, attachments: Vec<Attachment>) {
        let folder = folder.to_string();
        let subject = clean_subject(&self.str(tag::SUBJECT));
        match self.kind() {
            Kind::Calendar => book.events.push(Event {
                folder,
                title: subject,
                start: self
                    .named_time(PSETID_APPOINTMENT, named::APPOINTMENT_START)
                    .or_else(|| self.time(tag::START_DATE)),
                end: self
                    .named_time(PSETID_APPOINTMENT, named::APPOINTMENT_END)
                    .or_else(|| self.time(tag::END_DATE)),
                all_day: matches!(
                    self.named(PSETID_APPOINTMENT, named::ALL_DAY),
                    Some(Value::Bool(true))
                ),
                location: self.named_str(PSETID_APPOINTMENT, named::LOCATION),
                organiser: self.sender(),
                attendees: join(self.recipients.iter().map(|(_, r)| r.clone())),
                recurrence: self.named_str(PSETID_APPOINTMENT, named::RECURRENCE_PATTERN),
                description: self.body(),
            }),
            Kind::Contacts => {
                let phone = |id: u16, what: &str| {
                    let n = self.str(id);
                    if n.trim().is_empty() {
                        String::new()
                    } else {
                        format!("{n} ({what})")
                    }
                };
                book.contacts.push(Contact {
                    folder,
                    name: self.str(tag::DISPLAY_NAME),
                    given_name: self.str(tag::GIVEN_NAME),
                    family_name: self.str(tag::SURNAME),
                    company: self.str(tag::COMPANY),
                    job_title: self.str(tag::TITLE),
                    emails: join(
                        [named::EMAIL1, named::EMAIL2, named::EMAIL3]
                            .map(|id| self.named_str(PSETID_ADDRESS, id)),
                    ),
                    phones: join([
                        phone(tag::BUSINESS_PHONE, "work"),
                        phone(tag::HOME_PHONE, "home"),
                        phone(tag::MOBILE, "mobile"),
                    ]),
                    address: [tag::STREET, tag::CITY, tag::POSTAL_CODE, tag::COUNTRY]
                        .map(|id| self.str(id).trim().to_string())
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(", "),
                    birthday: self
                        .time(tag::BIRTHDAY)
                        .and_then(|t| t.get(..10).map(str::to_string)),
                    note: self.body(),
                });
            }
            Kind::Tasks => book.tasks.push(Task {
                folder,
                subject,
                start: self.named_time(PSETID_TASK, named::TASK_START),
                due: self.named_time(PSETID_TASK, named::TASK_DUE),
                status: match self.named(PSETID_TASK, named::TASK_STATUS) {
                    Some(Value::Int(0)) => "not started".into(),
                    Some(Value::Int(1)) => "in progress".into(),
                    Some(Value::Int(2)) => "complete".into(),
                    Some(Value::Int(3)) => "waiting".into(),
                    Some(Value::Int(4)) => "deferred".into(),
                    _ => String::new(),
                },
                // Stored as 0.0 to 1.0.
                percent_done: match self.named(PSETID_TASK, named::PERCENT_COMPLETE) {
                    Some(Value::Float(f)) => Some((f * 1000.0).round() / 10.0),
                    _ => None,
                },
                priority: self.priority(),
                body: self.body(),
            }),
            Kind::Notes => book.notes.push(Note {
                folder,
                text: self.body(),
                created: self.time(tag::CREATION_TIME),
                modified: self.time(tag::LAST_MODIFICATION_TIME),
            }),
            Kind::Mail | Kind::Attachments => {
                let mail = Mail {
                    folder,
                    sent: self.time(tag::CLIENT_SUBMIT_TIME),
                    received: self.time(tag::DELIVERY_TIME),
                    from: self.sender(),
                    to: self.recipients_of(1, tag::DISPLAY_TO),
                    cc: self.recipients_of(2, tag::DISPLAY_CC),
                    subject,
                    body: self.body(),
                    message_id: self
                        .str(tag::INTERNET_MESSAGE_ID)
                        .trim()
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .to_string(),
                };
                book.push_mail(mail, attachments);
            }
        }
    }
}

#[cfg(test)]
#[path = "mapi_tests.rs"]
mod tests;
