use super::*;

fn with_class(class: &str) -> Props {
    let mut p = Props::default();
    p.tags.insert(tag::MESSAGE_CLASS, Value::Str(class.into()));
    p
}

#[test]
fn the_guids_are_the_published_ones() {
    assert_eq!(
        guid_text(&PSETID_APPOINTMENT),
        "00062002-0000-0000-C000-000000000046"
    );
    assert_eq!(
        guid_text(&PSETID_TASK),
        "00062003-0000-0000-C000-000000000046"
    );
    assert_eq!(
        guid_text(&PSETID_ADDRESS),
        "00062004-0000-0000-C000-000000000046"
    );
}

#[test]
fn the_message_class_picks_the_table() {
    for (class, kind) in [
        ("IPM.Note", Kind::Mail),
        ("IPM.Note.SMIME", Kind::Mail),
        ("IPM.Schedule.Meeting.Request", Kind::Mail),
        ("IPM.Appointment", Kind::Calendar),
        ("ipm.contact", Kind::Contacts),
        ("IPM.DistList", Kind::Contacts),
        ("IPM.Task", Kind::Tasks),
        ("IPM.StickyNote", Kind::Notes),
        ("IPM.Something.Else", Kind::Mail),
        ("", Kind::Mail),
    ] {
        assert_eq!(with_class(class).kind(), kind, "{class}");
    }
}

#[test]
fn the_subject_marker_is_dropped() {
    assert_eq!(clean_subject("\u{1}\u{4}RE: Budget"), "RE: Budget");
    assert_eq!(clean_subject("Budget"), "Budget");
}

#[test]
fn filetime_counts_from_1601() {
    let ft = (1_730_797_200u64 + 11_644_473_600) * 10_000_000;
    assert_eq!(filetime(ft).as_deref(), Some("2024-11-05 09:00:00"));
    assert_eq!(filetime(0), None);
    // Outlook's "no date", 4501-01-01.
    assert_eq!(filetime(0x0CB3_4557_A3DD_4000), None);
}

#[test]
fn an_html_only_body_becomes_text() {
    let mut p = with_class("IPM.Note");
    p.tags.insert(
        tag::BODY_HTML,
        Value::Bin(b"<html><body><p>Hello <b>there</b></p></body></html>".to_vec()),
    );
    let mut book = MailBook::default();
    p.push_into(&mut book, "", Vec::new());
    let body = &book.mail[0].0.body;
    assert!(body.contains("Hello") && body.contains("there"), "{body}");
    assert!(!body.contains('<'), "{body}");
}

#[test]
fn eight_bit_text_uses_the_code_page() {
    assert_eq!(decode_8bit(b"Gr\xFC\xDFe\0", None), "Grüße");
    assert_eq!(decode_8bit(b"\xC4\xE9", Some(1253)), "Δι");
}

#[test]
fn a_task_reads_its_named_fields() {
    let mut p = with_class("IPM.Task");
    p.tags.insert(tag::SUBJECT, Value::Str("File taxes".into()));
    p.named
        .insert((PSETID_TASK, named::TASK_STATUS), Value::Int(1));
    p.named
        .insert((PSETID_TASK, named::PERCENT_COMPLETE), Value::Float(0.25));
    let mut book = MailBook::default();
    p.push_into(&mut book, "Tasks", Vec::new());
    let t = &book.tasks[0];
    assert_eq!(
        (t.subject.as_str(), t.status.as_str(), t.percent_done),
        ("File taxes", "in progress", Some(25.0))
    );
}
