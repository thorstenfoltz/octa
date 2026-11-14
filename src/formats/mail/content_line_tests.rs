use super::*;

#[test]
fn folded_lines_are_joined() {
    let lines = unfold("SUMMARY:Long\r\n  title\r\nLOCATION:Room 1\r\n");
    assert_eq!(lines, ["SUMMARY:Long title", "LOCATION:Room 1"]);
}

#[test]
fn params_and_quoted_colons() {
    let p = parse("ORGANIZER;CN=\"Doe: Jane\":mailto:jane@x.org").unwrap();
    assert_eq!(p.name, "ORGANIZER");
    assert_eq!(p.param("CN"), Some("Doe: Jane"));
    assert_eq!(p.value, "mailto:jane@x.org");
}

#[test]
fn vcard_21_quoted_printable_and_bare_types() {
    let lines = unfold(
        "NOTE;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Gr=C3=BC=\r\nße\r\nTEL;WORK;VOICE:+49 1\r\n",
    );
    let note = parse(&lines[0]).unwrap();
    assert_eq!(note.text(), "Grüße");
    let tel = parse(&lines[1]).unwrap();
    assert_eq!(tel.param("TYPE"), Some("WORK,VOICE"));
}

#[test]
fn escapes_are_undone() {
    assert_eq!(
        parse("DESCRIPTION:a\\, b\\nc\\;d").unwrap().text(),
        "a, b\nc;d"
    );
}

#[test]
fn ics_times() {
    assert_eq!(
        ics_time(&parse("DTSTART;VALUE=DATE:20241105").unwrap()),
        (Some("2024-11-05 00:00:00".into()), true)
    );
    assert_eq!(
        ics_time(&parse("DTSTART:20241105T090000Z").unwrap()),
        (Some("2024-11-05 09:00:00".into()), false)
    );
    assert_eq!(
        ics_time(&parse("DTSTART;TZID=Europe/Berlin:20241105T090000").unwrap()),
        (Some("2024-11-05 09:00:00".into()), false)
    );
}
