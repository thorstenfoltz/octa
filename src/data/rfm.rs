//! Customer segments (RFM): every customer gets three marks from 1 to 5,
//! for how recently they bought, how often and how much, and a segment name
//! read off the marks. Shared by the dialog, `--rfm`, the MCP tool
//! `rfm_segments` and the database path (`db::pushdown::rfm` fills
//! [`Totals`], the same [`table`] marks and names them, so the server and the
//! local rows cannot disagree).
//!
//! Marks are quintiles: customers are sorted by a measure and cut into five
//! equal parts. Equal values always get the same mark, the one the first of
//! them would get, so a mark counts the customers strictly below a value.
//! Fewer days since the last purchase is the better recency mark. The name
//! comes from the recency mark and the average of the frequency and monetary
//! marks, halves rounded up.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::data::decimal::Sum;
use crate::data::{CellValue, ColumnInfo, DataTable};

/// Which names the segments get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Names {
    /// Seven plain names (Best, Loyal, ...).
    #[default]
    Seven,
    /// The eleven names most RFM articles use (Champions, ...).
    Eleven,
    /// Marks only, no segment column.
    None,
}

impl Names {
    pub const ALL: [Names; 3] = [Names::Seven, Names::Eleven, Names::None];

    /// The word the CLI and MCP take.
    pub fn id(self) -> &'static str {
        match self {
            Names::Seven => "seven",
            Names::Eleven => "eleven",
            Names::None => "none",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|n| n.id().eq_ignore_ascii_case(s.trim()))
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Names::Seven => "rfm.names_seven",
            Names::Eleven => "rfm.names_eleven",
            Names::None => "rfm.names_none",
        }
    }

    /// The segment for a recency mark and the rounded average of the
    /// frequency and monetary marks, both 1 to 5.
    pub fn name(self, recency: u8, fm: u8) -> Option<&'static str> {
        let (r, fm) = (
            recency.clamp(1, 5) as usize - 1,
            fm.clamp(1, 5) as usize - 1,
        );
        match self {
            Names::Seven => Some(SEVEN[r][fm]),
            Names::Eleven => Some(ELEVEN[r][fm]),
            Names::None => None,
        }
    }
}

/// `[recency - 1][fm - 1]`.
const SEVEN: [[&str; 5]; 5] = [
    [
        "Lost",
        "Lost",
        "Slipping away",
        "Slipping away",
        "Slipping away",
    ],
    [
        "Lost",
        "Lost",
        "Slipping away",
        "Slipping away",
        "Slipping away",
    ],
    [
        "Needs attention",
        "Needs attention",
        "Needs attention",
        "Loyal",
        "Loyal",
    ],
    ["New", "Promising", "Loyal", "Best", "Best"],
    ["New", "Promising", "Loyal", "Best", "Best"],
];

/// `[recency - 1][fm - 1]`.
const ELEVEN: [[&str; 5]; 5] = [
    [
        "Lost",
        "Lost",
        "At risk",
        "Can't lose them",
        "Can't lose them",
    ],
    [
        "Hibernating",
        "Hibernating",
        "At risk",
        "At risk",
        "Can't lose them",
    ],
    [
        "About to sleep",
        "Needs attention",
        "Needs attention",
        "Loyal customers",
        "Loyal customers",
    ],
    [
        "Promising",
        "Potential loyalists",
        "Loyal customers",
        "Champions",
        "Champions",
    ],
    [
        "Recent customers",
        "Potential loyalists",
        "Potential loyalists",
        "Champions",
        "Champions",
    ],
];

/// Every segment name with the i18n key of its hover text. Literals because
/// the names in cells are stable English; `every_name_is_explained` guards
/// the pairing.
pub const VALUE_HINTS: [(&str, &str); 15] = [
    ("Best", "rfm.seg_best"),
    ("Loyal", "rfm.seg_loyal"),
    ("Promising", "rfm.seg_promising"),
    ("New", "rfm.seg_new"),
    ("Needs attention", "rfm.seg_needs_attention"),
    ("Slipping away", "rfm.seg_slipping_away"),
    ("Lost", "rfm.seg_lost"),
    ("Champions", "rfm.seg_champions"),
    ("Loyal customers", "rfm.seg_loyal_customers"),
    ("Potential loyalists", "rfm.seg_potential_loyalists"),
    ("Recent customers", "rfm.seg_recent_customers"),
    ("About to sleep", "rfm.seg_about_to_sleep"),
    ("Hibernating", "rfm.seg_hibernating"),
    ("At risk", "rfm.seg_at_risk"),
    ("Can't lose them", "rfm.seg_cant_lose_them"),
];

/// Column ids of the result, and the i18n keys of their header hover texts.
pub const COLUMNS: [(&str, &str); 9] = [
    ("customer", "rfm.hint_customer"),
    ("last_date", "rfm.hint_last_date"),
    ("days_since", "rfm.hint_days_since"),
    ("orders", "rfm.hint_orders"),
    ("amount", "rfm.hint_amount"),
    ("recency", "rfm.hint_recency"),
    ("frequency", "rfm.hint_frequency"),
    ("monetary", "rfm.hint_monetary"),
    ("segment", "rfm.hint_segment"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct RfmSpec {
    pub customer: usize,
    pub date: usize,
    pub amount: usize,
    /// `None`: every row is one order. A row without an order number is an
    /// order of its own.
    pub order: Option<usize>,
    /// Purchases after this day are left out, so the segments are those of
    /// that day. `None`: every row counts.
    pub as_of: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfmError {
    /// No row has a customer and a readable date.
    NoRows,
    /// Nobody bought on or before the As of day.
    NoneBy(NaiveDate),
}

impl RfmError {
    pub fn key(self) -> &'static str {
        match self {
            Self::NoRows => "rfm.err_no_rows",
            Self::NoneBy(_) => "rfm.err_none_by",
        }
    }

    /// The message in the active language.
    pub fn message(self) -> String {
        let text = crate::i18n::t(self.key());
        match self {
            Self::NoneBy(day) => text.replace("{date}", &day.format("%Y-%m-%d").to_string()),
            Self::NoRows => text,
        }
    }
}

/// One customer's totals, what the server or the local rows add up.
#[derive(Debug, Clone, PartialEq)]
pub struct Customer {
    pub id: String,
    pub last: NaiveDate,
    pub orders: i64,
    /// The exact sum; `Int(0)` when no row had an amount.
    pub amount: CellValue,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    pub customers: Vec<Customer>,
    /// Rows without a customer or a readable date.
    pub skipped: usize,
}

/// The calendar day of a cell, read as `calculated::seconds` reads times.
pub fn day_of(cell: &CellValue) -> Option<NaiveDate> {
    let secs = super::calculated::seconds(cell)?;
    day_of_seconds(secs)
}

/// The calendar day of seconds since 1970.
pub fn day_of_seconds(secs: f64) -> Option<NaiveDate> {
    chrono::DateTime::from_timestamp(secs.floor() as i64, 0).map(|d| d.date_naive())
}

struct Acc {
    last: NaiveDate,
    rows: i64,
    orders: HashSet<String>,
    /// Rows without an order number, each an order of its own.
    unnumbered: i64,
    sum: Sum,
}

/// Each customer's last date, orders and amount. An empty customer or an
/// unreadable date skips the row and is counted; a purchase after `as_of` is
/// left out; an amount that is no number adds nothing.
pub fn collect(table: &DataTable, spec: &RfmSpec) -> Totals {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut accs: Vec<(String, Acc)> = Vec::new();
    let mut skipped = 0;
    for r in 0..table.row_count() {
        let who = table
            .get(r, spec.customer)
            .filter(|c| !matches!(c, CellValue::Null))
            .map(ToString::to_string)
            .unwrap_or_default();
        let day = table.get(r, spec.date).and_then(day_of);
        let Some(day) = day.filter(|_| !who.trim().is_empty()) else {
            skipped += 1;
            continue;
        };
        if spec.as_of.is_some_and(|as_of| day > as_of) {
            continue;
        }
        let i = *index.entry(who.clone()).or_insert_with(|| {
            accs.push((
                who,
                Acc {
                    last: day,
                    rows: 0,
                    orders: HashSet::new(),
                    unnumbered: 0,
                    sum: Sum::default(),
                },
            ));
            accs.len() - 1
        });
        let acc = &mut accs[i].1;
        acc.last = acc.last.max(day);
        acc.rows += 1;
        if let Some(c) = spec.order {
            match table
                .get(r, c)
                .filter(|v| !matches!(v, CellValue::Null))
                .map(ToString::to_string)
                .filter(|v| !v.trim().is_empty())
            {
                Some(v) => {
                    acc.orders.insert(v);
                }
                None => acc.unnumbered += 1,
            }
        }
        if let Some(a) = table.get(r, spec.amount) {
            acc.sum.add_cell(a);
        }
    }
    Totals {
        customers: accs
            .into_iter()
            .map(|(id, a)| Customer {
                id,
                last: a.last,
                orders: if spec.order.is_some() {
                    a.orders.len() as i64 + a.unnumbered
                } else {
                    a.rows
                },
                amount: a.sum.cell().unwrap_or(CellValue::Int(0)),
            })
            .collect(),
        skipped,
    }
}

/// A mark from 1 to 5 per value, higher values higher: the share of values
/// strictly below it, in fifths.
pub fn marks(values: &[f64]) -> Vec<u8> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = values.len().max(1);
    values
        .iter()
        .map(|v| {
            let below = sorted.partition_point(|x| x.total_cmp(v).is_lt());
            1 + (5 * below / n) as u8
        })
        .collect()
}

fn amount_f64(c: &CellValue) -> f64 {
    match c {
        CellValue::Int(i) => *i as f64,
        CellValue::Float(f) => *f,
        CellValue::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// One row per customer, the best customers by amount first, and the date
/// the recency was counted to (`as_of`, else the latest date in the data).
pub fn table(
    totals: &Totals,
    as_of: Option<NaiveDate>,
    names: Names,
) -> Result<(DataTable, NaiveDate), RfmError> {
    let latest = totals
        .customers
        .iter()
        .map(|c| c.last)
        .max()
        .ok_or(as_of.map_or(RfmError::NoRows, RfmError::NoneBy))?;
    let as_of = as_of.unwrap_or(latest);
    let days: Vec<i64> = totals
        .customers
        .iter()
        .map(|c| (as_of - c.last).num_days())
        .collect();
    let money: Vec<f64> = totals
        .customers
        .iter()
        .map(|c| amount_f64(&c.amount))
        .collect();
    let r = marks(&days.iter().map(|d| -(*d as f64)).collect::<Vec<_>>());
    let f = marks(
        &totals
            .customers
            .iter()
            .map(|c| c.orders as f64)
            .collect::<Vec<_>>(),
    );
    let m = marks(&money);

    let mut order: Vec<usize> = (0..totals.customers.len()).collect();
    order.sort_by(|&a, &b| {
        money[b]
            .total_cmp(&money[a])
            .then_with(|| totals.customers[a].id.cmp(&totals.customers[b].id))
    });
    let amount_type =
        crate::data::column_type_from_cells(totals.customers.iter().map(|c| &c.amount));
    let types = [
        "Utf8",
        "Date32",
        "Int64",
        "Int64",
        amount_type.as_str(),
        "Int64",
        "Int64",
        "Int64",
        "Utf8",
    ];
    let shown = if names == Names::None { 8 } else { 9 };
    let columns = COLUMNS
        .iter()
        .zip(types)
        .take(shown)
        .map(|((name, _), ty)| ColumnInfo {
            name: (*name).into(),
            data_type: ty.into(),
        })
        .collect();
    let rows = order
        .into_iter()
        .map(|i| {
            let c = &totals.customers[i];
            let mut row = vec![
                CellValue::String(c.id.clone()),
                CellValue::Date(c.last.format("%Y-%m-%d").to_string()),
                CellValue::Int(days[i]),
                CellValue::Int(c.orders),
                c.amount.clone(),
                CellValue::Int(r[i].into()),
                CellValue::Int(f[i].into()),
                CellValue::Int(m[i].into()),
            ];
            if let Some(name) = names.name(r[i], (f[i] + m[i]).div_ceil(2)) {
                row.push(CellValue::String(name.into()));
            }
            row
        })
        .collect();
    Ok((
        DataTable {
            columns,
            rows,
            ..DataTable::empty()
        },
        as_of,
    ))
}

#[cfg(test)]
#[path = "rfm_tests.rs"]
mod tests;
