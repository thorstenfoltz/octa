//! SFTP as a cloud storage type. One SSH session (plus its SFTP channel) per
//! connection stays open while the user browses, so a click in the tree is
//! one round trip, not a fresh handshake. Connect, host-key check and sign-in
//! are the shared [`crate::ssh::connect`], the path the DB tunnel takes too.
//!
//! Keys are server paths without the leading slash (`home/anna/x.csv`), the
//! shape of an object key, so the tree, Union and inventory need nothing
//! SFTP-specific.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use russh_sftp::client::SftpSession;
use russh_sftp::client::fs::{File, Metadata};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// `provider` is a private sibling module; COPY_CHUNK_BYTES is not re-exported
// from `cloud`, so it comes from there directly.
use super::provider::{COPY_CHUNK_BYTES, walk_recursive};
use super::{CloudProvider, CloudUpload, ObjectEntry, copy_across, runtime};
use crate::ssh::{SshAuth, SshTarget};

/// A live SFTP channel and the SSH session carrying it. The session handle is
/// held only to keep the connection open.
struct Live {
    _ssh: crate::ssh::Session,
    sftp: SftpSession,
}

/// Open sessions, keyed by user, host, port and sign-in method.
///
/// ponytail: a session lives until the process exits or an error drops it;
/// nothing closes an idle one. Add an idle timeout if someone browses dozens
/// of servers in one sitting.
fn sessions() -> &'static Mutex<HashMap<String, Arc<Live>>> {
    static S: OnceLock<Mutex<HashMap<String, Arc<Live>>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A [`CloudProvider`] over one SFTP server.
pub struct SftpProvider {
    target: SshTarget,
    secret: Option<String>,
    key: String,
}

impl SftpProvider {
    /// `secret` is the password for [`SshAuth::Password`], the passphrase for
    /// [`SshAuth::PrivateKey`], unused for the agent.
    pub fn new(target: SshTarget, secret: Option<String>) -> Self {
        let auth = match &target.auth {
            SshAuth::Agent => "agent".to_string(),
            SshAuth::PrivateKey { path } => format!("key:{path}"),
            SshAuth::Password => "password".to_string(),
        };
        let key = format!("{}@{}:{}|{auth}", target.username, target.host, target.port);
        Self {
            target,
            secret,
            key,
        }
    }

    fn live(&self) -> Result<Arc<Live>> {
        if let Some(live) = sessions().lock().unwrap().get(&self.key) {
            return Ok(Arc::clone(live));
        }
        let rt = runtime();
        let ssh = crate::ssh::connect(rt, &self.target, self.secret.as_deref())?;
        let channel = rt
            .block_on(ssh.channel_open_session())
            .context("opening an SSH channel for SFTP")?;
        rt.block_on(channel.request_subsystem(true, "sftp"))
            .context("starting SFTP on the server (is SFTP enabled there?)")?;
        let sftp = rt
            .block_on(SftpSession::new(channel.into_stream()))
            .context("starting the SFTP session")?;
        let live = Arc::new(Live { _ssh: ssh, sftp });
        sessions()
            .lock()
            .unwrap()
            .insert(self.key.clone(), Arc::clone(&live));
        Ok(live)
    }

    /// Run `op` on the session, reconnecting once if the connection failed. An
    /// idle session is often cut by a NAT or the server, and the first call
    /// afterwards is the one that notices. A refusal from the server (no such
    /// file, permission denied) proves the session works and is returned as is.
    fn with_session<T>(&self, op: impl Fn(&SftpSession) -> Result<T>) -> Result<T> {
        let live = self.live()?;
        match op(&live.sftp) {
            Ok(v) => Ok(v),
            Err(first) if server_answered(&first) => Err(first),
            Err(first) => {
                sessions().lock().unwrap().remove(&self.key);
                let Ok(live) = self.live() else {
                    return Err(first);
                };
                op(&live.sftp)
            }
        }
    }
}

/// True when `e` is the server's own status reply, so the session is alive.
fn server_answered(e: &anyhow::Error) -> bool {
    matches!(
        e.downcast_ref::<russh_sftp::client::error::Error>(),
        Some(russh_sftp::client::error::Error::Status(_))
    )
}

/// The provider for a saved (or ephemeral) SFTP connection.
pub fn build_sftp_provider(
    conn: &super::CloudConnection,
    creds: &super::ProviderCreds,
) -> SftpProvider {
    let (host, port) = conn.sftp_host_port();
    let target = SshTarget {
        host,
        port,
        username: conn.username.clone().unwrap_or_default(),
        auth: conn.ssh_auth.clone().unwrap_or_default(),
        accept_new_host_key: conn.accept_new_host_key,
    };
    let secret = match creds {
        super::ProviderCreds::Password(p) => Some(p.clone()),
        _ => None,
    };
    SftpProvider::new(target, secret)
}

/// Server path of a key: keys carry no leading slash, SFTP paths do.
fn remote(key: &str) -> String {
    format!("/{}", key.trim_matches('/'))
}

/// The key a listed child gets under `prefix`. Folders end in `/`.
fn child_key(prefix: &str, name: &str, is_dir: bool) -> String {
    let p = prefix.trim_matches('/');
    let base = if p.is_empty() {
        name.to_string()
    } else {
        format!("{p}/{name}")
    };
    if is_dir { format!("{base}/") } else { base }
}

/// Hidden sibling a save is written to before it replaces the real file.
fn temp_sibling(key: &str) -> String {
    let path = remote(key);
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
    format!("{dir}/.{name}.octa-part")
}

/// The folder part of a server path (`""` for a file at the root).
fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

fn entry(prefix: &str, name: String, meta: &Metadata) -> ObjectEntry {
    let is_dir = meta.file_type().is_dir();
    ObjectEntry {
        key: child_key(prefix, &name, is_dir),
        name,
        is_prefix: is_dir,
        size: if is_dir { None } else { meta.size },
        modified: if is_dir {
            None
        } else {
            meta.mtime
                .and_then(|t| DateTime::<Utc>::from_timestamp(i64::from(t), 0))
        },
        etag: None,
        version: None,
    }
}

/// `mkdir -p`, one folder at a time from the root.
async fn mkdir_p(s: &SftpSession, dir: &str) -> Result<()> {
    let mut path = String::new();
    for seg in dir.split('/').filter(|s| !s.is_empty()) {
        path.push('/');
        path.push_str(seg);
        if !s.try_exists(path.clone()).await? {
            s.create_dir(path.clone())
                .await
                .with_context(|| format!("creating folder {path}"))?;
        }
    }
    Ok(())
}

/// Replace `target` with `part`. SFTP v3 `rename` refuses an existing target
/// and russh-sftp 3 has no `posix-rename@openssh.com`.
///
/// ponytail: remove-then-rename leaves a moment with no file under the name;
/// send the posix-rename extension through `RawSftpSession` if that window
/// ever matters.
async fn replace(s: &SftpSession, part: &str, target: &str) -> Result<()> {
    if s.try_exists(target.to_string()).await? {
        s.remove_file(target.to_string())
            .await
            .with_context(|| format!("replacing {target}"))?;
    }
    s.rename(part.to_string(), target.to_string())
        .await
        .with_context(|| format!("renaming {part} to {target}"))
}

/// Read until `buf` is full or the file ends: one SFTP read returns at most
/// one packet, far less than a copy block.
async fn fill(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = file.read(&mut buf[filled..]).await?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

impl CloudProvider for SftpProvider {
    fn list(&self, prefix: &str) -> Result<Vec<ObjectEntry>> {
        let rt = runtime();
        let dir = remote(prefix);
        self.with_session(|s| {
            let listing = rt
                .block_on(s.read_dir(dir.clone()))
                .with_context(|| format!("listing {dir}"))?;
            let mut out = Vec::new();
            for e in listing {
                let name = e.file_name();
                let mut meta = e.metadata();
                if meta.file_type().is_symlink() {
                    // Follow the link, so a linked folder opens like a folder.
                    let path = remote(&child_key(prefix, &name, false));
                    if let Ok(target) = rt.block_on(s.metadata(path)) {
                        meta = target;
                    }
                }
                out.push(entry(prefix, name, &meta));
            }
            Ok(out)
        })
    }

    fn list_recursive(&self, prefix: &str, cap: usize) -> Result<(Vec<ObjectEntry>, bool)> {
        self.list_recursive_until(prefix, cap, &AtomicBool::new(false))
    }

    fn list_recursive_until(
        &self,
        prefix: &str,
        cap: usize,
        stop: &AtomicBool,
    ) -> Result<(Vec<ObjectEntry>, bool)> {
        walk_recursive(&|p| self.list(p), prefix, cap, stop)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>> {
        let rt = runtime();
        let path = remote(key);
        self.with_session(|s| {
            rt.block_on(s.read(path.clone()))
                .with_context(|| format!("downloading {path}"))
        })
    }

    fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        let mut upload = self.put_streaming(key)?;
        upload.write_chunk(bytes)?;
        upload.finish()
    }

    fn delete(&self, key: &str) -> Result<()> {
        let rt = runtime();
        let path = remote(key);
        self.with_session(|s| {
            if !rt.block_on(s.try_exists(path.clone()))? {
                return Ok(());
            }
            rt.block_on(s.remove_file(path.clone()))
                .with_context(|| format!("deleting {path}"))
        })
    }

    /// SFTP has no server-side copy: the bytes travel through Octa in blocks.
    fn copy(&self, from: &str, to: &str) -> Result<()> {
        copy_across(self, from, self, to)
    }

    fn rename(&self, from: &str, to: &str) -> Result<()> {
        let rt = runtime();
        let (from_p, to_p) = (remote(from), remote(to));
        self.with_session(|s| {
            rt.block_on(async {
                mkdir_p(s, parent_dir(&to_p)).await?;
                replace(s, &from_p, &to_p).await
            })
        })
    }

    fn get_streaming(
        &self,
        key: &str,
        on_chunk: &mut dyn FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        let rt = runtime();
        let path = remote(key);
        let mut file = self.with_session(|s| {
            rt.block_on(s.open(path.clone()))
                .with_context(|| format!("opening {path}"))
        })?;
        let mut buf = vec![0u8; COPY_CHUNK_BYTES];
        loop {
            // One block_on per read, never one around the loop: `on_chunk`
            // may block on the runtime itself (a copy writes each block), and
            // a nested block_on panics.
            let filled = rt
                .block_on(fill(&mut file, &mut buf))
                .with_context(|| format!("reading {path}"))?;
            if filled == 0 {
                return Ok(());
            }
            on_chunk(&buf[..filled])?;
        }
    }

    fn put_streaming(&self, key: &str) -> Result<Box<dyn CloudUpload + '_>> {
        let rt = runtime();
        let target = remote(key);
        let part = temp_sibling(key);
        let file = self.with_session(|s| {
            rt.block_on(async {
                mkdir_p(s, parent_dir(&target)).await?;
                s.create(part.clone())
                    .await
                    .with_context(|| format!("creating {part}"))
            })
        })?;
        Ok(Box::new(SftpUpload {
            live: self.live()?,
            file: Some(file),
            part,
            target,
        }))
    }
}

/// A save in progress: blocks go to the hidden sibling, `finish` swaps it in.
/// Dropping one unfinished leaves the `.octa-part` file behind, never a
/// half-written target.
struct SftpUpload {
    live: Arc<Live>,
    file: Option<File>,
    part: String,
    target: String,
}

impl CloudUpload for SftpUpload {
    fn write_chunk(&mut self, bytes: Vec<u8>) -> Result<()> {
        let Some(file) = self.file.as_mut() else {
            anyhow::bail!("the upload to {} is already finished", self.target);
        };
        runtime()
            .block_on(file.write_all(&bytes))
            .with_context(|| format!("writing {}", self.part))
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        let rt = runtime();
        if let Some(mut file) = self.file.take() {
            rt.block_on(async {
                file.flush().await?;
                file.close().await
            })
            .with_context(|| format!("closing {}", self.part))?;
        }
        rt.block_on(replace(&self.live.sftp, &self.part, &self.target))
    }
}

#[cfg(test)]
#[path = "sftp_tests.rs"]
mod tests;
