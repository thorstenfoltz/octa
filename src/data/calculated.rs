//! Calculated columns: rank, dense rank, row number, running total, percent
//! of total, the change to the previous row, the previous or next row's value,
//! visits numbered from timestamps and ABC classes, each restarting for every
//! group. One engine for the dialog, `--calculate`, the MCP tool
//! `calculate_column` and the recipe step `calculated`. Pure, no egui.
//!
//! Empty cells (null or blank text): rank and row number put them last; a
//! running total adds nothing for them and carries on; a difference or a
//! percent change is empty when either side is; percent of total leaves them
//! empty. Percentages are percentages (12.5 means 12.5 %).

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::decimal::{Exact, Sum};
use super::{CellValue, DataTable, cmp_cell_values};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CalcKind {
    #[default]
    Rank,
    DenseRank,
    RowNumber,
    RunningTotal,
    PercentOfTotal,
    DiffPrev,
    PctChangePrev,
    /// The value `offset` rows before, in the row order.
    Previous,
    /// The value `offset` rows after.
    Next,
    /// Visits numbered 1, 2, 3: a new one starts after a pause longer than
    /// the gap. The value column holds the timestamps.
    Session,
    /// A, B or C by the running share of the total, largest value first.
    Abc,
}

impl CalcKind {
    pub const ALL: [CalcKind; 11] = [
        CalcKind::Rank,
        CalcKind::DenseRank,
        CalcKind::RowNumber,
        CalcKind::RunningTotal,
        CalcKind::PercentOfTotal,
        CalcKind::DiffPrev,
        CalcKind::PctChangePrev,
        CalcKind::Previous,
        CalcKind::Next,
        CalcKind::Session,
        CalcKind::Abc,
    ];

    /// The CLI / MCP / recipe spelling.
    pub fn id(self) -> &'static str {
        match self {
            CalcKind::Rank => "rank",
            CalcKind::DenseRank => "dense_rank",
            CalcKind::RowNumber => "row_number",
            CalcKind::RunningTotal => "running_total",
            CalcKind::PercentOfTotal => "percent_of_total",
            CalcKind::DiffPrev => "diff_prev",
            CalcKind::PctChangePrev => "pct_change_prev",
            CalcKind::Previous => "previous",
            CalcKind::Next => "next",
            CalcKind::Session => "session",
            CalcKind::Abc => "abc",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|k| k.id() == s)
    }

    pub fn label_key(self) -> String {
        format!("calc.kind_{}", self.id())
    }

    pub fn hint_key(self) -> String {
        format!("calc.kind_{}_hint", self.id())
    }

    pub fn needs_value(self) -> bool {
        self != CalcKind::RowNumber
    }

    /// Follows a row order (an order column, or the table's own). Rank and
    /// Dense rank order by the value itself; Percent of total has no order.
    pub fn uses_order(self) -> bool {
        matches!(
            self,
            CalcKind::RowNumber
                | CalcKind::RunningTotal
                | CalcKind::DiffPrev
                | CalcKind::PctChangePrev
                | CalcKind::Previous
                | CalcKind::Next
        )
    }

    pub fn whole_numbers(self) -> bool {
        matches!(
            self,
            CalcKind::Rank | CalcKind::DenseRank | CalcKind::RowNumber | CalcKind::Session
        )
    }

    /// Does arithmetic on the value, so it must hold numbers.
    pub fn needs_numbers(self) -> bool {
        matches!(
            self,
            CalcKind::RunningTotal
                | CalcKind::PercentOfTotal
                | CalcKind::DiffPrev
                | CalcKind::PctChangePrev
                | CalcKind::Abc
        )
    }

    /// The new column's type. Previous and Next take their value column's
    /// instead, see [`result_type`].
    pub fn data_type(self) -> &'static str {
        if self.whole_numbers() {
            "Int64"
        } else if self == CalcKind::Abc {
            "Utf8"
        } else {
            "Float64"
        }
    }

    /// Rank's usual meaning: the largest value gets 1.
    pub fn default_descending(self) -> bool {
        matches!(self, CalcKind::Rank | CalcKind::DenseRank)
    }
}

/// The settings only some kinds read. Every one has a default, so a recipe,
/// a CLI call or an MCP call names only what it changes.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CalcExtras {
    /// previous / next: how many rows away. Default 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    /// session: a pause longer than this many minutes starts a new visit.
    /// Default 30.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_minutes: Option<f64>,
    /// abc: class A holds the rows up to this running share of the total, in
    /// percent. Default 80.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abc_a: Option<f64>,
    /// abc: class B ends at this running share, the rest is C. Default 95.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abc_b: Option<f64>,
}

impl CalcExtras {
    pub const OFFSET: usize = 1;
    pub const GAP_MINUTES: f64 = 30.0;
    /// The classic 80 / 15 / 5 split.
    pub const ABC: (f64, f64) = (80.0, 95.0);

    pub fn offset(&self) -> usize {
        self.offset.unwrap_or(Self::OFFSET)
    }

    pub fn gap_secs(&self) -> f64 {
        self.gap_minutes.unwrap_or(Self::GAP_MINUTES) * 60.0
    }

    pub fn abc(&self) -> (f64, f64) {
        (
            self.abc_a.unwrap_or(Self::ABC.0),
            self.abc_b.unwrap_or(Self::ABC.1),
        )
    }

    /// Why `kind` cannot run with these settings.
    pub fn check(&self, kind: CalcKind) -> Result<(), String> {
        match kind {
            CalcKind::Previous | CalcKind::Next if self.offset() == 0 => {
                Err("the number of rows away must be 1 or more".into())
            }
            CalcKind::Session if self.gap_secs().is_nan() || self.gap_secs() < 0.0 => {
                Err("the gap must be 0 minutes or more".into())
            }
            CalcKind::Abc => {
                let (a, b) = self.abc();
                if a > 0.0 && a < b && b <= 100.0 {
                    Ok(())
                } else {
                    Err(format!(
                        "the ABC limits must rise and lie between 0 and 100, got {a} and {b}"
                    ))
                }
            }
            _ => Ok(()),
        }
    }
}

/// What to calculate, by column index.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CalcSpec {
    pub kind: CalcKind,
    /// The column the kind works on; ignored by Row number.
    pub value: Option<usize>,
    /// The row order for the kinds that follow one; `None` is the table's
    /// own order.
    pub order: Option<usize>,
    /// Largest (or latest) first.
    pub descending: bool,
    /// The calculation starts again for each combination of these columns.
    pub groups: Vec<usize>,
    pub extras: CalcExtras,
}

/// The same by column name, as the CLI, the MCP tool and a recipe give it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CalcByName {
    pub kind: CalcKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    /// `None`: the kind's default (largest first for the ranks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descending: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(flatten)]
    pub extras: CalcExtras,
}

impl CalcByName {
    pub fn resolve(&self, table: &DataTable) -> anyhow::Result<CalcSpec> {
        if self.kind.needs_value() && self.value.is_none() {
            anyhow::bail!("{} needs a value column", self.kind.id());
        }
        let idx = |n: &String| table.column_index(n);
        Ok(CalcSpec {
            kind: self.kind,
            value: self.value.as_ref().map(idx).transpose()?,
            order: self.order.as_ref().map(idx).transpose()?,
            descending: self.descending.unwrap_or(self.kind.default_descending()),
            groups: self.groups.iter().map(idx).collect::<anyhow::Result<_>>()?,
            extras: self.extras,
        })
    }

    pub fn of(spec: &CalcSpec, table: &DataTable) -> Self {
        let name = |c: usize| table.columns.get(c).map(|ci| ci.name.clone());
        Self {
            kind: spec.kind,
            value: spec.value.and_then(name),
            order: spec.order.and_then(name),
            descending: Some(spec.descending),
            groups: spec.groups.iter().filter_map(|&c| name(c)).collect(),
            extras: spec.extras,
        }
    }

    /// Every column the spec names, for a recipe's missing-column check.
    pub fn columns(&self) -> Vec<String> {
        self.value
            .iter()
            .chain(self.order.iter())
            .chain(self.groups.iter())
            .cloned()
            .collect()
    }
}

static NULL: CellValue = CellValue::Null;

fn cell(t: &DataTable, r: usize, c: usize) -> &CellValue {
    t.get(r, c).unwrap_or(&NULL)
}

fn is_empty(v: &CellValue) -> bool {
    match v {
        CellValue::Null => true,
        CellValue::String(s) => s.trim().is_empty(),
        _ => false,
    }
}

/// A value cell held exactly, text with spaces round it included.
fn exact_number(v: &CellValue) -> Option<Exact> {
    match v {
        CellValue::String(s) => Exact::parse(s.trim()),
        other => Exact::of_cell(other),
    }
}

/// A result cell the way this column always typed them: an exact whole sum
/// comes back an Int, and the column holds Floats.
fn float_cell(cell: CellValue) -> CellValue {
    match cell {
        CellValue::Int(n) => CellValue::Float(n as f64),
        other => other,
    }
}

fn number(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Int(i) => Some(*i as f64),
        CellValue::Float(f) if f.is_finite() => Some(*f),
        CellValue::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

/// A moment as seconds: a date is its midnight, text is read as a date or a
/// timestamp, a plain number counts as seconds already.
pub(crate) fn seconds(v: &CellValue) -> Option<f64> {
    use super::chart::cell_to_f64;
    match v {
        CellValue::Date(_) => cell_to_f64(v).map(|d| d * 86_400.0),
        CellValue::String(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .or_else(|| cell_to_f64(&CellValue::DateTime(s.clone()))),
        other => cell_to_f64(other),
    }
    .filter(|s| s.is_finite())
}

/// Orders rows on column `col`: as numbers when every filled cell reads as
/// one (text "10" after "9"), else by the table's cell order. Empty cells
/// last in either direction. Callers sort stably, so ties keep table order.
struct Sorter<'a> {
    t: &'a DataTable,
    col: usize,
    nums: Option<Vec<Option<f64>>>,
}

impl<'a> Sorter<'a> {
    fn new(t: &'a DataTable, col: usize) -> Self {
        let n = t.row_count();
        let nums: Vec<Option<f64>> = (0..n).map(|r| number(cell(t, r, col))).collect();
        let all = (0..n).all(|r| nums[r].is_some() || is_empty(cell(t, r, col)));
        Self {
            t,
            col,
            nums: all.then_some(nums),
        }
    }

    fn cmp(&self, a: usize, b: usize, desc: bool) -> Ordering {
        let (ea, eb) = (
            is_empty(cell(self.t, a, self.col)),
            is_empty(cell(self.t, b, self.col)),
        );
        match (ea, eb) {
            (true, true) => return Ordering::Equal,
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            _ => {}
        }
        let o = match &self.nums {
            Some(n) => n[a].partial_cmp(&n[b]).unwrap_or(Ordering::Equal),
            None => cmp_cell_values(cell(self.t, a, self.col), cell(self.t, b, self.col)),
        };
        if desc { o.reverse() } else { o }
    }
}

/// Row indices per group, groups in first-seen order, rows in table order.
fn groups(t: &DataTable, cols: &[usize]) -> Vec<Vec<usize>> {
    let mut at: HashMap<Vec<String>, usize> = HashMap::new();
    let mut out: Vec<Vec<usize>> = Vec::new();
    for r in 0..t.row_count() {
        let key: Vec<String> = cols.iter().map(|&c| cell(t, r, c).to_string()).collect();
        let i = *at.entry(key).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[i].push(r);
    }
    out
}

/// The new column's values, one per table row.
pub fn compute(t: &DataTable, spec: &CalcSpec) -> Result<Vec<CellValue>, String> {
    let n = t.row_count();
    spec.extras.check(spec.kind)?;
    let value = match (spec.kind.needs_value(), spec.value) {
        (true, None) => return Err(format!("{} needs a value column", spec.kind.id())),
        (true, Some(c)) if c >= t.col_count() => return Err("the value column is gone".into()),
        (_, v) => v,
    };
    let nums: Vec<Option<f64>> = match value {
        Some(c) => (0..n).map(|r| number(cell(t, r, c))).collect(),
        None => vec![None; n],
    };
    // The same values held exactly, for the kinds that add and subtract: a
    // running total of 0.1, 0.2, 0.3 ends on 0.6, not 0.6000000000000001.
    let exact: Vec<Option<Exact>> = match value {
        Some(c) => (0..n).map(|r| exact_number(cell(t, r, c))).collect(),
        None => vec![None; n],
    };
    if spec.kind.needs_numbers()
        && let Some(c) = value
        && let Some(r) = (0..n).find(|&r| nums[r].is_none() && !is_empty(cell(t, r, c)))
    {
        return Err(format!(
            "{} holds values that are not numbers, for example '{}'",
            t.columns[c].name,
            cell(t, r, c)
        ));
    }
    if spec.kind == CalcKind::Abc
        && let Some(c) = value
        && let Some(r) = (0..n).find(|&r| nums[r].is_some_and(|v| v < 0.0))
    {
        return Err(format!(
            "{} holds negative values, for example '{}'; a share of a total needs values of 0 or more",
            t.columns[c].name,
            cell(t, r, c)
        ));
    }
    let secs: Vec<Option<f64>> = match value.filter(|_| spec.kind == CalcKind::Session) {
        Some(c) => {
            let secs: Vec<Option<f64>> = (0..n).map(|r| seconds(cell(t, r, c))).collect();
            if let Some(r) = (0..n).find(|&r| secs[r].is_none() && !is_empty(cell(t, r, c))) {
                return Err(format!(
                    "{} holds values that are not dates or times, for example '{}'",
                    t.columns[c].name,
                    cell(t, r, c)
                ));
            }
            secs
        }
        None => Vec::new(),
    };
    let order = spec
        .order
        .filter(|&c| c < t.col_count())
        .map(|c| Sorter::new(t, c));
    let ranked = value.map(|c| Sorter::new(t, c));
    let mut out = vec![CellValue::Null; n];
    for mut rows in groups(t, &spec.groups) {
        match spec.kind {
            CalcKind::Rank | CalcKind::DenseRank => {
                let s = ranked.as_ref().expect("value checked above");
                rows.sort_by(|&a, &b| s.cmp(a, b, spec.descending));
                let (mut place, mut dense) = (0usize, 0usize);
                for (i, &r) in rows.iter().enumerate() {
                    let tie = i > 0 && s.cmp(rows[i - 1], r, spec.descending) == Ordering::Equal;
                    if !tie {
                        place = i + 1;
                        dense += 1;
                    }
                    let v = if spec.kind == CalcKind::Rank {
                        place
                    } else {
                        dense
                    };
                    out[r] = CellValue::Int(v as i64);
                }
            }
            CalcKind::PercentOfTotal => {
                let mut sum = Sum::default();
                rows.iter()
                    .filter_map(|&r| nums[r])
                    .for_each(|v| sum.add_f64(v));
                let total = sum.value();
                for &r in &rows {
                    out[r] = match nums[r] {
                        Some(v) if total != 0.0 => CellValue::Float(v / total * 100.0),
                        _ => CellValue::Null,
                    };
                }
            }
            CalcKind::Abc => {
                let (a, b) = spec.extras.abc();
                let mut sum = Sum::default();
                rows.iter()
                    .filter_map(|&r| nums[r])
                    .for_each(|v| sum.add_f64(v));
                let total = sum.value();
                rows.retain(|&r| nums[r].is_some());
                rows.sort_by(|&x, &y| nums[y].partial_cmp(&nums[x]).unwrap_or(Ordering::Equal));
                // The share BEFORE the row decides, so the row that crosses a
                // limit still belongs to the class it started in and the
                // largest value is always an A.
                let mut before = Sum::default();
                for &r in &rows {
                    if total > 0.0 {
                        let share = before.value() / total * 100.0;
                        let class = if share < a {
                            "A"
                        } else if share < b {
                            "B"
                        } else {
                            "C"
                        };
                        out[r] = CellValue::String(class.to_string());
                    }
                    before.add_f64(nums[r].unwrap_or(0.0));
                }
            }
            CalcKind::Session => {
                let gap = spec.extras.gap_secs();
                rows.retain(|&r| secs[r].is_some());
                rows.sort_by(|&x, &y| secs[x].partial_cmp(&secs[y]).unwrap_or(Ordering::Equal));
                let (mut visit, mut prev) = (0i64, None::<f64>);
                for &r in &rows {
                    let s = secs[r].unwrap_or(0.0);
                    if prev.is_none_or(|p| s - p > gap) {
                        visit += 1;
                    }
                    prev = Some(s);
                    out[r] = CellValue::Int(visit);
                }
            }
            _ => {
                if let Some(s) = &order {
                    rows.sort_by(|&a, &b| s.cmp(a, b, spec.descending));
                } else if spec.descending {
                    rows.reverse();
                }
                if let (CalcKind::Previous | CalcKind::Next, Some(c)) = (spec.kind, value) {
                    let k = spec.extras.offset();
                    for (i, &r) in rows.iter().enumerate() {
                        let from = if spec.kind == CalcKind::Previous {
                            i.checked_sub(k)
                        } else {
                            i.checked_add(k).filter(|&j| j < rows.len())
                        };
                        if let Some(j) = from {
                            out[r] = cell(t, rows[j], c).clone();
                        }
                    }
                    continue;
                }
                let mut total: Option<Sum> = None;
                let mut prev: Option<Option<f64>> = None;
                let mut prev_exact: Option<Exact> = None;
                for (i, &r) in rows.iter().enumerate() {
                    out[r] = match spec.kind {
                        CalcKind::RowNumber => CellValue::Int(i as i64 + 1),
                        CalcKind::RunningTotal => {
                            if let Some(v) = nums[r] {
                                total.get_or_insert_with(Sum::default).add(exact[r], v);
                            }
                            total
                                .and_then(|sum| sum.cell())
                                .map_or(CellValue::Null, float_cell)
                        }
                        CalcKind::DiffPrev => match (prev.flatten(), nums[r]) {
                            (Some(p), Some(c)) => prev_exact
                                .zip(exact[r])
                                .and_then(|(p, c)| c.checked_sub(p))
                                .map_or(CellValue::Float(c - p), |d| float_cell(d.to_cell())),
                            _ => CellValue::Null,
                        },
                        CalcKind::PctChangePrev => match (prev.flatten(), nums[r]) {
                            (Some(p), Some(c)) if p != 0.0 => CellValue::Float((c - p) / p * 100.0),
                            _ => CellValue::Null,
                        },
                        _ => unreachable!("handled above"),
                    };
                    prev = Some(nums[r]);
                    prev_exact = exact[r];
                }
            }
        }
    }
    Ok(out)
}

/// `<value>_<kind>`, or `row_number`, made unique against the table.
pub fn default_name(t: &DataTable, spec: &CalcSpec) -> String {
    let base = match spec.value.and_then(|c| t.columns.get(c)) {
        Some(ci) if spec.kind.needs_value() => format!("{}_{}", ci.name, spec.kind.id()),
        _ => spec.kind.id().to_string(),
    };
    super::unique_name(&base, |n| t.columns.iter().any(|c| c.name == n))
}

/// The new column's type: Previous and Next hand a value on unchanged, so
/// they keep their value column's type.
pub fn result_type(t: &DataTable, spec: &CalcSpec) -> String {
    match (spec.kind, spec.value.and_then(|c| t.columns.get(c))) {
        (CalcKind::Previous | CalcKind::Next, Some(ci)) => ci.data_type.clone(),
        _ => spec.kind.data_type().to_string(),
    }
}

/// Right of the value column; Row number at the left edge.
pub fn insert_index(spec: &CalcSpec) -> usize {
    match spec.value {
        Some(c) if spec.kind.needs_value() => c + 1,
        _ => 0,
    }
}

/// Insert the column through the undoable `insert_column` + `set`, so the
/// GUI folds it into one undo step with `coalesce_undo_since`. A name already
/// taken gets `_2`.
pub fn add_calculated_column(
    t: &mut DataTable,
    spec: &CalcSpec,
    name: Option<&str>,
) -> anyhow::Result<(String, usize)> {
    let values = compute(t, spec).map_err(anyhow::Error::msg)?;
    // A name you chose is yours or refused: a script that asks for `total` and quietly
    // gets `total_2` reads the wrong column next. Only a default gets a number.
    let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) if t.columns.iter().any(|c| c.name == n) => {
            anyhow::bail!("a column named {n} already exists; pick another name")
        }
        Some(n) => n.to_string(),
        None => default_name(t, spec),
    };
    let idx = insert_index(spec).min(t.col_count());
    let data_type = result_type(t, spec);
    t.insert_column(idx, name.clone(), data_type);
    for (r, v) in values.into_iter().enumerate() {
        t.set(r, idx, v);
    }
    Ok((name, idx))
}

#[cfg(test)]
#[path = "calculated_tests.rs"]
mod tests;
