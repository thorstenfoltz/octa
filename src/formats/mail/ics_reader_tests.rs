use super::*;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mail")
        .join(name)
}

fn cell(t: &DataTable, row: usize, col: &str) -> String {
    let c = t.columns.iter().position(|c| c.name == col).expect(col);
    t.get(row, c).map(ToString::to_string).unwrap_or_default()
}

#[test]
fn events_become_the_calendar_table() {
    let t = IcsReader.read_file(&fixture("calendar.ics")).unwrap();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "title",
            "start",
            "end",
            "all_day",
            "location",
            "organiser",
            "attendees",
            "recurrence",
            "description"
        ]
    );
    assert_eq!(t.row_count(), 2);
    assert_eq!(
        cell(&t, 0, "title"),
        "Quarterly planning with the whole team and a very long title that is folded"
    );
    assert_eq!(cell(&t, 0, "start"), "2024-11-05 09:00:00");
    assert_eq!(cell(&t, 0, "all_day"), "false");
    assert_eq!(cell(&t, 0, "location"), "Room 4, second floor");
    assert_eq!(cell(&t, 0, "organiser"), "Doe: Jane <jane@example.org>");
    assert_eq!(
        cell(&t, 0, "attendees"),
        "Max Mustermann <max@example.org>; sam@example.org"
    );
    assert_eq!(cell(&t, 0, "recurrence"), "FREQ=WEEKLY;BYDAY=TU");
    // The alarm's own DESCRIPTION is not the event's.
    assert_eq!(cell(&t, 0, "description"), "Agenda:\n1. Budget\n2. Hiring");
    assert_eq!(cell(&t, 1, "all_day"), "true");
    assert_eq!(cell(&t, 1, "start"), "2024-12-25 00:00:00");
    assert_eq!(t.format_name.as_deref(), Some("iCalendar"));
}

#[test]
fn to_dos_become_a_second_table() {
    let path = fixture("calendar.ics");
    let tables = IcsReader.list_tables(&path).unwrap().expect("two kinds");
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Calendar", "Tasks"]);
    let t = IcsReader.read_table(&path, "Tasks").unwrap();
    assert_eq!(cell(&t, 0, "subject"), "Send the invoices");
    assert_eq!(cell(&t, 0, "due"), "2024-11-30 17:00:00");
    assert_eq!(cell(&t, 0, "percent_done"), "40.0");
    assert_eq!(cell(&t, 0, "status"), "IN-PROCESS");
}

#[test]
fn an_old_vcalendar_reads_too() {
    let path = fixture("old.vcs");
    assert!(
        IcsReader.list_tables(&path).unwrap().is_none(),
        "events only"
    );
    let t = IcsReader.read_file(&path).unwrap();
    assert_eq!(cell(&t, 0, "title"), "Dentist");
    assert_eq!(cell(&t, 0, "description"), "Bring the X-räy");
}

#[test]
fn the_registry_finds_it() {
    let reg = crate::formats::FormatRegistry::new();
    assert_eq!(
        reg.reader_for_path(&fixture("old.vcs")).unwrap().name(),
        "iCalendar"
    );
}
