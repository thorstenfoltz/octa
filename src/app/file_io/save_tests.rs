use super::{delimited_target, is_csv_path, save_target};
use crate::app::state::{CsvChoice, TabState};
use octa::formats::write_options::CsvOptions;
use std::path::Path;

#[test]
fn the_target_extension_decides_how_a_file_is_written() {
    assert_eq!(delimited_target(Path::new("a.CSV")), Some(b','));
    assert_eq!(delimited_target(Path::new("a.tsv")), Some(b'\t'));
    // A `;` CSV saved as Parquet is written as Parquet.
    assert_eq!(delimited_target(Path::new("a.parquet")), None);
    assert!(is_csv_path(Path::new("x.csv")));
    assert!(!is_csv_path(Path::new("x.csv.gz")));
}

#[test]
fn settings_tab_and_save_as_choice_merge_in_one_place() {
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    let settings = CsvOptions::default();
    assert_eq!(tab.delimited_options(&settings, None, b',').1, b',');

    // A European CSV is written back the way it came.
    tab.csv_delimiter = b';';
    tab.csv_decimal_comma = true;
    tab.csv_bom = true;
    let (csv, delimiter) = tab.delimited_options(&settings, None, b',');
    assert_eq!((delimiter, csv.decimal_comma, csv.bom), (b';', true, true));
    // Saved as `.tsv` it is tab separated, whatever it had.
    assert_eq!(tab.delimited_options(&settings, None, b'\t').1, b'\t');

    // The Save As choice wins over both.
    let choice = CsvChoice {
        delimiter: b'|',
        decimal_comma: false,
        bom: false,
    };
    let (csv, delimiter) = tab.delimited_options(&settings, Some(choice), b',');
    assert_eq!(
        (delimiter, csv.decimal_comma, csv.bom),
        (b'|', false, false)
    );

    // The Settings delimiter applies only while the tab's own is a comma.
    let semicolon = CsvOptions {
        delimiter: b';',
        ..Default::default()
    };
    tab.csv_delimiter = b',';
    assert_eq!(tab.delimited_options(&semicolon, None, b',').1, b';');
}

/// A backup or a kept version of a temp copy would vanish with the tab (and
/// for a protected zip would be plaintext): both are taken of the real file.
#[test]
fn backups_and_versions_are_of_the_real_file_not_a_temp_copy() {
    let mut tab = TabState::new(octa::data::SearchMode::Plain);
    let picked = Path::new("/data/picked.csv");
    assert_eq!(save_target(&tab, picked), picked);

    tab.compressed_origin = Some(crate::app::state::CompressedOrigin {
        original: "/data/a.csv.gz".into(),
        codec: octa::formats::compression::Codec::Gzip,
        temp: "/tmp/a.csv".into(),
    });
    assert_eq!(
        save_target(&tab, Path::new("/tmp/a.csv")),
        Path::new("/data/a.csv.gz")
    );
    // A Save As replaces the file it was pointed at.
    assert_eq!(save_target(&tab, picked), picked);

    tab.compressed_origin = None;
    let temp = std::sync::Arc::new(octa::formats::encrypt::ProtectedTempDir::new().unwrap());
    let temp_file = temp.path().join("report.csv");
    tab.protected_origin = Some(crate::app::state::ProtectedOrigin {
        zip: "/data/report.csv.zip".into(),
        entry: "report.csv".into(),
        passphrase: "pw".into(),
        temp_file: temp_file.clone(),
        _temp: temp,
    });
    assert_eq!(
        save_target(&tab, &temp_file),
        Path::new("/data/report.csv.zip")
    );
    assert_eq!(save_target(&tab, picked), picked);
}
