//! Validation rules as a file, so a rule set can be committed beside the data
//! and re-run by a CI step.
//!
//! Rules are stored by **column name**, not index: an index is meaningless the
//! moment a column moves, and a rules file outlives the table it was written
//! from. [`resolve`] maps names back to indices against whatever columns the
//! table actually has now, and **reports what it could not find** rather than
//! quietly dropping it. A rules file that half applies is worse than one that
//! fails loudly, because the run still goes green.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::data::ColumnInfo;
use crate::data::validation::{ValidationKind, ValidationRule};

/// A rules file. The array is named `rule` so the TOML reads `[[rule]]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RulesFile {
    #[serde(default)]
    pub rule: Vec<NamedRule>,
}

/// One rule as written on disk. Every parameter is optional because each kind
/// uses a different subset; the kind decides which ones are read.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct NamedRule {
    /// Column this rule applies to. Absent means every column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    /// One of `not_null`, `range`, `regex`, `unique`, `max_length`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}

/// The on-disk name of a kind. Kept next to the parser below so the two
/// cannot drift.
fn kind_id(kind: &ValidationKind) -> &'static str {
    match kind {
        ValidationKind::NotNull => "not_null",
        ValidationKind::Range { .. } => "range",
        ValidationKind::Regex(_) => "regex",
        ValidationKind::Unique => "unique",
        ValidationKind::MaxLength(_) => "max_length",
    }
}

/// Turn live rules into their file form, naming the columns they point at.
pub fn to_named(rules: &[ValidationRule], columns: &[ColumnInfo]) -> RulesFile {
    let rule = rules
        .iter()
        .map(|r| {
            let mut out = NamedRule {
                column: r
                    .column
                    .and_then(|i| columns.get(i))
                    .map(|c| c.name.clone()),
                kind: kind_id(&r.kind).to_string(),
                ..Default::default()
            };
            match &r.kind {
                ValidationKind::Range { min, max } => {
                    out.min = *min;
                    out.max = *max;
                }
                ValidationKind::Regex(p) => out.pattern = Some(p.clone()),
                ValidationKind::MaxLength(n) => out.max_length = Some(*n),
                ValidationKind::NotNull | ValidationKind::Unique => {}
            }
            out
        })
        .collect();
    RulesFile { rule }
}

/// Map a file's rules onto a table's columns.
///
/// Returns the rules that resolved plus a list of problems: an unknown column
/// name, or an unrecognised kind. A caller that treats a non-empty problem
/// list as a failure gets a rules file that cannot rot silently.
pub fn resolve(file: &RulesFile, columns: &[ColumnInfo]) -> (Vec<ValidationRule>, Vec<String>) {
    let mut rules = Vec::new();
    let mut unknown = Vec::new();

    for r in &file.rule {
        let column = match &r.column {
            None => None,
            Some(name) => match columns.iter().position(|c| &c.name == name) {
                Some(i) => Some(i),
                None => {
                    unknown.push(name.clone());
                    continue;
                }
            },
        };
        let kind = match r.kind.as_str() {
            "not_null" => ValidationKind::NotNull,
            "unique" => ValidationKind::Unique,
            "range" => ValidationKind::Range {
                min: r.min,
                max: r.max,
            },
            "regex" => ValidationKind::Regex(r.pattern.clone().unwrap_or_default()),
            "max_length" => ValidationKind::MaxLength(r.max_length.unwrap_or(0)),
            other => {
                unknown.push(format!("unknown rule kind '{other}'"));
                continue;
            }
        };
        rules.push(ValidationRule { column, kind });
    }
    (rules, unknown)
}

pub fn load(path: &Path) -> anyhow::Result<RulesFile> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    Ok(toml::from_str(&text)?)
}

pub fn save(path: &Path, file: &RulesFile) -> anyhow::Result<()> {
    let text = toml::to_string_pretty(file)?;
    std::fs::write(path, text)
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ColumnInfo;
    use crate::data::validation::{ValidationKind, ValidationRule};

    fn cols() -> Vec<ColumnInfo> {
        vec![
            ColumnInfo {
                name: "order_id".into(),
                data_type: "Int64".into(),
            },
            ColumnInfo {
                name: "amount".into(),
                data_type: "Float64".into(),
            },
        ]
    }

    #[test]
    fn round_trips_through_toml() {
        let rules = vec![
            ValidationRule {
                column: Some(0),
                kind: ValidationKind::Unique,
            },
            ValidationRule {
                column: Some(1),
                kind: ValidationKind::Range {
                    min: Some(0.0),
                    max: Some(100.0),
                },
            },
            ValidationRule {
                column: None,
                kind: ValidationKind::NotNull,
            },
        ];
        let file = to_named(&rules, &cols());
        let text = toml::to_string_pretty(&file).unwrap();
        let back: RulesFile = toml::from_str(&text).unwrap();
        let (resolved, unknown) = resolve(&back, &cols());
        assert!(unknown.is_empty());
        assert_eq!(resolved, rules);
    }

    #[test]
    fn an_unknown_column_is_reported_not_skipped() {
        let text = r#"
[[rule]]
column = "nope"
kind = "unique"
"#;
        let file: RulesFile = toml::from_str(text).unwrap();
        let (resolved, unknown) = resolve(&file, &cols());
        assert!(resolved.is_empty());
        assert_eq!(unknown, vec!["nope".to_string()]);
    }

    #[test]
    fn a_rule_without_a_column_applies_to_every_column() {
        let text = r#"
[[rule]]
kind = "not_null"
"#;
        let file: RulesFile = toml::from_str(text).unwrap();
        let (resolved, unknown) = resolve(&file, &cols());
        assert!(unknown.is_empty());
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].column, None);
    }

    #[test]
    fn an_unknown_kind_resolves_to_nothing_and_is_reported() {
        let text = r#"
[[rule]]
column = "amount"
kind = "sparkles"
"#;
        let file: RulesFile = toml::from_str(text).unwrap();
        let (resolved, unknown) = resolve(&file, &cols());
        assert!(resolved.is_empty());
        assert!(
            unknown.iter().any(|u| u.contains("sparkles")),
            "got {unknown:?}"
        );
    }
}
