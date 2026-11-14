//! Classify with Jev: label every row through TypeSafe AI's decision model.
//!
//! Jev answers typed questions about a piece of state: pick one of the
//! categories you list (`choice`), a yes/no probability (`noul`), or a grade on
//! an ordered scale (`score`). It writes no text, so this is not an assistant
//! feature. The chosen cells of a row are the state, the dialog's question and
//! lists are the question, and the answer plus its certainty become two new
//! columns.
//!
//! Each distinct state is sent once and its answer copied to every row holding
//! it: fewer requests, a smaller bill, less data leaving the machine.
//!
//! This file knows the wire format and nothing about HTTP. The request goes
//! through a `send` closure (`octa::api::jev` supplies the real one), so the
//! dialog, the CLI action and the MCP tool share one engine and the tests need
//! no network.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::data::{CellValue, ColumnInfo, DataTable};

/// TypeSafe's documented limits on a question's criteria.
pub const MAX_CATEGORIES: usize = 255;
pub const MAX_LEVELS: usize = 10;

/// Every request carries one question; this is its key in `questions` and
/// `answers`.
const QUESTION: &str = "q";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ClassifyMode {
    /// One of the listed categories (`choice`).
    #[default]
    Categories,
    /// A yes/no question (`noul`).
    YesNo,
    /// A grade on an ordered scale (`score`).
    Scale,
}

impl ClassifyMode {
    pub const ALL: [ClassifyMode; 3] = [Self::Categories, Self::YesNo, Self::Scale];

    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Categories => "classify.mode_categories",
            Self::YesNo => "classify.mode_yes_no",
            Self::Scale => "classify.mode_scale",
        }
    }

    /// `categories` / `yesno` / `scale`, as the CLI spells it.
    pub fn parse(s: &str) -> Option<Self> {
        match s
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_'], "")
            .as_str()
        {
            "categories" | "choice" => Some(Self::Categories),
            "yesno" | "noul" => Some(Self::YesNo),
            "scale" | "score" => Some(Self::Scale),
            _ => None,
        }
    }

    /// Suffix of the second column, which holds how sure the answer is.
    fn certainty_suffix(self) -> &'static str {
        match self {
            Self::YesNo => "_probability",
            Self::Categories | Self::Scale => "_confidence",
        }
    }

    fn answer_type(self) -> &'static str {
        match self {
            Self::Categories => "Utf8",
            Self::YesNo => "Boolean",
            Self::Scale => "Float64",
        }
    }
}

/// One category the model may pick. The description tells it what belongs
/// there; an empty one falls back to the name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Category {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClassifySpec {
    /// Column indices whose cells form the state of a row.
    pub columns: Vec<usize>,
    pub mode: ClassifyMode,
    /// The question, in plain words.
    pub instructions: String,
    /// `Categories` only.
    pub categories: Vec<Category>,
    /// `Scale` only: lowest level first.
    pub levels: Vec<String>,
}

impl ClassifySpec {
    /// Why this spec cannot be sent, as an i18n key, or `None` when it can.
    /// The dialog greys Run with it; the CLI and MCP print its English text.
    pub fn problem(&self) -> Option<&'static str> {
        if self.columns.is_empty() {
            return Some("classify.need_columns");
        }
        if self.instructions.trim().is_empty() {
            return Some("classify.need_question");
        }
        match self.mode {
            ClassifyMode::YesNo => None,
            ClassifyMode::Categories => {
                let names: Vec<&str> = self.categories.iter().map(|c| c.name.trim()).collect();
                if names.len() < 2 || names.iter().any(|n| n.is_empty()) {
                    Some("classify.need_categories")
                } else if names.len() > MAX_CATEGORIES {
                    Some("classify.too_many_categories")
                } else if (1..names.len()).any(|i| names[..i].contains(&names[i])) {
                    Some("classify.duplicate_category")
                } else {
                    None
                }
            }
            ClassifyMode::Scale => {
                if self.levels.len() < 2 || self.levels.iter().any(|l| l.trim().is_empty()) {
                    Some("classify.need_levels")
                } else if self.levels.len() > MAX_LEVELS {
                    Some("classify.too_many_levels")
                } else {
                    None
                }
            }
        }
    }
}

/// The state of one row: `{column name: cell text}`, NULL cells as JSON null.
/// `None` when every chosen cell is empty, since there is nothing to judge
/// and nothing worth sending.
pub fn row_state(table: &DataTable, row: usize, spec: &ClassifySpec) -> Option<Value> {
    let mut state = Map::new();
    let mut any = false;
    for &c in &spec.columns {
        let name = table.columns.get(c).map(|x| x.name.clone())?;
        let value = match table.get(row, c) {
            None | Some(CellValue::Null) => Value::Null,
            Some(v) => {
                let text = v.to_string();
                any |= !text.trim().is_empty();
                Value::String(text)
            }
        };
        state.insert(name, value);
    }
    any.then_some(Value::Object(state))
}

/// The distinct states of a table, in first-seen order, and for every row the
/// index of its state (`None` for a row with nothing to send).
pub fn distinct_states(table: &DataTable, spec: &ClassifySpec) -> (Vec<Value>, Vec<Option<usize>>) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut states = Vec::new();
    let rows = (0..table.row_count())
        .map(|r| {
            let state = row_state(table, r, spec)?;
            let key = state.to_string();
            Some(*seen.entry(key).or_insert_with(|| {
                states.push(state);
                states.len() - 1
            }))
        })
        .collect();
    (states, rows)
}

/// The body of `POST /v1/systemone` for one state.
pub fn request_body(model: &str, state: &Value, spec: &ClassifySpec) -> Value {
    let instructions = spec.instructions.trim();
    let question = match spec.mode {
        ClassifyMode::YesNo => json!({"type": "noul", "instructions": instructions}),
        ClassifyMode::Categories => {
            let criteria: Map<String, Value> = spec
                .categories
                .iter()
                .map(|c| {
                    let name = c.name.trim();
                    let text = match c.description.trim() {
                        "" => name,
                        d => d,
                    };
                    (name.to_string(), Value::String(text.to_string()))
                })
                .collect();
            json!({"type": "choice", "instructions": instructions, "criteria": criteria})
        }
        ClassifyMode::Scale => {
            let criteria: Vec<&str> = spec.levels.iter().map(|l| l.trim()).collect();
            json!({"type": "score", "instructions": instructions, "criteria": criteria})
        }
    };
    json!({"model": model, "state": state, "questions": {QUESTION: question}})
}

/// A rough size of what a run sends, in tokens (four characters each, the
/// usual rule of thumb). For the notice before a run; the real count comes
/// back from the server afterwards.
pub fn estimate_tokens(states: &[Value], spec: &ClassifySpec) -> u64 {
    let question = request_body("", &Value::Null, spec).to_string().len();
    let total: usize = states.iter().map(|s| s.to_string().len() + question).sum();
    (total / 4) as u64
}

/// A classifier kept in a file: the question without the table. Columns are
/// names, so it fits any table that has them. The fields are those of the MCP
/// tool `classify_rows`, so a saved file can be handed to it as it is.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedClassifier {
    pub columns: Vec<String>,
    pub mode: ClassifyMode,
    pub instructions: String,
    pub categories: Vec<Category>,
    pub levels: Vec<String>,
    /// Name of the answer column; `None` takes the default.
    pub new_column: Option<String>,
}

impl SavedClassifier {
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        use anyhow::Context;
        std::fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("writing {}", path.display()))
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        use anyhow::Context;
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("{} is not a saved classifier", path.display()))
    }
}

/// One answer: the value for the result column and how sure the model is.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub value: CellValue,
    pub certainty: CellValue,
}

fn number(v: Option<&Value>) -> CellValue {
    v.and_then(Value::as_f64)
        .map_or(CellValue::Null, CellValue::Float)
}

/// Read the answer out of a response body.
pub fn parse_answer(response: &Value, spec: &ClassifySpec) -> anyhow::Result<Answer> {
    let a = response
        .get("answers")
        .and_then(|a| a.get(QUESTION))
        .ok_or_else(|| anyhow::anyhow!("the response holds no answer: {response}"))?;
    let missing = |field: &str| anyhow::anyhow!("the answer has no `{field}`: {a}");
    Ok(match spec.mode {
        ClassifyMode::Categories => Answer {
            value: CellValue::String(
                a.get("choice")
                    .and_then(Value::as_str)
                    .ok_or_else(|| missing("choice"))?
                    .to_string(),
            ),
            certainty: number(a.get("confidence")),
        },
        ClassifyMode::YesNo => {
            let p = a
                .get("noul")
                .and_then(Value::as_f64)
                .ok_or_else(|| missing("noul"))?;
            Answer {
                value: CellValue::Bool(p >= 0.5),
                certainty: CellValue::Float(p),
            }
        }
        ClassifyMode::Scale => Answer {
            value: CellValue::Float(
                a.get("score")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| missing("score"))?,
            ),
            certainty: number(a.get("confidence")),
        },
    })
}

/// Shared between a run and whoever watches it: the dialog reads `done` for
/// its progress line and raises `cancel` from its Cancel button.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub cancel: AtomicBool,
}

/// How a run went, apart from the answers themselves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RunReport {
    /// Distinct values sent.
    pub sent: usize,
    /// Of those, how many got no answer. Their rows stay empty.
    pub failed: usize,
    /// The first failure's message, whole chain.
    pub first_error: Option<String>,
    /// Input tokens the server counted, summed.
    pub input_tokens: u64,
}

/// Sends one request body, returns the response body.
pub type Sender<'a> = &'a (dyn Fn(&Value) -> anyhow::Result<Value> + Sync);

/// A run that has only ever failed stops after this many requests: a wrong key
/// or a dead network fails every value the same way.
const GIVE_UP_AFTER: usize = 5;

/// Ask about every state, `parallel` requests at a time. One answer slot per
/// state; a failed one stays `None`. Errors only when nothing was answered at
/// all, or on Cancel.
pub fn classify_states(
    states: &[Value],
    spec: &ClassifySpec,
    model: &str,
    parallel: usize,
    send: Sender,
    progress: &Progress,
) -> anyhow::Result<(Vec<Option<Answer>>, RunReport)> {
    progress.total.store(states.len(), Ordering::Relaxed);
    let answers: Mutex<Vec<Option<Answer>>> = Mutex::new(vec![None; states.len()]);
    let first_error: Mutex<Option<String>> = Mutex::new(None);
    let next = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let answered = AtomicUsize::new(0);
    let tokens = AtomicU64::new(0);
    let gave_up = AtomicBool::new(false);

    let worker = || {
        loop {
            if progress.cancel.load(Ordering::Relaxed) || gave_up.load(Ordering::Relaxed) {
                break;
            }
            let i = next.fetch_add(1, Ordering::Relaxed);
            let Some(state) = states.get(i) else { break };
            let result = send(&request_body(model, state, spec)).and_then(|resp| {
                let used = resp
                    .get("usage")
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                tokens.fetch_add(used, Ordering::Relaxed);
                parse_answer(&resp, spec)
            });
            match result {
                Ok(a) => {
                    answered.fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut g) = answers.lock() {
                        g[i] = Some(a);
                    }
                }
                Err(e) => {
                    if let Ok(mut g) = first_error.lock() {
                        g.get_or_insert_with(|| format!("{e:#}"));
                    }
                    let n = failed.fetch_add(1, Ordering::Relaxed) + 1;
                    if n >= GIVE_UP_AFTER && answered.load(Ordering::Relaxed) == 0 {
                        gave_up.store(true, Ordering::Relaxed);
                    }
                }
            }
            progress.done.fetch_add(1, Ordering::Relaxed);
        }
    };
    std::thread::scope(|s| {
        for _ in 0..parallel.clamp(1, states.len().max(1)) {
            s.spawn(worker);
        }
    });

    if progress.cancel.load(Ordering::Relaxed) {
        anyhow::bail!("cancelled");
    }
    let first_error = first_error.into_inner().unwrap_or_default();
    let answered = answered.into_inner();
    if answered == 0
        && let Some(e) = &first_error
    {
        anyhow::bail!("TypeSafe answered none of the values: {e}");
    }
    let report = RunReport {
        sent: states.len(),
        // Everything not answered, so values skipped after giving up count too.
        failed: states.len() - answered,
        first_error,
        input_tokens: tokens.into_inner(),
    };
    Ok((answers.into_inner().unwrap_or_default(), report))
}

/// The two result columns for `name`: the answer and its certainty.
pub fn result_columns(mode: ClassifyMode, name: &str) -> [ColumnInfo; 2] {
    [
        ColumnInfo {
            name: name.to_string(),
            data_type: mode.answer_type().into(),
        },
        ColumnInfo {
            name: format!("{name}{}", mode.certainty_suffix()),
            data_type: "Float64".into(),
        },
    ]
}

/// Classify every row of `table`: the answer and certainty per row, in row
/// order, and the run's report. Rows with nothing to send, and rows whose
/// value got no answer, hold NULLs.
pub fn classify_table(
    table: &DataTable,
    spec: &ClassifySpec,
    model: &str,
    parallel: usize,
    send: Sender,
    progress: &Progress,
) -> anyhow::Result<(Vec<[CellValue; 2]>, RunReport)> {
    let (states, rows) = distinct_states(table, spec);
    let (answers, report) = classify_states(&states, spec, model, parallel, send, progress)?;
    let values = rows
        .into_iter()
        .map(|i| match i.and_then(|i| answers[i].clone()) {
            Some(a) => [a.value, a.certainty],
            None => [CellValue::Null, CellValue::Null],
        })
        .collect();
    Ok((values, report))
}

/// Default name for the answer column.
pub fn default_name(mode: ClassifyMode) -> &'static str {
    match mode {
        ClassifyMode::Categories => "category",
        ClassifyMode::YesNo => "answer",
        ClassifyMode::Scale => "score",
    }
}

/// Resolve `names`, classify, append the two columns (no undo: for the CLI and
/// MCP, which write a file; the dialog goes through `insert_column`). Returns
/// the answer column's name and the report.
pub fn add_classify_columns(
    table: &mut DataTable,
    names: &[String],
    mut spec: ClassifySpec,
    new_column: Option<&str>,
    model: &str,
    parallel: usize,
    send: Sender,
) -> anyhow::Result<(String, RunReport)> {
    spec.columns = names
        .iter()
        .map(|n| {
            table
                .columns
                .iter()
                .position(|c| &c.name == n)
                .ok_or_else(|| anyhow::anyhow!("no such column: {n}"))
        })
        .collect::<anyhow::Result<_>>()?;
    if let Some(key) = spec.problem() {
        anyhow::bail!("{}", crate::i18n::t(key));
    }
    let name = new_column.unwrap_or(default_name(spec.mode)).to_string();
    let columns = result_columns(spec.mode, &name);
    if let Some(taken) = columns
        .iter()
        .find(|n| table.columns.iter().any(|c| c.name == n.name))
    {
        anyhow::bail!(
            "a column named {} already exists; pick another name for the new column",
            taken.name
        );
    }
    let (values, report) =
        classify_table(table, &spec, model, parallel, send, &Progress::default())?;
    table.columns.extend(columns);
    for (row, v) in table.rows.iter_mut().zip(values) {
        row.extend(v);
    }
    Ok((name, report))
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
