//! Clock readings without a date (`14:30:00`) and amounts of time
//! (`30:15:00`, `-01:30:00`): the one parser and formatter for both.
//!
//! Every reader, writer, database decoder, conversion and the load-time
//! detection go through here, so a value reads the same wherever it came
//! from. Values are nanoseconds: since midnight for a time, signed for a
//! duration.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

use super::{CellValue, DataTable};

pub const NANOS_PER_SEC: i64 = 1_000_000_000;
pub const NANOS_PER_DAY: i64 = 86_400 * NANOS_PER_SEC;

/// Which of the two types a value or a column is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeKind {
    /// A clock reading, 00:00:00 to 23:59:59.999999999.
    Time,
    /// An amount of time: signed, any length.
    Duration,
}

impl TimeKind {
    /// The kind a column type names. Readers spell it the Arrow way
    /// (`Time64(µs)`), Octa's own columns the long way
    /// (`Time64(Microsecond)`); both start the same.
    pub fn of_type(data_type: &str) -> Option<TimeKind> {
        if data_type.starts_with("Time32") || data_type.starts_with("Time64") {
            Some(TimeKind::Time)
        } else if data_type.starts_with("Duration") {
            Some(TimeKind::Duration)
        } else {
            None
        }
    }

    /// The type a column Octa creates gets: microseconds, unless a value has
    /// digits below a microsecond.
    pub fn data_type(self, nanos: bool) -> &'static str {
        match (self, nanos) {
            (TimeKind::Time, false) => "Time64(Microsecond)",
            (TimeKind::Time, true) => "Time64(Nanosecond)",
            (TimeKind::Duration, false) => "Duration(Microsecond)",
            (TimeKind::Duration, true) => "Duration(Nanosecond)",
        }
    }

    /// Read `s` as this kind.
    pub fn parse(self, s: &str) -> Option<i64> {
        match self {
            TimeKind::Time => parse_time(s),
            TimeKind::Duration => parse_duration(s),
        }
    }
}

/// The unit a column stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Second,
    Milli,
    Micro,
    Nano,
}

impl Unit {
    /// Nanoseconds in one of this unit.
    pub fn nanos(self) -> i64 {
        match self {
            Unit::Second => NANOS_PER_SEC,
            Unit::Milli => 1_000_000,
            Unit::Micro => 1_000,
            Unit::Nano => 1,
        }
    }
}

/// The unit inside a type name (`Time64(µs)`, `Duration(Nanosecond)`).
/// Microseconds when the name carries none.
pub fn type_unit(data_type: &str) -> Unit {
    let inner = data_type
        .split_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .unwrap_or("");
    match inner.trim() {
        "s" | "Second" => Unit::Second,
        "ms" | "Millisecond" => Unit::Milli,
        "ns" | "Nanosecond" => Unit::Nano,
        _ => Unit::Micro,
    }
}

/// How a duration is written: on screen and in every text-based save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DurationStyle {
    /// `30:15:00`
    #[default]
    Clock,
    /// `1 day 06:15:00`
    Days,
    /// `PT30H15M`
    Iso,
}

impl DurationStyle {
    pub const ALL: [DurationStyle; 3] = [
        DurationStyle::Clock,
        DurationStyle::Days,
        DurationStyle::Iso,
    ];

    /// The example the Settings picker shows. Not translated: it is the
    /// text that will be written.
    pub fn example(self) -> &'static str {
        match self {
            DurationStyle::Clock => "30:15:00",
            DurationStyle::Days => "1 day 06:15:00",
            DurationStyle::Iso => "PT30H15M",
        }
    }

    pub fn hint_key(self) -> &'static str {
        match self {
            DurationStyle::Clock => "settings_hint.duration_clock",
            DurationStyle::Days => "settings_hint.duration_days",
            DurationStyle::Iso => "settings_hint.duration_iso",
        }
    }
}

/// Process-wide, like `formats::compression::max_decompressed`: `Display`
/// has no access to settings and every text writer goes through it. The GUI
/// sets it at start and on Apply, the CLI and MCP server from settings.toml.
static DURATION_STYLE: AtomicU8 = AtomicU8::new(0);

pub fn duration_style() -> DurationStyle {
    match DURATION_STYLE.load(Ordering::Relaxed) {
        1 => DurationStyle::Days,
        2 => DurationStyle::Iso,
        _ => DurationStyle::Clock,
    }
}

pub fn set_duration_style(style: DurationStyle) {
    DURATION_STYLE.store(style as u8, Ordering::Relaxed);
}

/// Whether opening a text file types its clock and duration columns. Same
/// shape as the duration style, so a headless read honours settings.toml.
static DETECT_ON_OPEN: AtomicBool = AtomicBool::new(true);

pub fn detect_on_open() -> bool {
    DETECT_ON_OPEN.load(Ordering::Relaxed)
}

pub fn set_detect_on_open(on: bool) {
    DETECT_ON_OPEN.store(on, Ordering::Relaxed);
}

// ---------------------------------------------------------------- formatting

/// `.25` for a quarter second, nothing for a whole one.
fn fraction(nanos: u64) -> String {
    if nanos == 0 {
        return String::new();
    }
    let digits = format!("{nanos:09}");
    format!(".{}", digits.trim_end_matches('0'))
}

fn hms(secs: u64) -> (u64, u64, u64) {
    (secs / 3_600, secs / 60 % 60, secs % 60)
}

/// `14:30:00`, `14:30:00.25`.
pub fn format_time(ns: i64) -> String {
    let ns = ns.rem_euclid(NANOS_PER_DAY) as u64;
    let (h, m, s) = hms(ns / NANOS_PER_SEC as u64);
    format!(
        "{h:02}:{m:02}:{s:02}{}",
        fraction(ns % NANOS_PER_SEC as u64)
    )
}

/// A duration in `style`. Negative values carry the sign on every part in
/// the day style (`-1 day -06:15:00`), which is how Postgres and DuckDB
/// print one, so their parsers and ours read it the same.
pub fn format_duration(ns: i64, style: DurationStyle) -> String {
    let sign = if ns < 0 { "-" } else { "" };
    let abs = ns.unsigned_abs();
    let secs = abs / NANOS_PER_SEC as u64;
    let frac = fraction(abs % NANOS_PER_SEC as u64);
    match style {
        DurationStyle::Clock => {
            let (h, m, s) = hms(secs);
            format!("{sign}{h:02}:{m:02}:{s:02}{frac}")
        }
        DurationStyle::Days => {
            let days = secs / 86_400;
            let (h, m, s) = hms(secs % 86_400);
            let clock = format!("{sign}{h:02}:{m:02}:{s:02}{frac}");
            match days {
                0 => clock,
                1 => format!("{sign}1 day {clock}"),
                d => format!("{sign}{d} days {clock}"),
            }
        }
        DurationStyle::Iso => {
            let (h, m, s) = hms(secs);
            let mut out = format!("{sign}PT");
            if h > 0 {
                out.push_str(&format!("{h}H"));
            }
            if m > 0 {
                out.push_str(&format!("{m}M"));
            }
            if s > 0 || !frac.is_empty() || (h == 0 && m == 0) {
                out.push_str(&format!("{s}{frac}S"));
            }
            out
        }
    }
}

// ------------------------------------------------------------------- parsing

fn digits(s: &str, max_len: usize) -> Option<u64> {
    (!s.is_empty() && s.len() <= max_len && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// Up to nine fraction digits as nanoseconds.
fn fraction_nanos(f: &str) -> Option<u64> {
    digits(f, 9).map(|_| format!("{f:0<9}").parse().unwrap_or(0))
}

/// `H:MM`, `H:MM:SS`, `H:MM:SS.f` into (hours, minutes, seconds, nanos).
/// Hours may be any length. `two_digit` asks for two-digit minutes and
/// seconds, which every source prints except BigQuery (`3:0:0`).
fn parse_clock(s: &str, two_digit: bool) -> Option<(u64, u64, u64, u64)> {
    let (main, frac) = match s.split_once('.') {
        Some((m, f)) => (m, Some(f)),
        None => (s, None),
    };
    let parts: Vec<&str> = main.split(':').collect();
    if !(2..=3).contains(&parts.len()) || (frac.is_some() && parts.len() != 3) {
        return None;
    }
    let sub = |p: &str| {
        if two_digit && p.len() != 2 {
            return None;
        }
        digits(p, 2).filter(|v| *v < 60)
    };
    let hours = digits(parts[0], 18)?;
    let minutes = sub(parts[1])?;
    let seconds = match parts.get(2) {
        Some(p) => sub(p)?,
        None => 0,
    };
    let nanos = match frac {
        Some(f) => fraction_nanos(f)?,
        None => 0,
    };
    Some((hours, minutes, seconds, nanos))
}

fn clock_nanos(h: u64, m: u64, s: u64, n: u64) -> Option<i64> {
    let secs = h.checked_mul(3_600)?.checked_add(m * 60 + s)?;
    i64::try_from(secs)
        .ok()?
        .checked_mul(NANOS_PER_SEC)?
        .checked_add(n as i64)
}

/// `(true, rest)` for a leading `-`, `(false, rest)` for `+` or none.
fn split_sign(s: &str) -> (bool, &str) {
    match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    }
}

fn signed(neg: bool, v: i64) -> i64 {
    if neg { -v } else { v }
}

/// A clock reading: 00:00 to 23:59:59.999999999, or `h:mm[:ss] AM/PM`.
pub fn parse_time(s: &str) -> Option<i64> {
    let lower = s.trim().to_ascii_lowercase();
    let (body, pm) = if let Some(b) = lower.strip_suffix("am") {
        (b.trim_end(), Some(false))
    } else if let Some(b) = lower.strip_suffix("pm") {
        (b.trim_end(), Some(true))
    } else {
        (lower.as_str(), None)
    };
    if body.split(':').next()?.len() > 2 {
        return None;
    }
    let (h, m, sec, n) = parse_clock(body, true)?;
    let h = match pm {
        None if h < 24 => h,
        Some(pm) if (1..=12).contains(&h) => h % 12 + if pm { 12 } else { 0 },
        _ => return None,
    };
    clock_nanos(h, m, sec, n)
}

fn signed_clock(s: &str, two_digit: bool) -> Option<i64> {
    let (neg, rest) = split_sign(s);
    let (h, m, sec, n) = parse_clock(rest, two_digit)?;
    clock_nanos(h, m, sec, n).map(|v| signed(neg, v))
}

fn days(s: &str) -> Option<i64> {
    let (neg, rest) = split_sign(s);
    let d = i64::try_from(digits(rest, 18)?).ok()?;
    d.checked_mul(NANOS_PER_DAY).map(|v| signed(neg, v))
}

fn is_day_word(s: &str) -> bool {
    s.eq_ignore_ascii_case("day") || s.eq_ignore_ascii_case("days")
}

/// BigQuery's `Y-M` lead, which must be zero for a duration.
fn is_zero_year_month(s: &str) -> bool {
    let (_, rest) = split_sign(s);
    rest.split_once('-')
        .is_some_and(|(y, m)| digits(y, 18) == Some(0) && digits(m, 18) == Some(0))
}

/// `30H15M` into `[("30", 'H'), ("15", 'M')]`.
fn iso_parts(s: &str) -> Option<Vec<(&str, char)>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphabetic() {
            if i == start {
                return None;
            }
            out.push((&s[start..i], c.to_ascii_uppercase()));
            start = i + 1;
        }
    }
    (start == s.len()).then_some(out)
}

/// The part after `P`. Years and months are calendar amounts, not
/// durations, so they refuse the whole value.
fn parse_iso(body: &str) -> Option<i64> {
    let (date, time) = match body.split_once(['T', 't']) {
        Some((d, t)) => (d, Some(t)),
        None => (body, None),
    };
    let whole = |n: &str| digits(n, 18).and_then(|v| i64::try_from(v).ok());
    let mut total: i64 = 0;
    let mut any = false;
    for (num, unit) in iso_parts(date)? {
        let per = match unit {
            'W' => 7 * NANOS_PER_DAY,
            'D' => NANOS_PER_DAY,
            _ => return None,
        };
        total = total.checked_add(whole(num)?.checked_mul(per)?)?;
        any = true;
    }
    if let Some(time) = time {
        if time.is_empty() {
            return None;
        }
        for (num, unit) in iso_parts(time)? {
            let add = match unit {
                'H' => whole(num)?.checked_mul(3_600 * NANOS_PER_SEC)?,
                'M' => whole(num)?.checked_mul(60 * NANOS_PER_SEC)?,
                'S' => {
                    let (s, f) = num.split_once('.').unwrap_or((num, ""));
                    let frac = if f.is_empty() {
                        0
                    } else {
                        fraction_nanos(f)? as i64
                    };
                    whole(s)?.checked_mul(NANOS_PER_SEC)?.checked_add(frac)?
                }
                _ => return None,
            };
            total = total.checked_add(add)?;
            any = true;
        }
    }
    any.then_some(total)
}

/// An amount of time in any spelling Octa writes or a source prints:
/// `30:15:00`, `-01:30:00`, `1 day 06:15:00`, `2 days`, `PT30H15M`,
/// `P1DT2H`, Postgres / DuckDB interval text without months, BigQuery's
/// `0-0 2 3:0:0`, Trino / Oracle `2 03:04:05.000`.
pub fn parse_duration(s: &str) -> Option<i64> {
    let t = s.trim();
    let (neg, unsigned) = split_sign(t);
    if let Some(body) = unsigned.strip_prefix(['P', 'p']) {
        return parse_iso(body).map(|v| signed(neg, v));
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    match words.as_slice() {
        [clock] => signed_clock(clock, true),
        [n, unit] if is_day_word(unit) => days(n),
        // Postgres and DuckDB sign each part on its own.
        [n, unit, clock] if is_day_word(unit) => days(n)?.checked_add(signed_clock(clock, true)?),
        [ym, d, clock] if is_zero_year_month(ym) => {
            days(d)?.checked_add(signed_clock(clock, false)?)
        }
        // Trino and Oracle: one sign in front covers the whole value.
        [d, clock] if !clock.starts_with(['-', '+']) => {
            let (neg, d) = split_sign(d);
            let total = days(d)?.checked_add(signed_clock(clock, true)?)?;
            Some(signed(neg, total))
        }
        _ => None,
    }
}

/// The detection rule: every non-empty value a clock reading makes a Time
/// column; otherwise every value an amount of time makes a Duration column;
/// anything else stays text. A column with no values stays text.
pub fn classify_column(values: &[Option<&str>]) -> Option<TimeKind> {
    let (mut time, mut duration, mut any) = (true, true, false);
    for v in values
        .iter()
        .flatten()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        any = true;
        time = time && parse_time(v).is_some();
        duration = duration && parse_duration(v).is_some();
        if !time && !duration {
            return None;
        }
    }
    match (any, time, duration) {
        (false, _, _) => None,
        (true, true, _) => Some(TimeKind::Time),
        (true, false, true) => Some(TimeKind::Duration),
        _ => None,
    }
}

// ------------------------------------------------------------------- cells

impl TimeKind {
    pub fn cell(self, ns: i64) -> CellValue {
        match self {
            TimeKind::Time => CellValue::Time(ns),
            TimeKind::Duration => CellValue::Duration(ns),
        }
    }
}

/// `s` as a cell of `kind`: blank text is null, text that does not parse is
/// `None` so the caller keeps it as it was.
pub fn text_to_cell(kind: TimeKind, s: &str) -> Option<CellValue> {
    if s.trim().is_empty() {
        return Some(CellValue::Null);
    }
    kind.parse(s).map(|ns| kind.cell(ns))
}

/// For decoders that receive text for a column whose type they already
/// mapped: the typed cell when `data_type` is a time type and `s` parses.
pub fn typed_cell(data_type: &str, s: &str) -> Option<CellValue> {
    let kind = TimeKind::of_type(data_type)?;
    kind.parse(s).map(|ns| kind.cell(ns))
}

/// Whether any of `cells` has digits below a microsecond, so the column
/// needs a nanosecond type to keep them.
pub fn needs_nanos<'a>(cells: impl IntoIterator<Item = &'a CellValue>) -> bool {
    cells.into_iter().any(|c| match c {
        CellValue::Time(ns) | CellValue::Duration(ns) => ns % 1_000 != 0,
        _ => false,
    })
}

/// Convert every text cell of `col` to `kind` and set the column's type.
/// Blank text becomes null; text that does not parse stays text.
pub fn apply_kind(table: &mut DataTable, col: usize, kind: TimeKind) {
    for row in &mut table.rows {
        if let Some(cell) = row.get_mut(col)
            && let CellValue::String(s) = cell
            && let Some(v) = text_to_cell(kind, s)
        {
            *cell = v;
        }
    }
    let nanos = needs_nanos(table.rows.iter().filter_map(|r| r.get(col)));
    if let Some(c) = table.columns.get_mut(col) {
        c.data_type = kind.data_type(nanos).to_string();
    }
}

/// Formats whose readers only ever see text, so a column of clock or
/// duration text there is a column of times. Typed sources (Parquet,
/// databases, Excel cells, TOML) say what they hold and are left alone.
const TEXT_FORMATS: &[&str] = &[
    "CSV",
    "TSV",
    "JSON",
    "JSON Lines",
    "YAML",
    "XML",
    "Fixed-Width",
    "Log",
    "HTML",
    "PDF",
    "MessagePack",
    "BSON",
    "ODS",
];

/// Type the clock and duration columns of a freshly read text table, if the
/// detection switch is on. Returns what it changed.
pub fn promote_columns(table: &mut DataTable) -> Vec<(usize, TimeKind)> {
    promote_columns_with(table, detect_on_open())
}

/// [`promote_columns`] with the switch passed in, for tests and callers
/// that already read it.
pub fn promote_columns_with(table: &mut DataTable, detect: bool) -> Vec<(usize, TimeKind)> {
    let is_text = table
        .format_name
        .as_deref()
        .is_some_and(|f| TEXT_FORMATS.contains(&f));
    if !detect || !is_text {
        return Vec::new();
    }
    let mut out = Vec::new();
    for col in 0..table.columns.len() {
        if !crate::data::num_parse::column_is_candidate(table, col) {
            continue;
        }
        let kind = {
            let values = crate::data::date_infer::collect_column_strings(table, col);
            classify_column(&values)
        };
        if let Some(kind) = kind {
            apply_kind(table, col, kind);
            out.push((col, kind));
        }
    }
    out
}

/// Intervals with a month or year part arrive as text (decision 7). A
/// Duration column holding any such text becomes a text column as a whole,
/// every value shown as before, so no writer meets a month inside a
/// Duration column and writes NULL for it. The values are written in the day
/// style (`1 day 01:00:00`), the form Postgres and DuckDB print, so the text
/// does not depend on the display setting.
pub fn settle_interval_columns(table: &mut DataTable) {
    for col in 0..table.columns.len() {
        if TimeKind::of_type(&table.columns[col].data_type) != Some(TimeKind::Duration) {
            continue;
        }
        let mixed = table
            .rows
            .iter()
            .any(|r| matches!(r.get(col), Some(CellValue::String(_))));
        if !mixed {
            continue;
        }
        for row in &mut table.rows {
            if let Some(cell) = row.get_mut(col)
                && let CellValue::Duration(ns) = *cell
            {
                *cell = CellValue::String(format_duration(ns, DurationStyle::Days));
            }
        }
        table.columns[col].data_type = "Utf8".to_string();
    }
}

/// For sources that cannot tell a clock time from an amount (ODS time
/// cells, SPSS / SAS TIME formats): a Duration column whose every value
/// lies within one day becomes a Time column. Returns the kind it settled on.
pub fn narrow_to_time(table: &mut DataTable, col: usize) -> TimeKind {
    let within_day = table.rows.iter().all(|r| match r.get(col) {
        Some(CellValue::Duration(ns)) => (0..NANOS_PER_DAY).contains(ns),
        Some(CellValue::Null) | None => true,
        _ => false,
    });
    if !within_day {
        return TimeKind::Duration;
    }
    for row in &mut table.rows {
        if let Some(cell) = row.get_mut(col)
            && let CellValue::Duration(ns) = *cell
        {
            *cell = CellValue::Time(ns);
        }
    }
    let nanos = needs_nanos(table.rows.iter().filter_map(|r| r.get(col)));
    if let Some(c) = table.columns.get_mut(col) {
        c.data_type = TimeKind::Time.data_type(nanos).to_string();
    }
    TimeKind::Time
}

/// Whether two column types are one type: equal, or the same time kind in
/// any unit (a `Time64(µs)` file and a `Time64(ns)` file hold the same
/// kind of value). For schema comparisons, drift and union.
pub fn same_type(a: &str, b: &str) -> bool {
    a == b || TimeKind::of_type(a).is_some_and(|k| TimeKind::of_type(b) == Some(k))
}

/// The column type a type word stands for wherever a user names one (casts,
/// column specs, schema files): `time` and `duration`, in any case, give
/// Octa's own types. `None` for anything else; a full name such as
/// `Time64(µs)` is already a type and used as given.
pub fn type_from_name(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "time" => Some(TimeKind::Time.data_type(false)),
        "duration" => Some(TimeKind::Duration.data_type(false)),
        _ => None,
    }
}

/// Text read as an amount of time in nanoseconds: a duration in any of the
/// three styles, or a clock time (`9:05 AM`) as the time since midnight. The
/// one reader for comparisons and Range bounds, so `> 08:30` and `> 8:30 AM`
/// agree and a clock time compares with a duration of the same length.
pub fn read_time_text(s: &str) -> Option<i64> {
    parse_duration(s.trim()).or_else(|| parse_time(s))
}

/// `func(col)` as DuckDB SQL (`col` already quoted), written so a Duration
/// column works: DuckDB has no `sum` over INTERVAL, so a Duration sum adds
/// microseconds and turns the total back into an INTERVAL. `over` is a window
/// frame (`PARTITION BY .. ORDER BY ..`), which belongs to the aggregate.
pub fn sql_aggregate(func: &str, col: &str, kind: Option<TimeKind>, over: Option<&str>) -> String {
    let over = over.map(|f| format!(" OVER ({f})")).unwrap_or_default();
    if func == "sum" && kind == Some(TimeKind::Duration) {
        return format!(
            "to_microseconds(CAST(sum(CAST(round(epoch({col}) * 1000000) AS BIGINT)){over} AS BIGINT))"
        );
    }
    format!("{func}({col}){over}")
}

/// The kind of the column called `name` among `cols`.
pub fn kind_of(cols: &[crate::data::ColumnInfo], name: &str) -> Option<TimeKind> {
    cols.iter()
        .find(|c| c.name == name)
        .and_then(|c| TimeKind::of_type(&c.data_type))
}

#[cfg(test)]
#[path = "times_tests.rs"]
mod tests;
