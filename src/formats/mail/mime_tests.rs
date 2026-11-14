use super::*;
use crate::formats::FormatReader;
use crate::formats::mail::eml_reader::EmlReader;
use crate::formats::mail::mbox_reader::MboxReader;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mail")
        .join(name)
}

fn cell(t: &crate::data::DataTable, row: usize, col: &str) -> String {
    let c = t.columns.iter().position(|c| c.name == col).expect(col);
    t.get(row, c).map(ToString::to_string).unwrap_or_default()
}

#[test]
fn a_plain_mail_fills_the_mail_row() {
    let t = EmlReader.read_file(&fixture("plain.eml")).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "mail_id",
            "sent",
            "received",
            "from",
            "to",
            "cc",
            "subject",
            "attachments",
            "body",
            "message_id"
        ]
    );
    assert_eq!(cell(&t, 0, "mail_id"), "1");
    assert_eq!(cell(&t, 0, "from"), "Jane Doe <jane@example.org>");
    assert_eq!(
        cell(&t, 0, "to"),
        "Max Mustermann <max@example.org>; sam@example.org"
    );
    assert_eq!(cell(&t, 0, "cc"), "Jürgen <juergen@example.org>");
    assert_eq!(cell(&t, 0, "subject"), "Grüße and the invoice");
    assert_eq!(cell(&t, 0, "sent"), "2024-11-05 09:00:00");
    assert_eq!(cell(&t, 0, "attachments"), "1");
    assert!(
        cell(&t, 0, "body").starts_with("Hello Max,"),
        "{}",
        cell(&t, 0, "body")
    );
    assert_eq!(cell(&t, 0, "message_id"), "abc123@example.org");
    assert_eq!(t.format_name.as_deref(), Some("Email"));
}

#[test]
fn attachments_carry_their_bytes_and_mail_id() {
    let path = fixture("plain.eml");
    let names: Vec<String> = EmlReader
        .list_tables(&path)
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Mail", "Attachments"]);
    let t = EmlReader.read_table(&path, "Attachments").unwrap();
    assert_eq!(cell(&t, 0, "mail_id"), "1");
    assert_eq!(cell(&t, 0, "file_name"), "invoice.csv");
    assert_eq!(cell(&t, 0, "type"), "text/csv");
    assert_eq!(cell(&t, 0, "size"), "15");
    let content = t.columns.iter().position(|c| c.name == "content").unwrap();
    assert_eq!(
        t.get(0, content),
        Some(&crate::data::CellValue::Binary(
            b"id,amount\n1,10\n".to_vec()
        ))
    );
}

#[test]
fn an_html_only_mail_becomes_text() {
    let t = EmlReader.read_file(&fixture("html_only.eml")).unwrap();
    let body = cell(&t, 0, "body");
    assert!(body.contains("Your order"), "{body}");
    assert!(body.contains("4711"), "{body}");
    assert!(!body.contains('<'), "{body}");
}

#[test]
fn an_mbox_gives_a_row_per_message() {
    let t = MboxReader.read_file(&fixture("two.mbox")).unwrap();
    assert_eq!(t.row_count(), 2);
    assert_eq!(cell(&t, 0, "subject"), "First");
    assert_eq!(cell(&t, 1, "subject"), "Re: First");
    assert_eq!(cell(&t, 1, "mail_id"), "2");
    assert_eq!(cell(&t, 1, "to"), "Jane Doe <jane@example.org>");
}

#[test]
fn a_message_that_is_not_mail_adds_nothing() {
    let mut book = MailBook::default();
    push_message(&mut book, b"", "", true);
    assert!(book.mail.is_empty());
}
