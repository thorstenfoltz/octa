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
use crate::data::conditional_format::CondOp;
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
    /// One of `not_null`, `range`, `regex`, `unique`, `max_length`, or an ID
    /// check: `iban`, `card_number`, `gtin`, `vat_id`, `email`; or `compare`,
    /// `one_of`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    /// `compare`: one of `=`, `!=`, `<`, `<=`, `>`, `>=`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    /// `compare`: the column on the other side, in the same row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_column: Option<String>,
    /// `one_of`: the allowed values, exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
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
        ValidationKind::Id(k) => k.id(),
        ValidationKind::CompareColumn { .. } => "compare",
        ValidationKind::OneOf(_) => "one_of",
    }
}

/// The rules file writes a comparison as its symbol.
fn op_symbol(op: CondOp) -> &'static str {
    match op {
        CondOp::Eq => "=",
        CondOp::Ne => "!=",
        CondOp::Lt => "<",
        CondOp::Le => "<=",
        CondOp::Gt => ">",
        _ => ">=",
    }
}

fn op_from_symbol(s: &str) -> Option<CondOp> {
    CondOp::COMPARISONS
        .iter()
        .copied()
        .find(|op| op_symbol(*op) == s.trim())
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
                ValidationKind::CompareColumn { op, other } => {
                    out.op = Some(op_symbol(*op).to_string());
                    out.other_column = columns.get(*other).map(|c| c.name.clone());
                }
                ValidationKind::OneOf(values) => out.values = Some(values.clone()),
                ValidationKind::NotNull | ValidationKind::Unique | ValidationKind::Id(_) => {}
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
            "compare" => {
                if column.is_none() {
                    unknown.push("a compare rule needs a column".to_string());
                    continue;
                }
                let Some(op) = r.op.as_deref().and_then(op_from_symbol) else {
                    unknown.push(format!(
                        "compare rule on '{}' needs op =, !=, <, <=, > or >=",
                        r.column.as_deref().unwrap_or_default()
                    ));
                    continue;
                };
                let Some(other_name) = r.other_column.as_deref() else {
                    unknown.push(format!(
                        "compare rule on '{}' needs other_column",
                        r.column.as_deref().unwrap_or_default()
                    ));
                    continue;
                };
                let Some(other) = columns.iter().position(|c| c.name == other_name) else {
                    unknown.push(other_name.to_string());
                    continue;
                };
                ValidationKind::CompareColumn { op, other }
            }
            "one_of" => match &r.values {
                Some(values) if !values.is_empty() => ValidationKind::OneOf(values.clone()),
                _ => {
                    unknown.push(format!(
                        "one_of rule on '{}' lists no values",
                        r.column.as_deref().unwrap_or("(any column)")
                    ));
                    continue;
                }
            },
            other => match crate::data::id_checks::IdKind::from_id(other) {
                Some(k) => ValidationKind::Id(k),
                None => {
                    unknown.push(format!("unknown rule kind '{other}'"));
                    continue;
                }
            },
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
#[path = "rules_file_tests.rs"]
mod tests;
