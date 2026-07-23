//! The [`CellValue`] type - the single cell of a [`DataTable`](super::DataTable) -
//! plus the type-name helpers and value conversion/comparison free functions that
//! operate on it. Split out of `data/mod.rs` to keep the core table model focused.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Convert a wildcard pattern to a regex string.
/// `*` matches any sequence of characters, `?` matches a single character.
/// Use `\*` for a literal `*` and `\?` for a literal `?`.
pub fn wildcard_to_regex(pattern: &str) -> String {
    let mut regex = String::from("(?i)");
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && (chars[i + 1] == '*' || chars[i + 1] == '?') {
            // Escaped wildcard -> literal
            regex.push_str(&regex::escape(&chars[i + 1].to_string()));
            i += 2;
        } else if chars[i] == '*' {
            regex.push_str(".*");
            i += 1;
        } else if chars[i] == '?' {
            regex.push('.');
            i += 1;
        } else {
            regex.push_str(&regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }
    regex
}

/// Represents a single cell value in the data table.
/// Supports structured (typed columns) and semi-structured (mixed types) data.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Date(String),
    DateTime(String),
    Binary(Vec<u8>),
    /// For semi-structured nested data (JSON objects, arrays, etc.)
    Nested(String),
}

impl fmt::Display for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellValue::Null => write!(f, ""),
            CellValue::Bool(b) => write!(f, "{}", b),
            CellValue::Int(i) => write!(f, "{}", i),
            CellValue::Float(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    write!(f, "{:.1}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            CellValue::String(s) => write!(f, "{}", s),
            CellValue::Date(s) => write!(f, "{}", s),
            CellValue::DateTime(s) => write!(f, "{}", s),
            CellValue::Binary(b) => {
                // Default Display uses hex; for mode-aware display use display_binary()
                for (i, byte) in b.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{:02x}", byte)?;
                }
                Ok(())
            }
            CellValue::Nested(s) => write!(f, "{}", s),
        }
    }
}

impl CellValue {
    /// Format this value for display, using `mode` when the value is Binary.
    /// Non-binary values use their normal `Display` representation.
    pub fn display_with_binary_mode(&self, mode: BinaryDisplayMode) -> String {
        match self {
            CellValue::Binary(b) => match mode {
                BinaryDisplayMode::Binary => b
                    .iter()
                    .map(|byte| format!("{:08b}", byte))
                    .collect::<Vec<_>>()
                    .join(" "),
                BinaryDisplayMode::Hex => b
                    .iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<Vec<_>>()
                    .join(" "),
                BinaryDisplayMode::Text => {
                    if let Ok(s) = std::str::from_utf8(b)
                        && !s.is_empty()
                        && s.chars()
                            .all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
                    {
                        return s.to_string();
                    }
                    // Fall back to hex for non-printable / invalid UTF-8
                    b.iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            },
            other => other.to_string(),
        }
    }

    /// Parse a display string back into a Binary CellValue, respecting the display mode
    /// that was used to show it.
    pub fn parse_binary(text: &str, mode: BinaryDisplayMode) -> CellValue {
        if text.is_empty() {
            return CellValue::Null;
        }
        match mode {
            BinaryDisplayMode::Binary => {
                let bytes: Result<Vec<u8>, _> = text
                    .split_whitespace()
                    .map(|chunk| u8::from_str_radix(chunk, 2))
                    .collect();
                match bytes {
                    Ok(b) => CellValue::Binary(b),
                    Err(_) => CellValue::Binary(text.as_bytes().to_vec()),
                }
            }
            BinaryDisplayMode::Hex => {
                let bytes: Result<Vec<u8>, _> = text
                    .split_whitespace()
                    .map(|chunk| u8::from_str_radix(chunk, 16))
                    .collect();
                match bytes {
                    Ok(b) => CellValue::Binary(b),
                    Err(_) => CellValue::Binary(text.as_bytes().to_vec()),
                }
            }
            BinaryDisplayMode::Text => CellValue::Binary(text.as_bytes().to_vec()),
        }
    }

    /// Try to parse a display string back into a CellValue, keeping the same variant
    /// as the `hint` when possible.
    pub fn parse_like(hint: &CellValue, text: &str) -> CellValue {
        if text.is_empty() {
            return CellValue::Null;
        }
        match hint {
            CellValue::Bool(_) => match text.to_lowercase().as_str() {
                "true" | "1" | "yes" => CellValue::Bool(true),
                "false" | "0" | "no" => CellValue::Bool(false),
                _ => CellValue::String(text.to_string()),
            },
            CellValue::Int(_) => text
                .parse::<i64>()
                .map(CellValue::Int)
                .unwrap_or_else(|_| CellValue::String(text.to_string())),
            CellValue::Float(_) => text
                .parse::<f64>()
                .map(CellValue::Float)
                .unwrap_or_else(|_| CellValue::String(text.to_string())),
            CellValue::Date(_) => CellValue::Date(text.to_string()),
            CellValue::DateTime(_) => CellValue::DateTime(text.to_string()),
            CellValue::Binary(_) => Self::parse_binary(text, BinaryDisplayMode::Hex),
            _ => CellValue::String(text.to_string()),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            CellValue::Null => "null",
            CellValue::Bool(_) => "bool",
            CellValue::Int(_) => "int",
            CellValue::Float(_) => "float",
            CellValue::String(_) => "string",
            CellValue::Date(_) => "date",
            CellValue::DateTime(_) => "datetime",
            CellValue::Binary(_) => "binary",
            CellValue::Nested(_) => "nested",
        }
    }
}

/// How to display binary (`Vec<u8>`) cell values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BinaryDisplayMode {
    /// Raw binary digits grouped per byte (e.g., `01000001 01000010`).
    #[default]
    Binary,
    /// Hexadecimal (e.g., `41 42`).
    Hex,
    /// Decode as UTF-8 text; fall back to hex for invalid sequences.
    Text,
}

impl BinaryDisplayMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Binary => "Binary",
            Self::Hex => "Hex",
            Self::Text => "Text (UTF-8)",
        }
    }

    pub fn label_t(self) -> String {
        crate::i18n::t(match self {
            Self::Binary => "enum.bin_binary",
            Self::Hex => "enum.bin_hex",
            Self::Text => "enum.bin_text",
        })
    }
}

/// Whether an Arrow-style column type string is numeric.
///
/// Used to decide cell text alignment: numeric columns right-align every cell,
/// non-numeric columns left-align - independent of an individual cell's runtime
/// `CellValue` variant. This way a stray number stored in a `Utf8` column still
/// reads as text.
pub fn is_numeric_data_type(s: &str) -> bool {
    matches!(
        s,
        "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "UInt8"
            | "UInt16"
            | "UInt32"
            | "UInt64"
            | "Float16"
            | "Float32"
            | "Float64"
    )
}

/// Check if a single CellValue can be converted to the target data type.
pub fn can_convert_value(val: &CellValue, target_type: &str) -> bool {
    match val {
        CellValue::Null => true, // Null converts to anything
        CellValue::Bool(_) => matches!(
            target_type,
            "String" | "Utf8" | "Boolean" | "Int64" | "Float64"
        ),
        CellValue::Int(_) => matches!(
            target_type,
            "String" | "Utf8" | "Int64" | "Float64" | "Boolean"
        ),
        CellValue::Float(f) => match target_type {
            "String" | "Utf8" | "Float64" => true,
            "Int64" => f.fract() == 0.0 && f.abs() < i64::MAX as f64,
            _ => false,
        },
        CellValue::String(s) => {
            if s.is_empty() {
                return true;
            }
            match target_type {
                "String" | "Utf8" => true,
                "Int64" => s.parse::<i64>().is_ok(),
                "Float64" => s.parse::<f64>().is_ok(),
                "Boolean" => matches!(
                    s.to_lowercase().as_str(),
                    "true" | "false" | "1" | "0" | "yes" | "no"
                ),
                "Date32" => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok(),
                "Timestamp(Microsecond, None)" => {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").is_ok()
                        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
                        || chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f").is_ok()
                }
                _ => false,
            }
        }
        CellValue::Date(_) => matches!(
            target_type,
            "String" | "Utf8" | "Date32" | "Timestamp(Microsecond, None)"
        ),
        CellValue::DateTime(_) => match target_type {
            "String" | "Utf8" | "Timestamp(Microsecond, None)" => true,
            "Date32" => true, // truncate time portion
            _ => false,
        },
        CellValue::Binary(_) => matches!(target_type, "String" | "Utf8"),
        CellValue::Nested(_) => matches!(target_type, "String" | "Utf8"),
    }
}

/// Convert a CellValue to a target data type.
/// Assumes `can_convert_value` has already validated the conversion.
pub fn convert_value(val: &CellValue, target_type: &str) -> CellValue {
    match val {
        CellValue::Null => CellValue::Null,
        CellValue::Bool(b) => match target_type {
            "Boolean" => val.clone(),
            "Int64" => CellValue::Int(if *b { 1 } else { 0 }),
            "Float64" => CellValue::Float(if *b { 1.0 } else { 0.0 }),
            "String" | "Utf8" => CellValue::String(b.to_string()),
            _ => val.clone(),
        },
        CellValue::Int(n) => match target_type {
            "Int64" => val.clone(),
            "Float64" => CellValue::Float(*n as f64),
            "Boolean" => CellValue::Bool(*n != 0),
            "String" | "Utf8" => CellValue::String(n.to_string()),
            _ => val.clone(),
        },
        CellValue::Float(f) => match target_type {
            "Float64" => val.clone(),
            "Int64" => CellValue::Int(*f as i64),
            "String" | "Utf8" => CellValue::String(f.to_string()),
            _ => val.clone(),
        },
        CellValue::String(s) => {
            if s.is_empty() {
                return CellValue::Null;
            }
            match target_type {
                "String" | "Utf8" => val.clone(),
                "Int64" => CellValue::Int(s.parse::<i64>().unwrap_or(0)),
                "Float64" => CellValue::Float(s.parse::<f64>().unwrap_or(0.0)),
                "Boolean" => {
                    let lower = s.to_lowercase();
                    CellValue::Bool(matches!(lower.as_str(), "true" | "1" | "yes"))
                }
                "Date32" => CellValue::Date(s.clone()),
                "Timestamp(Microsecond, None)" => CellValue::DateTime(s.clone()),
                _ => val.clone(),
            }
        }
        CellValue::Date(s) => match target_type {
            "Date32" => val.clone(),
            "String" | "Utf8" => CellValue::String(s.clone()),
            "Timestamp(Microsecond, None)" => CellValue::DateTime(format!("{s} 00:00:00")),
            _ => val.clone(),
        },
        CellValue::DateTime(s) => match target_type {
            "Timestamp(Microsecond, None)" => val.clone(),
            "String" | "Utf8" => CellValue::String(s.clone()),
            "Date32" => CellValue::Date(s.chars().take(10).collect()),
            _ => val.clone(),
        },
        CellValue::Binary(b) => match target_type {
            "String" | "Utf8" => CellValue::String(format!("{b:?}")),
            _ => val.clone(),
        },
        CellValue::Nested(s) => match target_type {
            "String" | "Utf8" => CellValue::String(s.clone()),
            _ => val.clone(),
        },
    }
}

/// Compare two CellValues for sorting.
/// Ordering: Null < Bool < Int/Float (numeric) < String/Date/DateTime < Binary < Nested
pub fn cmp_cell_values(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (a, b) {
        (CellValue::Null, CellValue::Null) => Ordering::Equal,
        (CellValue::Null, _) => Ordering::Less,
        (_, CellValue::Null) => Ordering::Greater,

        (CellValue::Bool(a), CellValue::Bool(b)) => a.cmp(b),

        (CellValue::Int(a), CellValue::Int(b)) => a.cmp(b),
        (CellValue::Float(a), CellValue::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (CellValue::Int(a), CellValue::Float(b)) => {
            (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (CellValue::Float(a), CellValue::Int(b)) => {
            a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
        }

        (CellValue::String(a), CellValue::String(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
        (CellValue::Date(a), CellValue::Date(b)) => a.cmp(b),
        (CellValue::DateTime(a), CellValue::DateTime(b)) => a.cmp(b),

        // Fallback: compare display strings
        _ => a
            .to_string()
            .to_lowercase()
            .cmp(&b.to_string().to_lowercase()),
    }
}
