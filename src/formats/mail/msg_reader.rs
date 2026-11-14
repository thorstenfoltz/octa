//! One saved Outlook item, `.msg` (and the template form `.oft`): a compound
//! file holding the item's MAPI properties (MS-OXMSG). The item goes to the
//! table its message class names (a mail, an appointment, a contact...);
//! a mail's attachments to the Attachments table. Both open at once.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use cfb::CompoundFile;

use super::mapi::{Guid, Props, Value, decode_8bit, decode_utf16, filetime, tag};
use super::{Attachment, MailBook, with_format};
use crate::data::DataTable;
use crate::formats::{FormatReader, TableInfo};

pub struct MsgReader;

/// PS_MAPI and PS_PUBLIC_STRINGS, the two sets MS-OXMSG numbers 1 and 2.
const PS_MAPI: Guid = [
    0x28, 0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];
const PS_PUBLIC_STRINGS: Guid = [
    0x29, 0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

fn stream(cf: &mut CompoundFile<File>, path: &str) -> Option<Vec<u8>> {
    let mut s = cf.open_stream(path).ok()?;
    let mut out = Vec::new();
    s.read_to_end(&mut out).ok()?;
    Some(out)
}

/// The property bag of one storage: fixed-size values from its property
/// stream (after a `header` of 32, 24 or 8 bytes), the rest from their own
/// `__substg1.0_` streams. 8-bit strings use `code_page`, else the bag's own.
fn read_bag(
    cf: &mut CompoundFile<File>,
    dir: &str,
    header: usize,
    code_page: Option<i64>,
) -> HashMap<u16, Value> {
    let mut out = HashMap::new();
    let Some(raw) = stream(cf, &format!("{dir}/__properties_version1.0")) else {
        return out;
    };
    let mut eight_bit: Vec<(u16, Vec<u8>)> = Vec::new();
    for e in raw.get(header..).unwrap_or_default().as_chunks::<16>().0 {
        let tag_value = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        let (kind, id) = ((tag_value & 0xFFFF) as u16, (tag_value >> 16) as u16);
        let v = &e[8..16];
        let sub = || format!("{dir}/__substg1.0_{tag_value:08X}");
        let value = match kind {
            0x0002 => Some(Value::Int(i64::from(i16::from_le_bytes([v[0], v[1]])))),
            0x0003 => Some(Value::Int(i64::from(i32::from_le_bytes([
                v[0], v[1], v[2], v[3],
            ])))),
            0x0014 => Some(Value::Int(i64::from_le_bytes(
                v.try_into().unwrap_or_default(),
            ))),
            0x0004 => Some(Value::Float(f64::from(f32::from_le_bytes([
                v[0], v[1], v[2], v[3],
            ])))),
            0x0005 => Some(Value::Float(f64::from_le_bytes(
                v.try_into().unwrap_or_default(),
            ))),
            0x000B => Some(Value::Bool(v[0] != 0)),
            0x0040 => {
                filetime(u64::from_le_bytes(v.try_into().unwrap_or_default())).map(Value::Time)
            }
            0x001F => stream(cf, &sub()).map(|b| Value::Str(decode_utf16(&b))),
            0x0102 => stream(cf, &sub()).map(Value::Bin),
            0x001E => {
                if let Some(b) = stream(cf, &sub()) {
                    eight_bit.push((id, b));
                }
                None
            }
            _ => None,
        };
        if let Some(value) = value {
            out.insert(id, value);
        }
    }
    let cp = code_page.or_else(|| {
        [tag::INTERNET_CPID, tag::MESSAGE_CODEPAGE]
            .iter()
            .find_map(|id| match out.get(id) {
                Some(Value::Int(i)) => Some(*i),
                _ => None,
            })
    });
    for (id, bytes) in eight_bit {
        out.insert(id, Value::Str(decode_8bit(&bytes, cp)));
    }
    out
}

/// Names of the sub-storages of the root starting with `prefix`, sorted.
fn storages(cf: &CompoundFile<File>, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = cf
        .read_root_storage()
        .filter(|e| e.is_storage() && e.name().starts_with(prefix))
        .map(|e| e.name().to_string())
        .collect();
    names.sort();
    names
}

/// Property id (0x8000 + n) -> (set, dispid), from `__nameid_version1.0`.
/// String-named properties are skipped: none of the mapped fields use one.
fn name_map(cf: &mut CompoundFile<File>) -> HashMap<u16, (Guid, u32)> {
    let base = "/__nameid_version1.0";
    let guids = stream(cf, &format!("{base}/__substg1.0_00020102")).unwrap_or_default();
    let entries = stream(cf, &format!("{base}/__substg1.0_00030102")).unwrap_or_default();
    let mut out = HashMap::new();
    for e in entries.as_chunks::<8>().0 {
        let name = u32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        let index_kind = u32::from_le_bytes([e[4], e[5], e[6], e[7]]);
        if index_kind & 1 != 0 {
            continue;
        }
        let guid_index = ((index_kind >> 1) & 0x7FFF) as usize;
        let guid = match guid_index {
            1 => Some(PS_MAPI),
            2 => Some(PS_PUBLIC_STRINGS),
            n => guids
                .get((n - 3) * 16..(n - 2) * 16)
                .and_then(|g| g.try_into().ok()),
        };
        if let Some(guid) = guid {
            let prop_id = 0x8000u16.wrapping_add((index_kind >> 16) as u16);
            out.insert(prop_id, (guid, name));
        }
    }
    out
}

fn str_of(bag: &HashMap<u16, Value>, id: u16) -> String {
    match bag.get(&id) {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The item in `path` as a book. `with_bytes` reads attachment contents
/// (only the Attachments table needs them).
pub fn book(path: &Path, with_bytes: bool) -> Result<MailBook> {
    let mut cf = cfb::open(path)?;
    let mut tags = read_bag(&mut cf, "", 32, None);
    let code_page = [tag::INTERNET_CPID, tag::MESSAGE_CODEPAGE]
        .iter()
        .find_map(|id| match tags.get(id) {
            Some(Value::Int(i)) => Some(*i),
            _ => None,
        });

    let mut named = HashMap::new();
    let map = name_map(&mut cf);
    tags.retain(|id, v| match map.get(id) {
        Some(key) if *id >= 0x8000 => {
            named.insert(*key, v.clone());
            false
        }
        _ => true,
    });

    let mut recipients = Vec::new();
    for dir in storages(&cf, "__recip_version1.0_#") {
        let r = read_bag(&mut cf, &format!("/{dir}"), 8, code_page);
        let smtp = str_of(&r, tag::SMTP_ADDRESS);
        let email = str_of(&r, tag::EMAIL_ADDRESS);
        let address = if smtp.is_empty() && email.contains('@') {
            email
        } else {
            smtp
        };
        let kind = match r.get(&tag::RECIPIENT_TYPE) {
            Some(Value::Int(k)) => *k,
            _ => 1,
        };
        recipients.push((
            kind,
            super::mapi::person(&str_of(&r, tag::DISPLAY_NAME), &address),
        ));
    }

    let mut attachments = Vec::new();
    for dir in storages(&cf, "__attach_version1.0_#") {
        let a = read_bag(&mut cf, &format!("/{dir}"), 8, code_page);
        let file_name = [
            tag::ATTACH_LONG_FILENAME,
            tag::ATTACH_FILENAME,
            tag::DISPLAY_NAME,
        ]
        .iter()
        .map(|id| str_of(&a, *id))
        .find(|s| !s.is_empty())
        .unwrap_or_default();
        // Method 5 is an attached Outlook item: listed by name only.
        let data = match (a.get(&tag::ATTACH_METHOD), a.get(&tag::ATTACH_DATA)) {
            (Some(Value::Int(5)), _) => None,
            (_, Some(Value::Bin(b))) => Some(b),
            _ => None,
        };
        attachments.push(Attachment {
            mail_id: 0,
            file_name,
            size: data.map_or(0, |b| b.len() as i64),
            mime: str_of(&a, tag::ATTACH_MIME_TAG),
            content: data.filter(|_| with_bytes).cloned(),
        });
    }

    let props = Props {
        tags,
        named,
        recipients,
    };
    let mut book = MailBook::default();
    props.push_into(&mut book, "", attachments);
    Ok(book)
}

impl FormatReader for MsgReader {
    fn name(&self) -> &str {
        "Outlook item"
    }

    fn extensions(&self) -> &[&str] {
        &["msg", "oft"]
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

#[cfg(test)]
#[path = "msg_reader_tests.rs"]
mod tests;
