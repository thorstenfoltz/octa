//! Outlook data file, `.pst`: a whole mailbox (mail, calendar, contacts,
//! tasks, notes) read with Microsoft's `outlook-pst` crate. Opens through the
//! table picker like a database; every row carries the folder it sits in.
//! The walk follows the crate's own `browse_pst` example: the IPM subtree,
//! each folder's hierarchy table for subfolders and contents table for items.
//!
//! The picker's counts come from each folder's contents table (it holds the
//! message class), so listing opens no message. Reading a table opens only
//! the items of that kind, at most the initial-rows setting of them.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::rc::Rc;

use anyhow::Result;
use outlook_pst::ltp::prop_context::{GuidValue, PropertyValue};
use outlook_pst::ltp::table_context::TableContext;
use outlook_pst::messaging::attachment::{
    AnsiAttachment, Attachment as PstAttachment, AttachmentData, UnicodeAttachment,
};
use outlook_pst::messaging::folder::Folder;
use outlook_pst::messaging::message::{AnsiMessage, Message as PstMessage, UnicodeMessage};
use outlook_pst::messaging::named_prop::{
    NamedPropertyGuid, NamedPropertyId, PS_MAPI, PS_PUBLIC_STRINGS,
};
use outlook_pst::messaging::store::{AnsiStore, EntryId, Store, UnicodeStore};
use outlook_pst::ndb::node_id::NodeId;
use outlook_pst::{AnsiPstFile, UnicodePstFile};

use super::mapi::{self, Guid, Props, Value, tag};
use super::{Attachment, Kind, MailBook, with_format};
use crate::data::DataTable;
use crate::formats::{FormatReader, TableInfo};

pub struct PstReader;

/// PR_HASATTACH, in a contents table row.
const HAS_ATTACH: u16 = 0x0E1B;
/// PR_MESSAGE_FLAGS; bit 0x10 (MSGFLAG_HASATTACH) says the same.
const MESSAGE_FLAGS: u16 = 0x0E07;
/// PR_ATTACH_SIZE, in an attachment table row.
const ATTACH_SIZE: u16 = 0x0E20;

/// A store, Unicode (Outlook 2003 on) or ANSI (Outlook 97-2002). Only
/// opening attachments needs to know which.
enum Pst {
    Unicode(Rc<UnicodeStore>),
    Ansi(Rc<AnsiStore>),
}

impl Pst {
    fn open(path: &Path) -> io::Result<Self> {
        match UnicodePstFile::open(path) {
            Ok(f) => Ok(Self::Unicode(UnicodeStore::read(Rc::new(f))?)),
            Err(_) => Ok(Self::Ansi(AnsiStore::read(Rc::new(AnsiPstFile::open(
                path,
            )?))?)),
        }
    }

    fn store(&self) -> Rc<dyn Store> {
        match self {
            Self::Unicode(s) => s.clone(),
            Self::Ansi(s) => s.clone(),
        }
    }

    /// The attachments in `nodes` of the message `id`, opened for their bytes.
    fn attachments(
        &self,
        id: &EntryId,
        nodes: &[NodeId],
    ) -> io::Result<Vec<Rc<dyn PstAttachment>>> {
        match self {
            Self::Unicode(s) => {
                let m = UnicodeMessage::read(s.clone(), id, None)?;
                nodes
                    .iter()
                    .map(|n| UnicodeAttachment::read(m.clone(), *n, None).map(|a| a as _))
                    .collect()
            }
            Self::Ansi(s) => {
                let m = AnsiMessage::read(s.clone(), id, None)?;
                nodes
                    .iter()
                    .map(|n| AnsiAttachment::read(m.clone(), *n, None).map(|a| a as _))
                    .collect()
            }
        }
    }
}

/// A property as a MailBook value; 8-bit strings in `code_page`.
fn value(v: &PropertyValue, code_page: Option<i64>) -> Option<Value> {
    Some(match v {
        PropertyValue::Integer16(i) => Value::Int(i64::from(*i)),
        PropertyValue::Integer32(i) => Value::Int(i64::from(*i)),
        PropertyValue::Integer64(i) => Value::Int(*i),
        PropertyValue::Floating32(f) => Value::Float(f64::from(*f)),
        PropertyValue::Floating64(f) => Value::Float(*f),
        PropertyValue::Boolean(b) => Value::Bool(*b),
        PropertyValue::Time(t) => Value::Time(mapi::filetime(u64::try_from(*t).ok()?)?),
        PropertyValue::Unicode(u) => Value::Str(
            String::from_utf16_lossy(u.buffer())
                .trim_end_matches('\0')
                .to_string(),
        ),
        PropertyValue::String8(s) => Value::Str(mapi::decode_8bit(s.buffer(), code_page)),
        PropertyValue::Binary(b) => Value::Bin(b.buffer().to_vec()),
        _ => return None,
    })
}

fn guid_bytes(g: &GuidValue) -> Guid {
    let mut b = [0u8; 16];
    b[..4].copy_from_slice(&g.data1().to_le_bytes());
    b[4..6].copy_from_slice(&g.data2().to_le_bytes());
    b[6..8].copy_from_slice(&g.data3().to_le_bytes());
    b[8..].copy_from_slice(g.data4());
    b
}

/// Property id -> (set, dispid) for the store's numeric named properties.
fn name_map(store: &dyn Store) -> HashMap<u16, (Guid, u32)> {
    let Ok(map) = store.named_property_map() else {
        return HashMap::new();
    };
    let props = map.properties();
    let Ok(entries) = props.stream_entry() else {
        return HashMap::new();
    };
    entries
        .into_iter()
        .filter_map(|e| {
            let NamedPropertyId::Number(dispid) = e.id() else {
                return None;
            };
            let guid = match e.guid() {
                NamedPropertyGuid::None => return None,
                NamedPropertyGuid::Mapi => PS_MAPI,
                NamedPropertyGuid::PublicStrings => PS_PUBLIC_STRINGS,
                // `lookup_guid` multiplies what it gets by 16, so the index
                // goes in as the guid of that number, as the crate's own
                // `read_named_props` example does it.
                NamedPropertyGuid::GuidIndex(i) => props
                    .lookup_guid(NamedPropertyGuid::try_from(i).ok()?)
                    .ok()?,
            };
            Some((e.prop_id(), (guid_bytes(&guid), dispid)))
        })
        .collect()
}

/// Every row of a table as id -> value.
fn table_rows(t: &dyn TableContext, code_page: Option<i64>) -> Vec<(u32, HashMap<u16, Value>)> {
    let ctx = t.context();
    t.rows_matrix()
        .filter_map(|row| {
            let cols = row.columns(ctx).ok()?;
            let mut m = HashMap::new();
            for (col, v) in ctx.columns().iter().zip(cols) {
                let Some(v) = v else {
                    continue;
                };
                if let Some(x) = t
                    .read_column(&v, col.prop_type())
                    .ok()
                    .and_then(|pv| value(&pv, code_page))
                {
                    m.insert(col.prop_id(), x);
                }
            }
            Some((u32::from(row.id()), m))
        })
        .collect()
}

fn str_of(m: &HashMap<u16, Value>, id: u16) -> String {
    match m.get(&id) {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Every folder below the IPM subtree with its path (`Inbox/Projects`),
/// parents before children, siblings by name.
fn folders(store: &Rc<dyn Store>) -> Result<Vec<(String, Rc<dyn Folder>)>> {
    let root = store.open_folder(&store.properties().ipm_sub_tree_entry_id()?)?;
    let mut out = Vec::new();
    let mut stack = vec![(String::new(), root)];
    while let Some((path, folder)) = stack.pop() {
        if let Some(h) = folder.hierarchy_table() {
            let mut children = Vec::new();
            for row in h.rows_matrix() {
                let id = store
                    .properties()
                    .make_entry_id(NodeId::from(u32::from(row.id())))?;
                let child = store.open_folder(&id)?;
                let name = child.properties().display_name().unwrap_or_default();
                let child_path = if path.is_empty() {
                    name
                } else {
                    format!("{path}/{name}")
                };
                children.push((child_path, child));
            }
            children.sort_by(|a, b| a.0.cmp(&b.0));
            stack.extend(children.into_iter().rev());
        }
        if !path.is_empty() {
            out.push((path, folder));
        }
    }
    Ok(out)
}

/// The items of one folder: (entry id, kind by message class, has
/// attachments), read from its contents table without opening them.
fn items(store: &Rc<dyn Store>, folder: &dyn Folder) -> Vec<(EntryId, Kind, bool)> {
    let Some(ct) = folder.contents_table() else {
        return Vec::new();
    };
    table_rows(ct.as_ref(), None)
        .into_iter()
        .filter_map(|(row, cols)| {
            let id = store.properties().make_entry_id(NodeId::from(row)).ok()?;
            let kind = mapi::kind_of_class(&str_of(&cols, tag::MESSAGE_CLASS));
            let attached = matches!(cols.get(&HAS_ATTACH), Some(Value::Bool(true)))
                || matches!(cols.get(&MESSAGE_FLAGS), Some(Value::Int(f)) if f & 0x10 != 0);
            Some((id, kind, attached))
        })
        .collect()
}

/// A message's properties as one bag, named ones resolved.
fn props_of(msg: &dyn PstMessage, names: &HashMap<u16, (Guid, u32)>) -> Props {
    let raw = msg.properties();
    let code_page = [tag::INTERNET_CPID, tag::MESSAGE_CODEPAGE]
        .iter()
        .find_map(|id| match raw.get(*id) {
            Some(PropertyValue::Integer32(i)) => Some(i64::from(*i)),
            _ => None,
        });
    let mut p = Props::default();
    for (id, v) in raw.iter() {
        let Some(v) = value(v, code_page) else {
            continue;
        };
        if *id >= 0x8000 {
            if let Some(key) = names.get(id) {
                p.named.insert(*key, v);
            }
        } else {
            p.tags.insert(*id, v);
        }
    }
    if let Some(rt) = msg.recipient_table() {
        for (_, r) in table_rows(rt.as_ref(), code_page) {
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
            p.recipients
                .push((kind, mapi::person(&str_of(&r, tag::DISPLAY_NAME), &address)));
        }
    }
    p
}

/// A message's attachments, from its attachment table; with `bytes`, each
/// is opened for its contents.
fn attachments_of(pst: &Pst, id: &EntryId, msg: &dyn PstMessage, bytes: bool) -> Vec<Attachment> {
    let Some(at) = msg.attachment_table() else {
        return Vec::new();
    };
    let rows = table_rows(at.as_ref(), None);
    let mut out: Vec<Attachment> = rows
        .iter()
        .map(|(_, a)| Attachment {
            mail_id: 0,
            file_name: [
                tag::ATTACH_LONG_FILENAME,
                tag::ATTACH_FILENAME,
                tag::DISPLAY_NAME,
            ]
            .iter()
            .map(|t| str_of(a, *t))
            .find(|s| !s.is_empty())
            .unwrap_or_default(),
            size: match a.get(&ATTACH_SIZE) {
                Some(Value::Int(n)) => *n,
                _ => 0,
            },
            mime: str_of(a, tag::ATTACH_MIME_TAG),
            content: None,
        })
        .collect();
    if bytes {
        let nodes: Vec<NodeId> = rows.iter().map(|(n, _)| NodeId::from(*n)).collect();
        if let Ok(opened) = pst.attachments(id, &nodes) {
            for (slot, a) in out.iter_mut().zip(opened) {
                let p = a.properties();
                if slot.mime.is_empty()
                    && let Some(PropertyValue::Unicode(m)) = p.get(tag::ATTACH_MIME_TAG)
                {
                    slot.mime = String::from_utf16_lossy(m.buffer())
                        .trim_end_matches('\0')
                        .to_string();
                }
                // An attached Outlook item stays listed by name only.
                if let Some(AttachmentData::Binary(b)) = a.data() {
                    slot.size = b.buffer().len() as i64;
                    slot.content = Some(b.buffer().to_vec());
                }
            }
        }
    }
    out
}

/// The rows of one kind (Attachments go with their mails, so their
/// `mail_id`s match the Mail table's).
fn read_kind(path: &Path, kind: Kind) -> Result<MailBook> {
    let pst = Pst::open(path)?;
    let store = pst.store();
    let names = name_map(store.as_ref());
    let want = if kind == Kind::Attachments {
        Kind::Mail
    } else {
        kind
    };
    let cap = crate::formats::initial_load_rows();
    let mut book = MailBook {
        has_folders: true,
        ..MailBook::default()
    };
    let mut taken = 0;
    'walk: for (folder_path, folder) in folders(&store)? {
        for (id, item_kind, _) in items(&store, folder.as_ref()) {
            if item_kind != want {
                continue;
            }
            if taken >= cap {
                break 'walk;
            }
            taken += 1;
            let Ok(msg) = store.open_message(&id, None) else {
                continue;
            };
            let props = props_of(msg.as_ref(), &names);
            let atts = if want == Kind::Mail {
                attachments_of(&pst, &id, msg.as_ref(), kind == Kind::Attachments)
            } else {
                Vec::new()
            };
            props.push_into(&mut book, &folder_path, atts);
        }
    }
    Ok(book)
}

/// The kinds the store holds and how many items each has, from the
/// contents tables alone.
fn list(path: &Path) -> Result<Vec<TableInfo>> {
    let pst = Pst::open(path)?;
    let store = pst.store();
    let mut counts: HashMap<Kind, usize> = HashMap::new();
    let mut any_attachment = false;
    for (_, folder) in folders(&store)? {
        for (_, kind, attached) in items(&store, folder.as_ref()) {
            *counts.entry(kind).or_default() += 1;
            any_attachment |= attached && kind == Kind::Mail;
        }
    }
    Ok(Kind::ALL
        .into_iter()
        .filter_map(|k| {
            let row_count = match k {
                // How many attachments there are needs every mail opened.
                Kind::Attachments => any_attachment.then_some(None)?,
                _ => Some(*counts.get(&k).filter(|n| **n > 0)?),
            };
            Some(TableInfo {
                name: k.name().to_string(),
                schema: None,
                columns: MailBook::columns_of(k, true),
                row_count,
            })
        })
        .collect())
}

impl FormatReader for PstReader {
    fn name(&self) -> &str {
        "Outlook data file"
    }

    fn extensions(&self) -> &[&str] {
        &["pst"]
    }

    /// The first table the store holds (mail, usually).
    fn read_file(&self, path: &Path) -> Result<DataTable> {
        match list(path)?.first() {
            Some(t) => self.read_table(path, &t.name),
            None => Ok(with_format(MailBook::default().single(), self.name())),
        }
    }

    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        Ok(Some(list(path)?))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        let kind = Kind::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(table))
            .ok_or_else(|| anyhow::anyhow!("no table \"{table}\" in this file"))?;
        Ok(with_format(
            read_kind(path, kind)?.table(table)?,
            self.name(),
        ))
    }
}

#[cfg(test)]
#[path = "pst_reader_tests.rs"]
mod tests;
