//! One SSH connection to one host: dial, check the host key against
//! `~/.ssh/known_hosts`, sign in. Shared by the DB tunnel
//! (`crate::db::ssh_tunnel`) and the SFTP storage type, so both refuse a
//! changed host key the same way and offer the same three sign-in methods.
//!
//! Implemented with `russh` rather than by shelling out to `ssh`: a
//! self-contained binary is the point of Octa's packaging, and `ssh` cannot do
//! password authentication without a terminal, which a GUI has not got.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use russh::client::{self, Handle};
use russh::keys::known_hosts::learn_known_hosts;
use russh::keys::{PrivateKeyWithHashAlg, check_known_hosts, load_secret_key};
use serde::{Deserialize, Serialize};

/// A live, signed-in SSH session.
pub type Session = Handle<HostKeyCheck>;

/// Dial `target`, check its host key and sign in. Blocks on `rt`: russh spawns
/// the session's background task onto the runtime that runs the handshake, so
/// the caller picks the runtime the session lives on (the DB runtime for a
/// tunnel, the cloud runtime for SFTP).
pub fn connect(
    rt: &tokio::runtime::Runtime,
    target: &SshTarget,
    secret: Option<&str>,
) -> Result<Session> {
    let host = target.host.trim();
    if host.is_empty() {
        bail!("the SSH connection has no host");
    }
    let problem = Arc::new(Mutex::new(None));
    let handler = HostKeyCheck {
        host: host.to_string(),
        port: target.port,
        accept_new: target.accept_new_host_key,
        problem: Arc::clone(&problem),
    };
    let config = Arc::new(client::Config {
        nodelay: true,
        ..Default::default()
    });
    let addr = format!("{host}:{}", target.port);
    let session = rt
        .block_on(client::connect(config, addr.clone(), handler))
        .map_err(|e| host_key_error(&problem, &addr, e))?;
    authenticate(session, target, secret, &addr, rt)
}

/// `~/x` -> `<home>/x`. `load_secret_key` takes the path literally, and every
/// hint Octa shows writes key paths the way a shell user would. Also the
/// database CA file (`db::tls`).
pub(crate) fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn default_ssh_port() -> u16 {
    22
}

/// Where an SSH host is and how to sign in to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SshTarget {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub auth: SshAuth,
    /// Accept and remember the host's key the first time it is seen, the
    /// way OpenSSH's `StrictHostKeyChecking=accept-new` does.
    ///
    /// Off by default, and it only ever covers a host that is **not** in
    /// `known_hosts` yet. A host whose key has *changed* is refused whatever
    /// this says: that is the case worth being loud about.
    #[serde(default)]
    pub accept_new_host_key: bool,
}

/// How to sign in to the host. The passphrase for `PrivateKey` and the
/// password for `Password` both live in the keyring beside the connection's
/// other secrets, never in `settings.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SshAuth {
    /// Use the agent already holding unlocked keys: `SSH_AUTH_SOCK` on Linux
    /// and macOS, Pageant or the OpenSSH agent pipe on Windows.
    #[default]
    Agent,
    PrivateKey {
        path: String,
    },
    Password,
}

impl SshAuth {
    /// i18n key suffix under `[db]`, mirroring [`crate::db::DbAuthKind::i18n_key`].
    pub fn i18n_key(&self) -> &'static str {
        match self {
            SshAuth::Agent => "ssh_auth_agent",
            SshAuth::PrivateKey { .. } => "ssh_auth_key",
            SshAuth::Password => "ssh_auth_password",
        }
    }
}

/// What went wrong with the host's key, kept out of band because the
/// russh handler can only answer yes or no.
#[derive(Debug, Clone)]
enum HostKeyProblem {
    /// Never seen before, and the connection is not set to accept new keys.
    Unknown,
    /// Seen before with a *different* key. Refused unconditionally.
    Changed,
}

/// The russh handler that checks the host key against `~/.ssh/known_hosts`.
pub struct HostKeyCheck {
    host: String,
    port: u16,
    accept_new: bool,
    problem: Arc<Mutex<Option<HostKeyProblem>>>,
}

impl client::Handler for HostKeyCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match server_public_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            // A host certificate is a shape `known_hosts` cannot answer for.
            // Treat it as unknown rather than quietly trusting it.
            russh::keys::PublicKeyOrCertificate::Certificate(_) => {
                *self.problem.lock().unwrap() = Some(HostKeyProblem::Unknown);
                return Ok(false);
            }
        };
        match check_known_hosts(&self.host, self.port, &key) {
            Ok(true) => Ok(true),
            Ok(false) if self.accept_new => {
                // Trust on first use, and write it down so a later change is
                // detected as a change rather than as another first sight.
                let _ = learn_known_hosts(&self.host, self.port, &key);
                Ok(true)
            }
            Ok(false) => {
                *self.problem.lock().unwrap() = Some(HostKeyProblem::Unknown);
                Ok(false)
            }
            // The recorded key and the offered key disagree. Never accepted,
            // regardless of `accept_new_host_key`.
            Err(_) => {
                *self.problem.lock().unwrap() = Some(HostKeyProblem::Changed);
                Ok(false)
            }
        }
    }
}

/// Turn a failed handshake into something a person can act on. A rejected host
/// key surfaces from russh as a generic failure, so the real reason is the one
/// the handler recorded on its way past.
fn host_key_error(
    problem: &Arc<Mutex<Option<HostKeyProblem>>>,
    addr: &str,
    e: russh::Error,
) -> anyhow::Error {
    match problem.lock().unwrap().clone() {
        Some(HostKeyProblem::Changed) => anyhow::anyhow!(
            "the host key of {addr} does not match the one recorded in ~/.ssh/known_hosts. \
             Either the server was rebuilt, or something is impersonating it. Octa will not \
             connect until you check which, and remove the old line yourself if it was a rebuild"
        ),
        Some(HostKeyProblem::Unknown) => anyhow::anyhow!(
            "{addr} is not in ~/.ssh/known_hosts, so Octa cannot tell whether it is the right \
             server. Connect to it once with ssh to record its key, or tick \"Accept a new host \
             key\" on this connection to accept it the first time"
        ),
        None => anyhow::Error::new(e).context(format!("connecting to the SSH host {addr}")),
    }
}

/// Sign in to the host the way the connection says to.
fn authenticate(
    mut session: Session,
    ssh: &SshTarget,
    ssh_secret: Option<&str>,
    addr: &str,
    rt: &tokio::runtime::Runtime,
) -> Result<Session> {
    let user = ssh.username.trim().to_string();
    if user.is_empty() {
        bail!("the SSH connection to {addr} has no username");
    }
    let ok = match &ssh.auth {
        SshAuth::Password => {
            let password = ssh_secret.filter(|s| !s.is_empty()).with_context(|| {
                format!("no SSH password stored for {addr}; set one in Settings")
            })?;
            rt.block_on(session.authenticate_password(user, password))
                .context("authenticating to the SSH host with a password")?
                .success()
        }
        SshAuth::PrivateKey { path } => {
            if path.trim().is_empty() {
                bail!("the SSH connection to {addr} has no private-key path");
            }
            // An empty passphrase and no passphrase are the same thing to
            // `load_secret_key`, so an unencrypted key needs nothing stored.
            let passphrase = ssh_secret.filter(|s| !s.is_empty());
            let key = load_secret_key(expand_home(path.trim()), passphrase).with_context(|| {
                format!(
                    "reading the SSH private key {path} (an encrypted key needs its passphrase \
                     stored on this connection)"
                )
            })?;
            let hash = rt
                .block_on(session.best_supported_rsa_hash())
                .context("negotiating the signature algorithm with the SSH host")?
                .flatten();
            rt.block_on(
                session
                    .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)),
            )
            .context("authenticating to the SSH host with a private key")?
            .success()
        }
        SshAuth::Agent => {
            let mut agent = rt
                .block_on(agent_client())
                .context("connecting to the SSH agent")?;
            let identities = rt
                .block_on(agent.request_identities())
                .context("asking the SSH agent for its keys")?;
            if identities.is_empty() {
                bail!(
                    "the SSH agent is running but holds no keys; add one with ssh-add, or pick a \
                     private key file on this connection instead"
                );
            }
            let rsa_hash = rt
                .block_on(session.best_supported_rsa_hash())
                .context("negotiating the signature algorithm with the SSH host")?
                .flatten();
            let mut authenticated = false;
            // Offer every identity the agent holds, the way ssh itself does:
            // an agent usually carries several keys and only the server knows
            // which one it will take.
            for id in identities {
                let key = match id {
                    russh::keys::agent::AgentIdentity::PublicKey { key, .. } => key,
                    // A certificate identity goes through a different auth
                    // method. Skip it rather than failing the whole attempt,
                    // since the agent normally also holds plain keys.
                    _ => continue,
                };
                // `hash_alg` selects rsa-sha2-256/512 and means nothing for the
                // other algorithms, where sending one would name a signature
                // scheme that does not exist.
                let hash_alg = matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. })
                    .then_some(rsa_hash)
                    .flatten();
                let res = rt
                    .block_on(session.authenticate_publickey_with(
                        user.clone(),
                        key,
                        hash_alg,
                        &mut agent,
                    ))
                    .context("authenticating to the SSH host through the agent")?;
                if res.success() {
                    authenticated = true;
                    break;
                }
            }
            authenticated
        }
    };
    if !ok {
        bail!(
            "the SSH host {addr} refused the credentials for user '{}'",
            ssh.username.trim()
        );
    }
    Ok(session)
}

/// Connect to the platform's SSH agent: Pageant or the OpenSSH pipe on
/// Windows, `SSH_AUTH_SOCK` everywhere else.
async fn agent_client() -> Result<
    russh::keys::agent::client::AgentClient<
        Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>,
    >,
> {
    #[cfg(windows)]
    {
        if let Ok(c) = russh::keys::agent::client::AgentClient::connect_pageant().await {
            return Ok(c.dynamic());
        }
        // russh has no `connect_env` on Windows: OpenSSH exposes the agent as
        // a named pipe, not a socket. SSH_AUTH_SOCK carries the pipe path when
        // it is set at all, otherwise the ssh-agent service's default one.
        let pipe = std::env::var("SSH_AUTH_SOCK")
            .unwrap_or_else(|_| r"\\.\pipe\openssh-ssh-agent".to_string());
        let c = russh::keys::agent::client::AgentClient::connect_named_pipe(&pipe)
            .await
            .with_context(|| {
                format!(
                    "no SSH agent found (Pageant is not running and nothing answers at {pipe}); \
                     start one, or pick a private key file on this connection instead"
                )
            })?;
        Ok(c.dynamic())
    }
    #[cfg(not(windows))]
    {
        let c = russh::keys::agent::client::AgentClient::connect_env()
            .await
            .context(
                "no SSH agent found (SSH_AUTH_SOCK is unset or the agent is not running); start \
                 one, or pick a private key file on this connection instead",
            )?;
        Ok(c.dynamic())
    }
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
mod tests;
