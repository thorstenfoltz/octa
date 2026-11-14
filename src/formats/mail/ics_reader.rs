//! iCalendar `.ics` and the older vCalendar `.vcs`: events become the
//! Calendar table, to-dos the Tasks table. Recurrence rules are kept as text,
//! not expanded; zoned times are shown as written.

use std::path::Path;

use anyhow::Result;

use super::content_line::{Prop, ics_time, parse, unfold};
use super::{Event, MailBook, Task, join, read_text, with_format};
use crate::data::DataTable;
use crate::formats::{FormatReader, TableInfo};

pub struct IcsReader;

/// `Jane Doe <jane@x.org>` from `ORGANIZER;CN=Jane Doe:mailto:jane@x.org`.
fn person(p: &Prop) -> String {
    let v = p.value.trim();
    let addr = v
        .get(..7)
        .filter(|s| s.eq_ignore_ascii_case("mailto:"))
        .map_or(v, |_| &v[7..]);
    match p.param("CN").filter(|cn| !cn.is_empty()) {
        Some(cn) if addr.is_empty() => cn.to_string(),
        Some(cn) => format!("{cn} <{addr}>"),
        None => addr.to_string(),
    }
}

/// Every event and to-do in `text`. Properties of a nested block (an alarm
/// inside an event) are not the event's own and are skipped.
pub fn parse_book(text: &str) -> MailBook {
    let mut book = MailBook::default();
    let mut stack: Vec<String> = Vec::new();
    let mut event: Option<Event> = None;
    let mut attendees: Vec<String> = Vec::new();
    let mut task: Option<Task> = None;
    for line in unfold(text) {
        let Some(p) = parse(&line) else {
            continue;
        };
        match p.name.as_str() {
            "BEGIN" => {
                let block = p.value.trim().to_ascii_uppercase();
                match block.as_str() {
                    "VEVENT" => {
                        event = Some(Event::default());
                        attendees.clear();
                    }
                    "VTODO" => task = Some(Task::default()),
                    _ => {}
                }
                stack.push(block);
                continue;
            }
            "END" => {
                match p.value.trim().to_ascii_uppercase().as_str() {
                    "VEVENT" => {
                        if let Some(mut e) = event.take() {
                            e.attendees = join(attendees.drain(..));
                            book.events.push(e);
                        }
                    }
                    "VTODO" => book.tasks.extend(task.take()),
                    _ => {}
                }
                stack.pop();
                continue;
            }
            _ => {}
        }
        match (stack.last().map(String::as_str), &mut event, &mut task) {
            (Some("VEVENT"), Some(e), _) => match p.name.as_str() {
                "SUMMARY" => e.title = p.text(),
                "DTSTART" => (e.start, e.all_day) = ics_time(&p),
                "DTEND" => e.end = ics_time(&p).0,
                "LOCATION" => e.location = p.text(),
                "ORGANIZER" => e.organiser = person(&p),
                "ATTENDEE" => attendees.push(person(&p)),
                "RRULE" => e.recurrence = p.value.trim().to_string(),
                "DESCRIPTION" => e.description = p.text(),
                _ => {}
            },
            (Some("VTODO"), _, Some(t)) => match p.name.as_str() {
                "SUMMARY" => t.subject = p.text(),
                "DTSTART" => t.start = ics_time(&p).0,
                "DUE" => t.due = ics_time(&p).0,
                "STATUS" => t.status = p.text(),
                "PERCENT-COMPLETE" => t.percent_done = p.value.trim().parse().ok(),
                "PRIORITY" => t.priority = p.value.trim().to_string(),
                "DESCRIPTION" => t.body = p.text(),
                _ => {}
            },
            _ => {}
        }
    }
    book
}

fn book(path: &Path) -> Result<MailBook> {
    Ok(parse_book(&read_text(path)?))
}

impl FormatReader for IcsReader {
    fn name(&self) -> &str {
        "iCalendar"
    }

    fn extensions(&self) -> &[&str] {
        &["ics", "vcs"]
    }

    fn read_file(&self, path: &Path) -> Result<DataTable> {
        Ok(with_format(book(path)?.single(), self.name()))
    }

    /// Only a calendar holding both events and to-dos has two tables.
    fn list_tables(&self, path: &Path) -> Result<Option<Vec<TableInfo>>> {
        let infos = book(path)?.table_infos();
        Ok((infos.len() > 1).then_some(infos))
    }

    fn read_table(&self, path: &Path, table: &str) -> Result<DataTable> {
        Ok(with_format(book(path)?.table(table)?, self.name()))
    }

    fn opens_all_tables(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "ics_reader_tests.rs"]
mod tests;
