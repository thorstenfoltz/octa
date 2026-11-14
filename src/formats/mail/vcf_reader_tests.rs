use super::*;

fn contacts() -> DataTable {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mail/contacts.vcf");
    VcfReader.read_file(&path).unwrap()
}

fn cell(t: &DataTable, row: usize, col: &str) -> String {
    let c = t.columns.iter().position(|c| c.name == col).expect(col);
    t.get(row, c).map(ToString::to_string).unwrap_or_default()
}

#[test]
fn a_vcard_3_card_fills_every_column() {
    let t = contacts();
    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "name",
            "given_name",
            "family_name",
            "company",
            "job_title",
            "emails",
            "phones",
            "address",
            "birthday",
            "note"
        ]
    );
    assert_eq!(t.row_count(), 2);
    assert_eq!(cell(&t, 0, "name"), "Jane Doe");
    assert_eq!(cell(&t, 0, "given_name"), "Jane");
    assert_eq!(cell(&t, 0, "family_name"), "Doe");
    assert_eq!(cell(&t, 0, "company"), "Example Ltd");
    assert_eq!(
        cell(&t, 0, "emails"),
        "jane@example.org; jane.doe@example.net"
    );
    assert_eq!(cell(&t, 0, "phones"), "+44 20 7946 0000 (work, voice)");
    assert_eq!(
        cell(&t, 0, "address"),
        "1 Example Street, London, EC1A 1AA, United Kingdom"
    );
    assert_eq!(cell(&t, 0, "birthday"), "1980-04-02");
    assert_eq!(cell(&t, 0, "note"), "Met at the trade fair, 2023.");
}

#[test]
fn a_vcard_21_card_decodes_quoted_printable() {
    let t = contacts();
    assert_eq!(cell(&t, 1, "name"), "Jürgen Müller");
    assert_eq!(cell(&t, 1, "family_name"), "Müller");
    assert_eq!(cell(&t, 1, "given_name"), "Jürgen");
    assert_eq!(cell(&t, 1, "phones"), "+49 30 123456 (work, voice)");
    assert_eq!(cell(&t, 1, "note"), "Grüße aus Berlin");
    assert_eq!(cell(&t, 1, "birthday"), "1975-09-30");
}
