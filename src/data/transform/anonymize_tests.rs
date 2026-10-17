use super::*;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// One-column table from string literals (empty string -> empty `String`
/// cell, the literal `"<null>"` -> `Null`).
fn table(values: &[&str]) -> DataTable {
    let mut t = DataTable::empty();
    t.columns.push(ColumnInfo {
        name: "v".to_string(),
        data_type: "Utf8".to_string(),
    });
    t.rows = values
        .iter()
        .map(|v| {
            let cell = if *v == "<null>" {
                CellValue::Null
            } else {
                CellValue::String(v.to_string())
            };
            vec![cell]
        })
        .collect();
    t
}

fn run_one(t: &DataTable, strategy: AnonStrategy, salt: &str) -> Vec<CellValue> {
    let spec = AnonSpec {
        rules: vec![AnonRule {
            columns: vec![0],
            strategy,
            new_column: None,
        }],
        salt: salt.to_string(),
    };
    anonymize_table(t, &spec).remove(0).values
}

fn run_outputs(t: &DataTable, rules: Vec<AnonRule>, salt: &str) -> Vec<AnonOutput> {
    anonymize_table(
        t,
        &AnonSpec {
            rules,
            salt: salt.to_string(),
        },
    )
}

#[test]
fn hash_is_deterministic_and_truncated() {
    let t = table(&["alice", "alice", "bob"]);
    let out = run_one(
        &t,
        AnonStrategy::Hash {
            algo: HashAlgo::Sha256,
            length: Some(12),
        },
        "s",
    );
    assert_eq!(out[0], out[1]);
    assert_ne!(out[0], out[2]);
    if let CellValue::String(s) = &out[0] {
        assert_eq!(s.len(), 12);
    } else {
        panic!("expected String");
    }
}

#[test]
fn hash_salt_changes_output() {
    let t = table(&["alice"]);
    let a = run_one(
        &t,
        AnonStrategy::Hash {
            algo: HashAlgo::Sha256,
            length: Some(16),
        },
        "saltA",
    );
    let b = run_one(
        &t,
        AnonStrategy::Hash {
            algo: HashAlgo::Sha256,
            length: Some(16),
        },
        "saltB",
    );
    assert_ne!(a[0], b[0]);
}

#[test]
fn hash_blake3_differs_from_sha256() {
    let t = table(&["alice"]);
    let sha = run_one(
        &t,
        AnonStrategy::Hash {
            algo: HashAlgo::Sha256,
            length: Some(16),
        },
        "s",
    );
    let bl = run_one(
        &t,
        AnonStrategy::Hash {
            algo: HashAlgo::Blake3,
            length: Some(16),
        },
        "s",
    );
    assert_ne!(sha[0], bl[0]);
}

#[test]
fn redact_fixed_and_null() {
    let t = table(&["secret"]);
    let fixed = run_one(
        &t,
        AnonStrategy::Redact {
            token: RedactToken::Fixed("[X]".to_string()),
        },
        "",
    );
    assert_eq!(fixed[0], CellValue::String("[X]".to_string()));
    let nulled = run_one(
        &t,
        AnonStrategy::Redact {
            token: RedactToken::Null,
        },
        "",
    );
    assert_eq!(nulled[0], CellValue::Null);
}

#[test]
fn null_and_empty_pass_through() {
    let t = table(&["<null>", ""]);
    let out = run_one(
        &t,
        AnonStrategy::Hash {
            algo: HashAlgo::Sha256,
            length: Some(12),
        },
        "s",
    );
    assert_eq!(out[0], CellValue::Null);
    assert_eq!(out[1], CellValue::String(String::new()));
}

#[test]
fn out_of_range_column_skipped() {
    let t = table(&["alice"]);
    let spec = AnonSpec {
        rules: vec![AnonRule {
            columns: vec![9],
            strategy: AnonStrategy::Redact {
                token: RedactToken::Null,
            },
            new_column: None,
        }],
        salt: String::new(),
    };
    assert!(anonymize_table(&t, &spec).is_empty());
}

#[test]
fn partial_mask_keeps_last_n() {
    let t = table(&["5551234"]);
    let out = run_one(
        &t,
        AnonStrategy::PartialMask {
            keep: KeepEnd::Last,
            count: 4,
            mask_char: '*',
            mask_len: None,
        },
        "",
    );
    assert_eq!(out[0], CellValue::String("***1234".to_string()));
}

#[test]
fn partial_mask_keeps_first_n() {
    let t = table(&["abcdef"]);
    let out = run_one(
        &t,
        AnonStrategy::PartialMask {
            keep: KeepEnd::First,
            count: 2,
            mask_char: '#',
            mask_len: None,
        },
        "",
    );
    assert_eq!(out[0], CellValue::String("ab####".to_string()));
}

#[test]
fn partial_mask_count_ge_len_keeps_whole_value() {
    let t = table(&["ab"]);
    let out = run_one(
        &t,
        AnonStrategy::PartialMask {
            keep: KeepEnd::Last,
            count: 5,
            mask_char: '*',
            mask_len: None,
        },
        "",
    );
    assert_eq!(out[0], CellValue::String("ab".to_string()));
}

#[test]
fn partial_mask_fixed_len_hides_original_length() {
    let t = table(&["5551234", "12"]);
    let out = run_one(
        &t,
        AnonStrategy::PartialMask {
            keep: KeepEnd::Last,
            count: 2,
            mask_char: '*',
            mask_len: Some(5),
        },
        "",
    );
    // Both rows: 5 mask chars + 2 kept = same length regardless of input.
    assert_eq!(out[0], CellValue::String("*****34".to_string()));
    assert_eq!(out[1], CellValue::String("*****12".to_string()));
}

#[test]
fn partial_mask_counts_characters_not_bytes() {
    let t = table(&["aeiou\u{e9}"]); // "aeioué", é is 2 bytes
    let out = run_one(
        &t,
        AnonStrategy::PartialMask {
            keep: KeepEnd::Last,
            count: 1,
            mask_char: '*',
            mask_len: None,
        },
        "",
    );
    assert_eq!(out[0], CellValue::String("*****\u{e9}".to_string()));
}

#[test]
fn fake_is_consistent_for_duplicates() {
    let t = table(&["alice", "alice", "bob"]);
    let out = run_one(
        &t,
        AnonStrategy::Fake {
            kind: FakeKind::Name,
        },
        "s",
    );
    assert_eq!(out[0], out[1]);
    assert_ne!(out[0], out[2]);
}

#[test]
fn fake_email_looks_like_email() {
    let t = table(&["alice"]);
    let out = run_one(
        &t,
        AnonStrategy::Fake {
            kind: FakeKind::Email,
        },
        "s",
    );
    if let CellValue::String(s) = &out[0] {
        assert!(s.contains('@'), "email should contain @: {s}");
        assert!(s.contains('.'), "email should contain a dot: {s}");
    } else {
        panic!("expected String");
    }
}

#[test]
fn fake_uuid_has_uuid_shape() {
    let t = table(&["alice"]);
    let out = run_one(
        &t,
        AnonStrategy::Fake {
            kind: FakeKind::Uuid,
        },
        "s",
    );
    if let CellValue::String(s) = &out[0] {
        let parts: Vec<&str> = s.split('-').collect();
        assert_eq!(parts.len(), 5, "uuid has 5 dash-separated groups: {s}");
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(s.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    } else {
        panic!("expected String");
    }
}

#[test]
fn fake_phone_is_digits_and_separators() {
    let t = table(&["alice"]);
    let out = run_one(
        &t,
        AnonStrategy::Fake {
            kind: FakeKind::Phone,
        },
        "s",
    );
    if let CellValue::String(s) = &out[0] {
        assert!(s.chars().any(|c| c.is_ascii_digit()));
    } else {
        panic!("expected String");
    }
}

#[test]
fn fake_salt_changes_value() {
    let t = table(&["alice"]);
    let a = run_one(
        &t,
        AnonStrategy::Fake {
            kind: FakeKind::City,
        },
        "a",
    );
    let b = run_one(
        &t,
        AnonStrategy::Fake {
            kind: FakeKind::City,
        },
        "b",
    );
    assert_ne!(a[0], b[0]);
}

#[test]
fn rerun_with_same_salt_is_idempotent() {
    let t = table(&["alice", "bob"]);
    let strat = || AnonStrategy::Hash {
        algo: HashAlgo::Sha256,
        length: Some(12),
    };
    let first = run_one(&t, strat(), "s");
    let again = run_one(&t, strat(), "s");
    assert_eq!(first, again);
}

#[test]
fn multiple_rules_return_one_column_each() {
    let mut t = DataTable::empty();
    for name in ["email", "phone"] {
        t.columns.push(ColumnInfo {
            name: name.to_string(),
            data_type: "Utf8".to_string(),
        });
    }
    t.rows = vec![vec![
        CellValue::String("a@b.com".to_string()),
        CellValue::String("5551234".to_string()),
    ]];
    let spec = AnonSpec {
        rules: vec![
            AnonRule {
                columns: vec![0],
                strategy: AnonStrategy::Hash {
                    algo: HashAlgo::Sha256,
                    length: Some(8),
                },
                new_column: None,
            },
            AnonRule {
                columns: vec![1],
                strategy: AnonStrategy::PartialMask {
                    keep: KeepEnd::Last,
                    count: 4,
                    mask_char: '*',
                    mask_len: None,
                },
                new_column: None,
            },
        ],
        salt: String::new(),
    };
    let out = anonymize_table(&t, &spec);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].source, AnonSource::Column(0));
    assert_eq!(out[1].source, AnonSource::Column(1));
    assert_eq!(out[1].values[0], CellValue::String("***1234".to_string()));
}

#[test]
fn hash_full_by_default() {
    let t = table(&["alice"]);
    let out = run_outputs(
        &t,
        vec![AnonRule {
            columns: vec![0],
            strategy: AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: None,
            },
            new_column: None,
        }],
        "s",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].source, AnonSource::Column(0));
    if let CellValue::String(s) = &out[0].values[0] {
        assert_eq!(s.len(), 64);
    } else {
        panic!("expected String");
    }
}

#[test]
fn multi_column_hash_is_one_derived_column() {
    let mut t = DataTable::empty();
    for n in ["first", "last"] {
        t.columns.push(ColumnInfo {
            name: n.into(),
            data_type: "Utf8".into(),
        });
    }
    t.rows = vec![
        vec![
            CellValue::String("john".into()),
            CellValue::String("smith".into()),
        ],
        vec![
            CellValue::String("john".into()),
            CellValue::String("smith".into()),
        ],
        vec![
            CellValue::String("jane".into()),
            CellValue::String("smith".into()),
        ],
    ];
    let out = run_outputs(
        &t,
        vec![AnonRule {
            columns: vec![0, 1],
            strategy: AnonStrategy::Hash {
                algo: HashAlgo::Sha256,
                length: Some(16),
            },
            new_column: Some("person_id".into()),
        }],
        "s",
    );
    assert_eq!(out.len(), 1);
    match &out[0].source {
        AnonSource::Derived { name } => assert_eq!(name, "person_id"),
        _ => panic!("multi-column hash must be a derived new column"),
    }
    assert_eq!(out[0].values[0], out[0].values[1]);
    assert_ne!(out[0].values[0], out[0].values[2]);
}

#[test]
fn mask_with_two_columns_yields_two_column_outputs() {
    let mut t = DataTable::empty();
    for n in ["a", "b"] {
        t.columns.push(ColumnInfo {
            name: n.into(),
            data_type: "Utf8".into(),
        });
    }
    t.rows = vec![vec![
        CellValue::String("5551234".into()),
        CellValue::String("9998888".into()),
    ]];
    let out = run_outputs(
        &t,
        vec![AnonRule {
            columns: vec![0, 1],
            strategy: AnonStrategy::PartialMask {
                keep: KeepEnd::Last,
                count: 4,
                mask_char: '*',
                mask_len: None,
            },
            new_column: None,
        }],
        "",
    );
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].source, AnonSource::Column(0));
    assert_eq!(out[1].source, AnonSource::Column(1));
    assert_eq!(out[0].values[0], CellValue::String("***1234".into()));
    assert_eq!(out[1].values[0], CellValue::String("***8888".into()));
}
