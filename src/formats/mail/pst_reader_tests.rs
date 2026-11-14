//! Stores written with `outlook-pst-rw` (the `pst_writer` dev-dependency),
//! which writes mail only; contacts, appointments and tasks share the MAPI
//! mapping `mapi_tests.rs` covers.

use pst_writer::{
    UnicodePstAttachment, UnicodePstFile as Writer, UnicodePstMessage, UnicodePstRecipient,
    UnicodePstRecipientType,
};

use super::*;

/// 2024-11-05 09:00:00 UTC as a FILETIME.
const NOV_5: i64 = (1_730_797_200 + 11_644_473_600) * 10_000_000;

const RECIPIENTS: [UnicodePstRecipient<'static>; 2] = [
    UnicodePstRecipient {
        name: "Max Mustermann",
        email: "max@example.org",
        recipient_type: UnicodePstRecipientType::To,
    },
    UnicodePstRecipient {
        name: "Sam",
        email: "sam@example.org",
        recipient_type: UnicodePstRecipientType::Cc,
    },
];

fn mail(subject: &'static str, body: &'static str) -> UnicodePstMessage<'static> {
    UnicodePstMessage {
        subject,
        sender_name: "Jane Doe",
        sender_email: "jane@example.org",
        recipients: &RECIPIENTS,
        body,
        html_body: None,
        message_id: "<q3@example.org>",
        delivery_time: NOV_5,
    }
}

/// Inbox with one mail and `a.txt` attached, Projects with a second mail.
pub(crate) fn write_store(path: &Path) {
    let att = UnicodePstAttachment {
        filename: "a.txt",
        mime_type: "text/plain",
        content_id: None,
        data: b"hello",
    };
    drop(
        Writer::create_with_attachments(path, &mail("Quarterly figures", "Hello Max"), &[att])
            .unwrap(),
    );
    drop(Writer::append_in_folder(path, &["Projects"], &mail("Kick-off", "See you")).unwrap());
}

fn cell(t: &DataTable, row: usize, col: &str) -> String {
    let c = t.columns.iter().position(|c| c.name == col).expect(col);
    t.get(row, c).map(ToString::to_string).unwrap_or_default()
}

#[test]
fn the_picker_lists_mail_and_attachments_without_opening_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("box.pst");
    write_store(&path);
    let tables = PstReader.list_tables(&path).unwrap().unwrap();
    let listed: Vec<(&str, Option<usize>)> = tables
        .iter()
        .map(|t| (t.name.as_str(), t.row_count))
        .collect();
    assert_eq!(listed, [("Mail", Some(2)), ("Attachments", None)]);
    assert_eq!(tables[0].columns[1].name, "folder");
}

#[test]
fn mail_rows_carry_their_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("box.pst");
    write_store(&path);
    let t = PstReader.read_table(&path, "Mail").unwrap();
    assert_eq!(t.row_count(), 2);
    assert_eq!(cell(&t, 0, "folder"), "Inbox");
    assert_eq!(cell(&t, 0, "subject"), "Quarterly figures");
    assert_eq!(cell(&t, 0, "from"), "Jane Doe <jane@example.org>");
    assert_eq!(cell(&t, 0, "to"), "Max Mustermann <max@example.org>");
    assert_eq!(cell(&t, 0, "cc"), "Sam <sam@example.org>");
    assert_eq!(cell(&t, 0, "received"), "2024-11-05 09:00:00");
    assert_eq!(cell(&t, 0, "attachments"), "1");
    assert!(cell(&t, 0, "body").contains("Hello Max"));
    assert_eq!(cell(&t, 1, "folder"), "Projects");
    assert_eq!(cell(&t, 1, "subject"), "Kick-off");
    assert_eq!(t.format_name.as_deref(), Some("Outlook data file"));
}

#[test]
fn attachments_carry_their_bytes_and_mail_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("box.pst");
    write_store(&path);
    let t = PstReader.read_table(&path, "Attachments").unwrap();
    assert_eq!(t.row_count(), 1);
    assert_eq!(cell(&t, 0, "mail_id"), "1");
    assert_eq!(cell(&t, 0, "file_name"), "a.txt");
    let content = t.columns.iter().position(|c| c.name == "content").unwrap();
    assert_eq!(
        t.get(0, content),
        Some(&crate::data::CellValue::Binary(b"hello".to_vec()))
    );
}

#[test]
fn a_file_that_is_not_a_store_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake.pst");
    std::fs::write(&path, b"not a pst").unwrap();
    assert!(PstReader.list_tables(&path).is_err());
}

/// Regenerates `samples/mail/sample.pst`:
/// `cargo test --lib write_pst_sample -- --ignored`.
#[test]
#[ignore]
fn write_pst_sample() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/mail/sample.pst");
    let _ = std::fs::remove_file(&path);
    write_store(&path);
}
