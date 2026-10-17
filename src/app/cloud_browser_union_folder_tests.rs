use super::data_objects;
use octa::cloud::ObjectEntry;
use std::collections::HashSet;

fn entry(key: &str, name: &str, is_prefix: bool) -> ObjectEntry {
    ObjectEntry {
        name: name.to_string(),
        key: key.to_string(),
        is_prefix,
        size: None,
        modified: None,
        etag: None,
        version: None,
    }
}

#[test]
fn keeps_only_supported_files_not_prefixes() {
    let entries = vec![
        entry("data/a.csv", "a.csv", false),
        entry("data/sub/", "sub", true), // a folder: excluded
        entry("data/notes.txt", "notes.txt", false), // unsupported ext
        entry("data/b.parquet", "b.parquet", false),
        entry("data/noext", "noext", false), // no extension
    ];
    let allowed: HashSet<String> = ["csv", "parquet"].into_iter().map(String::from).collect();
    let got = data_objects(&entries, &allowed);
    assert_eq!(
        got,
        vec![
            ("data/a.csv".to_string(), "a.csv".to_string()),
            ("data/b.parquet".to_string(), "b.parquet".to_string()),
        ]
    );
}
