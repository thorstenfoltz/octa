//! Value shapes: what a column's values look like with the specifics taken
//! out. `D-80331` becomes `A-99999`, `anna@x.de` becomes `aaaa@a.aa`.
//! Grouping a column by shape shows the three postcodes typed in the wrong
//! format, which a "mostly text" type check cannot.
//!
//! Pure. Read by the header funnel's Shapes switch, the Quality Report, the
//! `--shapes` CLI action and the `value_shapes` MCP tool. Values are shaped
//! from their display text, the same text the funnel's filter keys on.

use std::collections::{HashMap, HashSet};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// Values longer than this compress runs, so a 40-character hash does not
/// print as 40 characters of `a9a9...`.
const COMPRESS_OVER: usize = 24;
/// Shortest run written as `a(5)` rather than `aaaaa`.
const MIN_RUN: usize = 4;
/// "Mixed" needs one shape covering at least this share of the values...
pub const MIXED_MIN_SHARE: f64 = 0.9;
/// ...and no more than this many shapes; more means free text.
pub const MIXED_MAX_SHAPES: usize = 20;

/// Shape of one value. Digits become `9`, capital letters `A`, other letters
/// (lower case, and scripts without case such as CJK) `a`. Everything else,
/// spaces and punctuation included, stays as it is.
pub fn shape_of(text: &str) -> String {
    let chars: Vec<char> = text.chars().map(shape_char).collect();
    if chars.len() <= COMPRESS_OVER {
        return chars.into_iter().collect();
    }
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..].iter().take_while(|&&x| x == c).count();
        if run >= MIN_RUN {
            out.push(c);
            out.push_str(&format!("({run})"));
        } else {
            out.extend(std::iter::repeat_n(c, run));
        }
        i += run;
    }
    out
}

fn shape_char(c: char) -> char {
    if c.is_numeric() {
        '9'
    } else if c.is_uppercase() {
        'A'
    } else if c.is_alphabetic() {
        'a'
    } else {
        c
    }
}

/// One shape, how many values have it, and the first value seen with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeCount {
    pub shape: String,
    pub count: usize,
    pub example: String,
}

/// A column's shapes, most common first. Empty cells are not a shape.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shapes {
    pub shapes: Vec<ShapeCount>,
    pub empty: usize,
}

/// Every shape in `col`, over the whole column (pending edits included).
/// Ties sort by shape so the order is stable.
pub fn shape_frequency(table: &DataTable, col: usize) -> Shapes {
    let mut counts: HashMap<String, (usize, String)> = HashMap::new();
    let mut empty = 0;
    for row in 0..table.row_count() {
        let text = match table.get(row, col) {
            None | Some(CellValue::Null) => String::new(),
            Some(v) => v.to_string(),
        };
        if text.is_empty() {
            empty += 1;
            continue;
        }
        let entry = counts
            .entry(shape_of(&text))
            .or_insert_with(|| (0, text.clone()));
        entry.0 += 1;
    }
    let mut shapes: Vec<ShapeCount> = counts
        .into_iter()
        .map(|(shape, (count, example))| ShapeCount {
            shape,
            count,
            example,
        })
        .collect();
    shapes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.shape.cmp(&b.shape)));
    Shapes { shapes, empty }
}

/// Every distinct value in `col` whose shape is in `shapes`: what ticking
/// those shapes in the funnel means as a value filter.
pub fn values_with_shapes(
    table: &DataTable,
    col: usize,
    shapes: &HashSet<String>,
) -> HashSet<String> {
    (0..table.row_count())
        .filter_map(|r| table.get(r, col))
        .map(|v| v.to_string())
        .filter(|t| !t.is_empty() && shapes.contains(&shape_of(t)))
        .collect()
}

/// The Quality Report's judgement on a column's shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeVerdict {
    /// Every value has the same shape.
    Consistent,
    /// One shape dominates and a few values stray from it.
    Mixed,
    /// Empty, not text, or too varied to call.
    NotApplicable,
}

impl ShapeVerdict {
    /// The cell text in the report; keys into [`VALUE_HINTS`].
    pub fn id(self) -> &'static str {
        match self {
            Self::Consistent => "consistent",
            Self::Mixed => "mixed",
            Self::NotApplicable => "na",
        }
    }
}

/// Hover text per verdict cell, same mechanism as `benford::VALUE_HINTS`.
pub const VALUE_HINTS: &[(&str, &str)] = &[
    ("consistent", "quality.verdict_shape_consistent"),
    ("mixed", "quality.verdict_shape_mixed"),
    ("na", "quality.verdict_shape_na"),
];

pub fn verdict(shapes: &Shapes) -> ShapeVerdict {
    let total: usize = shapes.shapes.iter().map(|s| s.count).sum();
    match shapes.shapes.first() {
        None => ShapeVerdict::NotApplicable,
        Some(_) if shapes.shapes.len() == 1 => ShapeVerdict::Consistent,
        Some(top)
            if shapes.shapes.len() <= MIXED_MAX_SHAPES
                && top.count as f64 >= MIXED_MIN_SHARE * total as f64 =>
        {
            ShapeVerdict::Mixed
        }
        Some(_) => ShapeVerdict::NotApplicable,
    }
}

/// Shapes only mean something for text: `5` and `12345` are both fine
/// numbers with different shapes.
pub fn is_text_type(data_type: &str) -> bool {
    let t = data_type.to_ascii_lowercase();
    !(t.contains("int")
        || t.contains("float")
        || t.contains("double")
        || t.contains("decimal")
        || t.contains("date")
        || t.contains("time")
        || t.contains("bool"))
}

/// Minority shapes of every Mixed text column, for the Quality Report's
/// section tab: `column, shape, count, example`. `None` when no column is
/// mixed, so a clean file opens no extra tab.
pub fn section_table(table: &DataTable) -> Option<DataTable> {
    let mut rows = Vec::new();
    for (col, info) in table.columns.iter().enumerate() {
        if !is_text_type(&info.data_type) {
            continue;
        }
        let shapes = shape_frequency(table, col);
        if verdict(&shapes) != ShapeVerdict::Mixed {
            continue;
        }
        for s in shapes.shapes.iter().skip(1) {
            rows.push(vec![
                CellValue::String(info.name.clone()),
                CellValue::String(s.shape.clone()),
                CellValue::Int(s.count as i64),
                CellValue::String(s.example.clone()),
            ]);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let col = |name: &str, ty: &str| ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
    };
    let mut out = DataTable::empty();
    out.columns = vec![
        col("column", "Utf8"),
        col("shape", "Utf8"),
        col("count", "Int64"),
        col("example", "Utf8"),
    ];
    out.rows = rows;
    Some(out)
}

#[cfg(test)]
#[path = "shapes_tests.rs"]
mod tests;
