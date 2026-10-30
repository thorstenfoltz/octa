//! Tests for `octa::data::validate_schema`. The most important
//! assertion is the *round-trip closure*: a JSON Schema produced by
//! `schema_export::json_schema::export` must parse back into the
//! exact same `Vec<ColumnInfo>` it was emitted from (modulo the
//! Timestamp unit/timezone tuple, which JSON Schema can't carry).

use octa::data::ColumnInfo;
use octa::data::schema_export::SchemaTarget;
use octa::data::validate_schema::{
    parse_json_schema, validate_against_json_schema, validate_against_schema,
};

fn cols(pairs: &[(&str, &str)]) -> Vec<ColumnInfo> {
    pairs
        .iter()
        .map(|(name, ty)| ColumnInfo {
            name: name.to_string(),
            data_type: ty.to_string(),
        })
        .collect()
}

#[test]
fn json_schema_round_trip_for_basic_types() {
    // Export then re-parse. Every column except Timestamp (which can't
    // carry unit/tz) must come back identical.
    let original = cols(&[
        ("id", "Int64"),
        ("name", "Utf8"),
        ("score", "Float64"),
        ("active", "Boolean"),
        ("born", "Date32"),
    ]);
    let schema_text = SchemaTarget::JsonSchema.export(&original, "people");
    let (round_tripped, unparsed) = parse_json_schema(&schema_text).unwrap();
    assert!(
        unparsed.is_empty(),
        "no types should be unrecognised, got: {unparsed:?}"
    );
    assert_eq!(round_tripped.len(), original.len());
    // The order should be preserved (serde_json preserves insertion
    // order with `preserve_order`; if not enabled, this test will
    // surface that fact so we can address it).
    for (a, b) in round_tripped.iter().zip(original.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.data_type, b.data_type, "column {} drift", a.name);
    }
}

#[test]
fn json_schema_round_trip_timestamp_normalised_to_microsecond_none() {
    // The JSON Schema exporter folds every Timestamp variant onto the
    // string/date-time pair, losing the unit + timezone. parse_json_schema
    // re-hydrates as `Timestamp(Microsecond, None)`.
    let original = cols(&[("ts", "Timestamp(Nanosecond, Some(\"UTC\"))")]);
    let schema_text = SchemaTarget::JsonSchema.export(&original, "t");
    let (round_tripped, unparsed) = parse_json_schema(&schema_text).unwrap();
    assert!(unparsed.is_empty());
    assert_eq!(round_tripped.len(), 1);
    assert_eq!(round_tripped[0].data_type, "Timestamp(Microsecond, None)");
}

#[test]
fn round_trip_matches_returns_true_for_self_validation() {
    // The end-to-end "I exported a schema, now I validate the same
    // file against it" workflow.
    let actual = cols(&[("id", "Int64"), ("name", "Utf8")]);
    let schema_text = SchemaTarget::JsonSchema.export(&actual, "t");
    let report = validate_against_json_schema(&actual, &schema_text).unwrap();
    assert!(report.matches, "self-validation must succeed");
    assert!(report.diff.identical);
    assert!(report.unparsed_types.is_empty());
}

#[test]
fn validation_flags_extra_actual_column() {
    let actual = cols(&[("id", "Int64"), ("legacy", "Boolean")]);
    let expected = cols(&[("id", "Int64")]);
    let schema_text = SchemaTarget::JsonSchema.export(&expected, "t");
    let report = validate_against_json_schema(&actual, &schema_text).unwrap();
    assert!(!report.matches);
    assert_eq!(report.diff.only_in_a.len(), 1);
    assert_eq!(report.diff.only_in_a[0].name, "legacy");
}

#[test]
fn validation_flags_missing_expected_column() {
    let actual = cols(&[("id", "Int64")]);
    let expected = cols(&[("id", "Int64"), ("region", "Utf8")]);
    let schema_text = SchemaTarget::JsonSchema.export(&expected, "t");
    let report = validate_against_json_schema(&actual, &schema_text).unwrap();
    assert!(!report.matches);
    assert_eq!(report.diff.only_in_b.len(), 1);
    assert_eq!(report.diff.only_in_b[0].name, "region");
}

#[test]
fn validation_flags_type_drift() {
    let actual = cols(&[("amount", "Utf8")]);
    let expected = cols(&[("amount", "Float64")]);
    let schema_text = SchemaTarget::JsonSchema.export(&expected, "t");
    let report = validate_against_json_schema(&actual, &schema_text).unwrap();
    assert!(!report.matches);
    assert_eq!(report.diff.type_mismatches.len(), 1);
    let m = &report.diff.type_mismatches[0];
    assert_eq!(m.name, "amount");
    assert_eq!(m.type_a, "Utf8");
    assert_eq!(m.type_b, "Float64");
}

#[test]
fn parser_rejects_non_json() {
    let err = parse_json_schema("not valid json").unwrap_err();
    assert!(err.to_string().contains("not valid JSON"));
}

#[test]
fn parser_rejects_missing_properties() {
    let err = parse_json_schema(r#"{"type": "object"}"#).unwrap_err();
    assert!(err.to_string().contains("properties"));
}

#[test]
fn unknown_type_falls_back_to_utf8_and_is_flagged() {
    let schema = r#"{
        "properties": {
            "weird": {"type": "tuple"}
        }
    }"#;
    let (cols, unparsed) = parse_json_schema(schema).unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0].data_type, "Utf8");
    assert_eq!(unparsed, vec!["type \"tuple\""]);
}

#[test]
fn binary_column_round_trips_via_content_encoding() {
    let original = cols(&[("blob", "Binary")]);
    let schema_text = SchemaTarget::JsonSchema.export(&original, "t");
    let (round_tripped, unparsed) = parse_json_schema(&schema_text).unwrap();
    assert!(unparsed.is_empty());
    assert_eq!(round_tripped[0].data_type, "Binary");
}

#[test]
fn pure_compare_path_works_without_json_schema() {
    let actual = cols(&[("id", "Int64"), ("name", "Utf8")]);
    let expected = cols(&[("id", "Int64"), ("name", "Utf8")]);
    let diff = validate_against_schema(&actual, &expected);
    assert!(diff.identical);
}

/// A schema's `time` / `duration` matches a Time / Duration column whatever
/// unit its reader gave it: a JSON Schema round trip, and two Arrow units.
#[test]
fn time_and_duration_match_in_any_unit() {
    let actual = cols(&[("start", "Time64(ns)"), ("worked", "Duration(µs)")]);
    let schema_text = SchemaTarget::JsonSchema.export(&actual, "shifts");
    let (expected, unparsed) = parse_json_schema(&schema_text).unwrap();
    assert!(unparsed.is_empty(), "{unparsed:?}");
    assert_eq!(expected[0].data_type, "Time64(Microsecond)");
    assert!(validate_against_schema(&actual, &expected).identical);
    let other_units = cols(&[("start", "Time64(µs)"), ("worked", "Duration(ns)")]);
    assert!(validate_against_schema(&actual, &other_units).identical);
}
