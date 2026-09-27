//! Timeline: rows with a start and an end (bookings, shifts, projects) as
//! bars on a time axis, grouped into lanes, with overlaps inside a lane
//! found.
//!
//! [`build`] turns a table into bars; [`overlaps`] finds every pair of bars
//! in the same lane that share time; [`overlaps_table`] is the result every
//! surface shows (the GUI's "Open overlaps..." tab, `--overlaps`, MCP
//! `find_overlaps`), so they cannot disagree.
//!
//! Intervals are half-open: a booking ending at 11:00 and the next starting
//! at 11:00 **touch** and do not overlap. A row without an end is a point;
//! it overlaps a bar it falls strictly inside.

use std::collections::HashMap;

use chrono::DateTime;

use crate::data::test_data::{parse_date, parse_datetime};
use crate::data::{CellValue, ColumnInfo, DataTable};

/// One drawable row.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    /// Data row index in the table.
    pub row: usize,
    pub lane: usize,
    /// Sub-row inside the lane, so overlapping bars do not hide each other.
    pub track: usize,
    /// Seconds since 1970.
    pub start: f64,
    /// Equal to `start` for a row without an end.
    pub end: f64,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
    /// Lane names in first-seen order; one unnamed lane without a lane column.
    pub lanes: Vec<String>,
    /// Tracks per lane (at least 1).
    pub tracks: Vec<usize>,
    pub bars: Vec<Bar>,
    /// Rows whose end is before their start: counted, not drawn.
    pub bad_rows: Vec<usize>,
    /// Rows with no readable start.
    pub skipped: usize,
    /// Names of the chosen start, end, label and lane columns, so the
    /// overlaps table can say what its columns hold.
    pub names: [Option<String>; 4],
}

/// A cell as seconds since 1970, from a date, a datetime or text holding one.
pub fn to_secs(v: &CellValue) -> Option<f64> {
    let s = match v {
        CellValue::Null => return None,
        other => other.to_string(),
    };
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    parse_datetime(s)
        .or_else(|| parse_date(s).and_then(|d| d.and_hms_opt(0, 0, 0)))
        .map(|d| d.and_utc().timestamp() as f64)
}

/// The column names a time column would show, for tooltips and tables.
pub fn format_secs(secs: f64) -> String {
    match DateTime::from_timestamp(secs as i64, 0) {
        Some(d) if d.time() == chrono::NaiveTime::MIN => d.format("%Y-%m-%d").to_string(),
        Some(d) => d.format("%Y-%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

/// Columns whose values are nearly all dates or datetimes, in table order.
pub fn time_columns(table: &DataTable) -> Vec<usize> {
    let rows = table.row_count();
    let step = (rows / 2000).max(1);
    (0..table.col_count())
        .filter(|&c| {
            let ty = table.columns[c].data_type.to_ascii_lowercase();
            if ty.contains("date") || ty.contains("timestamp") {
                return true;
            }
            if crate::data::is_numeric_data_type(&table.columns[c].data_type) {
                return false;
            }
            let (mut seen, mut ok) = (0usize, 0usize);
            for r in (0..rows).step_by(step) {
                let Some(v) = table.get(r, c) else { continue };
                if v.to_string().trim().is_empty() {
                    continue;
                }
                seen += 1;
                ok += usize::from(to_secs(v).is_some());
            }
            seen > 0 && ok * 10 >= seen * 9
        })
        .collect()
}

/// The start and end column a timeline opens with: the first two time
/// columns, or the first alone (points) when there is only one.
pub fn detect(table: &DataTable) -> Option<(usize, Option<usize>)> {
    let t = time_columns(table);
    Some((*t.first()?, t.get(1).copied()))
}

/// Bars for `rows` (a display order, usually the filtered rows).
pub fn build(
    table: &DataTable,
    rows: &[usize],
    start: usize,
    end: Option<usize>,
    label: Option<usize>,
    lane: Option<usize>,
) -> Timeline {
    let mut t = Timeline::default();
    let name = |c: Option<usize>| c.and_then(|c| table.columns.get(c)).map(|c| c.name.clone());
    t.names = [name(Some(start)), name(end), name(label), name(lane)];
    let mut lane_of: HashMap<String, usize> = HashMap::new();
    for &r in rows {
        let Some(s) = table.get(r, start).and_then(to_secs) else {
            t.skipped += 1;
            continue;
        };
        let e = end
            .and_then(|c| table.get(r, c))
            .and_then(to_secs)
            .unwrap_or(s);
        if e < s {
            t.bad_rows.push(r);
            continue;
        }
        let lane_name = lane
            .and_then(|c| table.get(r, c))
            .map(|v| v.to_string())
            .unwrap_or_default();
        let next = lane_of.len();
        let l = *lane_of.entry(lane_name.clone()).or_insert_with(|| {
            t.lanes.push(lane_name);
            next
        });
        t.bars.push(Bar {
            row: r,
            lane: l,
            track: 0,
            start: s,
            end: e,
            label: label
                .and_then(|c| table.get(r, c))
                .map(|v| v.to_string())
                .unwrap_or_default(),
        });
    }
    assign_tracks(&mut t);
    t
}

fn shares_time(a: &Bar, b: &Bar) -> bool {
    a.start < b.end && b.start < a.end
        // A point shares time with a bar it sits strictly inside.
        || (a.start == a.end && b.start < a.start && a.start < b.end)
        || (b.start == b.end && a.start < b.start && b.start < a.end)
}

/// Give each bar the lowest track in its lane that no earlier-starting bar
/// it overlaps occupies.
fn assign_tracks(t: &mut Timeline) {
    t.tracks = vec![1; t.lanes.len()];
    let mut order: Vec<usize> = (0..t.bars.len()).collect();
    order.sort_by(|&a, &b| {
        (t.bars[a].lane, t.bars[a].start)
            .partial_cmp(&(t.bars[b].lane, t.bars[b].start))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Per lane: the end of the last bar on each track.
    let mut ends: Vec<Vec<f64>> = vec![Vec::new(); t.lanes.len()];
    for i in order {
        let (lane, start, end) = (t.bars[i].lane, t.bars[i].start, t.bars[i].end);
        let tracks = &mut ends[lane];
        let track = match tracks.iter().position(|&e| e <= start) {
            Some(k) => k,
            None => {
                tracks.push(f64::NEG_INFINITY);
                tracks.len() - 1
            }
        };
        tracks[track] = end.max(start);
        t.bars[i].track = track;
        t.tracks[lane] = t.tracks[lane].max(track + 1);
    }
}

/// Pairs of bar indices (into `t.bars`) in the same lane that share time,
/// sorted by lane and start. A sweep per lane: O(n log n + pairs).
pub fn overlaps(t: &Timeline) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..t.bars.len()).collect();
    order.sort_by(|&a, &b| {
        (t.bars[a].lane, t.bars[a].start)
            .partial_cmp(&(t.bars[b].lane, t.bars[b].start))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut lane = usize::MAX;
    for i in order {
        let b = &t.bars[i];
        if b.lane != lane {
            active.clear();
            lane = b.lane;
        }
        // Keep what is still open when `b` starts. A point is kept only
        // while `b` starts at the same moment (bars come in start order, so
        // nothing later can contain it).
        active.retain(|&a| {
            let x = &t.bars[a];
            x.end > b.start || (x.start == x.end && x.start >= b.start)
        });
        for &a in &active {
            if shares_time(&t.bars[a], b) {
                out.push((a, i));
            }
        }
        active.push(i);
    }
    out
}

/// Lanes that hold at least one overlap.
pub fn lanes_with_overlaps(pairs: &[(usize, usize)], t: &Timeline) -> usize {
    let mut lanes: Vec<usize> = pairs.iter().map(|&(a, _)| t.bars[a].lane).collect();
    lanes.sort_unstable();
    lanes.dedup();
    lanes.len()
}

/// Column names of [`overlaps_table`]: the lane column's name, then row,
/// label, start and end for side `_a` (the row that starts first) and side
/// `_b`, named after the source columns (`room`, `row_a`, `check_in_a`...).
/// Falls back to generic names (`lane`, `label_a`...) for a column not
/// chosen, and entirely when the source names would collide.
pub fn overlaps_columns(t: &Timeline) -> [String; 9] {
    let [start, end, label, lane] = &t.names;
    let pick = |n: &Option<String>, generic: &str| n.clone().unwrap_or_else(|| generic.into());
    let build = |named: bool| {
        let n = |c: &Option<String>, g: &str| if named { pick(c, g) } else { g.to_string() };
        let side = |s: &str| {
            [
                format!("row_{s}"),
                format!("{}_{s}", n(label, "label")),
                format!("{}_{s}", n(start, "start")),
                format!("{}_{s}", n(end, "end")),
            ]
        };
        let [a0, a1, a2, a3] = side("a");
        let [b0, b1, b2, b3] = side("b");
        [n(lane, "lane"), a0, a1, a2, a3, b0, b1, b2, b3]
    };
    let named = build(true);
    let unique: std::collections::HashSet<&String> = named.iter().collect();
    if unique.len() == named.len() {
        named
    } else {
        build(false)
    }
}

/// One row per overlapping pair: the lane, then row number (1-based),
/// label, start and end of each side; see [`overlaps_columns`].
pub fn overlaps_table(t: &Timeline, pairs: &[(usize, usize)]) -> DataTable {
    let mut out = DataTable::empty();
    out.columns = overlaps_columns(t)
        .into_iter()
        .enumerate()
        .map(|(i, name)| ColumnInfo {
            name,
            data_type: if i == 1 || i == 5 { "Int64" } else { "Utf8" }.into(),
        })
        .collect();
    for &(a, b) in pairs {
        let (a, b) = (&t.bars[a], &t.bars[b]);
        let side = |x: &Bar| {
            [
                CellValue::Int(x.row as i64 + 1),
                CellValue::String(x.label.clone()),
                CellValue::String(format_secs(x.start)),
                CellValue::String(format_secs(x.end)),
            ]
        };
        let mut row = vec![CellValue::String(t.lanes[a.lane].clone())];
        row.extend(side(a));
        row.extend(side(b));
        out.rows.push(row);
    }
    out
}

/// Index of the column named `name`, or an error naming it.
pub fn column_named(table: &DataTable, name: &str) -> anyhow::Result<usize> {
    table
        .columns
        .iter()
        .position(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("column `{name}` not found"))
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
