//! Exact decimals. `CellValue` has no decimal of its own, so a decimal column is
//! a `Decimal128(p, s)` type over cells that hold its digits as text, or as a
//! `Float` where that says the same number. The type is what makes the column
//! numeric ([`crate::data::is_numeric_data_type`]) and sort by value, and what a
//! writer turns back into a real decimal.
//!
//! It exists because such a column used to be `Float64`, which every typed writer
//! then stored as a double: `DECIMAL(38,2)` value `...456.78` reached Parquet as
//! `1.23e+35`. Anything past about 15 digits, cents on large sums included, went.

use std::cmp::Ordering;

use super::CellValue;

/// The largest precision a `Decimal128` holds, and DuckDB's largest as well.
const MAX_PRECISION: u8 = 38;

/// Decimals an unrounded division keeps at most: every one of them still fits
/// 128 bits for amounts up to about 10^17.
pub const UNROUNDED_DECIMALS: u32 = 20;

/// `(precision, scale)` of `Decimal128(p, s)`, in any case. `None` for any other
/// type, and for a precision or scale no writer here could store.
pub fn decimal_parts(data_type: &str) -> Option<(u8, i8)> {
    let lower = data_type.to_ascii_lowercase();
    let inner = lower.strip_prefix("decimal128(")?.strip_suffix(')')?;
    let (precision, scale) = inner.split_once(',')?;
    let precision: u8 = precision.trim().parse().ok()?;
    let scale: i8 = scale.trim().parse().ok()?;
    let fits = (1..=MAX_PRECISION).contains(&precision)
        && i16::from(scale) >= 0
        && i16::from(scale) <= i16::from(precision);
    fits.then_some((precision, scale))
}

/// Octa's type for DuckDB's `DECIMAL(p,s)`.
pub fn from_duckdb_type(duckdb_type: &str) -> Option<String> {
    let upper = duckdb_type.to_ascii_uppercase();
    let inner = upper.strip_prefix("DECIMAL(")?.strip_suffix(')')?;
    let (precision, scale) = inner.split_once(',')?;
    let name = format!("Decimal128({}, {})", precision.trim(), scale.trim());
    decimal_parts(&name).map(|_| name)
}

/// DuckDB's type for a `Decimal128(p, s)` column.
pub fn duckdb_type(data_type: &str) -> Option<String> {
    decimal_parts(data_type).map(|(precision, scale)| format!("DECIMAL({precision},{scale})"))
}

/// `value` with its point `scale` digits from the right, every digit kept.
pub fn decimal_text(value: i128, scale: i8) -> String {
    let digits = value.unsigned_abs().to_string();
    let sign = if value < 0 { "-" } else { "" };
    let scale = usize::try_from(scale).unwrap_or(0);
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let digits = format!("{digits:0>width$}", width = scale + 1);
    let (whole, fraction) = digits.split_at(digits.len() - scale);
    format!("{sign}{whole}.{fraction}")
}

/// A number's sign and its digits either side of the point. `None` for anything
/// that is not a plain decimal number.
fn split_number(text: &str) -> Option<(bool, &str, &str)> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let any_digit = !whole.is_empty() || !fraction.is_empty();
    (any_digit && all_digits(whole) && all_digits(fraction)).then_some((negative, whole, fraction))
}

/// `text` as the integer a `Decimal128(precision, scale)` stores. Digits past the
/// scale round half away from zero, as DuckDB's own cast does. `None` for anything
/// that is not a number, or that does not fit the precision.
pub fn parse_decimal(text: &str, precision: u8, scale: i8) -> Option<i128> {
    let (negative, whole, fraction) = split_number(text)?;
    let scale = usize::try_from(scale).ok()?;
    let kept = fraction.get(..scale.min(fraction.len()))?;
    let padding = std::iter::repeat_n(b'0', scale - kept.len());
    let mut value: i128 = 0;
    for digit in whole.bytes().chain(kept.bytes()).chain(padding) {
        value = value
            .checked_mul(10)?
            .checked_add(i128::from(digit - b'0'))?;
    }
    if fraction.as_bytes().get(scale).is_some_and(|d| *d >= b'5') {
        value = value.checked_add(1)?;
    }
    if value >= 10i128.checked_pow(u32::from(precision))? {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// A cell's number as text, for a cell that can hold one.
fn number_text(cell: &CellValue) -> Option<String> {
    match cell {
        CellValue::Int(n) => Some(n.to_string()),
        CellValue::Float(f) if f.is_finite() => Some(f.to_string()),
        CellValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// A cell as the integer a `Decimal128(precision, scale)` stores. `None` when it
/// cannot be stored there, a null included: the caller tells those apart.
pub fn cell_decimal(cell: &CellValue, precision: u8, scale: i8) -> Option<i128> {
    parse_decimal(&number_text(cell)?, precision, scale)
}

/// The cell for a stored decimal: a `Float` when that says exactly this number, so
/// it behaves like every other number in Octa, and the exact digits as text when it
/// cannot (see [`number_cell`]). A column can hold both, which [`cmp_decimal`]
/// sorts correctly.
pub fn decimal_cell(raw: i128, scale: i8) -> CellValue {
    let exact = decimal_text(raw, scale);
    number_cell(&exact).unwrap_or(CellValue::String(exact))
}

/// Which of two decimal cells is larger, by value and exactly, where text order
/// says `10.00` comes before `9.00`. `None` when either is not a number (a null, or
/// text someone typed), for the caller's ordinary comparison.
pub fn cmp_decimal(a: &CellValue, b: &CellValue) -> Option<Ordering> {
    let (a, b) = (number_text(a)?, number_text(b)?);
    let (a_negative, a_whole, a_fraction) = split_number(&a)?;
    let (b_negative, b_whole, b_fraction) = split_number(&b)?;
    let magnitude = |whole: &str, fraction: &str| {
        let whole = whole.trim_start_matches('0');
        let fraction = fraction.trim_end_matches('0');
        (whole.len(), whole.to_string(), fraction.to_string())
    };
    let a_size = magnitude(a_whole, a_fraction);
    let b_size = magnitude(b_whole, b_fraction);
    let is_zero = |size: &(usize, String, String)| size.1.is_empty() && size.2.is_empty();
    let a_negative = a_negative && !is_zero(&a_size);
    let b_negative = b_negative && !is_zero(&b_size);
    Some(match (a_negative, b_negative) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        (false, false) => a_size.cmp(&b_size),
        (true, true) => b_size.cmp(&a_size),
    })
}

/// A number held exactly, `raw / 10^scale`: what a cell says, digit for digit.
///
/// It is how Octa adds up. A float sum of 0.1, 0.2 and 0.3 is 0.6000000000000001
/// and does not equal 0.6; adding the digits each value shows gives 0.6. A float
/// cell counts by its shortest text, the digits Octa displays for it, so a column
/// read from a binary source (a Parquet DOUBLE, an Excel cell) adds up the way it
/// reads, not the way its binary fraction does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exact {
    raw: i128,
    scale: u32,
}

/// Extra digits a mean gets past its values' own: enough that 1/3 reads as a
/// third, not so many that a mean of prices drowns in digits.
const MEAN_EXTRA_DIGITS: u32 = 12;

impl Exact {
    /// Plain decimal text: digits, at most one point, an optional sign. `None` for
    /// an exponent, a space, anything else, or more than 38 digits after the point.
    pub fn parse(text: &str) -> Option<Exact> {
        if text.trim().len() != text.len() {
            return None;
        }
        let (negative, whole, fraction) = split_number(text)?;
        let scale = u32::try_from(fraction.len())
            .ok()
            .filter(|s| *s <= u32::from(MAX_PRECISION))?;
        let mut raw: i128 = 0;
        for digit in whole.bytes().chain(fraction.bytes()) {
            raw = raw.checked_mul(10)?.checked_add(i128::from(digit - b'0'))?;
        }
        Some(Exact {
            raw: if negative { -raw } else { raw },
            scale,
        })
    }

    /// [`parse`](Self::parse), and scientific text too (`1.5E+3`, `-2e-4`),
    /// exactly: the exponent moves the point, no float involved.
    pub fn parse_scientific(text: &str) -> Option<Exact> {
        let Some(at) = text.find(['e', 'E']) else {
            return Self::parse(text);
        };
        let mantissa = Self::parse(&text[..at])?;
        let exponent: i64 = text[at + 1..].parse().ok()?;
        let scale = i64::from(mantissa.scale) - exponent;
        if scale >= 0 {
            let scale = u32::try_from(scale)
                .ok()
                .filter(|s| *s <= u32::from(MAX_PRECISION))?;
            return Some(Exact {
                raw: mantissa.raw,
                scale,
            });
        }
        let shift = u32::try_from(-scale).ok()?;
        Some(Exact {
            raw: mantissa.raw.checked_mul(10i128.checked_pow(shift)?)?,
            scale: 0,
        })
    }

    /// A float by the digits it shows. `None` for NaN, infinity, and a value
    /// too large or too small for 38 digits.
    pub fn of_f64(value: f64) -> Option<Exact> {
        // Rust's `Display` writes the shortest text that reads back as this
        // float, and never an exponent.
        value
            .is_finite()
            .then(|| Self::parse(&value.to_string()))
            .flatten()
    }

    /// A number cell: an Int, a Float by its digits, or text holding a number.
    pub fn of_cell(cell: &CellValue) -> Option<Exact> {
        match cell {
            CellValue::Int(n) => Some(Exact {
                raw: i128::from(*n),
                scale: 0,
            }),
            CellValue::Float(f) => Self::of_f64(*f),
            CellValue::String(s) => Self::parse(s),
            _ => None,
        }
    }

    /// The raw digits at `scale` decimals, `None` when they do not fit 128 bits
    /// or `scale` is below this number's own.
    pub fn raw_scaled(self, scale: u32) -> Option<i128> {
        self.raw
            .checked_mul(10i128.checked_pow(scale.checked_sub(self.scale)?)?)
    }

    /// The number `raw / 10^scale`.
    pub fn from_raw(raw: i128, scale: u32) -> Exact {
        Exact { raw, scale }
    }

    /// Digits after the point.
    pub fn scale(self) -> u32 {
        self.scale
    }

    pub fn checked_add(self, other: Exact) -> Option<Exact> {
        let scale = self.scale.max(other.scale);
        Some(Exact {
            raw: self
                .raw_scaled(scale)?
                .checked_add(other.raw_scaled(scale)?)?,
            scale,
        })
    }

    pub fn checked_sub(self, other: Exact) -> Option<Exact> {
        self.checked_add(Exact {
            raw: other.raw.checked_neg()?,
            scale: other.scale,
        })
    }

    /// Whether two numbers are equal, whatever their scales (`1.5` and `1.50`).
    pub fn same_value(self, other: Exact) -> bool {
        let scale = self.scale.max(other.scale);
        matches!((self.raw_scaled(scale), other.raw_scaled(scale)), (Some(a), Some(b)) if a == b)
    }

    /// `self / count` with [`MEAN_EXTRA_DIGITS`] more digits, the last one
    /// rounded half away from zero. Fewer extra digits when 38 would not hold them.
    pub fn div_count(self, count: usize) -> Option<Exact> {
        self.div_count_with(count, MEAN_EXTRA_DIGITS)
    }

    /// [`div_count`](Self::div_count) with up to `extra` more digits.
    fn div_count_with(self, count: usize, extra: u32) -> Option<Exact> {
        let count = i128::try_from(count).ok().filter(|n| *n > 0)?;
        let most = u32::from(MAX_PRECISION).max(self.scale);
        (self.scale..=(self.scale + extra).min(most))
            .rev()
            .find_map(|scale| {
                let numerator = self.raw_scaled(scale)?;
                let (quotient, remainder) = (numerator / count, numerator % count);
                let away = remainder.unsigned_abs() * 2 >= count.unsigned_abs();
                let raw = if away {
                    quotient.checked_add(numerator.signum())?
                } else {
                    quotient
                };
                Some(Exact { raw, scale })
            })
    }

    /// `self * other`, every digit kept. `None` when it does not fit 128 bits
    /// or would need more than 38 decimals.
    pub fn checked_mul(self, other: Exact) -> Option<Exact> {
        let (a, b) = (self.trimmed(), other.trimmed());
        let scale = a.scale + b.scale;
        if scale > u32::from(MAX_PRECISION) {
            return None;
        }
        Some(Exact {
            raw: a.raw.checked_mul(b.raw)?,
            scale,
        })
    }

    /// `self / other` to `scale` decimals, the last one rounded half away from
    /// zero. `None` for a zero divisor or when the digits do not fit.
    pub fn checked_div(self, other: Exact, scale: u32) -> Option<Exact> {
        if other.raw == 0 {
            return None;
        }
        // self.raw / 10^s1 / (other.raw / 10^s2) = self.raw * 10^(s2 + scale) / (other.raw * 10^s1) / 10^scale
        let up = other.scale.checked_add(scale)?;
        let (num_exp, den_exp) = if up >= self.scale {
            (up - self.scale, 0)
        } else {
            (0, self.scale - up)
        };
        let num = self.raw.checked_mul(10i128.checked_pow(num_exp)?)?;
        let den = other.raw.checked_mul(10i128.checked_pow(den_exp)?)?;
        let (q, r) = (num / den, num % den);
        let away = r.unsigned_abs() * 2 >= den.unsigned_abs();
        let sign = if (num < 0) != (den < 0) { -1 } else { 1 };
        let raw = if away && r != 0 {
            q.checked_add(sign)?
        } else {
            q
        };
        Some(Exact { raw, scale })
    }

    /// `self / other` with as many decimals as fit, at most `most`, trailing
    /// zeros dropped.
    pub fn div_up_to(self, other: Exact, most: u32) -> Option<Exact> {
        (0..=most)
            .rev()
            .find_map(|s| self.checked_div(other, s))
            .map(Exact::trimmed)
    }

    /// Rounded half away from zero to `decimals`. A number with fewer
    /// decimals comes back as it is: rounding never adds digits.
    pub fn round_to(self, decimals: u32) -> Exact {
        if self.scale <= decimals {
            return self;
        }
        let Some(div) = 10i128.checked_pow(self.scale - decimals) else {
            return Exact {
                raw: 0,
                scale: decimals,
            };
        };
        let (q, r) = (self.raw / div, self.raw % div);
        let away = r.unsigned_abs() * 2 >= div.unsigned_abs();
        Exact {
            raw: if away { q + self.raw.signum() } else { q },
            scale: decimals,
        }
    }

    /// The same number without trailing zeros after the point.
    fn trimmed(mut self) -> Exact {
        while self.scale > 0 && self.raw % 10 == 0 {
            self.raw /= 10;
            self.scale -= 1;
        }
        self
    }

    /// Every digit, as text.
    pub fn text(self) -> String {
        decimal_text(self.raw, self.scale as i8)
    }

    /// The cell for this number: an Int when it is whole and fits, else a Float
    /// when that shows exactly these digits, else the digits as text.
    pub fn to_cell(self) -> CellValue {
        let t = self.trimmed();
        match i64::try_from(t.raw) {
            Ok(n) if t.scale == 0 => CellValue::Int(n),
            _ => decimal_cell(t.raw, t.scale as i8),
        }
    }

    /// The nearest float. Only for what has to be a float (a chart's height,
    /// a ratio): the digits are lost past about 15.
    pub fn to_f64(self) -> f64 {
        self.text().parse().unwrap_or(f64::NAN)
    }

    /// Digits before the point (at least one) and after it.
    fn width(self) -> (u32, u32) {
        let whole = self.raw.unsigned_abs() / 10u128.pow(self.scale);
        let whole_digits = if whole == 0 { 1 } else { whole.ilog10() + 1 };
        (whole_digits, self.scale)
    }
}

/// A number written as text, kept exact: a `Float` when the float is the same
/// number (`1.50` is 1.5), the text itself when it would not be (past about 15
/// digits). `None` for anything that is no plain decimal number, an exponent
/// included, so the caller's own parsing decides those as before.
pub fn number_cell(text: &str) -> Option<CellValue> {
    let exact = Exact::parse(text)?;
    Some(match text.parse::<f64>() {
        Ok(f) if Exact::of_f64(f).is_some_and(|e| e.same_value(exact)) => CellValue::Float(f),
        _ => CellValue::String(text.to_string()),
    })
}

/// A number cell as a key that is the same for the same number: `1`, `1.0`
/// and the digits `1.00` all give `1`. `None` for a cell that is no number.
/// For comparing the values of a number column, where a whole number may be an
/// `Int` in one row and a `Float` in the next.
pub fn value_key(cell: &CellValue) -> Option<String> {
    Exact::of_cell(cell).map(|e| e.trimmed().text())
}

/// The narrowest `Decimal128(p, s)` that holds every number in a column, built
/// one cell at a time so the readers can type a column in the pass they
/// already make.
#[derive(Debug, Default, Clone, Copy)]
pub struct DecimalShape {
    whole: u32,
    scale: u32,
    numbers: usize,
    /// A number no `Decimal128` holds: NaN, infinity, more than 38 digits.
    too_wide: bool,
}

impl DecimalShape {
    /// Take one cell into the shape. `false` when the cell is no number, which
    /// the caller decides about (nulls are fine, text usually is not).
    pub fn add(&mut self, cell: &CellValue) -> bool {
        if !matches!(
            cell,
            CellValue::Int(_) | CellValue::Float(_) | CellValue::String(_)
        ) {
            return false;
        }
        match Exact::of_cell(cell) {
            Some(exact) => {
                let (whole, scale) = exact.width();
                self.whole = self.whole.max(whole);
                self.scale = self.scale.max(scale);
                self.numbers += 1;
                true
            }
            None if matches!(cell, CellValue::Float(_)) => {
                self.too_wide = true;
                true
            }
            None => false,
        }
    }

    /// `Decimal128(p, s)` for the numbers seen; `None` when there were none or
    /// one needs more than 38 digits.
    pub fn type_name(&self) -> Option<String> {
        let precision = self.whole + self.scale;
        (self.numbers > 0 && !self.too_wide && precision <= u32::from(MAX_PRECISION))
            .then(|| format!("Decimal128({precision}, {})", self.scale))
    }

    /// The shape of an existing `Decimal128(p, s)` type, to widen with new cells.
    pub fn of_type(data_type: &str) -> Option<DecimalShape> {
        let (precision, scale) = decimal_parts(data_type)?;
        let scale = u32::try_from(scale).ok()?;
        Some(DecimalShape {
            whole: u32::from(precision).saturating_sub(scale).max(1),
            scale,
            numbers: 1,
            too_wide: false,
        })
    }
}

/// Widen a `Decimal128` column's type so it holds `cells` too, for rows that
/// arrive after the type was set (a file read in chunks). Text cells are left
/// out; a column that would need more than 38 digits keeps its type, and a save
/// then refuses it in words rather than writing a wrong number.
pub fn widen_type<'a>(data_type: &mut String, cells: impl IntoIterator<Item = &'a CellValue>) {
    let Some(mut shape) = DecimalShape::of_type(data_type) else {
        return;
    };
    for cell in cells {
        shape.add(cell);
    }
    if let Some(wider) = shape.type_name() {
        *data_type = wider;
    }
}

/// Make the `Float64` columns of a table read from text exact decimals, the way
/// the CSV reader types its own (`column_type_from_cells`): the file wrote
/// digits, so they are summed and stored as digits. MessagePack and BSON carry
/// binary floats, so their floats stay floats. A column holding anything but
/// numbers, or needing more than 38 digits, is left alone. CSV and TSV are
/// skipped: their reader types exactly itself and knows which columns wrote an
/// exponent (scientific data, kept a float), which the cells no longer say.
pub fn type_text_columns(table: &mut super::DataTable) {
    const SKIPPED: &[&str] = &["MessagePack", "BSON", "CSV", "TSV"];
    let binary = table
        .format_name
        .as_deref()
        .is_some_and(|f| SKIPPED.contains(&f));
    if binary || !crate::data::times::is_text_format(table) {
        return;
    }
    for col in 0..table.columns.len() {
        if table.columns[col].data_type != "Float64" {
            continue;
        }
        let mut shape = DecimalShape::default();
        let all_numbers = table
            .rows
            .iter()
            .filter_map(|r| r.get(col))
            .all(|cell| matches!(cell, CellValue::Null) || shape.add(cell));
        if all_numbers && let Some(data_type) = shape.type_name() {
            table.columns[col].data_type = data_type;
        }
    }
}

/// `table`'s columns with each `Float64` that holds only numbers a decimal can
/// hold retyped as that decimal, so SQL sums it by the digits shown.
pub fn summing_columns(table: &super::DataTable) -> Vec<super::ColumnInfo> {
    let mut columns = table.columns.clone();
    for (col, column) in columns.iter_mut().enumerate() {
        if column.data_type != "Float64" {
            continue;
        }
        let mut shape = DecimalShape::default();
        let all_numbers = (0..table.row_count())
            .filter_map(|r| table.get(r, col))
            .all(|cell| matches!(cell, CellValue::Null) || shape.add(cell));
        if all_numbers && let Some(data_type) = shape.type_name() {
            column.data_type = data_type;
        }
    }
    columns
}

/// The type a database connector gives a DECIMAL / NUMERIC / NUMBER column,
/// until [`settle_server_decimals`] sizes it from the values. A real decimal
/// type, so a column that somehow misses the settling still holds numbers
/// exactly and a save still checks them; its width is only the fallback for a
/// result with no rows.
pub const SERVER_DECIMAL: &str = "Decimal128(38, 9)";

/// A server's decimal value, given as its text, as an exact cell: an Int when
/// whole, else [`number_cell`] (a Float, or the digits a float would round).
/// Scientific text (`1.5E+3`, which some servers print for large or small
/// values) is turned into plain digits, exactly. Anything else (`NaN`,
/// `Infinity`) stays the text it was.
pub fn server_decimal_cell(text: &str) -> CellValue {
    let t = text.trim();
    if let Ok(n) = t.parse::<i64>() {
        return CellValue::Int(n);
    }
    let Some(plain) = Exact::parse_scientific(t).map(Exact::text) else {
        return CellValue::String(text.to_string());
    };
    if let Ok(n) = plain.parse::<i64>() {
        return CellValue::Int(n);
    }
    number_cell(&plain).unwrap_or(CellValue::String(plain))
}

/// Size every [`SERVER_DECIMAL`] column of a query result from its values, the
/// way text files are typed: `Decimal128(p, s)` as wide as the widest value.
/// A column of whole numbers that all fit an i64 is `Int64`.
/// A column holding something no decimal holds (`NaN`, `Infinity`) becomes
/// text, every value as the server wrote it, so nothing is lost and a save
/// writes back what came.
pub fn settle_server_decimals(table: &mut super::DataTable) {
    for col in 0..table.columns.len() {
        if table.columns[col].data_type != SERVER_DECIMAL {
            continue;
        }
        let mut shape = DecimalShape::default();
        let all_numbers = table
            .rows
            .iter()
            .filter_map(|r| r.get(col))
            .all(|cell| matches!(cell, CellValue::Null) || shape.add(cell));
        let all_whole = table
            .rows
            .iter()
            .filter_map(|r| r.get(col))
            .all(|cell| matches!(cell, CellValue::Null | CellValue::Int(_)));
        if all_numbers && all_whole && shape.numbers > 0 {
            // Only whole numbers that fit an i64 (`SELECT 1`, a NUMBER(38, 0)
            // of ids): an integer column, as everywhere else in Octa.
            table.columns[col].data_type = "Int64".to_string();
        } else if !all_numbers {
            table.columns[col].data_type = "Utf8".to_string();
            for cell in table.rows.iter_mut().filter_map(|r| r.get_mut(col)) {
                if matches!(cell, CellValue::Int(_) | CellValue::Float(_)) {
                    *cell = CellValue::String(cell.to_string());
                }
            }
        } else if let Some(sized) = shape.type_name() {
            table.columns[col].data_type = sized;
        }
    }
}

/// Neumaier's compensated sum: the error stays at one rounding however many
/// values go in, where a plain float sum gathers one per value.
#[derive(Debug, Default, Clone, Copy)]
struct CarefulSum {
    sum: f64,
    carry: f64,
}

impl CarefulSum {
    fn add(&mut self, x: f64) {
        let t = self.sum + x;
        self.carry += if self.sum.abs() >= x.abs() {
            (self.sum - t) + x
        } else {
            (x - t) + self.sum
        };
        self.sum = t;
    }

    fn value(self) -> f64 {
        self.sum + self.carry
    }
}

/// A running sum of numbers, added exactly by the digits each one shows, so
/// 0.1 + 0.2 + 0.3 is 0.6. Past 38 digits, or for a value no decimal holds
/// (1e-300, NaN), it carries on as a careful float sum instead of failing.
#[derive(Debug, Clone, Copy)]
pub struct Sum {
    exact: Option<Exact>,
    float: CarefulSum,
    count: usize,
}

impl Default for Sum {
    fn default() -> Self {
        Sum {
            exact: Some(Exact { raw: 0, scale: 0 }),
            float: CarefulSum::default(),
            count: 0,
        }
    }
}

impl Sum {
    /// Add a cell if it holds a number; `false` (and nothing added) when not.
    pub fn add_cell(&mut self, cell: &CellValue) -> bool {
        let float = match cell {
            CellValue::Int(n) => *n as f64,
            CellValue::Float(f) => *f,
            CellValue::String(s) => match Exact::parse(s) {
                Some(_) => s.parse().unwrap_or(f64::NAN),
                None => return false,
            },
            _ => return false,
        };
        self.add(Exact::of_cell(cell), float);
        true
    }

    /// Add a float, by the digits it shows.
    pub fn add_f64(&mut self, value: f64) {
        self.add(Exact::of_f64(value), value);
    }

    /// Add one number given both ways: exactly (`None` when it has no exact
    /// form) and as the float the careful fallback adds.
    pub fn add(&mut self, exact: Option<Exact>, float: f64) {
        self.exact = self.exact.zip(exact).and_then(|(a, b)| a.checked_add(b));
        self.float.add(float);
        self.count += 1;
    }

    /// How many numbers went in.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The sum as an exact number, when it still is one.
    pub fn exact(&self) -> Option<Exact> {
        self.exact
    }

    /// The sum as a cell; `None` when no number went in.
    pub fn cell(&self) -> Option<CellValue> {
        (self.count > 0).then(|| match self.exact {
            Some(exact) => exact.to_cell(),
            None => CellValue::Float(self.float.value()),
        })
    }

    /// The sum as a float, for what has to be one.
    pub fn value(&self) -> f64 {
        match self.exact {
            Some(exact) => exact.to_f64(),
            None => self.float.value(),
        }
    }

    /// The mean as a cell; `None` when no number went in.
    pub fn mean_cell(&self) -> Option<CellValue> {
        (self.count > 0).then(|| match self.exact.and_then(|e| e.div_count(self.count)) {
            Some(mean) => mean.to_cell(),
            None => CellValue::Float(self.float.value() / self.count as f64),
        })
    }

    /// The mean as a float, the one nearest the exact quotient (13 / 3 is
    /// 4.333333333333333, as a float division of exact values gives); NaN when
    /// no number went in.
    pub fn mean(&self) -> f64 {
        let digits = u32::from(MAX_PRECISION);
        match self
            .exact
            .and_then(|e| e.div_count_with(self.count, digits))
        {
            Some(mean) => mean.to_f64(),
            None => self.float.value() / self.count as f64,
        }
    }
}

/// The sum of `values`, added by the digits each shows.
pub fn sum_f64(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut sum = Sum::default();
    values.into_iter().for_each(|v| sum.add_f64(v));
    sum.value()
}

/// The mean of `values`, added by the digits each shows; NaN for none.
pub fn mean_f64(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut sum = Sum::default();
    values.into_iter().for_each(|v| sum.add_f64(v));
    sum.mean()
}

#[cfg(test)]
#[path = "decimal_tests.rs"]
mod tests;
