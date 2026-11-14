//! Follow a growing file: every check reads only the bytes appended since
//! the last one. Polling (size and modification time), no file watcher.
//! A file that shrank, or whose bytes before the last offset changed
//! (checked on the last 4 KB), was replaced: the tab reloads in full.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

const TAIL: usize = 4096;

#[derive(Debug, Clone, PartialEq)]
pub enum FollowStep {
    Unchanged,
    /// Complete lines, each ending in a line break.
    Appended(String),
    Reload,
}

#[derive(Debug, Clone)]
pub struct FollowState {
    pub offset: u64,
    pub len: u64,
    pub modified: Option<SystemTime>,
    /// The last bytes before `offset`, to recognise a replaced file.
    pub tail: Vec<u8>,
    /// A line still being written.
    pub carry: Vec<u8>,
}

fn read_range(path: &Path, from: u64, len: u64) -> io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::with_capacity(len as usize);
    f.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

impl FollowState {
    pub fn start(path: &Path) -> io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let len = meta.len();
        let from = len.saturating_sub(TAIL as u64);
        Ok(Self {
            offset: len,
            len,
            modified: meta.modified().ok(),
            tail: read_range(path, from, len - from)?,
            carry: Vec::new(),
        })
    }

    /// Read at most `max_bytes` new bytes. ponytail: one slice per check so a
    /// burst of megabytes never stalls a frame; the rest comes next second.
    pub fn step(&mut self, path: &Path, max_bytes: u64) -> io::Result<FollowStep> {
        let meta = std::fs::metadata(path)?;
        let (len, modified) = (meta.len(), meta.modified().ok());
        if len < self.offset {
            return Ok(FollowStep::Reload);
        }
        if len == self.len && modified == self.modified && len == self.offset {
            return Ok(FollowStep::Unchanged);
        }
        let from = self.offset - self.tail.len() as u64;
        if read_range(path, from, self.tail.len() as u64)? != self.tail {
            return Ok(FollowStep::Reload);
        }
        let take = (len - self.offset).min(max_bytes);
        let new = read_range(path, self.offset, take)?;
        self.offset += new.len() as u64;
        self.len = len;
        self.modified = modified;
        self.tail.extend_from_slice(&new);
        if self.tail.len() > TAIL {
            self.tail.drain(..self.tail.len() - TAIL);
        }
        self.carry.extend_from_slice(&new);
        let Some(end) = self.carry.iter().rposition(|&b| b == b'\n') else {
            return Ok(FollowStep::Unchanged);
        };
        let rest = self.carry.split_off(end + 1);
        let lines = std::mem::replace(&mut self.carry, rest);
        Ok(FollowStep::Appended(
            String::from_utf8_lossy(&lines).into_owned(),
        ))
    }
}

/// The file's first line without BOM and line break: a delimited file's
/// header, put in front of appended lines so they parse with the same names.
pub fn first_line(path: &Path) -> io::Result<String> {
    let mut line = String::new();
    BufReader::new(File::open(path)?).read_line(&mut line)?;
    Ok(line
        .trim_start_matches('\u{feff}')
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

#[cfg(test)]
#[path = "follow_tests.rs"]
mod tests;
