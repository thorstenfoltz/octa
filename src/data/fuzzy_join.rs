//! Joining on "similar to" rather than "equals".
//!
//! Two tables describing the same real-world things with no shared key: the
//! CRM says `Mueller GmbH`, the sales sheet says `Mueller Gmbh.`, and an exact
//! join matches neither. `join.rs` compares with `=`, `<`, `<=`, `>` and `>=`
//! and nothing else, so this is a separate engine.
//!
//! It writes **no new similarity maths**: the measures, the normalisation and
//! the blocking idea all come from [`crate::data::fuzzy_duplicates`], which
//! already needed exactly these for near-duplicates within one table.
//!
//! Ceilings, all documented rather than guarded: values are compared as
//! normalised text; each left row keeps one partner; without a blocking column
//! the comparison is quadratic and `max_rows` applies per side; folding left to
//! right means a bad match early propagates, which is why the score and the
//! ambiguity flag are recorded per step rather than collapsed into one number.
//!
//! `Right` and `Full` emit the unmatched right rows **after** the left-driven
//! rows rather than interleaved, so the left side keeps its original order.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::data::fuzzy_duplicates::{NormalizeOpts, SimilarityMethod, normalize, similarity};
use crate::data::join::JoinType;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// How much worse than the winner a runner-up may be and still count as
/// "nearly as good". Fixed rather than configurable: it exists to draw
/// attention, and a knob would only invite tuning it until nothing is flagged.
pub const AMBIGUITY_MARGIN: f64 = 0.05;

/// One join in the fold.
#[derive(Debug, Clone)]
pub struct FuzzyJoinStep {
    /// Column pairs whose scores are averaged: `(left index, right index)`.
    pub pairs: Vec<(usize, usize)>,
    pub method: SimilarityMethod,
    /// Average similarity at or above which a pair is a candidate.
    pub threshold: f64,
    pub normalize: NormalizeOpts,
    /// Exact-match blocking columns, `(left index, right index)`. Only rows
    /// sharing a block value are compared. `None` means brute force under
    /// `max_rows`.
    pub block: Option<(usize, usize)>,
    pub how: JoinType,
    /// Rows considered per side. Matches `FuzzyDupConfig`'s default.
    pub max_rows: usize,
}

impl Default for FuzzyJoinStep {
    fn default() -> Self {
        Self {
            pairs: Vec::new(),
            method: SimilarityMethod::default(),
            threshold: 0.85,
            normalize: NormalizeOpts::default(),
            block: None,
            how: JoinType::Left,
            max_rows: 20_000,
        }
    }
}

/// What one step actually did, so every surface can report it honestly.
#[derive(Debug, Clone, PartialEq)]
pub struct StepReport {
    pub left_rows: usize,
    pub right_rows: usize,
    pub matched: usize,
    pub ambiguous: usize,
    /// True when either side was cut to `max_rows`.
    pub capped: bool,
}

#[derive(Debug, Clone)]
pub struct FuzzyJoinResult {
    pub table: DataTable,
    pub steps: Vec<StepReport>,
}

fn cell_text(t: &DataTable, row: usize, col: usize, opts: &NormalizeOpts) -> String {
    let raw = t.get(row, col).map(|c| c.to_string()).unwrap_or_default();
    normalize(&raw, opts)
}

/// Average similarity of one candidate pair across the step's column pairs.
fn pair_score(
    left: &DataTable,
    right: &DataTable,
    l: usize,
    r: usize,
    step: &FuzzyJoinStep,
) -> f64 {
    if step.pairs.is_empty() {
        return 0.0;
    }
    let sum: f64 = step
        .pairs
        .iter()
        .map(|(lc, rc)| {
            let a = cell_text(left, l, *lc, &step.normalize);
            let b = cell_text(right, r, *rc, &step.normalize);
            similarity(step.method, &a, &b)
        })
        .sum();
    sum / step.pairs.len() as f64
}

/// Unique output column name, so a second table carrying `id` does not collide.
fn unique_name(taken: &[String], want: &str) -> String {
    if !taken.iter().any(|t| t == want) {
        return want.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{want}_{n}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Join a list of tables, folding left to right: A to B, then that result to C.
///
/// `steps.len()` must be `tables.len() - 1`.
pub fn fuzzy_join(
    tables: &[&DataTable],
    steps: &[FuzzyJoinStep],
    cancel: &AtomicBool,
) -> anyhow::Result<FuzzyJoinResult> {
    if tables.len() < 2 {
        anyhow::bail!("a fuzzy join needs at least two tables");
    }
    if steps.len() != tables.len() - 1 {
        anyhow::bail!(
            "expected {} step(s) for {} tables, got {}",
            tables.len() - 1,
            tables.len(),
            steps.len()
        );
    }

    let mut acc = tables[0].clone();
    let mut reports = Vec::new();

    for (i, step) in steps.iter().enumerate() {
        let (next, report) = join_once(&acc, tables[i + 1], step, i + 1, cancel)?;
        acc = next;
        reports.push(report);
    }

    Ok(FuzzyJoinResult {
        table: acc,
        steps: reports,
    })
}

/// One step of the fold. `n` numbers the score and flag columns.
fn join_once(
    left: &DataTable,
    right: &DataTable,
    step: &FuzzyJoinStep,
    n: usize,
    cancel: &AtomicBool,
) -> anyhow::Result<(DataTable, StepReport)> {
    let left_rows = left.row_count().min(step.max_rows);
    let right_rows = right.row_count().min(step.max_rows);
    let capped = left.row_count() > step.max_rows || right.row_count() > step.max_rows;

    // Candidate right rows per left row. With a blocking column only rows
    // sharing an exact block value are compared, which is what makes a large
    // pair feasible at all.
    let blocks: Option<std::collections::HashMap<String, Vec<usize>>> =
        step.block.map(|(_, rc)| {
            let mut map: std::collections::HashMap<String, Vec<usize>> =
                std::collections::HashMap::new();
            for r in 0..right_rows {
                let key = right.get(r, rc).map(|c| c.to_string()).unwrap_or_default();
                map.entry(key).or_default().push(r);
            }
            map
        });

    let mut best: Vec<Option<(usize, f64, bool)>> = Vec::with_capacity(left_rows);
    let mut matched = 0usize;
    let mut ambiguous_count = 0usize;

    for l in 0..left_rows {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("cancelled");
        }

        let candidates: Vec<usize> = match (&blocks, step.block) {
            (Some(map), Some((lc, _))) => {
                let key = left.get(l, lc).map(|c| c.to_string()).unwrap_or_default();
                map.get(&key).cloned().unwrap_or_default()
            }
            _ => (0..right_rows).collect(),
        };

        let mut top: Option<(usize, f64)> = None;
        let mut runner: f64 = 0.0;
        for r in candidates {
            let score = pair_score(left, right, l, r, step);
            if score < step.threshold {
                continue;
            }
            match top {
                // Strictly greater keeps the first occurrence on a tie: an
                // equal score never displaces the row that got there first.
                Some((_, best_score)) if score > best_score => {
                    runner = best_score;
                    top = Some((r, score));
                }
                // Not a new winner, but possibly a new runner-up, which is
                // what the ambiguity flag is computed from.
                Some(_) => {
                    if score > runner {
                        runner = score;
                    }
                }
                None => top = Some((r, score)),
            }
        }

        match top {
            Some((r, score)) => {
                let close = runner > 0.0 && score - runner <= AMBIGUITY_MARGIN;
                if close {
                    ambiguous_count += 1;
                }
                matched += 1;
                best.push(Some((r, score, close)));
            }
            None => best.push(None),
        }
    }

    let out = assemble(left, right, &best, step.how, right_rows, n);

    Ok((
        out,
        StepReport {
            left_rows,
            right_rows,
            matched,
            ambiguous: ambiguous_count,
            capped,
        },
    ))
}

/// Build the output table: left columns, right columns, then this step's score
/// and ambiguity flag.
fn assemble(
    left: &DataTable,
    right: &DataTable,
    best: &[Option<(usize, f64, bool)>],
    how: JoinType,
    right_rows: usize,
    n: usize,
) -> DataTable {
    // Which side's unmatched rows survive. Derived here rather than passed in,
    // so the two rules live next to the code that applies them.
    let keep_left = matches!(how, JoinType::Left | JoinType::Full);
    let keep_right = matches!(how, JoinType::Right | JoinType::Full);
    let mut out = DataTable::empty();
    let mut names: Vec<String> = left.columns.iter().map(|c| c.name.clone()).collect();

    out.columns = left.columns.clone();
    for c in &right.columns {
        let name = unique_name(&names, &c.name);
        names.push(name.clone());
        out.columns.push(ColumnInfo {
            name,
            data_type: c.data_type.clone(),
        });
    }
    let score_name = unique_name(&names, &format!("match_score_{n}"));
    names.push(score_name.clone());
    out.columns.push(ColumnInfo {
        name: score_name,
        data_type: "Float64".into(),
    });
    let flag_name = unique_name(&names, &format!("ambiguous_{n}"));
    out.columns.push(ColumnInfo {
        name: flag_name,
        data_type: "Boolean".into(),
    });

    for (l, slot) in best.iter().enumerate() {
        if slot.is_none() && !keep_left {
            continue;
        }
        let mut row: Vec<CellValue> = (0..left.col_count())
            .map(|c| left.get(l, c).cloned().unwrap_or(CellValue::Null))
            .collect();

        match slot {
            Some((r, score, close)) => {
                for c in 0..right.col_count() {
                    row.push(right.get(*r, c).cloned().unwrap_or(CellValue::Null));
                }
                row.push(CellValue::Float(*score));
                row.push(CellValue::Bool(*close));
            }
            None => {
                for _ in 0..right.col_count() {
                    row.push(CellValue::Null);
                }
                row.push(CellValue::Null);
                row.push(CellValue::Bool(false));
            }
        }
        out.rows.push(row);
    }

    // Right / Full: the right rows nothing matched, appended after the
    // left-driven rows rather than interleaved, which keeps the left order
    // readable.
    if keep_right {
        let taken: std::collections::HashSet<usize> =
            best.iter().flatten().map(|(r, _, _)| *r).collect();
        for r in 0..right_rows {
            if taken.contains(&r) {
                continue;
            }
            let mut row: Vec<CellValue> = vec![CellValue::Null; left.col_count()];
            for c in 0..right.col_count() {
                row.push(right.get(r, c).cloned().unwrap_or(CellValue::Null));
            }
            row.push(CellValue::Null);
            row.push(CellValue::Bool(false));
            out.rows.push(row);
        }
    }

    out.structural_changes = true;
    out
}

#[cfg(test)]
#[path = "fuzzy_join_tests.rs"]
mod tests;
