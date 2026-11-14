//! Builds an `.olm` the way Outlook for Mac lays one out, then reads it.

use std::io::Write;

use super::*;

const MESSAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<emails><email>
<OPFMessageCopySubject>Quarterly figures</OPFMessageCopySubject>
<OPFMessageCopySentTime>2024-11-05T09:00:00</OPFMessageCopySentTime>
<OPFMessageCopyReceivedTime>2024-11-05T09:01:30Z</OPFMessageCopyReceivedTime>
<OPFMessageCopyFromAddresses><emailAddress OPFContactEmailAddressName="Jane Doe" OPFContactEmailAddressAddress="jane@example.org"/></OPFMessageCopyFromAddresses>
<OPFMessageCopyToAddresses><emailAddress OPFContactEmailAddressName="Max" OPFContactEmailAddressAddress="max@example.org"/><emailAddress OPFContactEmailAddressAddress="sam@example.org"/></OPFMessageCopyToAddresses>
<OPFMessageCopyHTMLBody>&lt;p&gt;Hello &lt;b&gt;Max&lt;/b&gt;&lt;/p&gt;</OPFMessageCopyHTMLBody>
<OPFMessageCopyMessageID>&lt;q3@example.org&gt;</OPFMessageCopyMessageID>
<OPFMessageCopyAttachmentList><messageAttachment OPFAttachmentName="a.txt" OPFAttachmentContentType="text/plain" OPFAttachmentURL="Accounts/me/com.microsoft.__Attachments/a.txt"/></OPFMessageCopyAttachmentList>
<OPFMessageCopySomethingNew>ignored</OPFMessageCopySomethingNew>
</email></emails>"#;

const CALENDAR: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<appointments><appointment>
<OPFCalendarEventCopySummary>Planning</OPFCalendarEventCopySummary>
<OPFCalendarEventCopyStartTime>2024-11-06T10:00:00</OPFCalendarEventCopyStartTime>
<OPFCalendarEventCopyEndTime>2024-11-06T11:00:00</OPFCalendarEventCopyEndTime>
<OPFCalendarEventCopyLocation>Room 4</OPFCalendarEventCopyLocation>
<OPFCalendarEventCopyIsAllDayEvent>0</OPFCalendarEventCopyIsAllDayEvent>
</appointment></appointments>"#;

const CONTACTS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<contacts><contact>
<OPFContactCopyDisplayName>Jane Doe</OPFContactCopyDisplayName>
<OPFContactCopyFirstName>Jane</OPFContactCopyFirstName>
<OPFContactCopyLastName>Doe</OPFContactCopyLastName>
<OPFContactCopyBusinessCompany>Example Ltd</OPFContactCopyBusinessCompany>
<OPFContactCopyDefaultEmailAddress>jane@example.org</OPFContactCopyDefaultEmailAddress>
<OPFContactCopyBusinessPhone>+44 20 7946 0000</OPFContactCopyBusinessPhone>
</contact></contacts>"#;

/// One mail in Inbox/Projects with `a.txt`, one appointment, one contact.
pub(crate) fn write_olm(path: &Path) {
    let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        (
            "Accounts/me/com.microsoft.__Messages/Inbox/Projects/message_00001.xml",
            MESSAGE.as_bytes(),
        ),
        (
            "Accounts/me/com.microsoft.__Attachments/a.txt",
            b"hello".as_slice(),
        ),
        (
            "Accounts/me/Local/com.microsoft.__Calendar/Calendar.xml",
            CALENDAR.as_bytes(),
        ),
        (
            "Accounts/me/Local/com.microsoft.__Contacts/Contacts.xml",
            CONTACTS.as_bytes(),
        ),
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(body).unwrap();
    }
    zip.finish().unwrap();
}

fn cell(t: &DataTable, row: usize, col: &str) -> String {
    let c = t.columns.iter().position(|c| c.name == col).expect(col);
    t.get(row, c).map(ToString::to_string).unwrap_or_default()
}

fn export() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.olm");
    write_olm(&path);
    (dir, path)
}

#[test]
fn the_export_lists_every_kind_it_holds() {
    let (_dir, path) = export();
    let names: Vec<String> = OlmReader
        .list_tables(&path)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Mail", "Calendar", "Contacts", "Attachments"]);
}

#[test]
fn a_message_reads_its_fields_and_folder() {
    let (_dir, path) = export();
    let t = OlmReader.read_table(&path, "Mail").unwrap();
    assert_eq!(cell(&t, 0, "folder"), "Inbox/Projects");
    assert_eq!(cell(&t, 0, "subject"), "Quarterly figures");
    assert_eq!(cell(&t, 0, "sent"), "2024-11-05 09:00:00");
    assert_eq!(cell(&t, 0, "received"), "2024-11-05 09:01:30");
    assert_eq!(cell(&t, 0, "from"), "Jane Doe <jane@example.org>");
    assert_eq!(cell(&t, 0, "to"), "Max <max@example.org>; sam@example.org");
    assert_eq!(cell(&t, 0, "message_id"), "q3@example.org");
    // The HTML arrives escaped (`&lt;p&gt;`) and is turned into text.
    assert_eq!(cell(&t, 0, "body"), "Hello Max");
    assert_eq!(t.format_name.as_deref(), Some("Outlook for Mac export"));
}

#[test]
fn attachments_come_from_the_zip() {
    let (_dir, path) = export();
    let t = OlmReader.read_table(&path, "Attachments").unwrap();
    assert_eq!(cell(&t, 0, "mail_id"), "1");
    assert_eq!(cell(&t, 0, "file_name"), "a.txt");
    assert_eq!(cell(&t, 0, "size"), "5");
    let content = t.columns.iter().position(|c| c.name == "content").unwrap();
    assert_eq!(
        t.get(0, content),
        Some(&crate::data::CellValue::Binary(b"hello".to_vec()))
    );
}

#[test]
fn calendar_and_contacts_read_too() {
    let (_dir, path) = export();
    let c = OlmReader.read_table(&path, "Calendar").unwrap();
    assert_eq!(cell(&c, 0, "title"), "Planning");
    assert_eq!(cell(&c, 0, "location"), "Room 4");
    assert_eq!(cell(&c, 0, "folder"), "Calendar");
    assert_eq!(cell(&c, 0, "all_day"), "false");
    let p = OlmReader.read_table(&path, "Contacts").unwrap();
    assert_eq!(cell(&p, 0, "name"), "Jane Doe");
    assert_eq!(cell(&p, 0, "company"), "Example Ltd");
    assert_eq!(cell(&p, 0, "phones"), "+44 20 7946 0000 (work)");
}

/// Regenerates `samples/mail/sample.olm`:
/// `cargo test --lib write_olm_sample -- --ignored`.
#[test]
#[ignore]
fn write_olm_sample() {
    write_olm(&Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/mail/sample.olm"));
}
