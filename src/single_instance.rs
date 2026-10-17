//! Hand files to the Octa that is already running.
//!
//! The first GUI instance listens on a loopback port and writes the port and a
//! random token into `<config dir>/instance`, readable only by the user. A
//! later start that has files connects, proves it can read that file by
//! sending the token, sends the files' absolute paths and exits. Any failure
//! (no file, stale port, wrong token, timeout) makes [`try_hand_over`] return
//! `false`, and the caller starts as its own Octa: exactly what happened
//! before this existed.
//!
//! Protocol, one TCP connection per hand-over, UTF-8 lines: `OCTA1 <token>`,
//! one absolute path per line, an empty line. The listener answers `ok`.
//! Zero paths is a ping ("is anyone there?").

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

const FILE_NAME: &str = "instance";
const MAGIC: &str = "OCTA1";
/// A shell glob over a big folder is the realistic worst case.
const MAX_PATHS: usize = 1_000;
const MAX_BYTES: u64 = 256 * 1024;

/// The running instance's end. Paths arrive on `rx`, one batch per hand-over.
pub struct Listener {
    pub rx: Receiver<Vec<PathBuf>>,
    file: PathBuf,
    token: String,
}

impl Listener {
    /// Delete the instance file if it still names this listener, so the next
    /// start does not wait out a timeout against a dead port. Call on exit.
    pub fn release(&self) {
        if read_file(&self.file).is_some_and(|(_, t)| t == self.token) {
            let _ = std::fs::remove_file(&self.file);
        }
    }
}

fn read_file(path: &Path) -> Option<(u16, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let (port, token) = text.trim().split_once(' ')?;
    Some((port.parse().ok()?, token.to_string()))
}

/// Send `paths` to the running Octa. `true` = it took them, or, with no
/// paths, it is alive.
pub fn try_hand_over(config_dir: &Path, paths: &[PathBuf]) -> bool {
    hand_over(config_dir, paths).is_ok()
}

fn hand_over(config_dir: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    let (port, token) = read_file(&config_dir.join(FILE_NAME))
        .ok_or_else(|| std::io::Error::other("no running instance recorded"))?;
    let mut msg = format!("{MAGIC} {token}\n");
    for p in paths {
        let abs = std::path::absolute(p)?;
        let s = abs
            .to_str()
            .ok_or_else(|| std::io::Error::other("path is not UTF-8"))?;
        if s.contains('\n') {
            return Err(std::io::Error::other("path holds a line break"));
        }
        msg.push_str(s);
        msg.push('\n');
    }
    msg.push('\n');
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    (&stream).write_all(msg.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    if reply.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "the running Octa refused the hand-over",
        ))
    }
}

/// Become the instance later starts hand their files to, unless another Octa
/// already answers (then `None`: that one keeps the role). `wake` runs after
/// each delivery, so the GUI repaints and drains `rx`.
pub fn listen(config_dir: &Path, wake: impl Fn() + Send + 'static) -> Option<Listener> {
    if try_hand_over(config_dir, &[]) {
        return None;
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    let token = {
        use rand::RngExt;
        format!("{:032x}", rand::rng().random::<u128>())
    };
    let file = config_dir.join(FILE_NAME);
    write_private(&file, &format!("{port} {token}\n")).ok()?;
    let (tx, rx) = channel();
    let expected = token.clone();
    std::thread::Builder::new()
        .name("octa-instance".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                if let Some(paths) = serve(stream, &expected)
                    && !paths.is_empty()
                {
                    if tx.send(paths).is_err() {
                        break;
                    }
                    wake();
                }
            }
        })
        .ok()?;
    Some(Listener { rx, file, token })
}

/// Read one hand-over. `None` for anything but a complete message carrying
/// the right token.
fn serve(stream: TcpStream, token: &str) -> Option<Vec<PathBuf>> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut writer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(stream.take(MAX_BYTES));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let sent = line.trim_end().strip_prefix(MAGIC)?.strip_prefix(' ')?;
    if sent != token {
        return None;
    }
    let mut paths = Vec::new();
    loop {
        line.clear();
        // Cut off before the closing empty line: not a complete message.
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let p = line.trim_end_matches(['\r', '\n']);
        if p.is_empty() {
            break;
        }
        if paths.len() == MAX_PATHS {
            return None;
        }
        paths.push(PathBuf::from(p));
    }
    writer.write_all(b"ok\n").ok()?;
    Some(paths)
}

/// Write `text` so only the owner can read it: created 0600 on Unix (no
/// window where it is world-readable); `%APPDATA%` is per-user on Windows.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)?.write_all(text.as_bytes())?;
    // An instance file left by an older, looser write keeps its old mode.
    crate::ui::settings::restrict_file_to_owner(path);
    Ok(())
}

#[cfg(test)]
#[path = "single_instance_tests.rs"]
mod tests;
