//! Reaching a database through a jump host.
//!
//! Almost every managed Postgres inside a company sits behind a bastion, and
//! without this the answer is "open a tunnel in a terminal first". A connection
//! carrying an [`SshTunnel`] gets a local forward opened for it on demand:
//! Octa binds a loopback port, dials the bastion over SSH, and every byte the
//! database client writes is carried over a `direct-tcpip` channel to the real
//! server.
//!
//! **The connection keeps its real host and port.** Only the socket goes to
//! loopback ([`DbConnection::dial_target`]); `conn.host` stays the database's
//! own name so TLS verification, Entra token audiences and error messages all
//! keep naming the database rather than `127.0.0.1`. Connectors whose client
//! library can separate the two (Postgres via `hostaddr`, MySQL via a TLS
//! hostname override, SQL Server because we hand it the socket) therefore
//! validate certificates exactly as they would without a tunnel.
//!
//! Implemented with `russh` rather than by shelling out to `ssh -L`: a
//! self-contained binary is the point of Octa's packaging, and `ssh` cannot do
//! password authentication without a terminal, which a GUI has not got.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use russh::client::{self, Handle};
use russh::keys::known_hosts::learn_known_hosts;
use russh::keys::{PrivateKeyWithHashAlg, check_known_hosts, load_secret_key};
use serde::{Deserialize, Serialize};

use super::DbConnection;

fn default_ssh_port() -> u16 {
    22
}

/// Where the bastion is and how to authenticate to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SshTunnel {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub auth: SshAuth,
    /// Accept and remember the bastion's key the first time it is seen, the
    /// way OpenSSH's `StrictHostKeyChecking=accept-new` does.
    ///
    /// Off by default, and it only ever covers a host that is **not** in
    /// `known_hosts` yet. A host whose key has *changed* is refused whatever
    /// this says: that is the case worth being loud about.
    #[serde(default)]
    pub accept_new_host_key: bool,
}

/// How to authenticate to the bastion. The passphrase for `PrivateKey` and the
/// password for `Password` both live in the keyring beside the connection's
/// database secret, never in `settings.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// i18n key suffix under `[db]`, mirroring [`super::DbAuthKind::i18n_key`].
    pub fn i18n_key(&self) -> &'static str {
        match self {
            SshAuth::Agent => "ssh_auth_agent",
            SshAuth::PrivateKey { .. } => "ssh_auth_key",
            SshAuth::Password => "ssh_auth_password",
        }
    }
}

/// What went wrong with the bastion's host key, kept out of band because the
/// russh handler can only answer yes or no.
#[derive(Debug, Clone)]
enum HostKeyProblem {
    /// Never seen before, and the connection is not set to accept new keys.
    Unknown,
    /// Seen before with a *different* key. Refused unconditionally.
    Changed,
}

struct HostKeyCheck {
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

/// Live tunnels for this process, keyed by everything that would make one
/// different. Editing the bastion settings therefore opens a fresh tunnel
/// rather than silently reusing the old one.
///
/// ponytail: tunnels live until the process exits; nothing counts users or
/// closes an idle one. A session opens a handful, each costing one loopback
/// socket and one task. Refcount them against the connection cache if someone
/// ever leaves Octa open for weeks against dozens of bastions.
fn tunnels() -> &'static Mutex<HashMap<String, u16>> {
    static T: OnceLock<Mutex<HashMap<String, u16>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Identity of a tunnel: change any of this and a new one is opened.
fn tunnel_key(conn: &DbConnection, ssh: &SshTunnel) -> String {
    let auth = match &ssh.auth {
        SshAuth::Agent => "agent".to_string(),
        SshAuth::PrivateKey { path } => format!("key:{path}"),
        SshAuth::Password => "password".to_string(),
    };
    format!(
        "{}|{}@{}:{}|{}|{}:{}",
        conn.id, ssh.username, ssh.host, ssh.port, auth, conn.host, conn.port
    )
}

/// The loopback port `conn` is reachable on, opening the tunnel if needed.
/// `None` when the connection has no bastion configured, which is the common
/// case and costs nothing.
///
/// `ssh_secret` is the keyring entry for this connection's SSH credential: the
/// key passphrase for [`SshAuth::PrivateKey`], the password for
/// [`SshAuth::Password`], and unused for [`SshAuth::Agent`].
pub fn ensure(conn: &DbConnection, ssh_secret: Option<&str>) -> Result<Option<u16>> {
    // Already tunnelled: this is a copy `db::connect` handed to a connector,
    // which may later reconnect for a cancel. Reuse the forward it is holding
    // rather than looking one up, so that path needs no credential at all.
    if let Some(port) = conn.tunnel_port {
        return Ok(Some(port));
    }
    let Some(ssh) = conn.ssh.as_ref() else {
        return Ok(None);
    };
    if ssh.host.trim().is_empty() {
        bail!("the SSH tunnel for connection '{}' has no host", conn.name);
    }
    let key = tunnel_key(conn, ssh);
    if let Some(port) = tunnels().lock().unwrap().get(&key).copied() {
        return Ok(Some(port));
    }
    let port = open(conn, ssh, ssh_secret)?;
    tunnels().lock().unwrap().insert(key, port);
    Ok(Some(port))
}

/// `conn` ready to dial: the same connection with its tunnel open and
/// [`DbConnection::tunnel_port`] filled in, or an unchanged copy when it has no
/// bastion. The one call every path that opens a socket to a database goes
/// through, `db::connect` and the DuckDB ATTACH lanes alike.
pub fn with_tunnel(conn: &DbConnection, ssh_secret: Option<&str>) -> Result<DbConnection> {
    let tunnel_port = ensure(conn, ssh_secret)?;
    Ok(DbConnection {
        tunnel_port,
        ..conn.clone()
    })
}

/// Dial the bastion, authenticate, and start forwarding on a fresh loopback
/// port. Blocks on the SSH handshake, so it runs where the connectors already
/// block: on the shared DB runtime.
fn open(conn: &DbConnection, ssh: &SshTunnel, ssh_secret: Option<&str>) -> Result<u16> {
    let problem = Arc::new(Mutex::new(None));
    let handler = HostKeyCheck {
        host: ssh.host.trim().to_string(),
        port: ssh.port,
        accept_new: ssh.accept_new_host_key,
        problem: Arc::clone(&problem),
    };
    let config = Arc::new(client::Config {
        nodelay: true,
        ..Default::default()
    });

    let bastion = format!("{}:{}", ssh.host.trim(), ssh.port);
    let session = super::runtime()
        .block_on(client::connect(config, bastion.clone(), handler))
        .map_err(|e| host_key_error(&problem, &bastion, e))?;

    let session = authenticate(session, ssh, ssh_secret, &bastion)?;

    // Port 0 lets the OS pick a free port, so two tunnels never collide and
    // nothing has to be configured. Bound to loopback only: a forward reachable
    // from the network would hand the whole company a route into the database.
    let target_host = conn.host.trim().to_string();
    let target_port = conn.port;
    let listener = super::runtime()
        .block_on(tokio::net::TcpListener::bind(("127.0.0.1", 0)))
        .context("binding a local port for the SSH tunnel")?;
    let local_port = listener
        .local_addr()
        .context("reading the SSH tunnel's local port")?
        .port();

    // One authenticated session carries every forward; russh multiplexes the
    // channels. `Handle` is not `Clone`, and `channel_open_direct_tcpip` only
    // needs `&self`, so an `Arc` is all the sharing required.
    let session = Arc::new(session);
    super::runtime().spawn(async move {
        loop {
            let Ok((mut socket, peer)) = listener.accept().await else {
                break;
            };
            let session = Arc::clone(&session);
            let host = target_host.clone();
            tokio::spawn(async move {
                let channel = match session
                    .channel_open_direct_tcpip(
                        host,
                        u32::from(target_port),
                        peer.ip().to_string(),
                        u32::from(peer.port()),
                    )
                    .await
                {
                    Ok(c) => c,
                    // The database client sees a closed socket and reports its
                    // own connection error, which is the right layer for it.
                    Err(e) => {
                        tracing::warn!("ssh tunnel: opening a forward failed: {e}");
                        return;
                    }
                };
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
            });
        }
    });

    Ok(local_port)
}

/// Turn a failed handshake into something a person can act on. A rejected host
/// key surfaces from russh as a generic failure, so the real reason is the one
/// the handler recorded on its way past.
fn host_key_error(
    problem: &Arc<Mutex<Option<HostKeyProblem>>>,
    bastion: &str,
    e: russh::Error,
) -> anyhow::Error {
    match problem.lock().unwrap().clone() {
        Some(HostKeyProblem::Changed) => anyhow::anyhow!(
            "the host key of {bastion} does not match the one recorded in ~/.ssh/known_hosts. \
             Either the server was rebuilt, or something is impersonating it. Octa will not \
             connect until you check which, and remove the old line yourself if it was a rebuild"
        ),
        Some(HostKeyProblem::Unknown) => anyhow::anyhow!(
            "{bastion} is not in ~/.ssh/known_hosts, so Octa cannot tell whether it is the right \
             server. Connect to it once with ssh to record its key, or tick \"Accept a new host \
             key\" on this connection to accept it the first time"
        ),
        None => anyhow::Error::new(e).context(format!("connecting to the SSH host {bastion}")),
    }
}

/// Authenticate to the bastion the way the connection says to.
fn authenticate(
    mut session: Handle<HostKeyCheck>,
    ssh: &SshTunnel,
    ssh_secret: Option<&str>,
    bastion: &str,
) -> Result<Handle<HostKeyCheck>> {
    let user = ssh.username.trim().to_string();
    if user.is_empty() {
        bail!("the SSH tunnel to {bastion} has no username");
    }
    let rt = super::runtime();
    let ok = match &ssh.auth {
        SshAuth::Password => {
            let password = ssh_secret.filter(|s| !s.is_empty()).with_context(|| {
                format!("no SSH password stored for {bastion}; set one in Settings -> Databases")
            })?;
            rt.block_on(session.authenticate_password(user, password))
                .context("authenticating to the SSH host with a password")?
                .success()
        }
        SshAuth::PrivateKey { path } => {
            if path.trim().is_empty() {
                bail!("the SSH tunnel to {bastion} has no private-key path");
            }
            // An empty passphrase and no passphrase are the same thing to
            // `load_secret_key`, so an unencrypted key needs nothing stored.
            let passphrase = ssh_secret.filter(|s| !s.is_empty());
            let key = load_secret_key(path.trim(), passphrase).with_context(|| {
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
            "the SSH host {bastion} refused the credentials for user '{}'",
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
    }
    let c = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .context(
            "no SSH agent found (SSH_AUTH_SOCK is unset or the agent is not running); start one, \
             or pick a private key file on this connection instead",
        )?;
    Ok(c.dynamic())
}

#[cfg(test)]
#[path = "ssh_tunnel_tests.rs"]
mod tests;
