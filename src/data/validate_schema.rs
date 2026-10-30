//! Validate a tabular file's column schema against an expected JSON
//! Schema. Pairs with `schema_export::json_schema::export` so a schema
//! exported from a known-good file can later be checked against new
//! arrivals.
//!
//! v1 scope: column-level only (names + types). Per-row data
//! validation is intentionally out of scope - readers already
//! type-check values as they parse them.
//!
//! Implementation is a thin shell over `compare_schemas`: parse the
//! JSON Schema into a `Vec<ColumnInfo>`, then diff. The interesting
//! work lives in [`parse_json_schema`], which is the inverse of the
//! exporter.

use serde_json::Value;

use crate::data::ColumnInfo;
use crate::data::compare_schemas::{SchemaDiff, compare_schemas};

/// Outcome of a validation run.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// True when the actual schema matches every column the expected
    /// schema declares with the same type, and no extras on either
    /// side.
    pub matches: bool,
    /// The full diff between actual and expected.
    pub diff: SchemaDiff,
    /// JSON Schema `type` values the parser did not recognise. Each
    /// such property defaulted to `"Utf8"` in the expected schema so
    /// the diff would still produce useful output. Inspect this when
    /// `matches` is unexpectedly `false`.
    pub unparsed_types: Vec<String>,
}

/// Parse Octa-flavoured JSON Schema into a list of `ColumnInfo`. The
/// schema is the one emitted by `schema_export::json_schema::export` -
/// a draft 2020-12 object schema with one entry per column under
/// `properties`. Column order follows the order keys appear in the
/// JSON object; `serde_json` preserves insertion order (with the
/// `preserve_order` feature, which Octa already depends on; without
/// it, order is by key - still deterministic for round-tripping
/// schemas Octa itself emits because the BTreeMap fallback sorts the
/// keys before the test queries them).
///
/// Returns the columns plus the list of `type` values that didn't
/// match a known mapping (those columns are still emitted, typed
/// `Utf8`).
pub fn parse_json_schema(json: &str) -> anyhow::Result<(Vec<ColumnInfo>, Vec<String>)> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| anyhow::anyhow!("not valid JSON: {e}"))?;
    let properties = v
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("missing or non-object `properties` field"))?;

    let mut columns = Vec::with_capacity(properties.len());
    let mut unparsed = Vec::new();
    for (name, prop) in properties {
        let (arrow_type, unrecognised) = property_to_arrow(prop);
        if let Some(raw) = unrecognised {
            unparsed.push(raw);
        }
        columns.push(ColumnInfo {
            name: name.clone(),
            data_type: arrow_type,
        });
    }
    Ok((columns, unparsed))
}

/// Inverse of `schema_export::json_schema::property_for`. Returns
/// `(arrow_type_name, optional unrecognised tag)`. Unrecognised types
/// (or `type: "string"` with an unknown `format`) fall back to `Utf8`.
fn property_to_arrow(prop: &Value) -> (String, Option<String>) {
    let ty = prop.get("type").and_then(Value::as_str).unwrap_or("");
    let format = prop.get("format").and_then(Value::as_str);
    match ty {
        "integer" => ("Int64".to_string(), None),
        "number" => ("Float64".to_string(), None),
        "boolean" => ("Boolean".to_string(), None),
        "string" => {
            // Binary detection via contentEncoding takes precedence
            // over format. The exporter emits `contentEncoding:
            // "base64"` for any Arrow Binary / LargeBinary column.
            if prop.get("contentEncoding").is_some() {
                return ("Binary".to_string(), None);
            }
            match format {
                Some("date") => ("Date32".to_string(), None),
                // Mirrors the exporter for any Timestamp(...) variant.
                // The exact unit / timezone tuple is lost in the round
                // trip; re-hydrate to the Octa default. Schemas needing
                // wire-level Timestamp matches should use another
                // schema-export target instead of JSON Schema.
                Some("date-time") => ("Timestamp(Microsecond, None)".to_string(), None),
                // The exporter's Time / Duration; the unit is lost likewise,
                // and a comparison ignores it.
                Some("time") => ("Time64(Microsecond)".to_string(), None),
                Some("duration") => ("Duration(Microsecond)".to_string(), None),
                None => ("Utf8".to_string(), None),
                Some(other) => (
                    "Utf8".to_string(),
                    Some(format!("string format \"{other}\"")),
                ),
            }
        }
        "" => ("Utf8".to_string(), Some("missing type".to_string())),
        other => ("Utf8".to_string(), Some(format!("type \"{other}\""))),
    }
}

/// Validate an actual schema against an expected one. Thin shell over
/// [`compare_schemas`]; `matches` is `diff.identical`.
pub fn validate_against_schema(actual: &[ColumnInfo], expected: &[ColumnInfo]) -> SchemaDiff {
    compare_schemas(actual, expected)
}

/// Build a `ValidationReport` from raw inputs: a `ColumnInfo` slice
/// (the actual file's schema) and a JSON Schema string (the expected
/// shape). Combines parsing + validation in one call so the binary
/// layer doesn't need to know about either step.
pub fn validate_against_json_schema(
    actual: &[ColumnInfo],
    json_schema: &str,
) -> anyhow::Result<ValidationReport> {
    let (expected, unparsed_types) = parse_json_schema(json_schema)?;
    let diff = validate_against_schema(actual, &expected);
    Ok(ValidationReport {
        matches: diff.identical,
        diff,
        unparsed_types,
    })
}
