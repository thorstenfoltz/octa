//! Builds `.msg` files with `cfb` the way Outlook lays them out (MS-OXMSG),
//! then reads them back.

use std::io::Write;

use super::*;
use crate::formats::mail::mapi::PSETID_APPOINTMENT;

/// A property value as the writer stores it.
pub(crate) enum P {
    Utf16(String),
    Eight(Vec<u8>),
    Bytes(Vec<u8>),
    I32(i32),
    Time(u64),
}

fn utf16(s: &str) -> P {
    P::Utf16(s.to_string())
}

/// Write one property bag: the property stream after a `header` of zero
/// bytes, and a `__substg1.0_` stream per variable-size value.
pub(crate) fn write_bag(cf: &mut CompoundFile<File>, dir: &str, header: usize, props: &[(u16, P)]) {
    let mut table = vec![0u8; header];
    for (id, p) in props {
        let (kind, inline, sub): (u16, [u8; 8], Option<Vec<u8>>) = match p {
            P::Utf16(s) => {
                let mut b: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
                b.extend([0, 0]);
                (0x001F, len8(b.len()), Some(b))
            }
            P::Eight(b) => {
                let mut b = b.clone();
                b.push(0);
                (0x001E, len8(b.len()), Some(b))
            }
            P::Bytes(b) => (0x0102, len8(b.len()), Some(b.clone())),
            P::I32(i) => {
                let mut v = [0u8; 8];
                v[..4].copy_from_slice(&i.to_le_bytes());
                (0x0003, v, None)
            }
            P::Time(ft) => (0x0040, ft.to_le_bytes(), None),
        };
        let t = (u32::from(*id) << 16) | u32::from(kind);
        table.extend(t.to_le_bytes());
        table.extend(6u32.to_le_bytes());
        table.extend(inline);
        if let Some(b) = sub {
            cf.create_stream(format!("{dir}/__substg1.0_{t:08X}"))
                .unwrap()
                .write_all(&b)
                .unwrap();
        }
    }
    cf.create_stream(format!("{dir}/__properties_version1.0"))
        .unwrap()
        .write_all(&table)
        .unwrap();
}

fn len8(n: usize) -> [u8; 8] {
    let mut v = [0u8; 8];
    v[..4].copy_from_slice(&(n as u32).to_le_bytes());
    v
}

/// One named property, numeric, in a set of its own: id 0x8000.
pub(crate) fn write_name_map(cf: &mut CompoundFile<File>, set: &[u8; 16], dispid: u32) {
    cf.create_storage("/__nameid_version1.0").unwrap();
    let base = "/__nameid_version1.0";
    cf.create_stream(format!("{base}/__substg1.0_00020102"))
        .unwrap()
        .write_all(set)
        .unwrap();
    let mut entry = dispid.to_le_bytes().to_vec();
    // Numeric (bit 0 clear), GUID index 3 (the first of the GUID stream),
    // property index 0.
    entry.extend((3u32 << 1).to_le_bytes());
    cf.create_stream(format!("{base}/__substg1.0_00030102"))
        .unwrap()
        .write_all(&entry)
        .unwrap();
}

/// 2024-11-05 09:00:00 UTC as a FILETIME.
pub(crate) const NOV_5: u64 = (1_730_797_200 + 11_644_473_600) * 10_000_000;

/// A mail from Jane to Max with `a.txt` attached.
pub(crate) fn write_mail(path: &Path) {
    let mut cf = cfb::create(path).unwrap();
    write_bag(
        &mut cf,
        "",
        32,
        &[
            (tag::MESSAGE_CLASS, utf16("IPM.Note")),
            (tag::SUBJECT, utf16("Quarterly figures")),
            (tag::BODY, utf16("Hello Max,\r\nthe figures are attached.")),
            (tag::SENDER_NAME, utf16("Jane Doe")),
            (tag::SENDER_SMTP, utf16("jane@example.org")),
            (tag::CLIENT_SUBMIT_TIME, P::Time(NOV_5)),
            (tag::INTERNET_MESSAGE_ID, utf16("<q3@example.org>")),
        ],
    );
    cf.create_storage("/__recip_version1.0_#00000000").unwrap();
    write_bag(
        &mut cf,
        "/__recip_version1.0_#00000000",
        8,
        &[
            (tag::DISPLAY_NAME, utf16("Max Mustermann")),
            (tag::SMTP_ADDRESS, utf16("max@example.org")),
            (tag::RECIPIENT_TYPE, P::I32(1)),
        ],
    );
    cf.create_storage("/__attach_version1.0_#00000000").unwrap();
    write_bag(
        &mut cf,
        "/__attach_version1.0_#00000000",
        8,
        &[
            (tag::ATTACH_LONG_FILENAME, utf16("a.txt")),
            (tag::ATTACH_DATA, P::Bytes(b"hello".to_vec())),
            (tag::ATTACH_MIME_TAG, utf16("text/plain")),
            (tag::ATTACH_METHOD, P::I32(1)),
        ],
    );
    cf.flush().unwrap();
}

/// An appointment whose location is a named property.
pub(crate) fn write_appointment(path: &Path) {
    let mut cf = cfb::create(path).unwrap();
    write_name_map(&mut cf, &PSETID_APPOINTMENT, 0x8208);
    write_bag(
        &mut cf,
        "",
        32,
        &[
            (tag::MESSAGE_CLASS, utf16("IPM.Appointment")),
            (tag::SUBJECT, utf16("Planning")),
            (0x8000, utf16("Room 4")),
            (tag::SENDER_NAME, utf16("Jane Doe")),
        ],
    );
    cf.flush().unwrap();
}

fn cell(t: &DataTable, row: usize, col: &str) -> String {
    let c = t.columns.iter().position(|c| c.name == col).expect(col);
    t.get(row, c).map(ToString::to_string).unwrap_or_default()
}

#[test]
fn a_mail_with_a_recipient_and_an_attachment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mail.msg");
    write_mail(&path);
    let names: Vec<String> = MsgReader
        .list_tables(&path)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Mail", "Attachments"]);
    let m = MsgReader.read_file(&path).unwrap();
    assert_eq!(cell(&m, 0, "from"), "Jane Doe <jane@example.org>");
    assert_eq!(cell(&m, 0, "to"), "Max Mustermann <max@example.org>");
    assert_eq!(cell(&m, 0, "subject"), "Quarterly figures");
    assert_eq!(cell(&m, 0, "sent"), "2024-11-05 09:00:00");
    assert_eq!(cell(&m, 0, "attachments"), "1");
    assert_eq!(cell(&m, 0, "message_id"), "q3@example.org");
    assert!(cell(&m, 0, "body").starts_with("Hello Max,"));
    assert_eq!(m.format_name.as_deref(), Some("Outlook item"));
    let a = MsgReader.read_table(&path, "Attachments").unwrap();
    assert_eq!(cell(&a, 0, "file_name"), "a.txt");
    assert_eq!(cell(&a, 0, "type"), "text/plain");
    let content = a.columns.iter().position(|c| c.name == "content").unwrap();
    assert_eq!(
        a.get(0, content),
        Some(&crate::data::CellValue::Binary(b"hello".to_vec()))
    );
}

#[test]
fn an_appointment_reads_its_named_location() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meeting.msg");
    write_appointment(&path);
    let names: Vec<String> = MsgReader
        .list_tables(&path)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Calendar"]);
    let t = MsgReader.read_file(&path).unwrap();
    assert_eq!(cell(&t, 0, "title"), "Planning");
    assert_eq!(cell(&t, 0, "location"), "Room 4");
    assert_eq!(cell(&t, 0, "organiser"), "Jane Doe");
}

#[test]
fn an_8_bit_subject_uses_the_code_page() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.msg");
    let mut cf = cfb::create(&path).unwrap();
    write_bag(
        &mut cf,
        "",
        32,
        &[
            (tag::MESSAGE_CLASS, P::Eight(b"IPM.Note".to_vec())),
            (tag::SUBJECT, P::Eight(b"Gr\xFC\xDFe".to_vec())),
            (tag::MESSAGE_CODEPAGE, P::I32(1252)),
        ],
    );
    cf.flush().unwrap();
    drop(cf);
    let t = MsgReader.read_file(&path).unwrap();
    assert_eq!(cell(&t, 0, "subject"), "Grüße");
}

#[test]
fn a_template_is_read_by_the_same_reader() {
    let reg = crate::formats::FormatRegistry::new();
    assert_eq!(
        reg.reader_for_path(Path::new("x.oft")).unwrap().name(),
        "Outlook item"
    );
}

/// Regenerates `samples/mail/sample.msg` and `sample.oft`:
/// `cargo test --lib write_mail_item_samples -- --ignored`.
#[test]
#[ignore]
fn write_mail_item_samples() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/mail");
    write_mail(&dir.join("sample.msg"));
    write_appointment(&dir.join("sample.oft"));
}
